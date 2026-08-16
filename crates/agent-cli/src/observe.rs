//! Record what the tools touch, so the knowledge graph improves through use.
//!
//! Two grains, deliberately different:
//!
//! * **Access signals.** Reading, listing, or searching a resource bumps a
//!   counter. A read is evidence of interest, not knowledge, so it produces no
//!   concept — one row per (resource, tool) keeps the table bounded by the
//!   resource count no matter how heavily the tools are used.
//! * **Observations.** Completing a task *is* knowledge, so it becomes a draft
//!   `Observation` concept linked to the resources the work touched. The access
//!   signals are what supply those links.
//!
//! Everything here is best-effort. This is bookkeeping on the side of a command
//! the user actually asked for: it runs after that command has produced its
//! output, and a failure is silently dropped rather than turned into an error
//! the user has to care about. Set `AGENT_TOOLS_OBSERVE=off` to disable.

use agent_knowledge::okf::{ConceptSynthesis, OkfConcept, OkfLimits, OkfMapping, OkfValue};
use anyhow::Result;
use std::path::Path;

/// Window used when attributing accesses to a unit of work.
const WORK_WINDOW_SECONDS: i64 = 12 * 60 * 60;

/// Resources linked from a single observation.
const OBSERVATION_LINKS: usize = 8;

fn is_disabled() -> bool {
    std::env::var("AGENT_TOOLS_OBSERVE")
        .map(|value| value.trim().eq_ignore_ascii_case("off"))
        .unwrap_or(false)
}

/// Note that `tool` read a repository path.
///
/// Silent when observation is off, when the path is not indexed, or when the
/// index cannot be opened.
pub(crate) fn path(tool: &str, path: &Path) {
    if is_disabled() {
        return;
    }
    let _ = record_path(tool, path);
}

/// Note that `tool` read a resource already resolved to an id.
pub(crate) fn resource(tool: &str, resource_id: i64) {
    if is_disabled() {
        return;
    }
    let _ = record_resource(tool, resource_id);
}

/// Note that `tool` read several repository paths, as `grep` does.
///
/// Deduplicated so one command counts once per file, not once per match.
pub(crate) fn paths<'a>(tool: &str, paths: impl IntoIterator<Item = &'a Path>) {
    if is_disabled() {
        return;
    }
    let _ = record_paths(tool, paths);
}

fn record_path(tool: &str, target: &Path) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    if index.is_ephemeral() {
        return Ok(());
    }
    let relative = relative_to(&root, target);
    index.record_path_access(&relative, tool)?;
    Ok(())
}

fn record_paths<'a>(tool: &str, targets: impl IntoIterator<Item = &'a Path>) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    if index.is_ephemeral() {
        return Ok(());
    }
    let mut seen = std::collections::BTreeSet::new();
    for target in targets {
        let relative = relative_to(&root, target);
        if seen.insert(relative.clone()) {
            index.record_path_access(&relative, tool)?;
        }
    }
    Ok(())
}

fn record_resource(tool: &str, resource_id: i64) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    if index.is_ephemeral() {
        return Ok(());
    }
    index.record_access(resource_id, tool)?;
    Ok(())
}

