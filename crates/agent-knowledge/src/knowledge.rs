//! Markdown and OKF producers for the shared project knowledge graph.

use crate::okf::{parse_bundle, OkfConcept, OkfLimits};
use crate::{
    ContentSegmentInput, EdgeInput, ProjectIndex, ProvenanceInput, ResourceInput,
    ResourceVersionInput, VerificationInput,
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

const MARKDOWN_EXTRACTOR: &str = "markdown/1";
const OKF_EXTRACTOR: &str = "okf/0.2";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnowledgeIndexStats {
    pub resources_seen: usize,
    pub resources_indexed: usize,
    pub resources_unchanged: usize,
    pub resources_removed: usize,
    pub segments_indexed: usize,
    pub edges_indexed: usize,
    pub unresolved_edges: usize,
    pub diagnostics: usize,
}

#[derive(Debug, Clone)]
struct MarkdownSegment {
    title: String,
    heading_path: String,
    text: String,
    start_line: usize,
    end_line: usize,
    start_byte: usize,
    end_byte: usize,
}

pub fn index_okf_bundle(
    index: &mut ProjectIndex,
    project_id: &str,
    project_root: &Path,
    bundle_root: &Path,
    limits: OkfLimits,
) -> Result<KnowledgeIndexStats> {
    let bundle = parse_bundle(bundle_root, limits)?;
    let origin_id = repository_relative(project_root, bundle_root)?;
    let mut stats = KnowledgeIndexStats {
        resources_seen: bundle.concepts.len(),
        diagnostics: bundle.diagnostics.len(),
        ..KnowledgeIndexStats::default()
    };
    index_concepts(
        index,
        project_id,
        &origin_id,
        "repository",
        "repository",
        &bundle.concepts,
        &mut stats,
    )?;
    let retained: BTreeSet<String> = bundle
        .concepts
        .iter()
        .map(|concept| concept.id.clone())
        .collect();
    stats.resources_removed =
        index.prune_origin_resources(project_id, "okf", &origin_id, &retained)?;
    Ok(stats)
}

/// Origin id for concepts synthesized from the local code index.
pub const SYNTH_ORIGIN: &str = "okf-synth";

/// Index concepts that were synthesized in memory rather than authored in the
/// repository.
///
/// These are labelled `local-derived`/`derived` so retrieval can rank them
/// below anything the repository or a gateway actually asserts. Pruning is left
/// to the caller: incremental producers skip unchanged inputs and must
/// reconcile the retained set across a whole run, not one call.
pub fn index_synthesized_concepts(
    index: &mut ProjectIndex,
    project_id: &str,
    concepts: &[OkfConcept],
) -> Result<KnowledgeIndexStats> {
    let mut stats = KnowledgeIndexStats {
        resources_seen: concepts.len(),
        diagnostics: concepts
            .iter()
            .map(|concept| concept.diagnostics.len())
            .sum(),
        ..KnowledgeIndexStats::default()
    };
    index_concepts(
        index,
        project_id,
        SYNTH_ORIGIN,
        "local-derived",
        "derived",
        concepts,
        &mut stats,
    )?;
    Ok(stats)
}

/// Origin id for concepts recorded from agent activity.
pub const OBSERVE_ORIGIN: &str = "okf-observe";

/// How many observations are kept before the oldest are pruned.
pub const OBSERVATION_LIMIT: usize = 200;

/// Index a concept that records what an agent did, linking it to the resources
/// the work touched.
///
/// Links are resolved against the whole `okf` namespace rather than a bundle,
/// because an observation points at concepts owned by other origins (the
/// synthesized code concepts it read). The result is capped: observations
/// accumulate with use and must not grow without bound.
pub fn index_observation(
    index: &mut ProjectIndex,
    project_id: &str,
    concept: &OkfConcept,
) -> Result<KnowledgeIndexStats> {
    let mut stats = KnowledgeIndexStats {
        resources_seen: 1,
        diagnostics: concept.diagnostics.len(),
        ..KnowledgeIndexStats::default()
    };
    // Bind links to whatever the index already holds. `bundle_from_concepts`
    // only resolves within a concept set, which an observation is not part of,
    // so resolution happens against stored identities instead.
    let mut concept = concept.clone();
    let mut seeded = BTreeMap::new();
    for link in &mut concept.links {
        if link.external {
            continue;
        }
        let target = link.target.trim_start_matches('/').to_owned();
        if let Some(id) = index.resource_id_by_external_id(project_id, "okf", &target)? {
            seeded.insert(target.clone(), id);
            link.resolved_id = Some(target);
        }
    }
    index_concepts_with(
        index,
        project_id,
        OBSERVE_ORIGIN,
        "local-derived",
        "derived",
        std::slice::from_ref(&concept),
        seeded,
        &mut stats,
    )?;
    stats.resources_removed =
        index.prune_origin_to_recent(project_id, "okf", OBSERVE_ORIGIN, OBSERVATION_LIMIT)?;
    Ok(stats)
}

fn index_concepts(
    index: &mut ProjectIndex,
    project_id: &str,
    origin_id: &str,
    origin_kind: &str,
    authority: &str,
    concepts: &[OkfConcept],
    stats: &mut KnowledgeIndexStats,
) -> Result<()> {
    index_concepts_with(
        index,
        project_id,
        origin_id,
        origin_kind,
        authority,
        concepts,
        BTreeMap::new(),
        stats,
    )
}

#[allow(clippy::too_many_arguments)]
fn index_concepts_with(
    index: &mut ProjectIndex,
    project_id: &str,
    origin_id: &str,
    origin_kind: &str,
    authority: &str,
    concepts: &[OkfConcept],
    seeded: BTreeMap<String, i64>,
    stats: &mut KnowledgeIndexStats,
) -> Result<()> {
    let mut resource_ids = seeded;
    for concept in concepts {
        let uri = okf_uri(project_id, origin_id, &concept.id);
        let metadata = yaml_to_json(&concept.metadata)?;
        let id = index.ensure_resource(&ResourceInput {
            project_id,
            namespace: "okf",
            external_id: &concept.id,
            canonical_uri: &uri,
            kind: &concept.kind,
            title: &concept.title,
            description: concept.description.as_deref(),
            origin_kind,
            origin_id,
            authority,
            status: Some(&concept.status),
            stale_after: concept.stale_after.as_deref(),
            metadata: &metadata,
        })?;
        resource_ids.insert(concept.id.clone(), id);
    }

    for concept in concepts {
        index_okf_concept(index, project_id, origin_id, concept, &resource_ids, stats)?;
    }
    Ok(())
}

pub fn index_markdown_file(
    index: &mut ProjectIndex,
    project_id: &str,
    project_root: &Path,
    path: &Path,
) -> Result<KnowledgeIndexStats> {
    let relative = repository_relative(project_root, path)?;
    let body = fs::read_to_string(path)
        .with_context(|| format!("read Markdown document {}", path.display()))?;
    let title = first_heading(&body).unwrap_or_else(|| relative.clone());
    let uri = format!("doc://{project_id}/{relative}");
    let metadata = json!({"path": relative});
    let resource_id = index.ensure_resource(&ResourceInput {
        project_id,
        namespace: "docs",
        external_id: &relative,
        canonical_uri: &uri,
        kind: "markdown",
        title: &title,
        description: None,
        origin_kind: "repository",
        origin_id: project_id,
        authority: "repository",
        status: None,
        stale_after: None,
        metadata: &metadata,
    })?;
    let hash = blake3::hash(body.as_bytes()).to_hex().to_string();
    let unchanged = index.current_content_hash(resource_id)?.as_deref() == Some(&hash);
    let version_id = index.put_version(&ResourceVersionInput {
        resource_id,
        revision: &hash,
        source_format: "markdown",
        media_type: Some("text/markdown"),
        body: Some(&body),
        raw_metadata: None,
        content_hash: &hash,
        generated_by: None,
        generated_at: None,
    })?;
    let segments = segment_markdown(&body, &title);
    replace_segments(index, version_id, &segments)?;
    let links = extract_links(&body);
    let mut edge_metadata = Vec::new();
    let mut edge_hashes = Vec::new();
    let mut destinations = Vec::new();
    let mut edges = Vec::new();
    for link in &links {
        edge_metadata.push(json!({"label": link.0}));
        edge_hashes.push(
            blake3::hash(format!("{}:{}", link.1, link.2).as_bytes())
                .to_hex()
                .to_string(),
        );
        destinations.push(
            if link.1.starts_with("http://") || link.1.starts_with("https://") {
                Some(ensure_external_resource(index, project_id, &link.1)?)
            } else {
                None
            },
        );
    }
    for (((link, metadata), edge_hash), destination) in links
        .iter()
        .zip(&edge_metadata)
        .zip(&edge_hashes)
        .zip(&destinations)
    {
        edges.push(EdgeInput {
            src_resource_id: resource_id,
            dst_resource_id: *destination,
            dst_ref: Some(&link.1),
            relation: if link.1.starts_with("http") {
                "cites"
            } else {
                "links_to"
            },
            confidence: if destination.is_some() {
                "resolved"
            } else {
                "extracted"
            },
            extractor: MARKDOWN_EXTRACTOR,
            source_resource_id: resource_id,
            start_line: Some(link.2),
            end_line: Some(link.2),
            start_byte: None,
            end_byte: None,
            content_hash: edge_hash,
            metadata,
        });
    }
    index.replace_edges_for_source(version_id, MARKDOWN_EXTRACTOR, &edges)?;
    Ok(KnowledgeIndexStats {
        resources_seen: 1,
        resources_indexed: usize::from(!unchanged),
        resources_unchanged: usize::from(unchanged),
        segments_indexed: segments.len(),
        edges_indexed: edges.len(),
        unresolved_edges: edges
            .iter()
            .filter(|edge| edge.dst_resource_id.is_none())
            .count(),
        ..KnowledgeIndexStats::default()
    })
}

fn index_okf_concept(
    index: &mut ProjectIndex,
    project_id: &str,
    origin_id: &str,
    concept: &OkfConcept,
    resource_ids: &BTreeMap<String, i64>,
    stats: &mut KnowledgeIndexStats,
) -> Result<()> {
    let resource_id = resource_ids[&concept.id];
    let content_hash =
        blake3::hash(format!("{}\0{}", concept.raw_frontmatter, concept.body).as_bytes())
            .to_hex()
            .to_string();
    let unchanged = index.current_content_hash(resource_id)?.as_deref() == Some(&content_hash);
    let (generated_by, generated_at) = concept.generated();
    let source_format = if concept
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code.starts_with("okf-0.1"))
    {
        "okf/0.1"
    } else {
        "okf/0.2"
    };
    let version_id = index.put_version(&ResourceVersionInput {
        resource_id,
        revision: &content_hash,
        source_format,
        media_type: Some("text/markdown"),
        body: Some(&concept.body),
        raw_metadata: Some(&concept.raw_frontmatter),
        content_hash: &content_hash,
        generated_by: generated_by.as_deref(),
        generated_at: generated_at.as_deref(),
    })?;
    let segments = segment_markdown(&concept.body, &concept.title);
    replace_segments(index, version_id, &segments)?;
    for tag in &concept.tags {
        index.add_tag(resource_id, tag)?;
    }
    for source in concept.sources() {
        index.add_provenance(&ProvenanceInput {
            resource_version_id: version_id,
            source_resource_id: None,
            source_ref: &source.resource,
            author: None,
            usage_count: None,
            last_modified: None,
            metadata: &json!({"id": source.id, "title": source.title}),
        })?;
    }
    for verification in concept.verifications() {
        let actor_uri = format!("actor:{}", verification.actor);
        let actor_metadata = json!({});
        let actor_id = index.ensure_resource(&ResourceInput {
            project_id,
            namespace: "actor",
            external_id: &verification.actor,
            canonical_uri: &actor_uri,
            kind: &verification.kind,
            title: &verification.actor,
            description: None,
            origin_kind: "repository",
            origin_id,
            authority: "repository",
            status: None,
            stale_after: None,
            metadata: &actor_metadata,
        })?;
        index.add_verification(&VerificationInput {
            resource_version_id: version_id,
            actor_resource_id: actor_id,
            verified_at: verification.at.as_deref(),
            verification_kind: &verification.kind,
            metadata: &actor_metadata,
        })?;
    }

    let edge_metadata: Vec<Value> = concept
        .links
        .iter()
        .map(|link| json!({"label": link.label, "external": link.external}))
        .collect();
    let edge_hashes: Vec<String> = concept
        .links
        .iter()
        .map(|link| {
            blake3::hash(format!("{}:{}", link.target, link.line).as_bytes())
                .to_hex()
                .to_string()
        })
        .collect();
    let mut external_ids = BTreeMap::new();
    for link in &concept.links {
        if link.external {
            external_ids.insert(
                link.target.clone(),
                ensure_external_resource(index, project_id, &link.target)?,
            );
        }
    }
    let mut edges = Vec::new();
    for ((link, metadata), edge_hash) in concept.links.iter().zip(&edge_metadata).zip(&edge_hashes)
    {
        let destination = link
            .resolved_id
            .as_ref()
            .and_then(|id| resource_ids.get(id))
            .copied()
            .or_else(|| external_ids.get(&link.target).copied());
        edges.push(EdgeInput {
            src_resource_id: resource_id,
            dst_resource_id: destination,
            dst_ref: Some(&link.target),
            relation: if link.external { "cites" } else { "links_to" },
            confidence: if destination.is_some() {
                "resolved"
            } else {
                "extracted"
            },
            extractor: OKF_EXTRACTOR,
            source_resource_id: resource_id,
            start_line: Some(link.line),
            end_line: Some(link.line),
            start_byte: None,
            end_byte: None,
            content_hash: edge_hash,
            metadata,
        });
    }
    index.replace_edges_for_source(version_id, OKF_EXTRACTOR, &edges)?;
    stats.resources_indexed += usize::from(!unchanged);
    stats.resources_unchanged += usize::from(unchanged);
    stats.segments_indexed += segments.len();
    stats.edges_indexed += edges.len();
    stats.unresolved_edges += edges
        .iter()
        .filter(|edge| edge.dst_resource_id.is_none())
        .count();
    Ok(())
}

