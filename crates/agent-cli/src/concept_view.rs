//! Compact, agent-facing rendering of a knowledge-graph concept.
//!
//! `agent-tools get` used to print the raw resource JSON plus every edge,
//! which for a 27-line function came to more bytes than the function itself.
//! A lookup that costs more than reading the source is never chosen, so the
//! default view is a short Markdown card: the labels an agent needs to weigh
//! the concept (authority, lifecycle, trust, use), the body without its
//! frontmatter, and a summary of *resolved* relationships grouped by kind.
//! `--json` still returns everything.

use agent_knowledge::{ResourceDetail, TraversedEdge};
use std::collections::BTreeMap;

/// Resolved targets listed per relationship group before eliding.
const TARGETS_PER_RELATION: usize = 8;

/// Render the compact card.
pub(crate) fn render(
    detail: &ResourceDetail,
    document: Option<&str>,
    edges: &[TraversedEdge],
    accesses: i64,
) -> String {
    let resource = &detail.resource;
    let mut out = String::new();
    out.push_str(&format!("# {}\n", resource.title));
    out.push_str(&format!("uri: {}\n", resource.canonical_uri));
    let lifecycle = match detail.stale_after.as_deref() {
        Some(stale_after) => format!("{} (stale after {stale_after})", resource.status),
        None => resource.status.clone(),
    };
    let trust = if detail.verification_count > 0 {
        format!("verified ({})", detail.verification_count)
    } else {
        "unverified".to_owned()
    };
    out.push_str(&format!(
        "kind: {} · authority: {} · lifecycle: {} · trust: {} · accesses: {}\n",
        resource.kind, resource.authority, lifecycle, trust, accesses
    ));
    let mut provenance = format!("origin: {}:{}", resource.origin_kind, resource.origin_id);
    if let Some(location) = source_location(detail) {
        provenance.push_str(&format!(" · source: {location}"));
    }
    if !detail.tags.is_empty() {
        provenance.push_str(&format!(" · tags: {}", detail.tags.join(", ")));
    }
    out.push_str(&provenance);
    out.push('\n');
    if let Some(description) = detail
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
    {
        out.push_str(&format!("\n{}\n", description.trim()));
    }

    if let Some(document) = document {
        let body = body_without_index_sections(strip_frontmatter(document));
        let body = without_duplicate_title(&body, &resource.title);
        if !body.trim().is_empty() {
            out.push('\n');
            out.push_str(body.trim_end());
            out.push('\n');
        }
    }

    let (groups, unresolved) = group_edges(edges);
    if !groups.is_empty() || unresolved > 0 {
        out.push_str("\n## Relationships\n");
        for ((relation, direction), targets) in &groups {
            let arrow = if direction == "in" { "←" } else { "→" };
            let shown = targets
                .iter()
                .take(TARGETS_PER_RELATION)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let more = targets.len().saturating_sub(TARGETS_PER_RELATION);
            if more > 0 {
                out.push_str(&format!("- {relation} {arrow} {shown} (+{more} more)\n"));
            } else {
                out.push_str(&format!("- {relation} {arrow} {shown}\n"));
            }
        }
        if unresolved > 0 {
            out.push_str(&format!(
                "({unresolved} unresolved references omitted; `--json` lists everything)\n"
            ));
        }
    }
    out
}

/// `path:start-end` when the concept was synthesized from code.
fn source_location(detail: &ResourceDetail) -> Option<String> {
    let meta = &detail.metadata;
    let ext = meta.get("x-agent-tools").unwrap_or(meta);
    let path = ext.get("path").and_then(|v| v.as_str())?;
    match (
        ext.get("start_line").and_then(|v| v.as_u64()),
        ext.get("end_line").and_then(|v| v.as_u64()),
    ) {
        (Some(start), Some(end)) => Some(format!("{path}:{start}-{end}")),
        (Some(start), None) => Some(format!("{path}:{start}")),
        _ => Some(path.to_owned()),
    }
}

/// Drop a leading YAML frontmatter block, if any.
pub(crate) fn strip_frontmatter(document: &str) -> &str {
    let rest = match document.strip_prefix("---\n") {
        Some(rest) => rest,
        None => match document.strip_prefix("---\r\n") {
            Some(rest) => rest,
            None => return document,
        },
    };
    // The closing fence is a line that is exactly `---`.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return &rest[offset + line.len()..];
        }
        offset += line.len();
    }
    document
}

/// Drop a leading `# <title>` line that merely repeats the card header.
fn without_duplicate_title(body: &str, title: &str) -> String {
    let trimmed = body.trim_start();
    match trimmed.strip_prefix("# ") {
        Some(rest) => {
            let (first, remainder) = rest.split_once('\n').unwrap_or((rest, ""));
            if first.trim() == title {
                remainder.trim_start_matches('\n').to_owned()
            } else {
                body.to_owned()
            }
        }
        None => body.to_owned(),
    }
}

