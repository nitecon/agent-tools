use crate::extractor::{Symbol, SymbolKind};
use crate::languages::Language;
use crate::parser::SymbolParser;
use crate::relationships::ExtractedRelationship;
use crate::synth::{self, FileSynthesis, SYNTH_PRODUCER};
use agent_knowledge::okf::OkfLimits;
use agent_knowledge::{
    CodeSnapshotInput, FileMetadataInput, ProjectIndex, RelationshipSnapshotInput, ResolutionStats,
    ResourceMatch, SymbolSnapshotInput, TraversedEdge,
};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rusqlite::params;
#[cfg(test)]
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::Duration;
use std::time::SystemTime;

/// Persistent symbol index backed by SQLite.
pub struct SymbolIndex {
    index: ProjectIndex,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl From<&Symbol> for SymbolMatch {
    fn from(s: &Symbol) -> Self {
        Self {
            name: s.name.clone(),
            kind: s.kind,
            file: s.file.clone(),
            start_line: s.start_line,
            end_line: s.end_line,
            language: s.language.to_string(),
            parent: s.parent.clone(),
        }
    }
}

impl SymbolIndex {
    /// Open or create a symbol index at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open(db_path)
                .with_context(|| format!("Failed to open index at {}", db_path.display()))?,
        })
    }

    #[cfg(test)]
    fn open_ephemeral() -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open_ephemeral()?,
        })
    }

    /// Open or create a symbol index in the centralized storage directory for the given project.
    pub fn open_for_project(project_root: &Path) -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open_for_project(project_root)?,
        })
    }

    pub fn is_ephemeral(&self) -> bool {
        self.index.is_ephemeral()
    }

    #[cfg(test)]
    fn open_persistent_or_ephemeral(db_path: &Path, busy_timeout: Duration) -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open_persistent_or_ephemeral(db_path, busy_timeout)?,
        })
    }

    /// Build or incrementally update the index for all supported files under root.
    pub fn build(&mut self, root: &Path) -> Result<IndexStats> {
        let mut parser = SymbolParser::new();
        let mut stats = IndexStats::default();
        let project_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let project_id = agent_core::project_ident(&project_root);
        let mut indexed_paths = BTreeSet::new();

        let walker = WalkBuilder::new(&project_root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Skip unsupported languages
            if Language::from_path(path).is_err() {
                continue;
            }

            stats.files_seen += 1;

            // Check if file needs re-indexing
            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let duration = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let mtime_secs = duration.as_secs() as i64;
            let mtime_nanos = duration.subsec_nanos() as i64;

            let path_str = relative_path_string(&project_root, path)?;
            let source = match std::fs::read_to_string(path) {
                Ok(source) => source,
                Err(_) => {
                    stats.files_errored += 1;
                    continue;
                }
            };
            let input_hash = blake3::hash(source.as_bytes()).to_hex().to_string();

            indexed_paths.insert(path_str.clone());

            // The extractor and the concept synthesizer are gated separately so
            // an existing index picks up synthesis without a full rebuild.
            let code_current =
                self.index
                    .producer_is_current(&path_str, "tree-sitter/1", &input_hash)?;
            let synth_current =
                self.index
                    .producer_is_current(&path_str, SYNTH_PRODUCER, &input_hash)?;
            if code_current && synth_current {
                stats.files_skipped += 1;
                continue;
            }

            // Parse and index
            match parser.parse_source_with_relationships(&source, Language::from_path(path)?, path)
            {
                Ok(parsed) => {
                    if !code_current {
                        self.index_code_file(
                            &project_id,
                            &project_root,
                            path,
                            &source,
                            &parsed.symbols,
                            &parsed.relationships,
                            mtime_secs,
                            mtime_nanos,
                        )?;
                        stats.files_indexed += 1;
                        stats.symbols_indexed += parsed.symbols.len();
                        stats.edges_indexed += parsed.relationships.len();
                    }
                    stats.concepts_indexed += self.synthesize_concepts(
                        &project_id,
                        &path_str,
                        Language::from_path(path)?,
                        &source,
                        &input_hash,
                        &parsed.symbols,
                        &parsed.relationships,
                    )?;
                }
                Err(_) => stats.files_errored += 1,
            }
        }

        stats.concepts_removed = self.prune_concepts(&project_id, &indexed_paths)?;

        let resolution = self
            .index
            .resolve_code_edges(&project_id, "tree-sitter/1")?;
        stats.edges_resolved = resolution.resolved;
        stats.edges_unresolved = resolution.unresolved;
        stats.edges_ambiguous = resolution.ambiguous;

        Ok(stats)
    }

    /// Build OKF concepts for one file and write them into the shared graph.
    ///
    /// Returns the number of concepts written. Synthesis failures are recorded
    /// as zero rather than aborting the build: a file the codec rejects must
    /// not cost the user their symbol index.
    #[allow(clippy::too_many_arguments)]
    fn synthesize_concepts(
        &mut self,
        project_id: &str,
        relative_path: &str,
        language: Language,
        source: &str,
        content_hash: &str,
        symbols: &[Symbol],
        relationships: &[ExtractedRelationship],
    ) -> Result<usize> {
        let stable_keys = stable_symbol_keys(symbols);
        let concepts = match synth::synthesize_file(
            &FileSynthesis {
                relative_path,
                language,
                source,
                content_hash,
                symbols,
                stable_keys: &stable_keys,
                relationships,
            },
            OkfLimits::default(),
        ) {
            Ok(concepts) => concepts,
            Err(_) => return Ok(0),
        };
        agent_knowledge::knowledge::index_synthesized_concepts(
            &mut self.index,
            project_id,
            &concepts,
        )?;
        self.index
            .mark_producer_state(relative_path, SYNTH_PRODUCER, content_hash)?;
        Ok(concepts.len())
    }

    /// Drop synthesized concepts whose backing file is no longer indexed.
    ///
    /// Reconciled against what is stored rather than what this run wrote, since
    /// unchanged files are skipped and never re-synthesized.
    fn prune_concepts(
        &mut self,
        project_id: &str,
        indexed_paths: &BTreeSet<String>,
    ) -> Result<usize> {
        let stored = self.index.origin_external_ids(
            project_id,
            "okf",
            agent_knowledge::knowledge::SYNTH_ORIGIN,
        )?;
        let retained = synth::retained_identities(stored.iter(), indexed_paths);
        self.index.prune_origin_resources(
            project_id,
            "okf",
            agent_knowledge::knowledge::SYNTH_ORIGIN,
            &retained,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn index_code_file(
        &mut self,
        project_id: &str,
        project_root: &Path,
        path: &Path,
        source: &str,
        symbols: &[Symbol],
        relationships: &[ExtractedRelationship],
        mtime_secs: i64,
        mtime_nanos: i64,
    ) -> Result<()> {
        let language = Language::from_path(path)?;
        let stable_keys = stable_symbol_keys(symbols);
        let symbol_kinds: Vec<String> = symbols
            .iter()
            .map(|symbol| symbol.kind.to_string())
            .collect();
        let symbol_inputs: Vec<_> = symbols
            .iter()
            .zip(&stable_keys)
            .zip(&symbol_kinds)
            .map(|((symbol, stable_key), kind)| SymbolSnapshotInput {
                stable_key,
                name: &symbol.name,
                kind,
                parent_stable_key: parent_stable_key(symbol, symbols, &stable_keys),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                start_byte: Some(symbol.start_byte),
                end_byte: Some(symbol.end_byte),
            })
            .collect();
        let relationship_metadata: Vec<Value> = relationships
            .iter()
            .map(|relationship| {
                serde_json::json!({
                    "source_symbol": relationship.source_symbol,
                    "raw_target": relationship.target,
                })
            })
            .collect();
        let relationship_hashes: Vec<String> = relationships
            .iter()
            .map(|relationship| {
                blake3::hash(
                    format!(
                        "{}:{}:{}:{}:{}",
                        relationship.kind,
                        relationship.source_symbol.as_deref().unwrap_or(""),
                        relationship.target,
                        relationship.start_byte,
                        relationship.end_byte
                    )
                    .as_bytes(),
                )
                .to_hex()
                .to_string()
            })
            .collect();
        let relationship_kinds: Vec<String> = relationships
            .iter()
            .map(|relationship| relationship.kind.to_string())
            .collect();
        let relationship_inputs: Vec<_> = relationships
            .iter()
            .zip(&relationship_metadata)
            .zip(&relationship_hashes)
            .zip(&relationship_kinds)
            .map(
                |(((relationship, metadata), content_hash), relation)| RelationshipSnapshotInput {
                    source_stable_key: relationship_source_key(relationship, symbols, &stable_keys),
                    dst_ref: &relationship.target,
                    relation,
                    confidence: match relationship.confidence {
                        crate::relationships::RelationshipConfidence::Extracted => "extracted",
                        crate::relationships::RelationshipConfidence::Ambiguous => "ambiguous",
                    },
                    start_line: relationship.start_line,
                    end_line: relationship.end_line,
                    start_byte: relationship.start_byte,
                    end_byte: relationship.end_byte,
                    content_hash,
                    metadata,
                },
            )
            .collect();
        let metadata = std::fs::metadata(path)?;
        let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let language_name = language.to_string();
        self.index.replace_code_snapshot(&CodeSnapshotInput {
            file: FileMetadataInput {
                project_id,
                project_root,
                path,
                language: Some(&language_name),
                extension: path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or(""),
                size: metadata.len(),
                mtime_secs,
                mtime_nanos,
                content_hash: Some(&hash),
                scan_id: None,
            },
            source,
            extractor: "tree-sitter/1",
            symbols: &symbol_inputs,
            relationships: &relationship_inputs,
        })?;
        Ok(())
    }

    /// Search symbols by name (exact, prefix, or contains).
    pub fn search(
        &self,
        query: &str,
        kind_filter: Option<&str>,
        file_pattern: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SymbolMatch>> {
        let mut sql = String::from(
            "SELECT s.name, s.symbol_kind, f.path, s.start_line, s.end_line, s.language, pr.title
             FROM symbols s
             JOIN files f ON s.file_resource_id = f.resource_id
             LEFT JOIN resources pr ON pr.id = s.parent_resource_id
             WHERE s.name LIKE ?1",
        );

        let name_pattern = format!("%{query}%");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(name_pattern)];

        if let Some(kind) = kind_filter {
            sql.push_str(" AND s.symbol_kind = ?");
            sql.push_str(&(param_values.len() + 1).to_string());
            param_values.push(Box::new(kind.to_string()));
        }

        if let Some(pattern) = file_pattern {
            sql.push_str(" AND f.path LIKE ?");
            sql.push_str(&(param_values.len() + 1).to_string());
            let file_like = format!("%{pattern}%");
            param_values.push(Box::new(file_like));
        }

        // Prioritize exact matches, then prefix, then contains
        sql.push_str(&format!(
            " ORDER BY
              CASE WHEN s.name = ?{} THEN 0
                   WHEN s.name LIKE ?{} THEN 1
                   ELSE 2
              END,
              s.name
             LIMIT ?{}",
            param_values.len() + 1,
            param_values.len() + 2,
            param_values.len() + 3,
        ));

        param_values.push(Box::new(query.to_string()));
        param_values.push(Box::new(format!("{query}%")));
        param_values.push(Box::new(limit as i64));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.index.connection().prepare(&sql)?;
        let results = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(SymbolMatch {
                name: row.get(0)?,
                kind: parse_symbol_kind(&row.get::<_, String>(1)?),
                file: PathBuf::from(row.get::<_, String>(2)?),
                start_line: row.get::<_, i64>(3)? as usize,
                end_line: row.get::<_, i64>(4)? as usize,
                language: row.get(5)?,
                parent: row.get(6)?,
            })
        })?;

        let mut matches = Vec::new();
        for result in results {
            matches.push(result?);
        }

        Ok(matches)
    }

    /// Get all symbols in a specific file.
    pub fn symbols_in_file(&self, path: &Path) -> Result<Vec<SymbolMatch>> {
        let path_str = path.to_string_lossy();

        let mut stmt = self.index.connection().prepare(
            "SELECT s.name, s.symbol_kind, f.path, s.start_line, s.end_line, s.language, pr.title
             FROM symbols s
             JOIN files f ON s.file_resource_id = f.resource_id
             LEFT JOIN resources pr ON pr.id = s.parent_resource_id
             WHERE f.path LIKE ?1
             ORDER BY s.start_line",
        )?;

        let pattern = format!("%{}", path_str);
        let results = stmt.query_map(params![pattern], |row| {
            Ok(SymbolMatch {
                name: row.get(0)?,
                kind: parse_symbol_kind(&row.get::<_, String>(1)?),
                file: PathBuf::from(row.get::<_, String>(2)?),
                start_line: row.get::<_, i64>(3)? as usize,
                end_line: row.get::<_, i64>(4)? as usize,
                language: row.get(5)?,
                parent: row.get(6)?,
            })
        })?;

        let mut matches = Vec::new();
        for result in results {
            matches.push(result?);
        }

        Ok(matches)
    }

    /// Get total counts.
    pub fn stats(&self) -> Result<(usize, usize)> {
        let file_count: i64 =
            self.index
                .connection()
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let symbol_count: i64 =
            self.index
                .connection()
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        Ok((file_count as usize, symbol_count as usize))
    }

    /// Locate graph resources by URI, title, or external identifier.
    pub fn find_graph_resources(
        &self,
        project_root: &Path,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ResourceMatch>> {
        let project_id = agent_core::project_ident(project_root);
        self.index
            .find_resources(&project_id, query, namespace, limit)
    }

    /// Traverse resolved and unresolved relationships from a resource.
    pub fn traverse_graph(
        &self,
        resource_id: i64,
        relation: Option<&str>,
        direction: &str,
        depth: usize,
        limit: usize,
    ) -> Result<Vec<TraversedEdge>> {
        self.index
            .traverse(resource_id, relation, direction, depth, limit)
    }

    /// Re-run cross-file resolution after external resources are indexed.
    pub fn resolve_graph(&mut self, project_root: &Path) -> Result<ResolutionStats> {
        let project_id = agent_core::project_ident(project_root);
        self.index.resolve_code_edges(&project_id, "tree-sitter/1")
    }
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn stable_symbol_keys(symbols: &[Symbol]) -> Vec<String> {
    let mut occurrences = BTreeMap::<String, usize>::new();
    symbols
        .iter()
        .map(|symbol| {
            let base = match symbol.parent.as_deref() {
                Some(parent) => format!("{parent}::{}:{}", symbol.name, symbol.kind),
                None => format!("{}:{}", symbol.name, symbol.kind),
            };
            let occurrence = occurrences.entry(base.clone()).or_default();
            let key = if *occurrence == 0 {
                base
            } else {
                format!("{base}~{occurrence}")
            };
            *occurrence += 1;
            key
        })
        .collect()
}

fn parent_stable_key<'a>(
    symbol: &Symbol,
    symbols: &'a [Symbol],
    stable_keys: &'a [String],
) -> Option<&'a str> {
    let parent = symbol.parent.as_deref()?;
    symbols
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.name == parent
                && candidate.start_line <= symbol.start_line
                && candidate.end_line >= symbol.end_line
        })
        .min_by_key(|(_, candidate)| candidate.end_line - candidate.start_line)
        .map(|(index, _)| stable_keys[index].as_str())
}

