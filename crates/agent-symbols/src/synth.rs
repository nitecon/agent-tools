//! Synthesize OKF concepts from tree-sitter output.
//!
//! The database is the canonical store: concepts are built in memory from the
//! symbol index and written straight into the shared graph. Nothing is written
//! to the user's working tree — `okf export` materializes a bundle only when
//! somebody asks for one.
//!
//! Two concept grains are produced per file:
//!
//! * one `CodeModule` concept for the file, and
//! * one `CodeSymbol` concept per *exported* symbol.
//!
//! Call and import relationships travel in the namespaced `x-agent-tools`
//! extension as portable edges rather than as Markdown links, because their
//! targets are raw spellings that only the SQL edge resolver can bind. Markdown
//! links are reserved for the containment pairs that always resolve inside the
//! file's own concept set, which keeps synthesis diagnostic-free.

use crate::extractor::{Symbol, SymbolKind};
use crate::languages::Language;
use crate::relationships::{ExtractedRelationship, RelationshipConfidence};
use agent_knowledge::okf::{
    bundle_from_concepts, ConceptSynthesis, OkfConcept, OkfLimits, OkfMapping as Mapping,
    OkfPortableEdge, OkfValue as Value,
};
use anyhow::Result;

/// Producer identity for incremental gating via `producer_is_current`.
pub const SYNTH_PRODUCER: &str = "okf-synth/1";

/// Root prefix for every synthesized code identity.
const CODE_PREFIX: &str = "code";

/// Symbol kinds that never earn their own concept: structural or local noise.
fn is_conceptual(kind: SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::Impl | SymbolKind::Variable | SymbolKind::Property
    )
}

/// Everything one file contributes to the graph.
pub struct FileSynthesis<'a> {
    pub relative_path: &'a str,
    pub language: Language,
    pub source: &'a str,
    pub content_hash: &'a str,
    pub symbols: &'a [Symbol],
    pub stable_keys: &'a [String],
    pub relationships: &'a [ExtractedRelationship],
}

/// Concept identity for a file.
pub fn file_concept_id(relative_path: &str) -> String {
    format!("{CODE_PREFIX}/{relative_path}.md")
}

/// Concept identity for a symbol inside a file.
///
/// Stable keys carry `::` and `:` separators that make poor path segments, so
/// they are slugified and suffixed with a short digest of the original key —
/// deterministic, path-safe, and collision-free across distinct keys.
pub fn symbol_concept_id(relative_path: &str, stable_key: &str) -> String {
    let slug: String = stable_key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let digest = blake3::hash(stable_key.as_bytes()).to_hex();
    format!(
        "{CODE_PREFIX}/{relative_path}/{}-{}.md",
        slug.trim_matches('-'),
        &digest[..8]
    )
}

/// True when every concept for `relative_path` is covered by this identity.
fn owns_path(external_id: &str, relative_path: &str) -> bool {
    external_id == file_concept_id(relative_path)
        || external_id.starts_with(&format!("{CODE_PREFIX}/{relative_path}/"))
}

/// Partition stored identities into those still backed by an indexed file and
/// those whose file is gone.
pub fn retained_identities<'a>(
    stored: impl IntoIterator<Item = &'a String>,
    indexed_paths: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    stored
        .into_iter()
        .filter(|external_id| {
            indexed_paths
                .iter()
                .any(|path| owns_path(external_id, path))
        })
        .cloned()
        .collect()
}

/// Build the file concept and one concept per exported symbol.
pub fn synthesize_file(input: &FileSynthesis<'_>, limits: OkfLimits) -> Result<Vec<OkfConcept>> {
    let language = input.language.to_string();
    let indexed: Vec<(usize, &Symbol, &String)> = input
        .symbols
        .iter()
        .zip(input.stable_keys)
        .enumerate()
        .map(|(position, (symbol, key))| (position, symbol, key))
        .collect();

    let exported: Vec<&(usize, &Symbol, &String)> = indexed
        .iter()
        .filter(|(_, symbol, key)| {
            is_conceptual(symbol.kind) && !is_test_symbol(key) && is_exported(symbol, input)
        })
        .collect();

    let mut concepts = Vec::with_capacity(exported.len() + 1);
    concepts.push(synthesize_module(
        input, &language, &indexed, &exported, limits,
    )?);
    for (_, symbol, stable_key) in &exported {
        concepts.push(synthesize_symbol(
            input, &language, symbol, stable_key, limits,
        )?);
    }
    // Resolve containment links through the same path parsed bundles use; a
    // file's concepts are self-contained, so this never leaves a broken link.
    Ok(bundle_from_concepts(concepts).concepts)
}