fn relative_to(root: &Path, target: &Path) -> String {
    let absolute = target.canonicalize().unwrap_or_else(|_| root.join(target));
    absolute
        .strip_prefix(root)
        .unwrap_or(&absolute)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Record a completed unit of work as an `Observation` concept.
///
/// Returns the number of resources it was linked to, or `None` when nothing was
/// recorded. Never fails the caller: finishing a task must not depend on
/// bookkeeping succeeding.
pub(crate) fn completed_work(id: &str, title: &str, outcome: &str) -> Option<usize> {
    if is_disabled() {
        return None;
    }
    record_completed_work(id, title, outcome).ok().flatten()
}

fn record_completed_work(id: &str, title: &str, outcome: &str) -> Result<Option<usize>> {
    let root = std::env::current_dir()?;
    let project_id = agent_core::project_ident(&root);
    let mut index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    if index.is_ephemeral() {
        return Ok(None);
    }
    let since = now_epoch_seconds() - WORK_WINDOW_SECONDS;
    let touched = index.recent_accesses(&project_id, since, OBSERVATION_LINKS)?;
    if touched.is_empty() {
        // Nothing was read through the tools, so there is no evidence to
        // attach. An observation with no links is not worth storing.
        return Ok(None);
    }

    let concept = build_observation(&project_id, id, title, outcome, &touched)?;
    agent_knowledge::knowledge::index_observation(&mut index, &project_id, &concept)?;
    Ok(Some(touched.len()))
}

fn build_observation(
    project_id: &str,
    id: &str,
    title: &str,
    outcome: &str,
    touched: &[(agent_knowledge::ResourceMatch, i64)],
) -> Result<OkfConcept> {
    let mut body = format!("# {title}\n\n{outcome}\n\n## Resources touched\n\n");
    for (resource, count) in touched {
        let times = if *count == 1 { "time" } else { "times" };
        match concept_link(project_id, resource) {
            // Concepts are linked so the graph connects work to code.
            Some(target) => body.push_str(&format!(
                "- [{}]({target}) — read {count} {times}\n",
                resource.title
            )),
            None => body.push_str(&format!(
                "- `{}` — read {count} {times}\n",
                resource.canonical_uri
            )),
        }
    }

    let mut extension = OkfMapping::new();
    extension.insert(
        OkfValue::String("extractor".to_owned()),
        OkfValue::String("okf-observe/1".to_owned()),
    );
    extension.insert(
        OkfValue::String("work_id".to_owned()),
        OkfValue::String(id.to_owned()),
    );

    OkfConcept::synthesize(
        ConceptSynthesis {
            id: &format!("observations/{}.md", slug(id)),
            kind: "Observation",
            title,
            // Drafts by construction: this is an unreviewed trace of what an
            // agent did, not something the repository asserts.
            status: "draft",
            description: None,
            tags: vec!["observation".to_owned()],
            body,
            extension,
            relationships: Vec::new(),
        },
        OkfLimits::default(),
    )
}

/// Bundle-root-relative link to the concept that describes a resource.
///
/// A touched file is recorded as the *concept* for that file, not the raw
/// `repo://` row: the concept is what carries the knowledge, and linking to it
/// is what connects an observation into the graph.
fn concept_link(project_id: &str, resource: &agent_knowledge::ResourceMatch) -> Option<String> {
    match resource.namespace.as_str() {
        "okf" => {
            let identity = resource
                .canonical_uri
                .rsplit_once(&format!("/{}/", resource.origin_id))
                .map(|(_, tail)| tail.to_owned());
            Some(format!(
                "/{}",
                identity.unwrap_or_else(|| resource.title.clone())
            ))
        }
        "file" => resource
            .canonical_uri
            .strip_prefix(&format!("repo://{project_id}/"))
            .map(|path| format!("/code/{path}.md")),
        _ => None,
    }
}

/// Path-safe single segment. Dots are dropped along with separators so no
/// identity can ever spell a traversal.
fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn now_epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(namespace: &str, uri: &str, title: &str) -> agent_knowledge::ResourceMatch {
        agent_knowledge::ResourceMatch {
            id: 1,
            canonical_uri: uri.to_owned(),
            namespace: namespace.to_owned(),
            kind: "CodeSymbol".to_owned(),
            title: title.to_owned(),
            authority: "derived".to_owned(),
            origin_kind: "local-derived".to_owned(),
            origin_id: "okf-synth".to_owned(),
            status: "stable".to_owned(),
            current_version_id: Some(1),
        }
    }

    #[test]
    fn observations_link_concepts_and_stay_draft() {
        let touched = vec![(
            resource(
                "okf",
                "okf://project/okf-synth/code/src/lib.rs/run-fn-abcd1234.md",
                "run",
            ),
            7,
        )];
        let concept =
            build_observation("proj", "task-1", "Fix the parser", "done", &touched).unwrap();

        assert_eq!(concept.kind, "Observation");
        assert_eq!(concept.status, "draft");
        assert_eq!(concept.id, "observations/task-1.md");
        assert_eq!(
            concept.links[0].target,
            "/code/src/lib.rs/run-fn-abcd1234.md"
        );
        assert!(concept.body.contains("read 7 times"));
    }

    #[test]
    fn touched_files_link_to_the_concept_that_describes_them() {
        let touched = vec![(
            resource("file", "repo://project/src/lib.rs", "src/lib.rs"),
            1,
        )];
        let concept =
            build_observation("project", "task-2", "Read some code", "done", &touched).unwrap();
        // The file row itself carries no knowledge; its concept does.
        assert_eq!(concept.links[0].target, "/code/src/lib.rs.md");
        assert!(concept.body.contains("read 1 time"));
        assert!(!concept.body.contains("1 times"));
    }

    #[test]
    fn resources_outside_the_project_get_no_dangling_link() {
        let touched = vec![(
            resource("file", "repo://elsewhere/src/lib.rs", "src/lib.rs"),
            2,
        )];
        let concept =
            build_observation("project", "task-3", "Read some code", "done", &touched).unwrap();
        assert!(
            concept.links.is_empty(),
            "no link is better than a broken one"
        );
        assert!(concept.body.contains("repo://elsewhere/src/lib.rs"));
    }

    #[test]
    fn work_identities_are_path_safe() {
        let concept = build_observation(
            "project",
            "01a0/../etc",
            "Odd id",
            "done",
            &[(resource("file", "repo://p/a", "a"), 1)],
        )
        .unwrap();
        assert_eq!(concept.id, "observations/01a0----etc.md");
    }
}
