//! Unified configuration system for agent-tools.
//!
//! Config is loaded from a three-layer hierarchy (lowest to highest priority):
//!
//! 1. `/opt/agentic/agent-tools/gateway.conf` -- system-wide global (KEY=VALUE)
//! 2. `~/.agentic/agent-tools/gateway.conf` -- per-user override (KEY=VALUE)
//! 3. Environment variables (`GATEWAY_URL`, `GATEWAY_API_KEY`, etc.)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

// -- Public types -------------------------------------------------------------

/// Top-level configuration container.
#[derive(Debug, Default, Clone)]
pub struct Config {
    pub gateway: GatewayConfig,
}

/// Gateway connection settings.
#[derive(Debug, Default, Clone)]
pub struct GatewayConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub default_project: Option<String>,
}

/// Non-secret repository declaration for an additional gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGateway {
    pub profile: String,
    pub url: String,
    #[serde(default = "default_read_capabilities")]
    pub read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGatewaysFile {
    #[serde(default = "gateway_file_version")]
    pub version: u32,
    #[serde(default)]
    pub gateways: Vec<ProjectGateway>,
}

#[derive(Debug, Clone)]
pub struct ResolvedGateway {
    pub profile: String,
    pub url: String,
    pub api_key: String,
    pub timeout_ms: u64,
    pub primary: bool,
    pub read: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectGatewayStatus {
    pub declaration: ProjectGateway,
    pub configured: bool,
    pub error: Option<String>,
}

fn gateway_file_version() -> u32 {
    1
}

fn default_read_capabilities() -> Vec<String> {
    vec!["tasks".into(), "patterns".into(), "docs".into()]
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

/// Path to the per-user gateway config: `~/.agentic/agent-tools/gateway.conf`.
pub fn user_gateway_conf_path() -> PathBuf {
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("gateway.conf")
}

/// Path to the system-wide gateway config: `/opt/agentic/agent-tools/gateway.conf`.
pub fn global_gateway_conf_path() -> PathBuf {
    PathBuf::from("/opt/agentic/agent-tools/gateway.conf")
}

pub fn gateway_profiles_dir() -> PathBuf {
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("gateways")
}

pub fn gateway_profile_path(profile: &str) -> Result<PathBuf> {
    validate_profile_name(profile)?;
    Ok(gateway_profiles_dir().join(format!("{profile}.conf")))
}

pub fn project_gateways_path_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    loop {
        let candidate = current.join(".agents").join("alternate-gateways.yml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if current.join(".git").exists() {
            return Some(candidate);
        }
        current = current.parent()?;
    }
}

pub fn project_gateways_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("resolve current directory")?;
    project_gateways_path_from(&cwd)
        .context("not inside a git repository; project gateway configuration requires a repository")
}

pub fn load_project_gateways() -> Result<ProjectGatewaysFile> {
    let cwd = std::env::current_dir().context("resolve current directory")?;
    let Some(path) = project_gateways_path_from(&cwd) else {
        return Ok(ProjectGatewaysFile {
            version: 1,
            gateways: Vec::new(),
        });
    };
    if !path.is_file() {
        return Ok(ProjectGatewaysFile {
            version: 1,
            gateways: Vec::new(),
        });
    }
    let body = std::fs::read_to_string(&path)
        .with_context(|| format!("read project gateways from {}", path.display()))?;
    let parsed: ProjectGatewaysFile =
        serde_yaml::from_str(&body).with_context(|| format!("parse {}", path.display()))?;
    if parsed.version != 1 {
        anyhow::bail!(
            "unsupported project gateway config version {} in {}",
            parsed.version,
            path.display()
        );
    }
    let mut seen = std::collections::HashSet::new();
    for gateway in &parsed.gateways {
        validate_profile_name(&gateway.profile)?;
        if !seen.insert(&gateway.profile) {
            anyhow::bail!(
                "duplicate gateway profile {:?} in {}",
                gateway.profile,
                path.display()
            );
        }
        if gateway.url.trim().is_empty() {
            anyhow::bail!("gateway profile {:?} has an empty url", gateway.profile);
        }
    }
    Ok(parsed)
}

