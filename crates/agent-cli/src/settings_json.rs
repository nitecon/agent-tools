//! Shared JSON settings-file helpers used across setup subcommands.
//!
//! Provides generic I/O and parse utilities that both `cmd_setup_perms` and
//! the hook-install machinery in `cmd_setup_hooks` need, so neither has to
//! duplicate the serde round-trip, atomic write, or backup logic.

use anyhow::{Context, Result};
pub(crate) use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

// --- Path helpers ---

/// Canonical path for the Claude Code settings file.
pub(crate) fn claude_settings_path() -> PathBuf {
    agent_comms::config::home_dir()
        .join(".claude")
        .join("settings.json")
}

/// Generic path: `<agent_home>/settings.json`.
pub(crate) fn agent_settings_path(agent_home: &Path) -> PathBuf {
    agent_home.join("settings.json")
}

// --- I/O helpers ---

/// Read the file text. Returns empty string if file is absent (not an error).
pub(crate) fn read_settings_text(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

/// Parse text as a JSON Value. Blank text becomes an empty object.
pub(crate) fn parse_or_empty(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    Ok(serde_json::from_str(text)?)
}

/// Serialize Value to 2-space-indented JSON with a trailing newline.
pub(crate) fn render_pretty(v: &Value) -> Result<String> {
    let mut out = serde_json::to_string_pretty(v).context("serialize settings to JSON")?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// `.bak` sibling path.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// Human-readable JSON type name for error messages.
pub(crate) fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// --- JSON hook merge (mirroring agent-memory json_hooks.rs) ---

/// Outcome of a settings-file merge operation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SettingsOutcome {
    Created,
    Updated,
    AlreadyCorrect,
    AlreadyAbsent,
    Removed,
}

/// Install (or refresh) a single hook group in `path` under `event`.
///
/// Mirrors agent-memory's `json_hooks::install` exactly: idempotent, preserves
/// unrelated keys, bails on wrong shapes, atomic write, no-mtime on
/// AlreadyCorrect.
pub(crate) fn merge_hook_group(
    path: &Path,
    event: &str,
    command: &str,
    timeout: i64,
    marker: &str,
) -> Result<SettingsOutcome> {
    let fresh_group = hook_group(command, timeout);

    match read_json_object(path)? {
        None => {
            let mut obj = Map::new();
            let mut hooks = Map::new();
            hooks.insert(event.to_string(), Value::Array(vec![fresh_group]));
            obj.insert("hooks".to_string(), Value::Object(hooks));
            write_json_object(path, &obj)?;
            Ok(SettingsOutcome::Created)
        }
        Some(mut obj) => {
            let before = serde_json::to_string(&obj).ok();
            let hooks = ensure_hooks_object(&mut obj, path)?;
            let arr = ensure_event_array(hooks, event, path)?;
            arr.retain(|g| !group_has_marker(g, marker));
            arr.push(fresh_group);
            let after = serde_json::to_string(&obj).ok();
            if before.is_some() && before == after {
                return Ok(SettingsOutcome::AlreadyCorrect);
            }
            write_json_object(path, &obj)?;
            Ok(SettingsOutcome::Updated)
        }
    }
}

/// Remove our marker-matching hook groups from `hooks[event]` in `path`.
///
/// Collapses empty `hooks[event]` → event key removed; empty `hooks` → key
/// removed. File never deleted. Returns AlreadyAbsent when nothing to remove.
pub(crate) fn remove_hook_group(path: &Path, event: &str, marker: &str) -> Result<SettingsOutcome> {
    match read_json_object(path)? {
        None => Ok(SettingsOutcome::AlreadyAbsent),
        Some(mut obj) => {
            let Some(hooks_val) = obj.get_mut("hooks") else {
                return Ok(SettingsOutcome::AlreadyAbsent);
            };
            let hooks = match hooks_val {
                Value::Object(m) => m,
                other => anyhow::bail!(
                    "settings file {} has wrong shape for `hooks`: expected object, got {}",
                    path.display(),
                    type_name(other)
                ),
            };
            let Some(event_val) = hooks.get_mut(event) else {
                return Ok(SettingsOutcome::AlreadyAbsent);
            };
            let arr = match event_val {
                Value::Array(a) => a,
                other => anyhow::bail!(
                    "settings file {} has wrong shape for `hooks.{}`: expected array, got {}",
                    path.display(),
                    event,
                    type_name(other)
                ),
            };
            let original_len = arr.len();
            arr.retain(|g| !group_has_marker(g, marker));
            if arr.len() == original_len {
                return Ok(SettingsOutcome::AlreadyAbsent);
            }
            // The mutable borrow of `arr`/`event_val`/`hooks` ends here, so we
            // can safely re-borrow `obj` to collapse the now-possibly-empty
            // containers without tripping the borrow checker.
            let hooks = match obj.get_mut("hooks").expect("hooks present above") {
                Value::Object(m) => m,
                _ => unreachable!("hooks confirmed object above"),
            };
            if hooks
                .get(event)
                .and_then(Value::as_array)
                .is_some_and(|a| a.is_empty())
            {
                hooks.remove(event);
            }
            if hooks.is_empty() {
                obj.remove("hooks");
            }
            write_json_object(path, &obj)?;
            Ok(SettingsOutcome::Removed)
        }
    }
}

// -- private helpers ---------------------------------------------------------

fn hook_group(command: &str, timeout: i64) -> Value {
    serde_json::json!({
        "matcher": "",
        "hooks": [
            { "type": "command", "command": command, "timeout": timeout }
        ]
    })
}

fn group_has_marker(group: &Value, marker: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|inner| {
            inner.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains(marker))
            })
        })
        .unwrap_or(false)
}