fn relationship_source_key<'a>(
    relationship: &ExtractedRelationship,
    symbols: &'a [Symbol],
    stable_keys: &'a [String],
) -> Option<&'a str> {
    let source = relationship.source_symbol.as_deref()?;
    symbols
        .iter()
        .enumerate()
        .filter(|(_, symbol)| {
            symbol.name == source
                && symbol.start_line <= relationship.start_line
                && symbol.end_line >= relationship.end_line
        })
        .min_by_key(|(_, symbol)| symbol.end_line - symbol.start_line)
        .map(|(index, _)| stable_keys[index].as_str())
}

fn parse_symbol_kind(s: &str) -> SymbolKind {
    match s {
        "fn" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "interface" => SymbolKind::Interface,
        "impl" => SymbolKind::Impl,
        "mod" => SymbolKind::Module,
        "namespace" => SymbolKind::Namespace,
        "macro" => SymbolKind::Macro,
        "type" => SymbolKind::Type,
        "const" => SymbolKind::Constant,
        "var" => SymbolKind::Variable,
        "prop" => SymbolKind::Property,
        _ => SymbolKind::Variable,
    }
}

#[derive(Debug, Default, Serialize)]
pub struct IndexStats {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_errored: usize,
    pub symbols_indexed: usize,
    pub edges_indexed: usize,
    pub edges_resolved: usize,
    pub edges_unresolved: usize,
    pub edges_ambiguous: usize,
    pub concepts_indexed: usize,
    pub concepts_removed: usize,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indexed {} files ({} symbols, {} edges; {} resolved, {} unresolved, {} ambiguous), {} concepts ({} removed), skipped {} unchanged, {} errors",
            self.files_indexed,
            self.symbols_indexed,
            self.edges_indexed,
            self.edges_resolved,
            self.edges_unresolved,
            self.edges_ambiguous,
            self.concepts_indexed,
            self.concepts_removed,
            self.files_skipped,
            self.files_errored
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_project(dir: &Path) {
        std::fs::write(
            dir.join("main.rs"),
            r#"
fn main() {
    println!("hello");
}

struct Config {
    name: String,
    value: i32,
}

impl Config {
    fn new(name: &str) -> Self {
        Config { name: name.to_string(), value: 0 }
    }
}
"#,
        )
        .unwrap();

        std::fs::write(
            dir.join("helper.py"),
            r#"
def process_data(items):
    return [x * 2 for x in items]

class DataProcessor:
    def __init__(self):
        self.data = []

    def run(self):
        pass
"#,
        )
        .unwrap();
    }