pub fn project_gateway_statuses() -> Result<Vec<ProjectGatewayStatus>> {
    Ok(load_project_gateways()?
        .gateways
        .into_iter()
        .map(|declaration| {
            let result = load_profile(&declaration.profile).and_then(|profile| {
                let url = profile.get("GATEWAY_URL").context("GATEWAY_URL missing")?;
                profile
                    .get("GATEWAY_API_KEY")
                    .context("GATEWAY_API_KEY missing")?;
                if normalize_url(url) != normalize_url(&declaration.url) {
                    anyhow::bail!(
                        "configured URL {url} does not match repository URL {}",
                        declaration.url
                    );
                }
                Ok(())
            });
            ProjectGatewayStatus {
                declaration,
                configured: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            }
        })
        .collect())
}

/// Resolve the default gateway plus locally configured repository upstreams.
/// Missing upstream credentials are returned as warnings so read operations can
/// remain useful while setup exposes the incomplete binding.
pub fn resolve_gateways(capability: &str) -> Result<(Vec<ResolvedGateway>, Vec<String>)> {
    let cfg = load_config();
    let mut gateways = Vec::new();
    if let (Some(url), Some(api_key)) = (cfg.gateway.url, cfg.gateway.api_key) {
        gateways.push(ResolvedGateway {
            profile: "default".into(),
            url,
            api_key,
            timeout_ms: cfg.gateway.timeout_ms.unwrap_or(5000),
            primary: true,
            read: default_read_capabilities(),
        });
    }
    let mut warnings = Vec::new();
    for declaration in load_project_gateways()?.gateways {
        if !declaration.read.iter().any(|item| item == capability) {
            continue;
        }
        match load_profile(&declaration.profile) {
            Ok(profile) => {
                let Some(url) = profile.get("GATEWAY_URL").cloned() else {
                    warnings.push(format!(
                        "project gateway {:?} is missing GATEWAY_URL",
                        declaration.profile
                    ));
                    continue;
                };
                let Some(api_key) = profile.get("GATEWAY_API_KEY").cloned() else {
                    warnings.push(format!(
                        "project gateway {:?} needs credentials; run `agent-tools setup gateway`",
                        declaration.profile
                    ));
                    continue;
                };
                if normalize_url(&url) != normalize_url(&declaration.url) {
                    warnings.push(format!(
                        "project gateway {:?} URL mismatch (local {url}, repository {})",
                        declaration.profile, declaration.url
                    ));
                    continue;
                }
                let timeout_ms = profile
                    .get("GATEWAY_TIMEOUT_MS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5000);
                gateways.push(ResolvedGateway {
                    profile: declaration.profile,
                    url,
                    api_key,
                    timeout_ms,
                    primary: false,
                    read: declaration.read,
                });
            }
            Err(_) => warnings.push(format!(
                "project gateway {:?} needs credentials; run `agent-tools setup gateway`",
                declaration.profile
            )),
        }
    }
    if gateways.is_empty() {
        anyhow::bail!("gateway is not configured -- run `agent-tools setup gateway`");
    }
    Ok((gateways, warnings))
}

fn load_profile(profile: &str) -> Result<HashMap<String, String>> {
    let path = gateway_profile_path(profile)?;
    read_key_value_file(&path).with_context(|| format!("profile {} is not configured", profile))
}

fn validate_profile_name(profile: &str) -> Result<()> {
    if profile == "default"
        || profile.is_empty()
        || !profile
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        anyhow::bail!("invalid gateway profile {profile:?}; use letters, numbers, '-' or '_'");
    }
    Ok(())
}

fn normalize_url(url: &str) -> &str {
    url.trim_end_matches('/')
}

// -- Config loading -----------------------------------------------------------

/// Load configuration from all layers and return the merged result.
///
/// Resolution order (later wins):
/// 1. Global gateway.conf (`/opt/agentic/agent-tools/gateway.conf`)
/// 2. User gateway.conf (`~/.agentic/agent-tools/gateway.conf`)
/// 3. Environment variables -- override everything
pub fn load_config() -> Config {
    let mut cfg = Config::default();

    // Layer 1: system-wide global gateway.conf
    if let Some(pairs) = read_key_value_file(&global_gateway_conf_path()) {
        apply_key_value_pairs(&mut cfg, &pairs);
    }

    // Layer 2: per-user override gateway.conf (overwrites any global values)
    if let Some(pairs) = read_key_value_file(&user_gateway_conf_path()) {
        apply_key_value_pairs(&mut cfg, &pairs);
    }

    // Layer 3: environment variables (highest priority)
    apply_env_overrides(&mut cfg);

    cfg
}

