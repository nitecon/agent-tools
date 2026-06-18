//! Codex `config.toml` hook merge for `agent-tools setup hooks`.
//!
//! Uses `toml_edit` for a conservative, comment/format-preserving merge.
//! Codex expresses per-turn hooks as an array-of-tables:
//!
//! ```toml
//! [[hooks.UserPromptSubmit]]
//! type = "command"
//! command = "<exe> hook user-prompt-submit --agent codex"
//! ```
//!
//! No `timeout` key is written (unit uncertain for Codex).
//!
//! Merge discipline: idempotent by marker, preserves other tables/events,
//! atomic write, collapses empties on remove.

use crate::settings_json::SettingsOutcome;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

/// Install (or refresh) our per-turn hook table in `config_path` under `event`.
pub fn install(
    config_path: &Path,
    event: &str,
    command: &str,
    marker: &str,
) -> Result<SettingsOutcome> {
    match read_document(config_path)? {
        None => {
            // Build a fresh document containing exactly `[[hooks.<event>]]`.
            let mut doc = DocumentMut::new();
            let mut hooks_table = Table::new();
            let mut aot = ArrayOfTables::new();
            aot.push(hook_entry(command));
            hooks_table.insert(event, Item::ArrayOfTables(aot));
            doc.insert("hooks", Item::Table(hooks_table));
            write_document(config_path, &doc)?;
            Ok(SettingsOutcome::Created)
        }
        Some(mut doc) => {
            let before = doc.to_string();
            apply_install(&mut doc, event, command, marker, config_path)?;
            let after = doc.to_string();
            if before == after {
                return Ok(SettingsOutcome::AlreadyCorrect);
            }
            write_document(config_path, &doc)?;
            Ok(SettingsOutcome::Updated)
        }
    }
}

/// Remove our marker-matching hook tables from `hooks.<event>` in `config_path`.
pub fn remove(config_path: &Path, event: &str, marker: &str) -> Result<SettingsOutcome> {
    match read_document(config_path)? {
        None => Ok(SettingsOutcome::AlreadyAbsent),
        Some(mut doc) => {
            let before = doc.to_string();
            let changed = apply_remove(&mut doc, event, marker, config_path)?;
            if !changed {
                return Ok(SettingsOutcome::AlreadyAbsent);
            }
            let after = doc.to_string();
            if before == after {
                return Ok(SettingsOutcome::AlreadyAbsent);
            }
            write_document(config_path, &doc)?;
            Ok(SettingsOutcome::Removed)
        }
    }
}

// -- internals ---------------------------------------------------------------

/// Build a single `[[hooks.<event>]]` table: `type = "command"`, `command = ...`.
fn hook_entry(command: &str) -> Table {
    let mut tbl = Table::new();
    tbl.insert("type", toml_edit::value("command"));
    tbl.insert("command", toml_edit::value(command));
    tbl
}

fn apply_install(
    doc: &mut DocumentMut,
    event: &str,
    command: &str,
    marker: &str,
    path: &Path,
) -> Result<()> {
    // Ensure [hooks] table exists.
    if !doc.contains_key("hooks") {
        doc["hooks"] = Item::Table(Table::new());
    }
    let hooks = doc["hooks"].as_table_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "config file {} has wrong shape for `[hooks]`: expected table",
            path.display()
        )
    })?;

    // Ensure hooks.<event> is an array of tables.
    if !hooks.contains_key(event) {
        hooks.insert(event, Item::ArrayOfTables(ArrayOfTables::new()));
    }

    let event_arr = hooks[event].as_array_of_tables_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "config file {} has wrong shape for `[[hooks.{}]]`: expected array of tables",
            path.display(),
            event
        )
    })?;

    // Remove stale copies of our entries.
    let mut i = 0;
    while i < event_arr.len() {
        if let Some(cmd) = event_arr
            .get(i)
            .and_then(|t| t.get("command"))
            .and_then(|v| v.as_str())
        {
            if cmd.contains(marker) {
                event_arr.remove(i);
                continue;
            }
        }
        i += 1;
    }

    // Append fresh entry.
    event_arr.push(hook_entry(command));

    Ok(())
}

fn apply_remove(doc: &mut DocumentMut, event: &str, marker: &str, path: &Path) -> Result<bool> {
    if !doc.contains_key("hooks") {
        return Ok(false);
    }
    let hooks = doc["hooks"].as_table_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "config file {} has wrong shape for `[hooks]`: expected table",
            path.display()
        )
    })?;

    if !hooks.contains_key(event) {
        return Ok(false);
    }

    let event_arr = match hooks[event].as_array_of_tables_mut() {
        Some(a) => a,
        None => bail!(
            "config file {} has wrong shape for `[[hooks.{}]]`: expected array of tables",
            path.display(),
            event
        ),
    };

    let original_len = event_arr.len();
    let mut i = 0;
    while i < event_arr.len() {
        if let Some(cmd) = event_arr
            .get(i)
            .and_then(|t| t.get("command"))
            .and_then(|v| v.as_str())
        {
            if cmd.contains(marker) {
                event_arr.remove(i);
                continue;
            }
        }
        i += 1;
    }

    if event_arr.len() == original_len {
        return Ok(false);
    }

    // Collapse empty event array.
    if event_arr.is_empty() {
        hooks.remove(event);
    }

    // Collapse empty hooks table.
    let hooks_empty = doc["hooks"]
        .as_table()
        .map(|t| t.is_empty())
        .unwrap_or(false);
    if hooks_empty {
        doc.remove("hooks");
    }

    Ok(true)
}

