//! Serve knowledge-graph concepts as Markdown documents that have no file.
//!
//! The index is the canonical store for synthesized knowledge, so `read`,
//! `doc outline`, and `doc section` accept a canonical URI (or an unambiguous
//! title) wherever they accept a path.
//!
//! A real path always wins. Synthesized concepts are a lossy summary of code and
//! must never shadow the source they were derived from, so resolution here is
//! attempted only for targets that do not exist on disk.

use anyhow::{bail, Context, Result};
use std::path::Path;

/// Render a stored resource as its OKF Markdown document.
///
/// Returns `None` when the target does not name a stored resource, leaving the
/// caller's own "not found" reporting intact.
pub(crate) fn render(target: &str) -> Result<Option<String>> {
    if target.trim().is_empty() {
        return Ok(None);
    }
    let root = std::env::current_dir()?;
    let project_id = agent_core::project_ident(&root);
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    let matches = index.find_resources(&project_id, target, None, 20)?;
    let resource = match matches.as_slice() {
        [] => return Ok(None),
        [resource] => resource,
        resources => {
            // An exact canonical URI is never ambiguous; anything else is the
            // user's to disambiguate.
            match resources
                .iter()
                .find(|resource| resource.canonical_uri == target)
            {
                Some(resource) => resource,
                None => {
                    let candidates = resources
                        .iter()
                        .map(|resource| format!("  {} ({})", resource.canonical_uri, resource.kind))
                        .collect::<Vec<_>>()
                        .join("\n");
                    bail!("'{target}' matches more than one resource:\n{candidates}")
                }
            }
        }
    };

    let document = index
        .resource_document(resource.id)?
        .with_context(|| format!("{} has no stored document", resource.canonical_uri))?;
    crate::observe::resource("read", resource.id);
    Ok(Some(document))
}

/// Resolve a target that is either a real file or a stored resource.
///
/// Used by commands that accept both. `Ok(None)` means "neither" and lets the
/// caller report the failure in its own terms.
pub(crate) fn read_file_or_resource(target: &Path) -> Result<Option<String>> {
    if target.exists() {
        let text = std::fs::read_to_string(target)
            .with_context(|| format!("failed to read UTF-8 file {}", target.display()))?;
        crate::observe::path("read", target);
        return Ok(Some(text));
    }
    render(&target.to_string_lossy())
}