fn synthesize_module(
    input: &FileSynthesis<'_>,
    language: &str,
    indexed: &[(usize, &Symbol, &String)],
    exported: &[&(usize, &Symbol, &String)],
    limits: OkfLimits,
) -> Result<OkfConcept> {
    let mut body = format!("# {}\n\n", input.relative_path);
    if let Some(header) = file_header_doc(input.source) {
        body.push_str(&header);
        body.push_str("\n\n");
    }
    body.push_str(&format!(
        "`{}` — {language}, {} symbols, {} exported.\n",
        input.relative_path,
        indexed.len(),
        exported.len()
    ));

    if !exported.is_empty() {
        body.push_str("\n## Exported symbols\n\n");
        for (_, symbol, stable_key) in exported {
            body.push_str(&format!(
                "- [{}](/{}) — {}, lines {}-{}\n",
                symbol.name,
                symbol_concept_id(input.relative_path, stable_key),
                symbol.kind,
                symbol.start_line,
                symbol.end_line
            ));
        }
    }

    let internal: Vec<&(usize, &Symbol, &String)> = indexed
        .iter()
        .filter(|(position, _, _)| {
            !exported
                .iter()
                .any(|(exported_position, _, _)| exported_position == position)
        })
        .collect();
    if !internal.is_empty() {
        body.push_str("\n## Internal symbols\n\n");
        for (_, symbol, _) in &internal {
            body.push_str(&format!(
                "- `{}` — {}, lines {}-{}\n",
                symbol.name, symbol.kind, symbol.start_line, symbol.end_line
            ));
        }
    }

    let mut relationships: Vec<OkfPortableEdge> = exported
        .iter()
        .map(|(_, _, stable_key)| OkfPortableEdge {
            relation: "contains".to_owned(),
            target: format!("/{}", symbol_concept_id(input.relative_path, stable_key)),
            confidence: Some("resolved".to_owned()),
            extensions: Mapping::new(),
        })
        .collect();
    relationships.extend(file_level_relationships(input));

    OkfConcept::synthesize(
        ConceptSynthesis {
            id: &file_concept_id(input.relative_path),
            kind: "CodeModule",
            title: input.relative_path,
            status: "stable",
            tags: vec![language.to_lowercase()],
            description: None,
            body,
            extension: module_extension(input, language, indexed.len(), exported.len()),
            relationships,
        },
        limits,
    )
}

