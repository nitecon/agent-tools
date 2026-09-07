//! What the hooks have already told this session.
//!
//! Hook context is appended to the agent's transcript and stays there, so
//! re-injecting the same concept, task, or pattern on every prompt costs tokens
//! for the rest of the session and adds nothing. Each agent CLI passes a
//! `session_id` in the hook payload; this module keeps a small per-session
//! record of injected identities so a hook can skip what the agent has already
//! seen.
//!
//! Best-effort throughout: a missing or corrupt state file reads as "nothing
//! seen yet", and a failed write is dropped. Hooks must never fail because of
//! bookkeeping.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Session files older than this are pruned when a newer one is written.
const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Longest session id accepted before it is treated as hostile.
const MAX_SESSION_ID_LEN: usize = 128;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SessionState {
    #[serde(default)]
    pub knowledge: BTreeSet<String>,
    #[serde(default)]
    pub tasks: BTreeSet<String>,
    #[serde(default)]
    pub patterns: BTreeSet<String>,
}

/// Per-session record of injected context.
///
/// Without a usable session id the memory is inert: nothing is remembered and
/// everything reads as unseen, which is exactly the pre-existing behaviour.
#[derive(Debug)]
pub(crate) struct SessionMemory {
    path: Option<PathBuf>,
    pub state: SessionState,
}

impl SessionMemory {
    /// Open the record for `session_id` under the project's state directory.
    pub fn open(project_root: &Path, session_id: Option<&str>) -> Self {
        let Some(id) = session_id.and_then(sanitize_session_id) else {
            return Self {
                path: None,
                state: SessionState::default(),
            };
        };
        let path = sessions_dir(project_root).join(format!("{id}.json"));
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            state,
        }
    }

    /// Persist the record and prune stale siblings. Errors are swallowed.
    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let Some(dir) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        if let Ok(raw) = serde_json::to_string(&self.state) {
            let _ = std::fs::write(path, raw);
        }
        prune_stale(dir, path);
    }
}

fn sessions_dir(project_root: &Path) -> PathBuf {
    agent_core::project_data_dir(project_root).join("hook-sessions")
}

/// Keep only ids that are safe to use as a file name.
pub(crate) fn sanitize_session_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_SESSION_ID_LEN
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(trimmed.to_owned())
}

/// Remove session files that have not been touched within `STALE_AFTER`.
fn prune_stale(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if stale {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests below mutate `AGENT_TOOLS_STATE_DIR`; serialize them.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn sanitize_accepts_uuid_like_ids_and_rejects_paths() {
        assert_eq!(
            sanitize_session_id("c2c97fd5-95ac-4d25-a0c8-a40be56f9e2f").as_deref(),
            Some("c2c97fd5-95ac-4d25-a0c8-a40be56f9e2f")
        );
        assert_eq!(sanitize_session_id(" abc_123 ").as_deref(), Some("abc_123"));
        assert!(sanitize_session_id("").is_none());
        assert!(sanitize_session_id("../etc/passwd").is_none());
        assert!(sanitize_session_id("a/b").is_none());
        assert!(sanitize_session_id(&"x".repeat(MAX_SESSION_ID_LEN + 1)).is_none());
    }

    #[test]
    fn missing_session_id_is_inert() {
        let memory = SessionMemory::open(Path::new("/nonexistent"), None);
        assert!(memory.path.is_none());
        assert!(memory.state.knowledge.is_empty());
        // Saving without a path is a no-op rather than an error.
        memory.save();
    }

    #[test]
    fn state_round_trips_through_the_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "agent-tools-hook-session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("AGENT_TOOLS_STATE_DIR", &root);

        let mut memory = SessionMemory::open(&root, Some("session-1"));
        assert!(memory.path.is_some());
        memory.state.knowledge.insert("okf://a".to_owned());
        memory.state.tasks.insert("t1:todo".to_owned());
        memory.save();

        let reopened = SessionMemory::open(&root, Some("session-1"));
        assert!(reopened.state.knowledge.contains("okf://a"));
        assert!(reopened.state.tasks.contains("t1:todo"));

        // A different session sees nothing.
        let other = SessionMemory::open(&root, Some("session-2"));
        assert!(other.state.knowledge.is_empty());

        std::env::remove_var("AGENT_TOOLS_STATE_DIR");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_state_reads_as_empty() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "agent-tools-hook-session-corrupt-{}",
            std::process::id()
        ));
        std::env::set_var("AGENT_TOOLS_STATE_DIR", &root);
        let dir = sessions_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.json"), "{not json").unwrap();
        let memory = SessionMemory::open(&root, Some("broken"));
        assert!(memory.path.is_some());
        assert_eq!(memory.state, SessionState::default());
        std::env::remove_var("AGENT_TOOLS_STATE_DIR");
        let _ = std::fs::remove_dir_all(&root);
    }
}
