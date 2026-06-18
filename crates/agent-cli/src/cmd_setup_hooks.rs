//! `agent-tools setup hooks` - sync app-scoped hooks from the gateway and
//! install the local `agent-tools hook` context-injection entries.
//!
//! Two distinct kinds of hook are synced per detected agent:
//!   1. Gateway-published hook *files* dropped under `<agent_home>/hooks/`.
//!   2. The local context-injection *command* entries that wire the calling
//!      agent CLI to `agent-tools hook ...`. These live in the agent's own
//!      settings file (`settings.json` for Claude/Gemini, `config.toml` for
//!      Codex) and are merged idempotently via `settings_json` /
//!      `codex_hooks_toml`.

use crate::cmd_setup_rules::codex_home;
use crate::codex_hooks_toml;
use crate::settings_json::{self, SettingsOutcome};
use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::hooks::HookRecord;
use agent_comms::identity::load_or_generate_agent_id;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

/// Marker substring that identifies a hook command entry as one we own.
/// Every command we install contains `<exe> hook ` so a single substring
/// match cleanly distinguishes our entries from user-authored hooks.
fn agent_tools_hook_marker(exe: &str) -> String {
    format!("{exe} hook ")
}

/// Resolve the current executable path for building hook command strings.
/// Falls back to the bare `agent-tools` name if resolution fails so installs
/// still produce a runnable command on a PATH-configured system.
fn current_exe_string() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "agent-tools".to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HookTarget {
    Claude,
    Codex,
    Gemini,
}

impl HookTarget {
    pub const ALL: [HookTarget; 3] = [HookTarget::Claude, HookTarget::Codex, HookTarget::Gemini];

    fn label(self) -> &'static str {
        match self {
            HookTarget::Claude => "Claude",
            HookTarget::Codex => "Codex",
            HookTarget::Gemini => "Gemini",
        }
    }

    fn app(self) -> &'static str {
        match self {
            HookTarget::Claude => "claude",
            HookTarget::Codex => "codex",
            HookTarget::Gemini => "gemini",
        }
    }

    fn agent_home(self) -> PathBuf {
        match self {
            HookTarget::Claude => home_dir().join(".claude"),
            HookTarget::Codex => codex_home(),
            HookTarget::Gemini => home_dir().join(".gemini"),
        }
    }

    fn hooks_root(self) -> PathBuf {
        self.agent_home().join("hooks")
    }

    /// Path to the settings file that holds local context-injection hook
    /// entries: `settings.json` for Claude/Gemini, `config.toml` for Codex.
    fn local_hook_settings_path(self) -> PathBuf {
        match self {
            HookTarget::Claude | HookTarget::Gemini => {
                settings_json::agent_settings_path(&self.agent_home())
            }
            HookTarget::Codex => self.agent_home().join("config.toml"),
        }
    }

    fn detected(self) -> bool {
        self.agent_home().exists()
    }

    fn parse_app(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Ok(HookTarget::Claude),
            "codex" => Ok(HookTarget::Codex),
            "gemini" => Ok(HookTarget::Gemini),
            other => anyhow::bail!(
                "unsupported hook app `{other}`; expected one of: codex, claude, gemini"
            ),
        }
    }
}

pub fn detected_target_labels() -> Vec<&'static str> {
    HookTarget::ALL
        .iter()
        .copied()
        .filter(|target| target.detected())
        .map(HookTarget::label)
        .collect()
}

/// Whether every detected agent has our local context-injection hook entries
/// present in its settings file. Returns `None` when no agents are detected so
/// callers can distinguish "nothing to install" from "installed/not installed".
pub fn local_hook_entries_installed() -> Option<bool> {
    let exe = current_exe_string();
    let marker = agent_tools_hook_marker(&exe);
    let detected: Vec<HookTarget> = HookTarget::ALL
        .iter()
        .copied()
        .filter(|t| t.detected())
        .collect();
    if detected.is_empty() {
        return None;
    }
    Some(
        detected
            .iter()
            .all(|t| settings_file_has_marker(&t.local_hook_settings_path(), &marker)),
    )
}

/// True when the settings file at `path` contains our hook marker anywhere.
/// A missing or unreadable file reads as "no marker" rather than erroring.
fn settings_file_has_marker(path: &Path, marker: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(marker))
        .unwrap_or(false)
}

pub fn run(apps: Vec<String>, dry_run: bool, remove: bool) -> Result<()> {
    let targets = resolve_targets(apps)?;
    if targets.is_empty() {
        anyhow::bail!(
            "No supported agent homes detected. Tried ~/.claude, ~/.gemini, and ~/.codex or $CODEX_HOME."
        );
    }

    // Step 1: local context-injection command entries. These wire the agent
    // CLI to `agent-tools hook ...` and are gateway-independent, so they run
    // (or unwire on --remove) even when no gateway is configured.
    sync_local_hook_entries(&targets, dry_run, remove)?;

    // Step 2: gateway-published hook files. Removal mode never touches these
    // (the gateway owns their lifecycle), and a missing gateway config is a
    // soft skip rather than a hard error so local wiring still succeeds.
    if remove {
        return Ok(());
    }

    let cfg = load_config();
    let (Some(gateway_url), Some(api_key)) =
        (cfg.gateway.url.clone(), cfg.gateway.api_key.clone())
    else {
        println!(
            "Gateway not configured — skipped gateway hook-file sync. \
             Run `agent-tools setup gateway` to enable it."
        );
        return Ok(());
    };
    let timeout_ms = cfg.gateway.timeout_ms.unwrap_or(5000);
    let gateway = GatewayClient::new(gateway_url, api_key, timeout_ms)?;
    let agent_id = load_or_generate_agent_id()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(sync_targets(&gateway, &agent_id, &targets, dry_run))
}