fn synthesize_symbol(
    input: &FileSynthesis<'_>,
    language: &str,
    symbol: &Symbol,
    stable_key: &str,
    limits: OkfLimits,
) -> Result<OkfConcept> {
    let mut body = format!("# {}\n\n", symbol.name);
    body.push_str(&format!(
        "{} defined in [{}](/{}) at lines {}-{} ({language}).\n",
        symbol.kind,
        input.relative_path,
        file_concept_id(input.relative_path),
        symbol.start_line,
        symbol.end_line
    ));
    if let Some(documentation) = doc_comment_above(input.source, symbol.start_byte) {
        body.push_str(&format!("\n{documentation}\n"));
    }
    if let Some(signature) = signature(input.source, symbol) {
        body.push_str(&format!(
            "\n```{}\n{signature}\n```\n",
            language.to_lowercase()
        ));
    }

    let outgoing: Vec<&ExtractedRelationship> = input
        .relationships
        .iter()
        .filter(|relationship| {
            relationship.source_symbol.as_deref() == Some(symbol.name.as_str())
                && relationship.start_line >= symbol.start_line
                && relationship.end_line <= symbol.end_line
        })
        .collect();
    if !outgoing.is_empty() {
        body.push_str("\n## Relationships\n\n");
        for relationship in &outgoing {
            body.push_str(&format!(
                "- {} `{}` (line {})\n",
                relationship.kind, relationship.target, relationship.start_line
            ));
        }
    }

    let mut relationships = vec![OkfPortableEdge {
        relation: "defined_in".to_owned(),
        target: format!("/{}", file_concept_id(input.relative_path)),
        confidence: Some("resolved".to_owned()),
        extensions: Mapping::new(),
    }];
    relationships.extend(outgoing.iter().map(|relationship| OkfPortableEdge {
        relation: relationship.kind.to_string(),
        target: relationship.target.clone(),
        confidence: Some(confidence_label(relationship.confidence).to_owned()),
        extensions: Mapping::new(),
    }));

    let mut extension = base_extension(input, language);
    extension.insert(
        Value::String("symbol_kind".to_owned()),
        Value::String(symbol.kind.to_string()),
    );
    extension.insert(
        Value::String("stable_key".to_owned()),
        Value::String(stable_key.to_owned()),
    );
    extension.insert(
        Value::String("start_line".to_owned()),
        Value::Number(symbol.start_line.into()),
    );
    extension.insert(
        Value::String("end_line".to_owned()),
        Value::Number(symbol.end_line.into()),
    );

    OkfConcept::synthesize(
        ConceptSynthesis {
            id: &symbol_concept_id(input.relative_path, stable_key),
            kind: "CodeSymbol",
            title: &symbol.name,
            status: "stable",
            tags: vec![language.to_lowercase(), symbol.kind.to_string()],
            description: None,
            body,
            extension,
            relationships,
        },
        limits,
    )
}

fn base_extension(input: &FileSynthesis<'_>, language: &str) -> Mapping {
    let mut extension = Mapping::new();
    extension.insert(
        Value::String("extractor".to_owned()),
        Value::String(SYNTH_PRODUCER.to_owned()),
    );
    extension.insert(
        Value::String("language".to_owned()),
        Value::String(language.to_owned()),
    );
    extension.insert(
        Value::String("path".to_owned()),
        Value::String(input.relative_path.to_owned()),
    );
    extension.insert(
        Value::String("content_hash".to_owned()),
        Value::String(input.content_hash.to_owned()),
    );
    extension
}

fn module_extension(
    input: &FileSynthesis<'_>,
    language: &str,
    symbols: usize,
    exported: usize,
) -> Mapping {
    let mut extension = base_extension(input, language);
    extension.insert(
        Value::String("symbol_count".to_owned()),
        Value::Number(symbols.into()),
    );
    extension.insert(
        Value::String("exported_count".to_owned()),
        Value::Number(exported.into()),
    );
    extension
}

/// File-scoped relationships (imports and the like) that no symbol owns.
fn file_level_relationships(input: &FileSynthesis<'_>) -> Vec<OkfPortableEdge> {
    let mut edges: Vec<OkfPortableEdge> = input
        .relationships
        .iter()
        .filter(|relationship| relationship.source_symbol.is_none())
        .map(|relationship| OkfPortableEdge {
            relation: relationship.kind.to_string(),
            target: relationship.target.clone(),
            confidence: Some(confidence_label(relationship.confidence).to_owned()),
            extensions: Mapping::new(),
        })
        .collect();
    edges.sort_by(|left, right| {
        left.relation
            .cmp(&right.relation)
            .then_with(|| left.target.cmp(&right.target))
    });
    edges.dedup_by(|left, right| left.relation == right.relation && left.target == right.target);
    edges
}

fn confidence_label(confidence: RelationshipConfidence) -> &'static str {
    match confidence {
        RelationshipConfidence::Extracted => "extracted",
        RelationshipConfidence::Ambiguous => "ambiguous",
    }
}

/// Test scaffolding is indexed for navigation but is not project knowledge.
fn is_test_symbol(stable_key: &str) -> bool {
    stable_key.starts_with("tests::")
        || stable_key.starts_with("test::")
        || stable_key.contains("::tests::")
}