/// Apply values from a KEY=VALUE map onto the config. Any key present in the
/// map overwrites the corresponding config field.
fn apply_key_value_pairs(cfg: &mut Config, pairs: &HashMap<String, String>) {
    if let Some(v) = pairs.get("GATEWAY_URL") {
        cfg.gateway.url = Some(v.clone());
    }
    if let Some(v) = pairs.get("GATEWAY_API_KEY") {
        cfg.gateway.api_key = Some(v.clone());
    }
    if let Some(v) = pairs.get("GATEWAY_TIMEOUT_MS") {
        if let Ok(ms) = v.parse::<u64>() {
            cfg.gateway.timeout_ms = Some(ms);
        }
    }
    if let Some(v) = pairs.get("DEFAULT_PROJECT_IDENT") {
        cfg.gateway.default_project = Some(v.clone());
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

// -- Interactive setup --------------------------------------------------------

/// Run an interactive setup wizard that writes `~/.agentic/agent-tools/gateway.conf`.
///
/// Prompts the user for gateway URL, API key, and timeout, then writes the
/// resulting KEY=VALUE file.
///
/// # Errors
/// Returns an error if stdin/stdout interaction fails or the config file cannot
/// be written.
pub fn run_setup_gateway() -> Result<()> {
    let cfg = load_config();
    if cfg.gateway.url.is_none() || cfg.gateway.api_key.is_none() {
        println!("No default gateway is configured.");
        println!();
        return configure_default_gateway();
    }

    print_gateway_status()?;
    println!();
    println!("Gateway actions:");
    println!("  1) Update default gateway");
    println!("  2) Add project-based upstream gateway");
    println!("  3) Configure credentials for a declared project upstream");
    println!("  4) Remove a project upstream");
    println!("  c) Cancel");
    print!("> ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    match input.trim().to_ascii_lowercase().as_str() {
        "1" => configure_default_gateway(),
        "2" => run_add_project_gateway(None),
        "3" => configure_declared_gateway(),
        "4" => run_remove_project_gateway(None, false),
        "" | "c" | "cancel" => {
            println!("Cancelled — nothing changed.");
            Ok(())
        }
        other => anyhow::bail!("unknown gateway action {other:?}"),
    }
}

fn configure_default_gateway() -> Result<()> {
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
    let api_key = api_key.trim();
    // Reject empty + any character that would later blow up HeaderValue parsing
    // (newlines, NBSP from a paste, etc.) so we never write a broken key to disk.
    crate::sanitize::validate_api_key(api_key).map_err(anyhow::Error::msg)?;

    // Timeout
    write!(out, "Request timeout in ms [5000]: ")?;
    out.flush()?;
    let mut timeout_input = String::new();
    reader.read_line(&mut timeout_input)?;
    let timeout: u64 = timeout_input.trim().parse().unwrap_or(5000);

    // Build KEY=VALUE content
    let mut content = String::new();
    content.push_str(&format!("GATEWAY_URL={url}\n"));
    content.push_str(&format!("GATEWAY_API_KEY={api_key}\n"));
    content.push_str(&format!("GATEWAY_TIMEOUT_MS={timeout}\n"));

    // Write the file
    let config_path = user_gateway_conf_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    std::fs::write(&config_path, &content)
        .with_context(|| format!("write config to {}", config_path.display()))?;

    writeln!(out)?;
    writeln!(out, "Gateway config written to {}", config_path.display())?;
    writeln!(out)?;
    writeln!(
        out,
        "To register the MCP server, add to your Claude config:"
    )?;
    writeln!(out, "  {{")?;
    writeln!(out, "    \"mcpServers\": {{")?;
    writeln!(out, "      \"agent-tools\": {{")?;
    writeln!(
        out,
        "        \"command\": \"/opt/agentic/bin/agent-tools-mcp\""
    )?;
    writeln!(out, "      }}")?;
    writeln!(out, "    }}")?;
    writeln!(out, "  }}")?;

    Ok(())
}

pub fn print_gateway_status() -> Result<()> {
    let cfg = load_config();
    println!("Gateway configuration");
    println!();
    match cfg.gateway.url {
        Some(url) if cfg.gateway.api_key.is_some() => {
            println!("Default:\n  default — {url} [configured]")
        }
        _ => println!("Default:\n  not configured"),
    }
    println!();
    println!("Project upstreams:");
    let statuses = project_gateway_statuses()?;
    if statuses.is_empty() {
        println!("  (none declared in .agents/alternate-gateways.yml)");
    } else {
        for status in statuses {
            let state = if status.configured {
                "configured"
            } else {
                "needs credentials"
            };
            println!(
                "  {} — {} [{state}]",
                status.declaration.profile, status.declaration.url
            );
        }
    }
    Ok(())
}

pub fn run_add_project_gateway(profile_override: Option<&str>) -> Result<()> {
    let profile = match profile_override {
        Some(profile) => profile.trim().to_string(),
        None => prompt_line("Profile name (for example prod-sre): ", None)?,
    };
    validate_profile_name(&profile)?;
    let url = prompt_line("Gateway URL: ", None)?;
    if url.trim().is_empty() {
        anyhow::bail!("gateway URL cannot be empty");
    }
    let api_key =
        rpassword::prompt_password("Gateway API key: ").context("failed to read API key")?;
    crate::sanitize::validate_api_key(api_key.trim()).map_err(anyhow::Error::msg)?;
    let timeout = prompt_line("Request timeout in ms [5000]: ", Some("5000"))?;
    let timeout_ms: u64 = timeout.parse().context("timeout must be an integer")?;
    write_profile(&profile, &url, api_key.trim(), timeout_ms)?;

    let mut project = load_project_gateways()?;
    let declaration = ProjectGateway {
        profile: profile.clone(),
        url: url.clone(),
        read: default_read_capabilities(),
    };
    if let Some(existing) = project.gateways.iter_mut().find(|g| g.profile == profile) {
        *existing = declaration;
    } else {
        project.gateways.push(declaration);
    }
    write_project_gateways(&project)?;
    println!("Configured project upstream {profile} at {url}.");
    println!(
        "Repository declaration: {}",
        project_gateways_path()?.display()
    );
    println!(
        "Local credentials: {}",
        gateway_profile_path(&profile)?.display()
    );
    Ok(())
}

fn configure_declared_gateway() -> Result<()> {
    let missing: Vec<_> = project_gateway_statuses()?
        .into_iter()
        .filter(|s| !s.configured)
        .collect();
    if missing.is_empty() {
        println!("All declared project gateways are configured.");
        return Ok(());
    }
    println!("Declared gateways needing credentials:");
    for (index, status) in missing.iter().enumerate() {
        println!(
            "  {}) {} — {}",
            index + 1,
            status.declaration.profile,
            status.declaration.url
        );
    }
    let selected = prompt_line("Select gateway: ", None)?
        .parse::<usize>()
        .context("selection must be a number")?;
    let status = missing
        .get(selected.saturating_sub(1))
        .context("selection out of range")?;
    let api_key =
        rpassword::prompt_password("Gateway API key: ").context("failed to read API key")?;
    crate::sanitize::validate_api_key(api_key.trim()).map_err(anyhow::Error::msg)?;
    let timeout = prompt_line("Request timeout in ms [5000]: ", Some("5000"))?;
    let timeout_ms: u64 = timeout.parse().context("timeout must be an integer")?;
    write_profile(
        &status.declaration.profile,
        &status.declaration.url,
        api_key.trim(),
        timeout_ms,
    )?;
    println!("Configured credentials for {}.", status.declaration.profile);
    Ok(())
}

pub fn run_remove_project_gateway(
    profile_override: Option<&str>,
    credentials_only: bool,
) -> Result<()> {
    let mut project = load_project_gateways()?;
    if project.gateways.is_empty() {
        println!("No project upstream gateways are declared.");
        return Ok(());
    }
    let profile = match profile_override {
        Some(profile) => profile.to_string(),
        None => {
            for (index, gateway) in project.gateways.iter().enumerate() {
                println!("  {}) {} — {}", index + 1, gateway.profile, gateway.url);
            }
            let selected = prompt_line("Select gateway to remove: ", None)?
                .parse::<usize>()
                .context("selection must be a number")?;
            project
                .gateways
                .get(selected.saturating_sub(1))
                .context("selection out of range")?
                .profile
                .clone()
        }
    };
    validate_profile_name(&profile)?;
    let path = gateway_profile_path(&profile)?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    if !credentials_only {
        project
            .gateways
            .retain(|gateway| gateway.profile != profile);
        write_project_gateways(&project)?;
        println!("Removed project upstream {profile} and its local credentials.");
    } else {
        println!("Removed local credentials for {profile}; repository declaration remains.");
    }
    Ok(())
}

fn write_profile(profile: &str, url: &str, api_key: &str, timeout_ms: u64) -> Result<()> {
    let path = gateway_profile_path(profile)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        "GATEWAY_URL={}\nGATEWAY_API_KEY={}\nGATEWAY_TIMEOUT_MS={}\n",
        url.trim_end_matches('/'),
        api_key,
        timeout_ms
    );
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))
}