    #[test]
    fn synthesized_concepts_land_in_the_graph_as_derived_and_are_idempotent() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        std::fs::write(
            project_dir.path().join("lib.rs"),
            "/// Exported entry point.\npub fn run() {}\nfn hidden() {}\n",
        )
        .unwrap();

        let mut index = SymbolIndex::open(&db_dir.path().join("project.db")).unwrap();
        let first = index.build(project_dir.path()).unwrap();
        // One CodeModule for the file plus one CodeSymbol for the exported fn.
        assert_eq!(first.concepts_indexed, 2);

        let rows: Vec<(String, String, String, String)> = index
            .index
            .connection()
            .prepare(
                "SELECT external_id, kind, origin_kind, authority FROM resources
                 WHERE namespace = 'okf' ORDER BY external_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "code/lib.rs.md");
        assert_eq!(rows[0].1, "CodeModule");
        // Synthesized knowledge must never claim repository authority.
        for row in &rows {
            assert_eq!(row.2, "local-derived");
            assert_eq!(row.3, "derived");
        }
        assert!(rows[1].0.starts_with("code/lib.rs/"));
        assert_eq!(rows[1].1, "CodeSymbol");

        // Unchanged input is skipped and produces no new concepts or rows.
        let second = index.build(project_dir.path()).unwrap();
        assert_eq!(second.concepts_indexed, 0);
        assert_eq!(second.files_skipped, 1);
        let count: i64 = index
            .index
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM resources WHERE namespace = 'okf'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn deleting_a_file_prunes_only_its_own_synthesized_concepts() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        std::fs::write(project_dir.path().join("kept.rs"), "pub fn kept() {}\n").unwrap();
        let removed_path = project_dir.path().join("removed.rs");
        std::fs::write(&removed_path, "pub fn gone() {}\n").unwrap();

