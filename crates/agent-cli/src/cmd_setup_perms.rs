//! `agent-tools setup perms` — add Claude Code permission denies for the
//! native task system so agents are forced onto the gateway-backed
//! `agent-tools tasks` board.
//!
//! Targets `~/.claude/settings.json`. The `claude` CLI has no first-class
//! subcommand for editing persisted settings, so we own the JSON round-trip
//! here: preserve unrelated keys verbatim, dedupe the deny list, and write
//! back with stable two-space indentation.
//!
//! `TaskOutput` and `TaskStop` are **deliberately** not denied — those
//! manage background-subagent execution, which is orthogonal to the todo
//! list. `TodoWrite` is included defensively: if it still exists in a given
//! Claude Code build we block it; if it's been retired the entry is a
//! harmless no-op.

use agent_comms::config::home_dir;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The canonical deny list. Adding a new entry here is the ONLY place new
/// tools become part of the install contract.
pub const DENY_TOOLS: &[&str] = &[
    "TaskCreate",
    "TaskGet",
    "TaskList",
    "TaskUpdate",
    "TodoWrite",
];

pub fn run(remove: bool, dry_run: bool, print: bool) -> Result<()> {
    let path = settings_path();
    let existing_text = read_settings_text(&path)?;
    let existing: Value = parse_or_empty(&existing_text)
        .with_context(|| format!("parse {} as JSON", path.display()))?;

    let updated = apply_denies(existing, !remove)?;
    let rendered = render_pretty(&updated)?;

    if print {
        print!("{rendered}");
        return Ok(());
    }

    if dry_run {
        println!("--- DRY RUN: {} ---", path.display());
        print!("{rendered}");
        println!("--- end preview ---");
        return Ok(());
    }

    if existing_text.trim() == rendered.trim() {
        println!(
            "No changes — {} already in the desired state.",
            path.display()
        );
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent of {}", path.display()))?;
    }
    if path.exists() {
        let backup = backup_path(&path);
        std::fs::write(&backup, &existing_text)
            .with_context(|| format!("write backup to {}", backup.display()))?;
        println!("  backup: {}", backup.display());
    }
    std::fs::write(&path, &rendered)
        .with_context(|| format!("write updated {}", path.display()))?;

    if remove {
        println!("Removed native task denies from {}", path.display());
    } else {
        println!("Added native task denies to {}", path.display());
    }
    Ok(())
}

/// True iff **every** `DENY_TOOLS` entry is present in the settings file's
/// `permissions.deny` array. Used by the interactive menu to render state.
pub fn is_fully_installed() -> bool {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let deny = v
        .get("permissions")
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array());
    let Some(deny) = deny else { return false };
    let present: std::collections::HashSet<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    DENY_TOOLS.iter().all(|t| present.contains(*t))
}

pub fn settings_path() -> PathBuf {
    home_dir().join(".claude").join("settings.json")
}

// -- Internals ---------------------------------------------------------------