fn ensure_hooks_object<'a>(
    obj: &'a mut Map<String, Value>,
    path: &Path,
) -> Result<&'a mut Map<String, Value>> {
    let entry = obj
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    match entry {
        Value::Object(m) => Ok(m),
        other => anyhow::bail!(
            "settings file {} has wrong shape for `hooks`: expected object, got {}",
            path.display(),
            type_name(other)
        ),
    }
}

fn ensure_event_array<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
    path: &Path,
) -> Result<&'a mut Vec<Value>> {
    let entry = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    match entry {
        Value::Array(a) => Ok(a),
        other => anyhow::bail!(
            "settings file {} has wrong shape for `hooks.{}`: expected array, got {}",
            path.display(),
            event,
            type_name(other)
        ),
    }
}

fn read_json_object(path: &Path) -> Result<Option<Map<String, Value>>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(
                anyhow::Error::new(e).context(format!("read settings file {}", path.display()))
            )
        }
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse settings file {} as JSON", path.display()))?;
    match value {
        Value::Object(map) => Ok(Some(map)),
        other => anyhow::bail!(
            "settings file {} has unexpected top-level shape: expected object, got {}",
            path.display(),
            type_name(&other)
        ),
    }
}

fn write_json_object(path: &Path, obj: &Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent of {}", path.display()))?;
    }
    let sorted: std::collections::BTreeMap<&String, &Value> = obj.iter().collect();
    let mut body = serde_json::to_string_pretty(&sorted)
        .with_context(|| format!("serialize settings for {}", path.display()))?;
    body.push('\n');
    let tmp_path = {
        let mut s = path.as_os_str().to_owned();
        s.push(".new");
        PathBuf::from(s)
    };
    std::fs::write(&tmp_path, &body)
        .with_context(|| format!("write temp settings file {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "atomically rename {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MARKER: &str = "agent-tools hook ";
    const CMD: &str = "/usr/local/bin/agent-tools hook user-prompt-submit --agent claude";

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "agent-tools-settings-json-test-{name}-{}",
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

    fn commands_in(obj: &Value, event: &str) -> Vec<String> {
        obj.get("hooks")
            .and_then(|h| h.get(event))
            .and_then(Value::as_array)
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|g| g.get("hooks").and_then(Value::as_array))
                    .flatten()
                    .filter_map(|h| h.get("command").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn merge_hook_group_creates_file_when_absent() {
        let dir = TempDir::new("create");
        let path = dir.path.join("settings.json");
        let out = merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::Created);
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            commands_in(&parsed, "UserPromptSubmit"),
            vec![CMD.to_string()]
        );
    }

    #[test]
    fn merge_hook_group_is_idempotent() {
        let dir = TempDir::new("idempotent");
        let path = dir.path.join("settings.json");
        merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap();
        let before_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let out = merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::AlreadyCorrect);
        let after_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before_mtime, after_mtime);
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let ours = commands_in(&parsed, "UserPromptSubmit")
            .into_iter()
            .filter(|c| c.contains(MARKER))
            .count();
        assert_eq!(ours, 1);
    }

    #[test]
    fn merge_hook_group_preserves_unrelated_keys() {
        let dir = TempDir::new("preserve");
        let path = dir.path.join("settings.json");
        fs::write(
            &path,
            r#"{"theme":"dark","hooks":{"PreToolUse":[{"matcher":"","hooks":[{"type":"command","command":"user.sh"}]}]}}"#,
        )
        .unwrap();
        merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap();
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["theme"], serde_json::json!("dark"));
        assert!(!commands_in(&parsed, "PreToolUse").is_empty());
    }

    #[test]
    fn merge_hook_group_bails_on_wrong_shape() {
        let dir = TempDir::new("bail");
        let path = dir.path.join("settings.json");
        fs::write(&path, r#"{"hooks":[1,2,3]}"#).unwrap();
        let err = merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap_err();
        assert!(format!("{err:#}").contains("expected object"));
    }

    #[test]
    fn remove_hook_group_strips_and_collapses() {
        let dir = TempDir::new("remove");
        let path = dir.path.join("settings.json");
        merge_hook_group(&path, "UserPromptSubmit", CMD, 10, MARKER).unwrap();
        let out = remove_hook_group(&path, "UserPromptSubmit", MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::Removed);
        let parsed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(parsed.as_object().unwrap().get("hooks").is_none());
    }

    #[test]
    fn remove_hook_group_noop_when_absent() {
        let dir = TempDir::new("noop");
        let path = dir.path.join("settings.json");
        let out = remove_hook_group(&path, "UserPromptSubmit", MARKER).unwrap();
        assert_eq!(out, SettingsOutcome::AlreadyAbsent);
    }

    #[test]
    fn parse_or_empty_blank_input() {
        let v = parse_or_empty("   \n  ").unwrap();
        assert_eq!(v, Value::Object(Map::new()));
    }

    #[test]
    fn render_pretty_ends_with_newline() {
        let v = serde_json::json!({"a": 1});
        let s = render_pretty(&v).unwrap();
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn claude_settings_path_ends_correctly() {
        let p = claude_settings_path();
        assert!(p.ends_with(".claude/settings.json"));
    }
}