fn resolve_targets(apps: Vec<String>) -> Result<Vec<HookTarget>> {
    if apps.is_empty() {
        return Ok(HookTarget::ALL
            .iter()
            .copied()
            .filter(|target| target.detected())
            .collect());
    }

    let mut selected = BTreeSet::new();
    for app in apps {
        let target = HookTarget::parse_app(&app)?;
        if !target.detected() {
            anyhow::bail!(
                "{} home not detected at {}; install that client first or set its home env var",
                target.label(),
                target.agent_home().display()
            );
        }
        selected.insert(target);
    }
    Ok(selected.into_iter().collect())
}

/// One local context-injection hook entry to install for an agent: the event
/// name in that agent's settings vocabulary and the command to run.
struct LocalHookEntry {
    event: &'static str,
    command: String,
    /// JSON hook timeout. `None` for Codex (its TOML form omits timeout).
    timeout: Option<i64>,
}

/// Build the local hook entries for a target. The command always routes
/// through `agent-tools hook <kind> --agent <app>` so the marker
/// (`<exe> hook `) matches on remove.
fn local_hook_entries(target: HookTarget, exe: &str) -> Vec<LocalHookEntry> {
    let app = target.app();
    match target {
        HookTarget::Claude => vec![
            // session-start is Claude-only: Codex/Gemini have no equivalent
            // session-scoped event in this wiring, so we only inject the
            // open-tasks context there.
            LocalHookEntry {
                event: "SessionStart",
                command: format!("{exe} hook session-start --agent {app}"),
                timeout: Some(10),
            },
            LocalHookEntry {
                event: "UserPromptSubmit",
                command: format!("{exe} hook user-prompt-submit --agent {app}"),
                timeout: Some(10),
            },
        ],
        HookTarget::Gemini => vec![LocalHookEntry {
            // Gemini's per-turn event is `BeforeAgent`; its timeouts are in
            // milliseconds, unlike Claude's seconds.
            event: "BeforeAgent",
            command: format!("{exe} hook user-prompt-submit --agent {app}"),
            timeout: Some(10000),
        }],
        HookTarget::Codex => vec![LocalHookEntry {
            event: "UserPromptSubmit",
            command: format!("{exe} hook user-prompt-submit --agent {app}"),
            timeout: None,
        }],
    }
}

/// Install (or remove) the local context-injection hook entries for every
/// target. JSON-backed agents (Claude, Gemini) merge into `settings.json`;
/// Codex merges into `config.toml` via `toml_edit`.
fn sync_local_hook_entries(targets: &[HookTarget], dry_run: bool, remove: bool) -> Result<()> {
    let exe = current_exe_string();
    let marker = agent_tools_hook_marker(&exe);

    for &target in targets {
        let entries = local_hook_entries(target, &exe);
        let path = target.local_hook_settings_path();
        let verb = if remove { "remove from" } else { "install into" };

        if dry_run {
            println!(
                "Would {verb} {} hook entries in {}:",
                target.label(),
                path.display()
            );
            for e in &entries {
                println!("  [{}] {}", e.event, e.command);
            }
            continue;
        }

        for entry in &entries {
            let outcome = if matches!(target, HookTarget::Codex) {
                apply_codex_entry(&path, entry, &marker, remove)?
            } else {
                apply_json_entry(&path, entry, &marker, remove)?
            };
            report_outcome(target, entry, &path, &outcome);
        }
    }
    Ok(())
}

/// Apply one JSON-backed (Claude/Gemini) hook entry.
fn apply_json_entry(
    path: &Path,
    entry: &LocalHookEntry,
    marker: &str,
    remove: bool,
) -> Result<SettingsOutcome> {
    if remove {
        settings_json::remove_hook_group(path, entry.event, marker)
    } else {
        let timeout = entry.timeout.unwrap_or(10);
        settings_json::merge_hook_group(path, entry.event, &entry.command, timeout, marker)
    }
}

/// Apply one Codex (TOML-backed) hook entry.
fn apply_codex_entry(
    path: &Path,
    entry: &LocalHookEntry,
    marker: &str,
    remove: bool,
) -> Result<SettingsOutcome> {
    if remove {
        codex_hooks_toml::remove(path, entry.event, marker)
    } else {
        codex_hooks_toml::install(path, entry.event, &entry.command, marker)
    }
}