fn read_settings_text(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn parse_or_empty(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    Ok(serde_json::from_str(text)?)
}

/// Core merge. Returns the updated Value with deny entries either added (when
/// `add == true`) or removed. Preserves insertion order of the deny array so
/// diffs stay small on re-runs.
fn apply_denies(root: Value, add: bool) -> Result<Value> {
    let mut root_map = match root {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        other => anyhow::bail!(
            "settings.json root must be a JSON object, found {}",
            type_name(&other)
        ),
    };

    // permissions object
    let permissions_entry = root_map
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    let permissions = match permissions_entry {
        Value::Object(m) => m,
        other => anyhow::bail!(
            "`permissions` must be an object, found {}",
            type_name(other)
        ),
    };

    // deny array
    let deny_entry = permissions
        .entry("deny")
        .or_insert_with(|| Value::Array(Vec::new()));
    let deny = match deny_entry {
        Value::Array(a) => a,
        other => anyhow::bail!(
            "`permissions.deny` must be an array, found {}",
            type_name(other)
        ),
    };

    if add {
        for tool in DENY_TOOLS {
            let needle = Value::String((*tool).to_string());
            if !deny.iter().any(|v| v == &needle) {
                deny.push(needle);
            }
        }
    } else {
        deny.retain(|v| match v.as_str() {
            Some(s) => !DENY_TOOLS.contains(&s),
            None => true,
        });
        if deny.is_empty() {
            permissions.remove("deny");
        }
        if permissions.is_empty() {
            root_map.remove("permissions");
        }
    }

    Ok(Value::Object(root_map))
}

fn render_pretty(v: &Value) -> Result<String> {
    // serde_json's default pretty printer uses two-space indent, which
    // matches what Claude Code writes from its own `/config` UI.
    let mut out = serde_json::to_string_pretty(v).context("serialize settings to JSON")?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn add_to_empty_creates_full_structure() {
        let out = apply_denies(Value::Object(Map::new()), true).unwrap();
        let deny = out["permissions"]["deny"].as_array().unwrap();
        let entries: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        for tool in DENY_TOOLS {
            assert!(entries.contains(tool), "missing {tool}");
        }
    }

    #[test]
    fn add_preserves_unrelated_keys() {
        let input = json!({
            "theme": "dark",
            "permissions": {
                "allow": ["Bash(git *)"],
                "deny": ["ExistingDeny"]
            },
            "hooks": { "Stop": [] }
        });
        let out = apply_denies(input, true).unwrap();
        assert_eq!(out["theme"], json!("dark"));
        assert_eq!(out["permissions"]["allow"], json!(["Bash(git *)"]));
        assert_eq!(out["hooks"], json!({ "Stop": [] }));
        let deny = out["permissions"]["deny"].as_array().unwrap();
        // Preserves the pre-existing entry *and* appends the new ones in
        // declaration order — users reading the file get a stable diff.
        assert_eq!(deny[0], json!("ExistingDeny"));
        assert_eq!(deny[1], json!("TaskCreate"));
        assert_eq!(deny.len(), 1 + DENY_TOOLS.len());
    }

    #[test]
    fn add_is_idempotent() {
        let input = Value::Object(Map::new());
        let once = apply_denies(input.clone(), true).unwrap();
        let twice = apply_denies(once.clone(), true).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn remove_strips_only_managed_entries() {
        let input = json!({
            "permissions": {
                "deny": ["TaskCreate", "TaskUpdate", "UserOwnedDeny", "TodoWrite"]
            }
        });
        let out = apply_denies(input, false).unwrap();
        let deny = out["permissions"]["deny"].as_array().unwrap();
        // Only UserOwnedDeny survives — we never touch entries we didn't add.
        assert_eq!(deny, &vec![json!("UserOwnedDeny")]);
    }

    #[test]
    fn remove_cleans_up_empty_containers() {
        let input = json!({
            "permissions": {
                "deny": ["TaskCreate", "TaskUpdate", "TaskList", "TaskGet", "TodoWrite"]
            }
        });
        let out = apply_denies(input, false).unwrap();
        // With no deny entries left, the whole permissions object goes too
        // so the file doesn't retain empty scaffolding.
        assert!(out.get("permissions").is_none());
    }

    #[test]
    fn remove_preserves_other_permissions_keys() {
        let input = json!({
            "permissions": {
                "allow": ["Bash(git *)"],
                "deny": ["TaskCreate"]
            }
        });
        let out = apply_denies(input, false).unwrap();
        // allow survives; deny becomes empty and is pruned but permissions
        // stays because allow is still there.
        assert_eq!(out["permissions"]["allow"], json!(["Bash(git *)"]));
        assert!(out["permissions"].get("deny").is_none());
    }

    #[test]
    fn errors_when_permissions_is_wrong_type() {
        let input = json!({ "permissions": "nope" });
        let err = apply_denies(input, true).unwrap_err();
        assert!(err.to_string().contains("permissions"));
    }

    #[test]
    fn errors_when_deny_is_wrong_type() {
        let input = json!({ "permissions": { "deny": "not-an-array" } });
        let err = apply_denies(input, true).unwrap_err();
        assert!(err.to_string().contains("deny"));
    }

    #[test]
    fn null_root_is_treated_as_empty_object() {
        let out = apply_denies(Value::Null, true).unwrap();
        assert!(out["permissions"]["deny"].is_array());
    }

    #[test]
    fn parse_or_empty_blank_input_returns_object() {
        let v = parse_or_empty("   \n  ").unwrap();
        assert_eq!(v, Value::Object(Map::new()));
    }

    #[test]
    fn render_pretty_ends_with_newline() {
        let v = json!({ "a": 1 });
        let s = render_pretty(&v).unwrap();
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn settings_path_is_in_claude_dir() {
        let p = settings_path();
        assert!(p.ends_with(".claude/settings.json"));
    }
}