/// Visibility is not on `Symbol`, so it is read back off the declaration.
fn is_exported(symbol: &Symbol, input: &FileSynthesis<'_>) -> bool {
    let declaration = declaration_line(input.source, symbol.start_byte);
    match input.language {
        Language::Rust => declaration.starts_with("pub ") || declaration.starts_with("pub("),
        Language::Go => symbol
            .name
            .chars()
            .next()
            .is_some_and(|first| first.is_uppercase()),
        Language::Python => !symbol.name.starts_with('_') && symbol.parent.is_none(),
        Language::TypeScript | Language::JavaScript => {
            declaration.starts_with("export") || declaration.contains(" export ")
        }
        Language::CSharp => declaration.contains("public"),
        // C and C++ headers have no in-band visibility marker; treat top-level
        // declarations as the interface and nested ones as internal.
        Language::Cpp => symbol.parent.is_none(),
    }
}

/// The declaration's own line, with any leading attribute lines skipped.
fn declaration_line(source: &str, start_byte: usize) -> &str {
    let slice = source.get(start_byte..).unwrap_or("");
    slice
        .lines()
        .find(|line| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//")
        })
        .map(str::trim_start)
        .unwrap_or("")
}

/// Contiguous comment block immediately above a declaration.
fn doc_comment_above(source: &str, start_byte: usize) -> Option<String> {
    let preceding = source.get(..start_byte)?;
    let mut lines: Vec<&str> = Vec::new();
    for line in preceding.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() && lines.is_empty() {
            continue;
        }
        let content = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"))
            .or_else(|| trimmed.strip_prefix("//"))
            .or_else(|| trimmed.strip_prefix("#"))
            .or_else(|| trimmed.strip_prefix("*"));
        match content {
            Some(text) => lines.push(text.trim()),
            None => break,
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    let text = lines.join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// The declaration head, up to the line that opens or ends it.
fn signature(source: &str, symbol: &Symbol) -> Option<String> {
    let slice = source.get(symbol.start_byte..symbol.end_byte)?;
    let mut collected = Vec::new();
    for line in slice.lines().take(5) {
        collected.push(line.trim_end());
        let trimmed = line.trim_end();
        if trimmed.ends_with('{') || trimmed.ends_with(':') || trimmed.ends_with(';') {
            break;
        }
    }
    let signature = collected.join("\n");
    (!signature.trim().is_empty()).then_some(signature)
}

/// Leading file-level comment block (`//!`, `"""`, banner comments).
fn file_header_doc(source: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() && lines.is_empty() {
            continue;
        }
        match trimmed
            .strip_prefix("//!")
            .or_else(|| trimmed.strip_prefix("///"))
        {
            Some(text) => lines.push(text.trim()),
            None => break,
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn rust_fixture() -> (String, Vec<Symbol>, Vec<String>) {
        let source = "//! Module header.\n\n/// Runs the thing.\npub fn run(value: u32) -> u32 {\n    helper(value)\n}\n\nfn helper(value: u32) -> u32 {\n    value\n}\n";
        let run_start = source.find("pub fn run").unwrap();
        let helper_start = source.find("fn helper").unwrap();
        let symbols = vec![
            Symbol {
                name: "run".to_owned(),
                kind: SymbolKind::Function,
                file: "src/lib.rs".into(),
                start_line: 4,
                end_line: 6,
                start_byte: run_start,
                end_byte: source.find("\n\nfn helper").unwrap(),
                language: Language::Rust,
                parent: None,
            },
            Symbol {
                name: "helper".to_owned(),
                kind: SymbolKind::Function,
                file: "src/lib.rs".into(),
                start_line: 8,
                end_line: 10,
                start_byte: helper_start,
                end_byte: source.len(),
                language: Language::Rust,
                parent: None,
            },
        ];
        let keys = vec!["run:fn".to_owned(), "helper:fn".to_owned()];
        (source.to_owned(), symbols, keys)
    }

    fn synthesize(source: &str, symbols: &[Symbol], keys: &[String]) -> Vec<OkfConcept> {
        synthesize_file(
            &FileSynthesis {
                relative_path: "src/lib.rs",
                language: Language::Rust,
                source,
                content_hash: "hash",
                symbols,
                stable_keys: keys,
                relationships: &[],
            },
            OkfLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn only_exported_symbols_earn_their_own_concept() {
        let (source, symbols, keys) = rust_fixture();
        let concepts = synthesize(&source, &symbols, &keys);
        assert_eq!(concepts.len(), 2, "module plus one exported symbol");
        assert_eq!(concepts[0].kind, "CodeModule");
        assert_eq!(concepts[1].kind, "CodeSymbol");
        assert_eq!(concepts[1].title, "run");
        // The private symbol is still listed for navigation, without a concept.
        assert!(concepts[0].body.contains("- `helper`"));
        assert!(concepts[0].body.contains("Module header."));
        assert!(concepts[1].body.contains("Runs the thing."));
        assert!(concepts[1].body.contains("pub fn run(value: u32) -> u32 {"));
    }

    #[test]
    fn containment_links_resolve_and_produce_no_diagnostics() {
        let (source, symbols, keys) = rust_fixture();
        let concepts = synthesize(&source, &symbols, &keys);
        let module = &concepts[0];
        assert!(module.diagnostics.is_empty());
        assert_eq!(
            module.links[0].resolved_id.as_deref(),
            Some(symbol_concept_id("src/lib.rs", "run:fn").as_str())
        );
        assert_eq!(
            concepts[1].links[0].resolved_id.as_deref(),
            Some(file_concept_id("src/lib.rs").as_str())
        );
    }

    #[test]
    fn synthesis_is_byte_stable_across_runs() {
        let (source, symbols, keys) = rust_fixture();
        let first = synthesize(&source, &symbols, &keys);
        let second = synthesize(&source, &symbols, &keys);
        assert_eq!(first, second);
    }

    #[test]
    fn identities_are_path_safe_and_collision_free() {
        let first = symbol_concept_id("src/lib.rs", "Foo::bar:method");
        let second = symbol_concept_id("src/lib.rs", "Foo-bar-method");
        assert_ne!(first, second, "slug collisions are broken by the digest");
        assert!(first.starts_with("code/src/lib.rs/"));
        assert!(first.ends_with(".md"));
        assert!(!first.contains(':'));
    }

    #[test]
    fn retained_identities_drop_only_concepts_whose_file_is_gone() {
        let stored = [
            file_concept_id("src/lib.rs"),
            symbol_concept_id("src/lib.rs", "run:fn"),
            file_concept_id("src/gone.rs"),
            symbol_concept_id("src/gone.rs", "run:fn"),
        ];
        let indexed = BTreeSet::from(["src/lib.rs".to_owned()]);
        let retained = retained_identities(stored.iter(), &indexed);
        assert_eq!(retained.len(), 2);
        assert!(retained.contains(&file_concept_id("src/lib.rs")));
        assert!(!retained.contains(&file_concept_id("src/gone.rs")));
    }

    #[test]
    fn visibility_rules_follow_the_language() {
        let go_source = "func Exported() {}\nfunc internal() {}\n";
        let symbols = vec![
            Symbol {
                name: "Exported".to_owned(),
                kind: SymbolKind::Function,
                file: "main.go".into(),
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                end_byte: 18,
                language: Language::Go,
                parent: None,
            },
            Symbol {
                name: "internal".to_owned(),
                kind: SymbolKind::Function,
                file: "main.go".into(),
                start_line: 2,
                end_line: 2,
                start_byte: 19,
                end_byte: go_source.len(),
                language: Language::Go,
                parent: None,
            },
        ];
        let keys = vec!["Exported:fn".to_owned(), "internal:fn".to_owned()];
        let concepts = synthesize_file(
            &FileSynthesis {
                relative_path: "main.go",
                language: Language::Go,
                source: go_source,
                content_hash: "hash",
                symbols: &symbols,
                stable_keys: &keys,
                relationships: &[],
            },
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(concepts.len(), 2);
        assert_eq!(concepts[1].title, "Exported");
    }
}