fn replace_segments(
    index: &mut ProjectIndex,
    version_id: i64,
    segments: &[MarkdownSegment],
) -> Result<()> {
    let hashes: Vec<String> = segments
        .iter()
        .map(|segment| blake3::hash(segment.text.as_bytes()).to_hex().to_string())
        .collect();
    let metadata = json!({});
    let inputs: Vec<_> = segments
        .iter()
        .zip(&hashes)
        .enumerate()
        .map(|(ordinal, (segment, hash))| ContentSegmentInput {
            ordinal,
            title: &segment.title,
            heading_path: Some(&segment.heading_path),
            text: &segment.text,
            start_line: Some(segment.start_line),
            end_line: Some(segment.end_line),
            start_byte: Some(segment.start_byte),
            end_byte: Some(segment.end_byte),
            token_count: Some(segment.text.split_whitespace().count()),
            content_hash: hash,
            metadata: &metadata,
        })
        .collect();
    index.replace_segments(version_id, &inputs)
}

fn segment_markdown(body: &str, fallback_title: &str) -> Vec<MarkdownSegment> {
    let mut starts = vec![(0usize, 1usize, fallback_title.to_owned())];
    let mut fenced = false;
    let mut byte = 0usize;
    for (line_index, line) in body.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced {
            let hashes = trimmed.bytes().take_while(|value| *value == b'#').count();
            if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
                starts.push((
                    byte,
                    line_index + 1,
                    trimmed[hashes + 1..].trim().to_owned(),
                ));
            }
        }
        byte += line.len();
    }
    starts.sort_by_key(|entry| entry.0);
    starts.dedup_by_key(|entry| entry.0);
    starts
        .iter()
        .enumerate()
        .filter_map(|(index, (start_byte, start_line, title))| {
            let end_byte = starts
                .get(index + 1)
                .map(|entry| entry.0)
                .unwrap_or(body.len());
            let text = body[*start_byte..end_byte].trim().to_owned();
            (!text.is_empty()).then(|| MarkdownSegment {
                title: title.clone(),
                heading_path: title.clone(),
                text,
                start_line: *start_line,
                end_line: body[..end_byte].lines().count().max(*start_line),
                start_byte: *start_byte,
                end_byte,
            })
        })
        .collect()
}