/// Remove `## Relationships` and `## Exported symbols` sections from a body.
///
/// Those sections are the concept's own edge listing; the card renders the
/// resolved edges from the graph instead, without the call-site noise.
pub(crate) fn body_without_index_sections(body: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            let heading = heading.trim();
            skipping = heading == "Relationships" || heading == "Exported symbols";
        } else if line.starts_with("# ") {
            skipping = false;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Group resolved edges by (relation, direction) and count the unresolved.
#[allow(clippy::type_complexity)]
fn group_edges(edges: &[TraversedEdge]) -> (BTreeMap<(String, String), Vec<String>>, usize) {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut unresolved = 0;
    for edge in edges {
        let other = if edge.direction == "in" {
            Some(edge.source_title.clone())
        } else {
            edge.target_title.clone()
        };
        let Some(other) = other.filter(|title| !title.is_empty()) else {
            unresolved += 1;
            continue;
        };
        let entry = groups
            .entry((edge.relation.clone(), edge.direction.clone()))
            .or_default();
        if !entry.contains(&other) {
            entry.push(other);
        }
    }
    (groups, unresolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_knowledge::ResourceMatch;

    fn edge(direction: &str, relation: &str, source: &str, target: Option<&str>) -> TraversedEdge {
        TraversedEdge {
            id: 0,
            depth: 1,
            direction: direction.to_owned(),
            relation: relation.to_owned(),
            confidence: if target.is_some() {
                "resolved".to_owned()
            } else {
                "extracted".to_owned()
            },
            source_uri: format!("okf://{source}"),
            source_title: source.to_owned(),
            target_uri: target.map(|t| format!("okf://{t}")),
            target_title: target.map(str::to_owned),
            unresolved_ref: if target.is_none() {
                Some("Ok".to_owned())
            } else {
                None
            },
            source_path: None,
            start_line: None,
        }
    }

    fn detail() -> ResourceDetail {
        ResourceDetail {
            resource: ResourceMatch {
                id: 1,
                canonical_uri: "okf://fixture/dispatch".to_owned(),
                namespace: "okf".to_owned(),
                kind: "CodeSymbol".to_owned(),
                title: "dispatch".to_owned(),
                authority: "derived".to_owned(),
                origin_kind: "local-derived".to_owned(),
                origin_id: "okf-synth".to_owned(),
                status: "stable".to_owned(),
                current_version_id: Some(1),
            },
            description: Some("Dispatch hook subcommands.".to_owned()),
            stale_after: None,
            metadata: serde_json::json!({
                "x-agent-tools": {"path": "src/cmd_hook.rs", "start_line": 37, "end_line": 63}
            }),
            revision: None,
            source_format: None,
            content_hash: None,
            generated_by: None,
            generated_at: None,
            tags: vec!["rust".to_owned(), "fn".to_owned()],
            provenance_count: 0,
            verification_count: 0,
        }
    }

    #[test]
    fn strips_frontmatter_and_index_sections() {
        let doc = "---\ntitle: x\nstatus: stable\n---\n# dispatch\n\nBody.\n\n## Relationships\n- calls `Ok`\n\n## Notes\nkeep\n";
        let body = body_without_index_sections(strip_frontmatter(doc));
        assert!(body.contains("# dispatch"));
        assert!(body.contains("Body."));
        assert!(body.contains("keep"));
        assert!(!body.contains("calls `Ok`"));
        assert!(!body.contains("title: x"));
    }

    #[test]
    fn unterminated_frontmatter_is_left_alone() {
        let doc = "---\ntitle: x\nno closing fence\n";
        assert_eq!(strip_frontmatter(doc), doc);
        assert_eq!(strip_frontmatter("plain"), "plain");
    }

    #[test]
    fn card_is_smaller_than_the_json_and_hides_unresolved_edges() {
        let edges = vec![
            edge("out", "calls", "dispatch", Some("run_session_start")),
            edge("out", "calls", "dispatch", Some("run_session_start")),
            edge("out", "calls", "dispatch", None),
            edge("out", "calls", "dispatch", None),
            edge("in", "links_to", "cmd_hook.rs", Some("dispatch")),
        ];
        let document = "---\ntitle: dispatch\n---\n# dispatch fn\n\n```rust\npub fn dispatch()\n```\n\n## Relationships\n- calls `Ok` (line 1)\n";
        let card = render(&detail(), Some(document), &edges, 3);
        assert!(card.starts_with("# dispatch\nuri: okf://fixture/dispatch\n"));
        // The body's own `# dispatch fn` heading differs from the title, so it stays;
        // an exact repeat of the title would be dropped.
        assert!(card.contains("# dispatch fn\n"));
        let repeated = "---\nt: 1\n---\n# dispatch\n\nBody only.\n";
        let repeated_card = render(&detail(), Some(repeated), &[], 0);
        assert_eq!(
            repeated_card.matches("# dispatch\n").count(),
            1,
            "{repeated_card}"
        );
        assert!(repeated_card.contains("Body only."));
        assert!(card.contains("authority: derived"));
        assert!(card.contains("trust: unverified"));
        assert!(card.contains("accesses: 3"));
        assert!(card.contains("source: src/cmd_hook.rs:37-63"));
        assert!(card.contains("tags: rust, fn"));
        assert!(card.contains("- calls → run_session_start\n"));
        assert!(card.contains("- links_to ← cmd_hook.rs\n"));
        assert!(card.contains("(2 unresolved references omitted"));
        assert!(!card.contains("calls `Ok`"));
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "resource": detail(), "relationships": edges, "accesses": 3
        }))
        .unwrap();
        assert!(
            card.len() < json.len() / 2,
            "{} vs {}",
            card.len(),
            json.len()
        );
    }

    #[test]
    fn elides_long_target_lists() {
        let edges: Vec<_> = (0..12)
            .map(|i| edge("out", "contains", "module", Some(&format!("sym{i}"))))
            .collect();
        let card = render(&detail(), None, &edges, 0);
        assert!(card.contains("(+4 more)"));
    }
}