fn read_document(path: &Path) -> Result<Option<DocumentMut>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("read config file {}", path.display())))
        }
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parse config file {} as TOML", path.display()))?;
    Ok(Some(doc))
}

fn write_document(path: &Path, doc: &DocumentMut) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent of {}", path.display()))?;
    }
    let body = doc.to_string();
    let tmp_path = {
        let mut s = path.as_os_str().to_owned();
        s.push(".new");
        PathBuf::from(s)
    };
    std::fs::write(&tmp_path, &body)
        .with_context(|| format!("write temp config {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("rename {} -> {}", tmp_path.display(), path.display()))?;
    Ok(())
}

// -- Tests -------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MARKER: &str = "agent-tools hook ";
    const CMD: &str = "/usr/local/bin/agent-tools hook user-prompt-submit --agent codex";
    const EVENT: &str = "UserPromptSubmit";

    struct TempDir {
        path: PathBuf,
    }
    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "agent-tools-codex-toml-test-{name}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn commands_in_doc(body: &str, event: &str) -> Vec<String> {
        let doc: DocumentMut = body.parse().unwrap();
        doc.get("hooks")
            .and_then(|h| h.as_table())
            .and_then(|t| t.get(event))
            .and_then(|e| e.as_array_of_tables())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("command").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn install_creates_file_when_absent() {
        let dir = TempDir::new("create");
        let path = dir.path.join("config.toml");
        let out = install(&path, EVENT, CMD, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::Created);
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("[[hooks.UserPromptSubmit]]"), "got: {body}");
        assert!(body.contains(CMD), "got: {body}");
        assert!(!body.contains("timeout"), "got: {body}");
    }

    #[test]
    fn install_is_idempotent() {
        let dir = TempDir::new("idempotent");
        let path = dir.path.join("config.toml");
        install(&path, EVENT, CMD, MARKER).unwrap();
        let before_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let out = install(&path, EVENT, CMD, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::AlreadyCorrect);
        let after_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before_mtime, after_mtime);
        let body = fs::read_to_string(&path).unwrap();
        let cmds = commands_in_doc(&body, EVENT);
        let ours = cmds.iter().filter(|c| c.contains(MARKER)).count();
        assert_eq!(ours, 1);
    }

    #[test]
    fn install_preserves_user_hook_in_same_event() {
        let dir = TempDir::new("preserve");
        let path = dir.path.join("config.toml");
        fs::write(
            &path,
            "[[hooks.UserPromptSubmit]]\ntype = \"command\"\ncommand = \"user-hook.sh\"\n",
        )
        .unwrap();
        install(&path, EVENT, CMD, MARKER).unwrap();
        let body = fs::read_to_string(&path).unwrap();
        let cmds = commands_in_doc(&body, EVENT);
        assert!(cmds.contains(&"user-hook.sh".to_string()), "got {cmds:?}");
        assert!(cmds.contains(&CMD.to_string()), "got {cmds:?}");
    }

    #[test]
    fn install_bails_on_corrupt_toml() {
        let dir = TempDir::new("corrupt");
        let path = dir.path.join("config.toml");
        let original = "[hooks\nbroken = true\n";
        fs::write(&path, original).unwrap();
        let err = install(&path, EVENT, CMD, MARKER).unwrap_err();
        assert!(format!("{err:#}").contains("config.toml"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn remove_strips_and_collapses() {
        let dir = TempDir::new("remove");
        let path = dir.path.join("config.toml");
        fs::write(&path, "model = \"gpt-5\"\n").unwrap();
        install(&path, EVENT, CMD, MARKER).unwrap();
        let out = remove(&path, EVENT, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::Removed);
        let body = fs::read_to_string(&path).unwrap();
        let doc: DocumentMut = body.parse().unwrap();
        assert!(!doc.contains_key("hooks"), "empty hooks must be dropped");
        assert_eq!(doc.get("model").and_then(|v| v.as_str()), Some("gpt-5"));
    }

    #[test]
    fn remove_preserves_user_hook() {
        let dir = TempDir::new("preserve-remove");
        let path = dir.path.join("config.toml");
        fs::write(
            &path,
            "[[hooks.UserPromptSubmit]]\ntype = \"command\"\ncommand = \"user-hook.sh\"\n",
        )
        .unwrap();
        install(&path, EVENT, CMD, MARKER).unwrap();
        let out = remove(&path, EVENT, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::Removed);
        let body = fs::read_to_string(&path).unwrap();
        let cmds = commands_in_doc(&body, EVENT);
        assert_eq!(cmds, vec!["user-hook.sh".to_string()]);
    }

    #[test]
    fn remove_noop_when_absent() {
        let dir = TempDir::new("noop");
        let path = dir.path.join("config.toml");
        let out = remove(&path, EVENT, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::AlreadyAbsent);
    }
}