fn write_project_gateways(project: &ProjectGatewaysFile) -> Result<()> {
    let path = project_gateways_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_yaml::to_string(project).context("serialize project gateway configuration")?;
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))
}

fn prompt_line(prompt: &str, default: Option<&str>) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().lock().read_line(&mut value)?;
    let value = value.trim();
    if value.is_empty() {
        return default.map(str::to_string).context("a value is required");
    }
    Ok(value.to_string())
}

/// Backwards-compatible alias for [`run_setup_gateway`].
///
/// # Errors
/// Delegates to `run_setup_gateway`; see its documentation for error conditions.
pub fn run_init() -> Result<()> {
    run_setup_gateway()
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
    fn user_gateway_conf_path_ends_correctly() {
        let p = user_gateway_conf_path();
        assert!(p.ends_with(".agentic/agent-tools/gateway.conf"));
    }

    #[test]
    fn global_gateway_conf_path_is_absolute() {
        let p = global_gateway_conf_path();
        assert_eq!(p, PathBuf::from("/opt/agentic/agent-tools/gateway.conf"));
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
    fn apply_key_value_pairs_overwrites() {
        let mut cfg = Config {
            gateway: GatewayConfig {
                url: Some("http://base".into()),
                api_key: Some("key-base".into()),
                timeout_ms: Some(1000),
                default_project: None,
            },
        };
        let mut pairs = HashMap::new();
        pairs.insert("GATEWAY_URL".into(), "http://overlay".into());
        pairs.insert("DEFAULT_PROJECT_IDENT".into(), "proj".into());
        apply_key_value_pairs(&mut cfg, &pairs);
        assert_eq!(cfg.gateway.url.as_deref(), Some("http://overlay"));
        assert_eq!(cfg.gateway.api_key.as_deref(), Some("key-base"));
        assert_eq!(cfg.gateway.timeout_ms, Some(1000));
        assert_eq!(cfg.gateway.default_project.as_deref(), Some("proj"));
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

    #[test]
    fn project_gateway_yaml_defaults_read_capabilities() {
        let parsed: ProjectGatewaysFile = serde_yaml::from_str(
            "version: 1\ngateways:\n  - profile: prod-sre\n    url: https://gateway.example\n",
        )
        .unwrap();
        assert_eq!(parsed.gateways[0].read, vec!["tasks", "patterns", "docs"]);
    }

    #[test]
    fn project_gateway_path_stops_at_git_root() {
        let root =
            std::env::temp_dir().join(format!("agent-tools-gateway-root-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            project_gateways_path_from(&nested).unwrap(),
            root.join(".agents").join("alternate-gateways.yml")
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn profile_names_cannot_escape_profile_directory() {
        assert!(validate_profile_name("prod-sre").is_ok());
        assert!(validate_profile_name("../prod").is_err());
        assert!(validate_profile_name("default").is_err());
    }
}