fn extract_links(body: &str) -> Vec<(String, String, usize)> {
    let mut links = Vec::new();
    let mut fenced = false;
    for (line_index, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut remaining = line;
        while let Some(start) = remaining.find('[') {
            let after = &remaining[start + 1..];
            let Some(end_label) = after.find("](") else {
                break;
            };
            let target_start = &after[end_label + 2..];
            let Some(end_target) = target_start.find(')') else {
                break;
            };
            links.push((
                after[..end_label].to_owned(),
                target_start[..end_target].trim().to_owned(),
                line_index + 1,
            ));
            remaining = &target_start[end_target + 1..];
        }
    }
    links
}

fn repository_relative(root: &Path, path: &Path) -> Result<String> {
    let root = root.canonicalize()?;
    let path = path.canonicalize()?;
    Ok(path
        .strip_prefix(&root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn okf_uri(project_id: &str, origin_id: &str, concept_id: &str) -> String {
    format!("okf://{project_id}/{origin_id}/{concept_id}")
}

fn ensure_external_resource(index: &ProjectIndex, project_id: &str, target: &str) -> Result<i64> {
    let metadata = json!({"fetch": false});
    index.ensure_resource(&ResourceInput {
        project_id,
        namespace: "external",
        external_id: target,
        canonical_uri: target,
        kind: "external",
        title: target,
        description: None,
        origin_kind: "external",
        origin_id: target,
        authority: "derived",
        status: None,
        stale_after: None,
        metadata: &metadata,
    })
}

fn yaml_to_json(value: &serde_yaml::Value) -> Result<Value> {
    serde_json::to_value(value).context("convert OKF metadata to JSON")
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../agent-cli/tests/fixtures/knowledge_graph")
    }

    #[test]
    fn okf_populates_shared_search_graph_and_metadata_idempotently() {
        let root = fixture_root();
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let first = index_okf_bundle(
            &mut index,
            "fixture",
            &root,
            &root.join("okf/v02"),
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(first.resources_indexed, 3);
        assert!(first.segments_indexed >= 3);
        assert_eq!(first.unresolved_edges, 1);
        assert!(!index.search_segments("checkout", 20).unwrap().is_empty());
        let filtered = index
            .search_segments_filtered(
                "fixture",
                "checkout",
                &crate::SearchFilter {
                    namespace: Some("okf"),
                    kind: Some("Service"),
                    status: Some("stable"),
                    relation: Some("links_to"),
                    ..crate::SearchFilter::default()
                },
                20,
            )
            .unwrap();
        assert_eq!(filtered.len(), 1);
        let service = index
            .find_resources("fixture", "Checkout Service", Some("okf"), 5)
            .unwrap();
        assert_eq!(service.len(), 1);
        let edges = index.edges_from(service[0].id, None).unwrap();
        assert!(edges.iter().any(|edge| edge.confidence == "resolved"));
        assert!(edges
            .iter()
            .any(|edge| edge.dst_ref.as_deref() == Some("retired.md")));

        let second = index_okf_bundle(
            &mut index,
            "fixture",
            &root,
            &root.join("okf/v02"),
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(second.resources_indexed, 0);
        assert_eq!(second.resources_unchanged, 3);
    }

    #[test]
    fn markdown_segmentation_and_links_are_fence_aware() {
        let root = fixture_root();
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let stats = index_markdown_file(
            &mut index,
            "fixture",
            &root,
            &root.join("markdown/architecture.md"),
        )
        .unwrap();
        assert_eq!(stats.resources_indexed, 1);
        assert_eq!(stats.segments_indexed, 2);
        assert_eq!(stats.edges_indexed, 3);
        let doc = index
            .find_resources("fixture", "Service Architecture", Some("docs"), 5)
            .unwrap();
        assert_eq!(doc.len(), 1);
        let edges = index.edges_from(doc[0].id, None).unwrap();
        assert!(!edges
            .iter()
            .any(|edge| edge.dst_ref.as_deref() == Some("../../outside.md")));
    }

    #[test]
    fn okf_incremental_edit_and_removal_replace_only_repository_origin() {
        let project = TempDir::new().unwrap();
        let bundle_root = project.path().join("knowledge");
        fs::create_dir_all(&bundle_root).unwrap();
        fs::write(
            bundle_root.join("a.md"),
            "---\ntype: Note\ntitle: A\n---\n# A\n\nSee [B](b.md) and [missing](missing.md).\n",
        )
        .unwrap();
        fs::write(
            bundle_root.join("b.md"),
            "---\ntype: Note\ntitle: B\n---\n# B\n\nOriginal.\n",
        )
        .unwrap();
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let first = index_okf_bundle(
            &mut index,
            "fixture",
            project.path(),
            &bundle_root,
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(first.resources_indexed, 2);

        fs::write(
            bundle_root.join("a.md"),
            "---\ntype: Note\ntitle: A\n---\n# A\n\nEdited. See [B](b.md) and [missing](missing.md).\n",
        )
        .unwrap();
        let second = index_okf_bundle(
            &mut index,
            "fixture",
            project.path(),
            &bundle_root,
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(second.resources_indexed, 1);
        assert_eq!(second.resources_unchanged, 1);

        let a = index
            .find_resources("fixture", "A", Some("okf"), 5)
            .unwrap();
        assert_eq!(a.len(), 1);
        let edited_edges = index.edges_from(a[0].id, Some("links_to")).unwrap();
        assert_eq!(edited_edges.len(), 2, "historical edges must not leak");

        fs::remove_file(bundle_root.join("b.md")).unwrap();
        let third = index_okf_bundle(
            &mut index,
            "fixture",
            project.path(),
            &bundle_root,
            OkfLimits::default(),
        )
        .unwrap();
        assert_eq!(third.resources_unchanged, 1);
        assert_eq!(third.resources_removed, 1);
        assert!(index
            .find_resources("fixture", "B", Some("okf"), 5)
            .unwrap()
            .is_empty());

        let current_edges = index.edges_from(a[0].id, Some("links_to")).unwrap();
        assert_eq!(current_edges.len(), 2, "current links appear exactly once");
        assert!(current_edges
            .iter()
            .all(|edge| edge.dst_resource_id.is_none() && edge.confidence == "extracted"));
        assert_eq!(
            current_edges
                .iter()
                .filter(|edge| edge.dst_ref.as_deref() == Some("b.md"))
                .count(),
            1
        );
        assert_eq!(
            current_edges
                .iter()
                .filter(|edge| edge.dst_ref.as_deref() == Some("missing.md"))
                .count(),
            1
        );

        let traversed = index
            .traverse(a[0].id, Some("links_to"), "out", 1, 20)
            .unwrap();
        assert_eq!(traversed.len(), 2);
        assert!(traversed
            .iter()
            .all(|edge| edge.target_uri.is_none() && edge.confidence == "extracted"));
    }
}
