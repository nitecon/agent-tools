//! Lossless, non-executing Open Knowledge Format (OKF) Markdown codec.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeSet;

// Producers build `x-agent-tools` payloads in the codec's own YAML types, so
// they never have to pin a matching serde_yaml themselves.
pub use serde_yaml::{Mapping as OkfMapping, Value as OkfValue};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct OkfLimits {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_frontmatter_bytes: usize,
    pub max_links_per_concept: usize,
}

impl Default for OkfLimits {
    fn default() -> Self {
        Self {
            max_files: 10_000,
            max_file_bytes: 4 * 1024 * 1024,
            max_frontmatter_bytes: 256 * 1024,
            max_links_per_concept: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OkfBundle {
    pub version: String,
    pub concepts: Vec<OkfConcept>,
    pub diagnostics: Vec<OkfDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OkfConcept {
    /// Bundle-relative Markdown path. This is the portable OKF identity.
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub status: String,
    pub stale_after: Option<String>,
    pub body: String,
    /// Exact YAML between the delimiters, retained for audit and lossless storage.
    pub raw_frontmatter: String,
    /// Complete parsed frontmatter, including unknown and Attested Computation fields.
    pub metadata: Value,
    pub links: Vec<OkfLink>,
    pub diagnostics: Vec<OkfDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfLink {
    pub label: String,
    pub target: String,
    pub resolved_id: Option<String>,
    pub external: bool,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfDiagnostic {
    pub path: String,
    pub level: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfVerification {
    pub kind: String,
    pub actor: String,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OkfSource {
    pub id: Option<String>,
    pub resource: String,
    pub title: Option<String>,
}

/// Inputs for building a concept in memory instead of parsing one from disk.
#[derive(Debug, Clone, Default)]
pub struct ConceptSynthesis<'a> {
    /// Bundle-relative Markdown path; the portable OKF identity.
    pub id: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub status: &'a str,
    pub description: Option<&'a str>,
    pub tags: Vec<String>,
    pub body: String,
    /// Producer facts retained under the namespaced `x-agent-tools` key.
    pub extension: Mapping,
    /// Typed relationships stored beside `extension` under `relationships`,
    /// matching the layout written by [`OkfConcept::set_portable_edges`].
    pub relationships: Vec<OkfPortableEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OkfPortableEdge {
    pub relation: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(flatten)]
    pub extensions: Mapping,
}

impl OkfConcept {
    pub fn verifications(&self) -> Vec<OkfVerification> {
        let Some(verified) = self.metadata.get("verified").and_then(Value::as_mapping) else {
            return Vec::new();
        };
        let at = mapping_string(verified, "at");
        verified
            .iter()
            .filter_map(|(key, value)| {
                let kind = key.as_str()?;
                if kind == "at" {
                    return None;
                }
                Some(OkfVerification {
                    kind: kind.to_owned(),
                    actor: value.as_str().map(ToOwned::to_owned).unwrap_or_else(|| {
                        serde_yaml::to_string(value)
                            .unwrap_or_default()
                            .trim()
                            .to_owned()
                    }),
                    at: at.clone(),
                })
            })
            .collect()
    }

    pub fn sources(&self) -> Vec<OkfSource> {
        let mut sources: Vec<OkfSource> = self
            .metadata
            .get("sources")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .filter_map(|source| {
                let mapping = source.as_mapping()?;
                Some(OkfSource {
                    id: mapping_string(mapping, "id"),
                    resource: mapping_string(mapping, "resource")?,
                    title: mapping_string(mapping, "title"),
                })
            })
            .collect();
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "okf-0.1-citations")
        {
            sources.extend(legacy_citation_urls(&self.body).map(|resource| OkfSource {
                id: None,
                resource,
                title: None,
            }));
        }
        sources
    }

    pub fn generated(&self) -> (Option<String>, Option<String>) {
        let generated = self.metadata.get("generated").and_then(Value::as_mapping);
        let by = generated.and_then(|mapping| mapping_string(mapping, "by"));
        let at = generated
            .and_then(|mapping| mapping_string(mapping, "at"))
            .or_else(|| self.metadata.get("timestamp").and_then(yaml_scalar_string));
        (by, at)
    }

    /// Build a concept in memory, without a backing file.
    ///
    /// Producers that derive knowledge from an index (rather than reading an
    /// authored Markdown document) go through here so synthesized concepts are
    /// byte-identical to what `parse_bundle` would have produced for the same
    /// document. `raw_frontmatter` is the deterministic rendering of the
    /// metadata mapping, which keeps content hashing — and therefore
    /// incremental re-indexing — stable across runs.
    pub fn synthesize(input: ConceptSynthesis<'_>, limits: OkfLimits) -> Result<Self> {
        validate_relative_path(input.id)?;
        if !input.id.ends_with(".md") {
            bail!(
                "synthesized OKF identity must be a Markdown path: {}",
                input.id
            );
        }
        let mut mapping = Mapping::new();
        mapping.insert(
            Value::String("okf_version".to_owned()),
            Value::String("0.2".to_owned()),
        );
        mapping.insert(
            Value::String("type".to_owned()),
            Value::String(input.kind.to_owned()),
        );
        mapping.insert(
            Value::String("title".to_owned()),
            Value::String(input.title.to_owned()),
        );
        mapping.insert(
            Value::String("status".to_owned()),
            Value::String(input.status.to_owned()),
        );
        if let Some(description) = input.description {
            mapping.insert(
                Value::String("description".to_owned()),
                Value::String(description.to_owned()),
            );
        }
        if !input.tags.is_empty() {
            mapping.insert(
                Value::String("tags".to_owned()),
                Value::Sequence(
                    input
                        .tags
                        .iter()
                        .map(|tag| Value::String(tag.clone()))
                        .collect(),
                ),
            );
        }
        let mut extension = input.extension.clone();
        if !input.relationships.is_empty() {
            extension.insert(
                Value::String("relationships".to_owned()),
                serde_yaml::to_value(&input.relationships)?,
            );
        }
        if !extension.is_empty() {
            mapping.insert(
                Value::String("x-agent-tools".to_owned()),
                Value::Mapping(extension),
            );
        }
        let metadata = Value::Mapping(mapping);
        // Match `split_frontmatter`, which yields the YAML without its trailing
        // newline, so a synthesized concept and the same concept re-parsed from
        // an export hash identically.
        let rendered = deterministic_frontmatter(&metadata)?;
        let raw_frontmatter = rendered.strip_suffix('\n').unwrap_or(&rendered).to_owned();
        if raw_frontmatter.len() > limits.max_frontmatter_bytes {
            bail!(
                "synthesized OKF frontmatter exceeds byte limit: {}",
                input.id
            );
        }
        let links = extract_markdown_links(&input.body, limits.max_links_per_concept)?;
        Ok(Self {
            id: input.id.to_owned(),
            kind: input.kind.to_owned(),
            title: input.title.to_owned(),
            description: input.description.map(ToOwned::to_owned),
            tags: input.tags,
            status: input.status.to_owned(),
            stale_after: None,
            body: input.body,
            raw_frontmatter,
            metadata,
            links,
            diagnostics: Vec::new(),
        })
    }

    /// Store internal typed relationships in OKF's namespaced extension surface.
    pub fn set_portable_edges(&mut self, edges: &[OkfPortableEdge]) -> Result<()> {
        let mapping = self
            .metadata
            .as_mapping_mut()
            .context("OKF metadata must be a mapping")?;
        let extension_key = Value::String("x-agent-tools".to_owned());
        let extension = mapping
            .entry(extension_key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        let extension_mapping = extension
            .as_mapping_mut()
            .context("x-agent-tools must be a mapping")?;
        extension_mapping.insert(
            Value::String("relationships".to_owned()),
            serde_yaml::to_value(edges)?,
        );
        Ok(())
    }
}

pub fn parse_bundle(root: &Path, limits: OkfLimits) -> Result<OkfBundle> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve OKF bundle root {}", root.display()))?;
    if !canonical_root.is_dir() {
        bail!("OKF bundle root is not a directory: {}", root.display());
    }
    let mut paths = Vec::new();
    collect_markdown_paths(
        &canonical_root,
        &canonical_root,
        &mut paths,
        limits.max_files,
    )?;
    paths.sort();

    let mut concepts = Vec::with_capacity(paths.len());
    for path in paths {
        concepts.push(parse_concept(&canonical_root, &path, limits)?);
    }
    Ok(bundle_from_concepts(concepts))
}

/// Resolve intra-bundle links and assemble a bundle from concepts that are
/// already in memory.
///
/// Disk parsing and in-memory synthesis share this so a synthesized bundle
/// resolves, diagnoses, and versions exactly like a parsed one.
pub fn bundle_from_concepts(mut concepts: Vec<OkfConcept>) -> OkfBundle {
    let known_ids: BTreeSet<String> = concepts.iter().map(|item| item.id.clone()).collect();
    for concept in &mut concepts {
        for link in &mut concept.links {
            if link.external {
                continue;
            }
            match resolve_bundle_link(&concept.id, &link.target) {
                Ok(id) if known_ids.contains(&id) => link.resolved_id = Some(id),
                Ok(id) => concept.diagnostics.push(OkfDiagnostic {
                    path: concept.id.clone(),
                    level: "warning".to_owned(),
                    code: "broken-link".to_owned(),
                    message: format!("link target does not exist: {id}"),
                }),
                Err(error) => concept.diagnostics.push(OkfDiagnostic {
                    path: concept.id.clone(),
                    level: "error".to_owned(),
                    code: "unsafe-link".to_owned(),
                    message: error.to_string(),
                }),
            }
        }
    }
    let version = concepts
        .iter()
        .find_map(|concept| yaml_string(&concept.metadata, "okf_version"))
        .unwrap_or_else(|| "0.2".to_owned());
    let diagnostics = concepts
        .iter()
        .flat_map(|concept| concept.diagnostics.clone())
        .collect();
    OkfBundle {
        version,
        concepts,
        diagnostics,
    }
}

/// Render a concept as its canonical OKF Markdown document.
///
/// This is the single rendering path: `export_bundle` writes it to disk and
/// virtual-document reads serve it straight from the index, so a stored concept
/// and an exported file can never diverge.
pub fn render_concept(concept: &OkfConcept) -> Result<String> {
    let yaml = deterministic_frontmatter(&concept.metadata)?;
    Ok(format!("---\n{}---\n{}", yaml, concept.body))
}

/// Render the bundle's `index.md` projection from the concept set.
///
/// Generated on demand rather than stored, so it can never drift from the
/// concepts it lists.
pub fn render_bundle_index(bundle: &OkfBundle) -> String {
    let mut concepts: Vec<&OkfConcept> = bundle.concepts.iter().collect();
    concepts.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut output = format!(
        "---\nokf_version: \"{}\"\ntype: Index\ntitle: Knowledge Index\n---\n# Knowledge Index\n\n{} concepts.\n",
        bundle.version,
        concepts.len()
    );
    let mut current_kind = "";
    for concept in concepts {
        if concept.kind != current_kind {
            current_kind = &concept.kind;
            output.push_str(&format!("\n## {current_kind}\n\n"));
        }
        // Bundle-root-relative so the projection resolves from any location.
        output.push_str(&format!("- [{}](/{})\n", concept.title, concept.id));
    }
    output
}

pub fn export_bundle(bundle: &OkfBundle, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("create OKF destination {}", destination.display()))?;
    let canonical_destination = destination.canonicalize()?;
    let mut concepts: Vec<&OkfConcept> = bundle.concepts.iter().collect();
    concepts.sort_by(|left, right| left.id.cmp(&right.id));
    for concept in concepts {
        validate_relative_path(&concept.id)?;
        let target = canonical_destination.join(&concept.id);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
            let canonical_parent = parent.canonicalize()?;
            if !canonical_parent.starts_with(&canonical_destination) {
                bail!("OKF export path escapes destination: {}", concept.id);
            }
        }
        fs::write(&target, render_concept(concept)?)
            .with_context(|| format!("write OKF concept {}", target.display()))?;
    }
    Ok(())
}

pub fn graph_fingerprint(bundle: &OkfBundle) -> String {
    let mut concepts: Vec<_> = bundle
        .concepts
        .iter()
        .map(|concept| {
            let mut links: Vec<_> = concept
                .links
                .iter()
                .map(|link| (&link.target, &link.resolved_id))
                .collect();
            links.sort();
            (
                &concept.id,
                &concept.kind,
                &concept.title,
                &concept.description,
                &concept.tags,
                &concept.status,
                &concept.stale_after,
                &concept.body,
                deterministic_frontmatter(&concept.metadata).expect("valid OKF metadata"),
                links,
            )
        })
        .collect();
    concepts.sort_by(|left, right| left.0.cmp(right.0));
    let serialized = serde_json::to_vec(&concepts).expect("OKF graph tuple serializes");
    blake3::hash(&serialized).to_hex().to_string()
}

fn parse_concept(root: &Path, path: &Path, limits: OkfLimits) -> Result<OkfConcept> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolve OKF concept {}", path.display()))?;
    if !canonical.starts_with(root) {
        bail!("OKF concept symlink escapes bundle: {}", path.display());
    }
    let id = canonical
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");
    let bytes = fs::read(&canonical)?;
    if bytes.len() > limits.max_file_bytes {
        bail!("OKF concept exceeds byte limit: {id}");
    }
    let document =
        String::from_utf8(bytes).with_context(|| format!("OKF concept is not UTF-8: {id}"))?;
    parse_document(&id, &document, limits)
}

/// Parse one concept document that is already in memory.
///
/// Used when reading concepts back out of the index, where there is no file to
/// open but the same validation and diagnostics must apply.
pub fn parse_document(id: &str, document: &str, limits: OkfLimits) -> Result<OkfConcept> {
    let id = id.to_owned();
    validate_relative_path(&id)?;
    if document.len() > limits.max_file_bytes {
        bail!("OKF concept exceeds byte limit: {id}");
    }
    let (raw_frontmatter, body) = split_frontmatter(document)?;
    if raw_frontmatter.len() > limits.max_frontmatter_bytes {
        bail!("OKF frontmatter exceeds byte limit: {id}");
    }
    let metadata: Value = serde_yaml::from_str(raw_frontmatter)
        .with_context(|| format!("invalid OKF frontmatter: {id}"))?;
    let mapping = metadata
        .as_mapping()
        .with_context(|| format!("OKF frontmatter must be a mapping: {id}"))?;
    let kind = mapping_string(mapping, "type")
        .with_context(|| format!("OKF concept is missing required type: {id}"))?;
    let title = mapping_string(mapping, "title")
        .or_else(|| first_markdown_heading(body))
        .unwrap_or_else(|| id.clone());
    let mut diagnostics = Vec::new();
    if mapping.contains_key(Value::String("timestamp".to_owned())) {
        diagnostics.push(OkfDiagnostic {
            path: id.clone(),
            level: "info".to_owned(),
            code: "okf-0.1-timestamp".to_owned(),
            message: "normalized legacy timestamp as generated.at fallback".to_owned(),
        });
    }
    if body.lines().any(|line| line.trim() == "# Citations") {
        diagnostics.push(OkfDiagnostic {
            path: id.clone(),
            level: "info".to_owned(),
            code: "okf-0.1-citations".to_owned(),
            message: "retained legacy Citations section as provenance fallback".to_owned(),
        });
    }
    let links = extract_markdown_links(body, limits.max_links_per_concept)?;
    Ok(OkfConcept {
        id,
        kind,
        title,
        description: mapping_string(mapping, "description"),
        tags: mapping_strings(mapping, "tags"),
        status: mapping_string(mapping, "status").unwrap_or_else(|| "stable".to_owned()),
        stale_after: mapping_string(mapping, "stale_after"),
        body: body.to_owned(),
        raw_frontmatter: raw_frontmatter.to_owned(),
        metadata,
        links,
        diagnostics,
    })
}

fn collect_markdown_paths(
    root: &Path,
    current: &Path,
    output: &mut Vec<PathBuf>,
    max_files: usize,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            let resolved = path.canonicalize()?;
            if !resolved.starts_with(root) {
                bail!("OKF symlink escapes bundle: {}", path.display());
            }
        }
        if path.is_dir() {
            collect_markdown_paths(root, &path, output, max_files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            if output.len() >= max_files {
                bail!("OKF bundle exceeds file limit of {max_files}");
            }
            output.push(path);
        }
    }
    Ok(())
}

fn split_frontmatter(document: &str) -> Result<(&str, &str)> {
    let rest = document
        .strip_prefix("---\n")
        .or_else(|| document.strip_prefix("---\r\n"))
        .context("OKF concept must begin with YAML frontmatter")?;
    let marker = if rest.contains("\n---\n") {
        "\n---\n"
    } else {
        "\r\n---\r\n"
    };
    let (frontmatter, body) = rest
        .split_once(marker)
        .context("OKF frontmatter is not terminated")?;
    Ok((frontmatter, body))
}

fn deterministic_frontmatter(metadata: &Value) -> Result<String> {
    let mapping = metadata.as_mapping().context("OKF metadata mapping")?;
    let mut sorted = Mapping::new();
    let mut entries: Vec<_> = mapping.iter().collect();
    entries.sort_by_key(|(key, _)| serde_yaml::to_string(key).unwrap_or_default());
    for (key, value) in entries {
        sorted.insert(key.clone(), value.clone());
    }
    let mut yaml = serde_yaml::to_string(&Value::Mapping(sorted))?;
    if let Some(stripped) = yaml.strip_prefix("---\n") {
        yaml = stripped.to_owned();
    }
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Ok(yaml)
}

fn extract_markdown_links(body: &str, limit: usize) -> Result<Vec<OkfLink>> {
    let mut links = Vec::new();
    let mut fenced = false;
    for (index, line) in body.lines().enumerate() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut remaining = line;
        while let Some(label_start) = remaining.find('[') {
            let after_start = &remaining[label_start + 1..];
            let Some(label_end) = after_start.find("](") else {
                break;
            };
            let after_label = &after_start[label_end + 2..];
            let Some(target_end) = after_label.find(')') else {
                break;
            };
            let target = after_label[..target_end].trim().to_owned();
            if !target.is_empty() {
                if links.len() >= limit {
                    bail!("OKF concept exceeds link limit of {limit}");
                }
                links.push(OkfLink {
                    label: after_start[..label_end].to_owned(),
                    external: is_external_ref(&target),
                    target,
                    resolved_id: None,
                    line: index + 1,
                });
            }
            remaining = &after_label[target_end + 1..];
        }
    }
    Ok(links)
}

fn resolve_bundle_link(source_id: &str, target: &str) -> Result<String> {
    let target = target.split('#').next().unwrap_or(target);
    if target.is_empty() {
        return Ok(source_id.to_owned());
    }
    let joined = if let Some(root_relative) = target.strip_prefix('/') {
        PathBuf::from(root_relative)
    } else {
        Path::new(source_id)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target)
    };
    normalize_relative_path(&joined)
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    bail!("OKF path escapes bundle: {}", path.display());
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("OKF path is host-absolute: {}", path.display())
            }
        }
    }
    let normalized = parts.join("/");
    validate_relative_path(&normalized)?;
    Ok(normalized)
}

fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\0') || path.contains('\\') {
        bail!("invalid OKF relative path: {path:?}");
    }
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || parsed
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        bail!("unsafe OKF relative path: {path}");
    }
    Ok(())
}

fn is_external_ref(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("file:")
        || target.contains("://")
}

fn mapping_string(mapping: &Mapping, key: &str) -> Option<String> {
    mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn mapping_strings(mapping: &Mapping, key: &str) -> Vec<String> {
    mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_sequence)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn yaml_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .as_mapping()
        .and_then(|mapping| mapping_string(mapping, key))
}

fn first_markdown_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
}

fn yaml_scalar_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| match value {
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
}

fn legacy_citation_urls(body: &str) -> impl Iterator<Item = String> + '_ {
    let mut in_citations = false;
    body.lines().filter_map(move |line| {
        if line.trim() == "# Citations" {
            in_citations = true;
            return None;
        }
        if in_citations && line.starts_with("# ") {
            in_citations = false;
        }
        in_citations
            .then(|| line.trim().strip_prefix("- "))
            .flatten()
            .filter(|value| is_external_ref(value))
            .map(ToOwned::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../agent-cli/tests/fixtures/knowledge_graph")
    }

    #[test]
    fn v02_round_trip_is_deterministic_and_graph_equivalent() {
        let source = fixtures().join("okf/v02");
        let bundle = parse_bundle(&source, OkfLimits::default()).unwrap();
        assert_eq!(bundle.concepts.len(), 3);
        let service = bundle
            .concepts
            .iter()
            .find(|concept| concept.id == "services/service.md")
            .unwrap();
        assert_eq!(service.kind, "Service");
        assert_eq!(service.status, "stable");
        assert!(service.raw_frontmatter.contains("x-fixture-unknown"));
        assert_eq!(service.verifications()[0].actor, "reviewer@example.test");
        assert_eq!(service.sources()[0].id.as_deref(), Some("design"));
        assert_eq!(
            service.generated(),
            (
                Some("process:fixture-generator".to_owned()),
                Some("2026-08-15T00:00:00Z".to_owned())
            )
        );
        assert!(service
            .diagnostics
            .iter()
            .any(|item| item.code == "broken-link"));

        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        export_bundle(&bundle, first.path()).unwrap();
        let imported = parse_bundle(first.path(), OkfLimits::default()).unwrap();
        export_bundle(&imported, second.path()).unwrap();
        assert_eq!(graph_fingerprint(&bundle), graph_fingerprint(&imported));
        for concept in &bundle.concepts {
            assert_eq!(
                fs::read(first.path().join(&concept.id)).unwrap(),
                fs::read(second.path().join(&concept.id)).unwrap()
            );
        }
    }

    #[test]
    fn legacy_fallbacks_and_attested_metadata_are_retained_without_execution() {
        let legacy_root = fixtures().join("okf/v01");
        let legacy = parse_bundle(&legacy_root, OkfLimits::default()).unwrap();
        assert!(legacy.concepts[0]
            .diagnostics
            .iter()
            .any(|item| item.code == "okf-0.1-timestamp"));
        assert!(legacy.concepts[0]
            .diagnostics
            .iter()
            .any(|item| item.code == "okf-0.1-citations"));
        assert_eq!(
            legacy.concepts[0].sources()[0].resource,
            "https://example.test/legacy-source"
        );

        let hostile = TempDir::new().unwrap();
        fs::copy(
            fixtures().join("hostile/attested-computation.md"),
            hostile.path().join("attested.md"),
        )
        .unwrap();
        let bundle = parse_bundle(hostile.path(), OkfLimits::default()).unwrap();
        let metadata = bundle.concepts[0].metadata.as_mapping().unwrap();
        assert_eq!(
            mapping_string(metadata, "runtime").as_deref(),
            Some("shell")
        );
        assert!(bundle.concepts[0].body.contains("exit 99"));
    }

    #[test]
    fn invalid_structures_and_limits_fail_safely() {
        let hostile = TempDir::new().unwrap();
        fs::copy(
            fixtures().join("hostile/invalid-frontmatter.md"),
            hostile.path().join("invalid.md"),
        )
        .unwrap();
        assert!(parse_bundle(hostile.path(), OkfLimits::default()).is_err());

        let limits = OkfLimits {
            max_file_bytes: 8,
            ..OkfLimits::default()
        };
        assert!(parse_bundle(&fixtures().join("okf/v02"), limits).is_err());
        assert!(resolve_bundle_link("services/service.md", "../../../escape.md").is_err());
    }

    #[test]
    fn typed_edges_use_the_namespaced_extension() {
        let mut bundle = parse_bundle(&fixtures().join("okf/v02"), OkfLimits::default()).unwrap();
        let concept = bundle
            .concepts
            .iter_mut()
            .find(|concept| concept.id == "services/service.md")
            .unwrap();
        concept
            .set_portable_edges(&[OkfPortableEdge {
                relation: "documents".to_owned(),
                target: "services/runbook.md".to_owned(),
                confidence: Some("resolved".to_owned()),
                extensions: Mapping::new(),
            }])
            .unwrap();
        let serialized = deterministic_frontmatter(&concept.metadata).unwrap();
        assert!(serialized.contains("x-agent-tools:"));
        assert!(serialized.contains("relationships:"));
        assert!(serialized.contains("relation: documents"));
    }

    #[test]
    fn external_resources_and_html_are_retained_without_network_or_execution() {
        use std::io::ErrorKind;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let bundle_root = TempDir::new().unwrap();
        fs::write(
            bundle_root.path().join("hostile.md"),
            format!(
                "---\ntype: Note\ntitle: Hostile\n---\n# Hostile\n\n[external](http://{address}/must-not-fetch)\n<script>run()</script>\n"
            ),
        )
        .unwrap();
        let bundle = parse_bundle(bundle_root.path(), OkfLimits::default()).unwrap();
        assert!(bundle.concepts[0].body.contains("<script>run()</script>"));
        assert!(bundle.concepts[0].links[0].external);
        let error = listener.accept().expect_err("codec never opens the URL");
        assert_eq!(error.kind(), ErrorKind::WouldBlock);
    }

    fn synthesized_pair() -> Vec<OkfConcept> {
        let mut extension = Mapping::new();
        extension.insert(
            Value::String("language".to_owned()),
            Value::String("Rust".to_owned()),
        );
        let module = OkfConcept::synthesize(
            ConceptSynthesis {
                id: "code/src/lib.rs.md",
                kind: "CodeModule",
                title: "src/lib.rs",
                status: "stable",
                tags: vec!["rust".to_owned()],
                body: "# src/lib.rs\n\n- [run](/code/src/lib.rs/run.md)\n".to_owned(),
                extension: extension.clone(),
                relationships: vec![OkfPortableEdge {
                    relation: "contains".to_owned(),
                    target: "/code/src/lib.rs/run.md".to_owned(),
                    confidence: Some("resolved".to_owned()),
                    extensions: Mapping::new(),
                }],
                ..ConceptSynthesis::default()
            },
            OkfLimits::default(),
        )
        .unwrap();
        let symbol = OkfConcept::synthesize(
            ConceptSynthesis {
                id: "code/src/lib.rs/run.md",
                kind: "CodeSymbol",
                title: "run",
                status: "stable",
                body: "# run\n\n- [src/lib.rs](/code/src/lib.rs.md)\n".to_owned(),
                extension,
                ..ConceptSynthesis::default()
            },
            OkfLimits::default(),
        )
        .unwrap();
        vec![module, symbol]
    }

    #[test]
    fn synthesized_bundles_are_deterministic_and_survive_export_reparse() {
        let first = bundle_from_concepts(synthesized_pair());
        let second = bundle_from_concepts(synthesized_pair());
        assert_eq!(graph_fingerprint(&first), graph_fingerprint(&second));

        // Links resolve through the same path parsed bundles use.
        assert_eq!(
            first.concepts[0].links[0].resolved_id.as_deref(),
            Some("code/src/lib.rs/run.md")
        );
        assert!(first.diagnostics.is_empty());

        let destination = TempDir::new().unwrap();
        export_bundle(&first, destination.path()).unwrap();
        let reparsed = parse_bundle(destination.path(), OkfLimits::default()).unwrap();
        assert_eq!(graph_fingerprint(&first), graph_fingerprint(&reparsed));
    }

    #[test]
    fn synthesized_frontmatter_matches_its_rendered_document() {
        let concept = synthesized_pair().remove(0);
        let document = render_concept(&concept).unwrap();
        // raw_frontmatter is what index_okf_concept hashes; it must equal what a
        // reader of the rendered document would parse back.
        let (frontmatter, body) = split_frontmatter(&document).unwrap();
        assert_eq!(frontmatter, concept.raw_frontmatter);
        assert_eq!(body, concept.body);
        assert!(
            document.contains("okf_version: '0.2'") || document.contains("okf_version: \"0.2\"")
        );
        assert!(document.contains("relationships:"));
    }

    #[test]
    fn synthesis_rejects_identities_that_escape_the_bundle() {
        for id in ["../outside.md", "/abs.md", "code/plain.txt"] {
            assert!(OkfConcept::synthesize(
                ConceptSynthesis {
                    id,
                    kind: "CodeModule",
                    title: "x",
                    status: "stable",
                    body: "# x\n".to_owned(),
                    ..ConceptSynthesis::default()
                },
                OkfLimits::default(),
            )
            .is_err());
        }
    }

    #[test]
    fn bundle_index_projection_is_deterministic_and_grouped() {
        let bundle = bundle_from_concepts(synthesized_pair());
        let rendered = render_bundle_index(&bundle);
        assert_eq!(rendered, render_bundle_index(&bundle));
        let module_at = rendered.find("## CodeModule").unwrap();
        let symbol_at = rendered.find("## CodeSymbol").unwrap();
        assert!(module_at < symbol_at);
        assert!(rendered.contains("- [run](/code/src/lib.rs/run.md)"));
    }
}
