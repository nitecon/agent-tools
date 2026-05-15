use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Resolved per-invocation gateway context shared by gateway-backed commands.
///
/// `ident` is the short, gateway-friendly slug sent to the server.
/// `canonical_ident` is the full normalized identifier (git remote URL or
/// canonical path) used to key local state like the registration marker, so two
/// different repositories with the same basename cannot clobber each other.
pub(crate) struct GatewayContext {
    pub(crate) ident: String,
    pub(crate) canonical_ident: String,
    pub(crate) agent_id: String,
    pub(crate) gateway: GatewayClient,
    pub(crate) gateway_url: String,
}

pub(crate) fn resolve_context(agent_id_override: Option<String>) -> Result<GatewayContext> {
    let canonical_ident =
        agent_core::project_ident_from_cwd().context("derive project ident from cwd")?;
    let ident = short_project_ident(&canonical_ident);
    if ident.is_empty() {
        anyhow::bail!(
            "could not derive a short project ident from {canonical_ident:?}; \
             pass --project-ident or set DEFAULT_PROJECT_IDENT in gateway.conf"
        );
    }

    let agent_id = match agent_id_override {
        Some(id) => id,
        None => load_or_generate_agent_id()?,
    };

    let config = load_config();
    let gateway_url = config
        .gateway
        .url
        .clone()
        .context("gateway URL not configured -- run `agent-tools setup gateway`")?;
    let api_key = config
        .gateway
        .api_key
        .clone()
        .context("gateway API key not configured -- run `agent-tools setup gateway`")?;
    let timeout_ms = config.gateway.timeout_ms.unwrap_or(5000);

    let gateway = GatewayClient::new(gateway_url.clone(), api_key, timeout_ms)?;

    Ok(GatewayContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
        gateway_url,
    })
}

/// Register the project with the gateway if we haven't already for this URL.
/// Returns the channel name, either cached or freshly registered.
pub(crate) async fn ensure_registered(
    ctx: &GatewayContext,
    channel_override: Option<&str>,
) -> Result<String> {
    if let Some(channel_name) = read_registration_marker(&ctx.canonical_ident, &ctx.gateway_url) {
        return Ok(channel_name);
    }
    let resp = ctx
        .gateway
        .register_project(&ctx.ident, channel_override)
        .await
        .context("register project with gateway")?;
    write_registration_marker(&ctx.canonical_ident, &ctx.gateway_url, &resp.channel_name)?;
    Ok(resp.channel_name)
}

/// Marker file that records which (project, gateway) pair has been registered.
/// Stored centrally so every cwd within a project shares the same state.
pub(crate) fn registration_marker_path(ident: &str) -> PathBuf {
    let hash = agent_core::hash_project_ident(ident);
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("registered")
        .join(hash)
}

/// Return Some(channel_name) if this (ident, gateway_url) has been registered.
pub(crate) fn read_registration_marker(ident: &str, gateway_url: &str) -> Option<String> {
    let path = registration_marker_path(ident);
    let content = std::fs::read_to_string(&path).ok()?;
    let mut url = None;
    let mut channel = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("GATEWAY_URL=") {
            url = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("CHANNEL_NAME=") {
            channel = Some(v.to_string());
        }
    }
    if url.as_deref() == Some(gateway_url) {
        channel
    } else {
        None
    }
}

pub(crate) fn write_registration_marker(
    ident: &str,
    gateway_url: &str,
    channel_name: &str,
) -> Result<()> {
    let path = registration_marker_path(ident);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    let body = format!("GATEWAY_URL={gateway_url}\nCHANNEL_NAME={channel_name}\n");
    std::fs::write(&path, body)
        .with_context(|| format!("write registration marker {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_path_has_stable_shape() {
        let p = registration_marker_path("github.com/foo/bar.git");
        let file = p.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(file.len(), 64);
        let parent = p.parent().unwrap();
        assert!(parent.ends_with(PathBuf::from(".agentic/agent-tools/registered")));
    }

    #[test]
    fn marker_round_trips() {
        let ident = format!("test-ident-{}", std::process::id());
        let url = "http://localhost:0";
        let path = registration_marker_path(&ident);
        let _ = std::fs::remove_file(&path);

        assert_eq!(read_registration_marker(&ident, url), None);

        write_registration_marker(&ident, url, "agent-test-channel").unwrap();
        assert_eq!(
            read_registration_marker(&ident, url),
            Some("agent-test-channel".to_string())
        );

        assert_eq!(read_registration_marker(&ident, "http://other"), None);

        let _ = std::fs::remove_file(&path);
    }
}
