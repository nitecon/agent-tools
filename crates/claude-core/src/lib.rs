mod error;
mod output;
mod path;

pub use error::ToolError;
pub use output::{OutputFormat, OutputFormatter};
pub use path::normalize_path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;

/// Build a Gitignore matcher for the given directory.
/// Walks up from `dir` to find all .gitignore files in parent directories.
pub fn build_gitignore(dir: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(dir);

    // Walk up directories looking for .gitignore files
    let mut current = Some(dir);
    while let Some(d) = current {
        let gitignore_path = d.join(".gitignore");
        if gitignore_path.exists() {
            let _ = builder.add(&gitignore_path);
        }
        current = d.parent();
    }

    builder.build().unwrap_or_else(|_| Gitignore::empty())
}