/// Print a concise per-entry status line.
fn report_outcome(
    target: HookTarget,
    entry: &LocalHookEntry,
    path: &Path,
    outcome: &SettingsOutcome,
) {
    let status = match outcome {
        SettingsOutcome::Created => "created",
        SettingsOutcome::Updated => "updated",
        SettingsOutcome::AlreadyCorrect => "already current",
        SettingsOutcome::AlreadyAbsent => "already absent",
        SettingsOutcome::Removed => "removed",
    };
    println!(
        "  {} [{}] {status} in {}",
        target.label(),
        entry.event,
        path.display()
    );
}

async fn sync_targets(
    gateway: &GatewayClient,
    agent_id: &str,
    targets: &[HookTarget],
    dry_run: bool,
) -> Result<()> {
    for target in targets {
        sync_target(gateway, agent_id, *target, dry_run).await?;
    }
    Ok(())
}

async fn sync_target(
    gateway: &GatewayClient,
    agent_id: &str,
    target: HookTarget,
    dry_run: bool,
) -> Result<()> {
    let hooks = gateway
        .list_hooks(target.app(), Some(agent_id))
        .await
        .with_context(|| format!("fetch {} hooks from gateway", target.app()))?;
    let hooks: Vec<HookRecord> = hooks
        .into_iter()
        .filter(|hook| hook.app.eq_ignore_ascii_case(target.app()))
        .collect();

    if hooks.is_empty() {
        println!(
            "No gateway hooks for {} ({}); skipped.",
            target.label(),
            target.app()
        );
        return Ok(());
    }

    println!(
        "{} {} hook(s) for {} into {}",
        if dry_run {
            "Would install"
        } else {
            "Installing"
        },
        hooks.len(),
        target.label(),
        target.hooks_root().display()
    );

    for hook in hooks {
        install_hook(target, &hook, dry_run)?;
    }
    Ok(())
}

fn install_hook(target: HookTarget, hook: &HookRecord, dry_run: bool) -> Result<()> {
    let relative = safe_relative_path(&hook.name)?;
    let path = target.hooks_root().join(&relative);
    validate_hook_payload(hook)?;

    if dry_run {
        println!("  {} -> {}", hook.name, path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&path, &hook.content).with_context(|| format!("write {}", path.display()))?;

    if hook.checksum.as_deref().is_some() {
        let written =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        verify_checksum(&hook.content, hook.checksum.as_deref())?;
        verify_checksum(&written, hook.checksum.as_deref())
            .with_context(|| format!("verify written hook {}", path.display()))?;
    }

    println!("  installed {} -> {}", hook.name, path.display());
    Ok(())
}

fn validate_hook_payload(hook: &HookRecord) -> Result<()> {
    if let Some(size) = hook.size {
        let actual = hook.content.len();
        if size != actual {
            anyhow::bail!(
                "hook {} size mismatch: gateway says {size} bytes, content is {actual} bytes",
                hook.name
            );
        }
    }
    verify_checksum(&hook.content, hook.checksum.as_deref())
        .with_context(|| format!("verify hook {} checksum", hook.name))
}

fn verify_checksum(content: &str, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let expected = expected.trim();
    if expected.is_empty() {
        return Ok(());
    }
    let expected = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_ascii_lowercase();
    let actual = sha256_hex(content);
    if expected != actual {
        anyhow::bail!("checksum mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

fn safe_relative_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hook name cannot be empty");
    }
    if trimmed.contains('\\') || trimmed.contains('\0') {
        anyhow::bail!("hook name `{trimmed}` must be a safe relative path");
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("hook name `{trimmed}` must be relative");
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            PathComponent::Normal(part) => out.push(part),
            _ => anyhow::bail!("hook name `{trimmed}` must not contain `.` or `..` segments"),
        }
    }

    if out.as_os_str().is_empty() {
        anyhow::bail!("hook name cannot be empty");
    }
    Ok(out)
}

fn sha256_hex(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_path_rejects_unsafe_names() {
        assert!(safe_relative_path("").is_err());
        assert!(safe_relative_path("../hook.sh").is_err());
        assert!(safe_relative_path("/tmp/hook.sh").is_err());
        assert!(safe_relative_path("hooks\\stop.sh").is_err());
    }

    #[test]
    fn safe_relative_path_allows_nested_names() {
        assert_eq!(
            safe_relative_path("session/start.sh").unwrap(),
            PathBuf::from("session/start.sh")
        );
    }

    #[test]
    fn verifies_sha256_with_optional_prefix() {
        let checksum = sha256_hex("hello");
        assert!(verify_checksum("hello", Some(&checksum)).is_ok());
        assert!(verify_checksum("hello", Some(&format!("sha256:{checksum}"))).is_ok());
        assert!(verify_checksum("hello!", Some(&checksum)).is_err());
    }

    #[test]
    fn detects_size_mismatch() {
        let hook = HookRecord {
            app: "codex".to_string(),
            name: "test.sh".to_string(),
            content: "hello".to_string(),
            size: Some(4),
            checksum: None,
            updated_at: None,
        };
        assert!(validate_hook_payload(&hook).is_err());
    }
}
