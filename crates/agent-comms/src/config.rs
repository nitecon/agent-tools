//! Unified configuration system for agent-tools.
//!
//! Config is loaded from a four-layer hierarchy (lowest to highest priority):
//!
//! 1. `/opt/agentic/agent-tools/config.toml` -- system-wide global
//! 2. `~/.agentic/config.toml` -- per-user override
//! 3. `~/.claude/agent-comms.conf` -- legacy KEY=VALUE fallback (via dotenvy)
//! 4. Environment variables (`GATEWAY_URL`, `GATEWAY_API_KEY`, etc.)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

// -- Public types -------------------------------------------------------------

/// Top-level configuration container.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway: GatewayConfig,
}

/// Gateway connection settings.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub default_project: Option<String>,
}

// -- Path helpers -------------------------------------------------------------

/// Return the user's home directory via `HOME` (unix) or `USERPROFILE` (windows).
///
/// # Panics
/// Panics if neither environment variable is set.
pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        return PathBuf::from(h);
    }
    panic!("neither HOME nor USERPROFILE is set");
}

/// Path to the per-user config file: `~/.agentic/config.toml`.
pub fn user_config_path() -> PathBuf {
    home_dir().join(".agentic").join("config.toml")
}

/// Path to the system-wide config file: `/opt/agentic/agent-tools/config.toml`.
pub fn global_config_path() -> PathBuf {
    PathBuf::from("/opt/agentic/agent-tools/config.toml")
}

/// Path to the legacy config file: `~/.claude/agent-comms.conf`.
fn legacy_config_path() -> PathBuf {
    home_dir().join(".claude").join("agent-comms.conf")
}

// -- Config loading -----------------------------------------------------------

/// Load configuration from all layers and return the merged result.
///
/// Resolution order (later wins):
/// 1. Global TOML (`/opt/agentic/agent-tools/config.toml`)
/// 2. User TOML (`~/.agentic/config.toml`)
/// 3. Legacy KEY=VALUE (`~/.claude/agent-comms.conf`) -- fills unset fields only
/// 4. Environment variables -- override everything
pub fn load_config() -> Config {
    let mut cfg = Config::default();

    // Layer 1: system-wide global
    if let Some(parsed) = load_toml_file(&global_config_path()) {
        cfg = parsed;
    }

    // Layer 2: per-user override (overlay on top of global)
    if let Some(user) = load_toml_file(&user_config_path()) {
        overlay_config(&mut cfg, &user);
    }

    // Layer 3: legacy fallback -- only fills fields that are still None
    apply_legacy_fallback(&mut cfg);

    // Layer 4: environment variables (highest priority)
    apply_env_overrides(&mut cfg);

    cfg
}

/// Attempt to read and parse a TOML config file. Returns `None` if the file
/// does not exist or cannot be parsed (a warning is printed to stderr on parse
/// failure).
fn load_toml_file(path: &PathBuf) -> Option<Config> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    match toml::from_str::<Config>(&content) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!(
                "[agent-tools] warning: failed to parse {}: {e}",
                path.display()
            );
            None
        }
    }
}

/// Overlay `src` onto `dst`: any `Some` value in `src` replaces the
/// corresponding value in `dst`.
fn overlay_config(dst: &mut Config, src: &Config) {
    if src.gateway.url.is_some() {
        dst.gateway.url.clone_from(&src.gateway.url);
    }
    if src.gateway.api_key.is_some() {
        dst.gateway.api_key.clone_from(&src.gateway.api_key);
    }
    if src.gateway.timeout_ms.is_some() {
        dst.gateway.timeout_ms = src.gateway.timeout_ms;
    }
    if src.gateway.default_project.is_some() {
        dst.gateway
            .default_project
            .clone_from(&src.gateway.default_project);
    }
}

/// Read the legacy `~/.claude/agent-comms.conf` KEY=VALUE file and fill any
/// config fields that are still `None`.
fn apply_legacy_fallback(cfg: &mut Config) {
    let path = legacy_config_path();
    let pairs = match read_key_value_file(&path) {
        Some(p) => p,
        None => return,
    };

    if cfg.gateway.url.is_none() {
        if let Some(v) = pairs.get("GATEWAY_URL") {
            cfg.gateway.url = Some(v.clone());
        }
    }
    if cfg.gateway.api_key.is_none() {
        if let Some(v) = pairs.get("GATEWAY_API_KEY") {
            cfg.gateway.api_key = Some(v.clone());
        }
    }
    if cfg.gateway.timeout_ms.is_none() {
        if let Some(v) = pairs.get("GATEWAY_TIMEOUT_MS") {
            if let Ok(ms) = v.parse::<u64>() {
                cfg.gateway.timeout_ms = Some(ms);
            }
        }
    }
    if cfg.gateway.default_project.is_none() {
        if let Some(v) = pairs.get("DEFAULT_PROJECT_IDENT") {
            cfg.gateway.default_project = Some(v.clone());
        }
    }
}

