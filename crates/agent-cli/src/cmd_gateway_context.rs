use agent_comms::config::{home_dir, resolve_gateways};
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
    pub(crate) gateways: Vec<GatewayTarget>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) struct GatewayTarget {
    pub(crate) profile: String,
    pub(crate) gateway: GatewayClient,
    pub(crate) gateway_url: String,
    pub(crate) primary: bool,
}

pub(crate) fn resolve_context(agent_id_override: Option<String>) -> Result<GatewayContext> {
    resolve_context_for("tasks", agent_id_override)
}

pub(crate) fn resolve_context_for(
    capability: &str,
    agent_id_override: Option<String>,
) -> Result<GatewayContext> {
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

    let (resolved, warnings) = resolve_gateways(capability)?;
    let mut gateways = Vec::new();
    for item in resolved {
        gateways.push(GatewayTarget {
            profile: item.profile,
            gateway: GatewayClient::new(item.url.clone(), item.api_key, item.timeout_ms)?,
            gateway_url: item.url,
            primary: item.primary,
        });
    }
    let primary = gateways
        .iter()
        .find(|item| item.primary)
        .unwrap_or(&gateways[0]);
    let gateway = primary.gateway.clone();
    let gateway_url = primary.gateway_url.clone();

    Ok(GatewayContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
        gateway_url,
        gateways,
        warnings,
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

pub(crate) async fn ensure_all_registered(ctx: &GatewayContext) -> Result<()> {
    for target in &ctx.gateways {
        if read_registration_marker(&ctx.canonical_ident, &target.gateway_url).is_some() {
            continue;
        }
        let response = match target.gateway.register_project(&ctx.ident, None).await {
            Ok(response) => response,
            Err(error) if !target.primary => {
                eprintln!(
                    "warning: gateway {} could not register project: {error:#}",
                    target.profile
                );
                continue;
            }
            Err(error) => return Err(error).context("register project with default gateway"),
        };
        write_registration_marker(
            &ctx.canonical_ident,
            &target.gateway_url,
            &response.channel_name,
        )?;
    }
    Ok(())
}

pub(crate) fn print_gateway_warnings(ctx: &GatewayContext) {
    for warning in &ctx.warnings {
        eprintln!("warning: {warning}");
    }
}

fn registration_marker_for_gateway(ident: &str, gateway_url: &str) -> PathBuf {
    let ident_hash = agent_core::hash_project_ident(ident);
    let gateway_hash = agent_core::hash_project_ident(gateway_url);
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("registered")
        .join(format!("{ident_hash}-{gateway_hash}"))
}

/// Return Some(channel_name) if this (ident, gateway_url) has been registered.
pub(crate) fn read_registration_marker(ident: &str, gateway_url: &str) -> Option<String> {
    let path = registration_marker_for_gateway(ident, gateway_url);
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
    let path = registration_marker_for_gateway(ident, gateway_url);
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
        let p = registration_marker_for_gateway("github.com/foo/bar.git", "https://gateway.test");
        let file = p.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(file.len(), 129);
        assert_eq!(file.chars().filter(|c| *c == '-').count(), 1);
        let parent = p.parent().unwrap();
        assert!(parent.ends_with(PathBuf::from(".agentic/agent-tools/registered")));
    }

    #[test]
    fn marker_round_trips() {
        let ident = format!("test-ident-{}", std::process::id());
        let url = "http://localhost:0";
        let gateway_path = registration_marker_for_gateway(&ident, url);
        let _ = std::fs::remove_file(&gateway_path);

        assert_eq!(read_registration_marker(&ident, url), None);

        write_registration_marker(&ident, url, "agent-test-channel").unwrap();
        assert_eq!(
            read_registration_marker(&ident, url),
            Some("agent-test-channel".to_string())
        );

        assert_eq!(read_registration_marker(&ident, "http://other"), None);

        let _ = std::fs::remove_file(&gateway_path);
    }
}
