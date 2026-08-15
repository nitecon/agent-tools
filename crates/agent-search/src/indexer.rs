use agent_knowledge::{FileMetadataInput, ProjectIndex};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;
use std::time::SystemTime;

#[cfg(test)]
use std::time::Duration;
/// File indexer that maintains a SQLite-backed file index with change detection.
pub struct FileIndexer {
    index: ProjectIndex,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRecord {
    pub path: String,
    pub extension: String,
    pub size: u64,
    pub mtime_secs: i64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct IndexStats {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_errored: usize,
    pub files_removed: usize,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indexed {} files, skipped {} unchanged, removed {}, {} errors ({} total)",
            self.files_indexed,
            self.files_skipped,
            self.files_removed,
            self.files_errored,
            self.files_seen
        )
    }
}

impl FileIndexer {
    pub fn open(db_path: &Path) -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open(db_path)
                .with_context(|| format!("Failed to open file index at {}", db_path.display()))?,
        })
    }

    #[cfg(test)]
    fn open_ephemeral() -> Result<Self> {
        Ok(Self {
            index: ProjectIndex::open_ephemeral()?,
        })
    }

    /// Open the file index in the centralized storage directory for the given project.
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

    /// Build or incrementally update the file index.
    pub fn build(&self, root: &Path, compute_hashes: bool) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        let project_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let project_id = agent_core::project_ident(&project_root);
        let scan_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

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

            stats.files_seen += 1;

            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => {
                    stats.files_errored += 1;
                    continue;
                }
            };

            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let duration = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let mtime_secs = duration.as_secs() as i64;
            let size = metadata.len();

            let path_str = relative_path_string(&project_root, path)?;
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();

            // Check if already indexed and unchanged
            let needs_update: bool = {
                match self.index.file_state(&path_str)? {
                    Some(state) => state.mtime_secs != mtime_secs || state.size != size,
                    None => true,
                }
            };

            if !needs_update {
                self.index.mark_file_seen(&path_str, &scan_id)?;
                stats.files_skipped += 1;
                continue;
            }

            let content_hash = if compute_hashes {
                match std::fs::read(path) {
                    Ok(contents) => Some(blake3::hash(&contents).to_hex().to_string()),
                    Err(_) => None,
                }
            } else {
                None
            };

            self.index.upsert_file_metadata(&FileMetadataInput {
                project_id: &project_id,
                project_root: &project_root,
                path,
                language: None,
                extension: &extension,
                size,
                mtime_secs,
                mtime_nanos: duration.subsec_nanos() as i64,
                content_hash: content_hash.as_deref(),
                scan_id: Some(&scan_id),
            })?;

            stats.files_indexed += 1;
        }

        stats.files_removed = self.index.complete_file_scan(&project_id, &scan_id)?;
        Ok(stats)
    }

    /// Get the underlying connection for queries.
    pub fn connection(&self) -> &Connection {
        self.index.connection()
    }

    /// Get total file count.
    pub fn file_count(&self) -> Result<usize> {
        let count: i64 =
            self.index
                .connection()
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_index() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let root = project_dir.path();

        std::fs::write(root.join("file.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("README.md"), "# Hello").unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn lib() {}").unwrap();

        let db_path = db_dir.path().join("files.db");
        let indexer = FileIndexer::open(&db_path).unwrap();
        let stats = indexer.build(root, false).unwrap();

        assert_eq!(stats.files_seen, 3);
        assert_eq!(stats.files_indexed, 3);
    }

    #[test]
    fn test_incremental() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let root = project_dir.path();

        std::fs::write(root.join("file.txt"), "hello").unwrap();

        let db_path = db_dir.path().join("files.db");
        let indexer = FileIndexer::open(&db_path).unwrap();

        let stats1 = indexer.build(root, false).unwrap();
        assert_eq!(stats1.files_indexed, 1);

        let stats2 = indexer.build(root, false).unwrap();
        assert_eq!(stats2.files_indexed, 0);
        assert_eq!(stats2.files_skipped, 1);
    }

    #[test]
    fn test_deleted_files_are_pruned_after_complete_scan() {
        let project_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let root = project_dir.path();
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();

        let indexer = FileIndexer::open(&db_dir.path().join("project.db")).unwrap();
        indexer.build(root, true).unwrap();
        assert_eq!(indexer.file_count().unwrap(), 2);

        std::fs::remove_file(second).unwrap();
        let stats = indexer.build(root, true).unwrap();
        assert_eq!(stats.files_removed, 1);
        assert_eq!(indexer.file_count().unwrap(), 1);
    }

    #[test]
    fn test_ephemeral_indexer_builds_in_memory() {
        let project_dir = TempDir::new().unwrap();
        let root = project_dir.path();

        std::fs::write(root.join("file.txt"), "hello").unwrap();

        let indexer = FileIndexer::open_ephemeral().unwrap();
        assert!(indexer.is_ephemeral());

        let stats = indexer.build(root, false).unwrap();
        assert_eq!(stats.files_indexed, 1);
        assert_eq!(indexer.file_count().unwrap(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_build_index_through_symlinked_root() {
        let project_dir = TempDir::new().unwrap();
        let link_dir = TempDir::new().unwrap();
        let root = project_dir.path();
        let linked_root = link_dir.path().join("project-link");

        std::fs::write(root.join("file.rs"), "fn main() {}").unwrap();
        std::os::unix::fs::symlink(root, &linked_root).unwrap();

        let indexer = FileIndexer::open_ephemeral().unwrap();
        let stats = indexer.build(&linked_root, false).unwrap();

        assert_eq!(stats.files_indexed, 1);
        assert_eq!(indexer.file_count().unwrap(), 1);
    }

    #[test]
    fn test_locked_persistent_index_does_not_fallback_to_ephemeral() {
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("files.db");
        let locker = Connection::open(&db_path).unwrap();
        locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let err =
            match FileIndexer::open_persistent_or_ephemeral(&db_path, Duration::from_millis(0)) {
                Ok(_) => {
                    panic!("locked persistent index should not fall back to ephemeral storage")
                }
                Err(err) => err,
            };
        let message = format!("{err:#}");
        assert!(message.contains("busy or locked"), "{message}");
        assert!(message.contains("files.db"), "{message}");

        locker.execute_batch("ROLLBACK;").unwrap();
    }
}