/// Apply environment variable overrides (highest priority layer).
fn apply_env_overrides(cfg: &mut Config) {
    if let Ok(v) = std::env::var("GATEWAY_URL") {
        cfg.gateway.url = Some(v);
    }
    if let Ok(v) = std::env::var("GATEWAY_API_KEY") {
        cfg.gateway.api_key = Some(v);
    }
    if let Ok(v) = std::env::var("GATEWAY_TIMEOUT_MS") {
        if let Ok(ms) = v.parse::<u64>() {
            cfg.gateway.timeout_ms = Some(ms);
        }
    }
    if let Ok(v) = std::env::var("DEFAULT_PROJECT_IDENT") {
        cfg.gateway.default_project = Some(v);
    }
}

/// Parse a simple KEY=VALUE file (lines starting with `#` are comments,
/// blank lines are skipped, values may be optionally quoted).
fn read_key_value_file(path: &PathBuf) -> Option<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            map.insert(key, val);
        }
    }
    Some(map)
}

// -- Migration ----------------------------------------------------------------

/// Migrate the legacy `~/.claude/agent-comms.conf` to `~/.agentic/config.toml`.
///
/// The migration only runs when the legacy file exists AND the user config file
/// does **not** yet exist, preventing accidental overwrites.
pub fn migrate_legacy_config() {
    let legacy = legacy_config_path();
    let user = user_config_path();

    if !legacy.exists() || user.exists() {
        return;
    }

    let pairs = match read_key_value_file(&legacy) {
        Some(p) => p,
        None => return,
    };

    let url = pairs.get("GATEWAY_URL").cloned().unwrap_or_default();
    let api_key = pairs.get("GATEWAY_API_KEY").cloned().unwrap_or_default();
    let timeout: u64 = pairs
        .get("GATEWAY_TIMEOUT_MS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);
    let project = pairs.get("DEFAULT_PROJECT_IDENT").cloned();

    let mut toml_content = String::from("[gateway]\n");
    if !url.is_empty() {
        toml_content.push_str(&format!("url = \"{url}\"\n"));
    }
    if !api_key.is_empty() {
        toml_content.push_str(&format!("api_key = \"{api_key}\"\n"));
    }
    toml_content.push_str(&format!("timeout_ms = {timeout}\n"));
    if let Some(ref p) = project {
        if !p.is_empty() {
            toml_content.push_str(&format!("default_project = \"{p}\"\n"));
        }
    }

    if let Some(parent) = user.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[agent-tools] failed to create {}: {e}", parent.display());
            return;
        }
    }

    match std::fs::write(&user, &toml_content) {
        Ok(()) => {
            eprintln!(
                "[agent-tools] Migrated config from ~/.claude/agent-comms.conf to ~/.agentic/config.toml"
            );
        }
        Err(e) => {
            eprintln!("[agent-tools] failed to write {}: {e}", user.display());
        }
    }
}

// -- Interactive init ---------------------------------------------------------