        let mut index = SymbolIndex::open(&db_dir.path().join("project.db")).unwrap();
        assert_eq!(index.build(project_dir.path()).unwrap().concepts_indexed, 4);

        std::fs::remove_file(&removed_path).unwrap();
        let after = index.build(project_dir.path()).unwrap();
        assert_eq!(after.concepts_removed, 2);

        let remaining: Vec<String> = index
            .index
            .connection()
            .prepare(
                "SELECT external_id FROM resources WHERE namespace = 'okf' ORDER BY external_id",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|id| id.starts_with("code/kept.rs")));
    }

    #[test]
    fn test_build_and_search() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        create_test_project(project_dir.path());

        let db_path = db_dir.path().join("symbols.db");
        let mut index = SymbolIndex::open(&db_path).unwrap();

        let stats = index.build(project_dir.path()).unwrap();
        assert!(stats.files_indexed >= 2);
        assert!(stats.symbols_indexed >= 4);

        // Search for 'main'
        let results = index.search("main", None, None, 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "main");

        // Search by kind
        let results = index.search("Config", Some("struct"), None, 10).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_incremental_update() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        create_test_project(project_dir.path());

        let db_path = db_dir.path().join("symbols.db");
        let mut index = SymbolIndex::open(&db_path).unwrap();

        // First build
        let stats1 = index.build(project_dir.path()).unwrap();
        assert!(stats1.files_indexed >= 2);

        // Second build without changes: should skip
        let stats2 = index.build(project_dir.path()).unwrap();
        assert_eq!(stats2.files_indexed, 0);
        assert!(stats2.files_skipped >= 2);
    }

    #[test]
    fn test_relationship_edges_replace_incrementally_in_project_database() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let source_path = project_dir.path().join("main.rs");
        std::fs::write(&source_path, "fn helper() {}\nfn main() { helper(); }\n").unwrap();
        let db_path = db_dir.path().join("project.db");
        let mut index = SymbolIndex::open(&db_path).unwrap();

        let first = index.build(project_dir.path()).unwrap();
        assert_eq!(first.files_indexed, 1);
        assert_eq!(first.edges_indexed, 1);
        let edge_count: i64 = index
            .index
            .connection()
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap();
        assert_eq!(edge_count, 1);

        std::fs::write(&source_path, "fn helper() {}\nfn main() {}\n").unwrap();
        let second = index.build(project_dir.path()).unwrap();
        assert_eq!(second.files_indexed, 1);
        assert_eq!(second.edges_indexed, 0);
        let edge_count: i64 = index
            .index
            .connection()
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap();
        // One row per (file, producer): the extractor and the concept
        // synthesizer each track the file, and re-indexing replaces rather than
        // accumulates.
        let producer_rows: Vec<(String, i64)> = index
            .index
            .connection()
            .prepare(
                "SELECT producer, COUNT(*) FROM producer_state GROUP BY producer ORDER BY producer",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(edge_count, 0);
        assert_eq!(
            producer_rows,
            vec![
                ("okf-synth/1".to_owned(), 1),
                ("tree-sitter/1".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn test_ephemeral_index_builds_in_memory() {
        let project_dir = TempDir::new().unwrap();
        create_test_project(project_dir.path());

        let mut index = SymbolIndex::open_ephemeral().unwrap();
        assert!(index.is_ephemeral());

        let stats = index.build(project_dir.path()).unwrap();
        assert!(stats.files_indexed >= 2);

        let results = index.search("main", None, None, 10).unwrap();
        assert!(!results.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_build_index_through_symlinked_root() {
        let project_dir = TempDir::new().unwrap();
        let link_dir = TempDir::new().unwrap();
        let linked_root = link_dir.path().join("project-link");

        create_test_project(project_dir.path());
        std::os::unix::fs::symlink(project_dir.path(), &linked_root).unwrap();

        let mut index = SymbolIndex::open_ephemeral().unwrap();
        let stats = index.build(&linked_root).unwrap();

        assert!(stats.files_indexed >= 2);
        assert!(stats.symbols_indexed > 0);
    }

    #[test]
    fn test_locked_persistent_index_does_not_fallback_to_ephemeral() {
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("symbols.db");
        let locker = Connection::open(&db_path).unwrap();
        locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let err =
            match SymbolIndex::open_persistent_or_ephemeral(&db_path, Duration::from_millis(0)) {
                Ok(_) => {
                    panic!("locked persistent index should not fall back to ephemeral storage")
                }
                Err(err) => err,
            };
        let message = format!("{err:#}");
        assert!(message.contains("busy or locked"), "{message}");
        assert!(message.contains("symbols.db"), "{message}");

        locker.execute_batch("ROLLBACK;").unwrap();
    }
}
