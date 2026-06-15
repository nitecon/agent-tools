//! `agent-tools setup hooks` - sync app-scoped hooks from the gateway.

use crate::cmd_setup_rules::codex_home;
use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::hooks::HookRecord;
use agent_comms::identity::load_or_generate_agent_id;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

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

pub fn installed_hook_roots() -> Vec<PathBuf> {
    HookTarget::ALL
        .iter()
        .copied()
        .filter(|target| target.detected() && target.hooks_root().exists())
        .map(HookTarget::hooks_root)
        .collect()
}

pub fn run(apps: Vec<String>, dry_run: bool) -> Result<()> {
    let targets = resolve_targets(apps)?;
    if targets.is_empty() {
        anyhow::bail!(
            "No supported agent homes detected. Tried ~/.claude, ~/.gemini, and ~/.codex or $CODEX_HOME."
        );
    }

    let cfg = load_config();
    let gateway_url = cfg
        .gateway
        .url
        .clone()
        .context("gateway URL not configured -- run `agent-tools setup gateway`")?;
    let api_key = cfg
        .gateway
        .api_key
        .clone()
        .context("gateway API key not configured -- run `agent-tools setup gateway`")?;
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