/// Run an interactive setup wizard that writes `~/.agentic/config.toml`.
///
/// Prompts the user for gateway URL, API key, default project, and timeout,
/// then writes the resulting TOML file.
///
/// # Errors
/// Returns an error if stdin/stdout interaction fails or the config file cannot
/// be written.
pub fn run_init() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut reader = stdin.lock();

    // Gateway URL
    write!(out, "Gateway URL [http://localhost:7913]: ")?;
    out.flush()?;
    let mut url_input = String::new();
    reader.read_line(&mut url_input)?;
    let url = url_input.trim();
    let url = if url.is_empty() {
        "http://localhost:7913"
    } else {
        url
    };

    // API key (masked input)
    let api_key =
        rpassword::prompt_password("Gateway API key: ").context("failed to read API key")?;
    if api_key.trim().is_empty() {
        anyhow::bail!("API key cannot be empty");
    }
    let api_key = api_key.trim();

    // Default project (optional)
    write!(
        out,
        "Default project ident (optional, press Enter to skip): "
    )?;
    out.flush()?;
    let mut project_input = String::new();
    reader.read_line(&mut project_input)?;
    let project = project_input.trim();

    // Timeout
    write!(out, "Request timeout in ms [5000]: ")?;
    out.flush()?;
    let mut timeout_input = String::new();
    reader.read_line(&mut timeout_input)?;
    let timeout: u64 = timeout_input.trim().parse().unwrap_or(5000);

    // Build TOML content
    let mut toml_content = String::from("[gateway]\n");
    toml_content.push_str(&format!("url = \"{url}\"\n"));
    toml_content.push_str(&format!("api_key = \"{api_key}\"\n"));
    toml_content.push_str(&format!("timeout_ms = {timeout}\n"));
    if !project.is_empty() {
        toml_content.push_str(&format!("default_project = \"{project}\"\n"));
    }

    // Write the file
    let config_path = user_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    std::fs::write(&config_path, &toml_content)
        .with_context(|| format!("write config to {}", config_path.display()))?;

    writeln!(out)?;
    writeln!(out, "Config written to {}", config_path.display())?;
    writeln!(out)?;
    writeln!(
        out,
        "To register the MCP server, add to your Claude config:"
    )?;
    writeln!(out, "  {{")?;
    writeln!(out, "    \"mcpServers\": {{")?;
    writeln!(out, "      \"agent-comms\": {{")?;
    writeln!(out, "        \"command\": \"/opt/agentic/bin/agent-mcp\"")?;
    writeln!(out, "      }}")?;
    writeln!(out, "    }}")?;
    writeln!(out, "  }}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn home_dir_returns_path() {
        // Should not panic in a normal environment.
        let h = home_dir();
        assert!(!h.as_os_str().is_empty());
    }

    #[test]
    fn user_config_path_ends_correctly() {
        let p = user_config_path();
        assert!(p.ends_with(".agentic/config.toml"));
    }

    #[test]
    fn global_config_path_is_absolute() {
        let p = global_config_path();
        assert_eq!(p, PathBuf::from("/opt/agentic/agent-tools/config.toml"));
    }

    #[test]
    fn env_overrides_take_precedence() {
        // Set env vars, load config, verify they appear.
        env::set_var("GATEWAY_URL", "http://test:9999");
        env::set_var("GATEWAY_TIMEOUT_MS", "1234");
        let cfg = load_config();
        assert_eq!(cfg.gateway.url.as_deref(), Some("http://test:9999"));
        assert_eq!(cfg.gateway.timeout_ms, Some(1234));
        env::remove_var("GATEWAY_URL");
        env::remove_var("GATEWAY_TIMEOUT_MS");
    }

    #[test]
    fn overlay_replaces_some_values() {
        let mut base = Config {
            gateway: GatewayConfig {
                url: Some("http://base".into()),
                api_key: Some("key-base".into()),
                timeout_ms: Some(1000),
                default_project: None,
            },
        };
        let overlay = Config {
            gateway: GatewayConfig {
                url: Some("http://overlay".into()),
                api_key: None,
                timeout_ms: None,
                default_project: Some("proj".into()),
            },
        };
        overlay_config(&mut base, &overlay);
        assert_eq!(base.gateway.url.as_deref(), Some("http://overlay"));
        assert_eq!(base.gateway.api_key.as_deref(), Some("key-base"));
        assert_eq!(base.gateway.timeout_ms, Some(1000));
        assert_eq!(base.gateway.default_project.as_deref(), Some("proj"));
    }

    #[test]
    fn read_key_value_parses_correctly() {
        let dir = std::env::temp_dir().join("agent-comms-test-kv");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.conf");
        std::fs::write(
            &file,
            "# comment\nGATEWAY_URL=http://localhost:7913\nAPI_KEY=\"secret\"\n\nTIMEOUT=5000\n",
        )
        .unwrap();
        let map = read_key_value_file(&file).unwrap();
        assert_eq!(map.get("GATEWAY_URL").unwrap(), "http://localhost:7913");
        assert_eq!(map.get("API_KEY").unwrap(), "secret");
        assert_eq!(map.get("TIMEOUT").unwrap(), "5000");
        std::fs::remove_dir_all(&dir).ok();
    }
}
