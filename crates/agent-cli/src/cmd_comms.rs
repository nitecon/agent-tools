//! `agent-tools comms` subcommands.
//!
//! Derives the project ident from the current working directory (via
//! `agent-core::project_ident_from_cwd`) and the agent id from a machine-wide
//! persistent file (`~/.agentic/agent-tools/agent-id`), so agents never need to
//! pass either explicitly. Registration with the gateway is cached per project
//! so repeat sends don't pay a register round-trip.

use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum CommsCommands {
    /// Send a message to the project channel (auto-derives project ident).
    Send {
        /// Message content.
        content: String,
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
        /// Channel plugin (discord, slack, email); only used on first register.
        #[arg(long)]
        channel: Option<String>,
        /// Emit JSON response instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Fetch unread messages for this project and agent.
    Recv {
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Confirm a message as read.
    Confirm {
        /// Numeric message id returned by `recv`.
        message_id: i64,
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Send a threaded reply to a specific message.
    Reply {
        /// Numeric message id to reply to.
        message_id: i64,
        /// Reply content.
        content: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Signal that this agent is actively working on a message.
    Action {
        /// Numeric message id being acted on.
        message_id: i64,
        /// Brief description of the action being taken.
        message: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Print derived project ident + agent-id (debug / verification).
    Whoami {
        #[arg(long)]
        json: bool,
    },
}

// -- Entry -------------------------------------------------------------------

/// Dispatch a comms subcommand. Builds a short-lived tokio runtime so the
/// non-comms paths of the CLI stay sync and cold-start fast.
pub fn dispatch(cmd: CommsCommands) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(cmd))
}

async fn run(cmd: CommsCommands) -> Result<()> {
    match cmd {
        CommsCommands::Whoami { json } => cmd_whoami(json),
        CommsCommands::Send {
            content,
            agent_id,
            channel,
            json,
        } => cmd_send(content, agent_id, channel, json).await,
        CommsCommands::Recv { agent_id, json } => cmd_recv(agent_id, json).await,
        CommsCommands::Confirm {
            message_id,
            agent_id,
            json,
        } => cmd_confirm(message_id, agent_id, json).await,
        CommsCommands::Reply {
            message_id,
            content,
            agent_id,
            json,
        } => cmd_reply(message_id, content, agent_id, json).await,
        CommsCommands::Action {
            message_id,
            message,
            agent_id,
            json,
        } => cmd_action(message_id, message, agent_id, json).await,
    }
}

// -- Context resolution ------------------------------------------------------

/// Resolved per-invocation context shared by every comms subcommand.
///
/// `ident` is the short, gateway-friendly slug (e.g. `eventic`) sent to the
/// server. `canonical_ident` is the full normalized identifier (git remote URL
/// or canonical path) used to key local state like the registration marker,
/// so two different repositories that share a basename cannot clobber each
/// other's cached registration.
struct CommsContext {
    ident: String,
    canonical_ident: String,
    agent_id: String,
    gateway: GatewayClient,
    gateway_url: String,
}

/// Resolve cwd -> ident, load-or-generate agent-id, build `GatewayClient`.
fn resolve_context(agent_id_override: Option<String>) -> Result<CommsContext> {
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

    Ok(CommsContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
        gateway_url,
    })
}

// -- Registration cache ------------------------------------------------------

/// Marker file that records which (project, gateway) pair has been registered.
/// Stored centrally so every cwd within a project shares the same state.
fn registration_marker_path(ident: &str) -> PathBuf {
    // Hash-of-ident keyed, same concept the symbol indexer uses for its data
    // dir. We stash the marker under the user's agentic dir rather than the
    // project data dir so it survives an `agent-tools index --rebuild`.
    let hash = agent_core::hash_project_ident(ident);
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("registered")
        .join(hash)
}

/// Return Some(channel_name) if this (ident, gateway_url) has been registered.
fn read_registration_marker(ident: &str, gateway_url: &str) -> Option<String> {
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

fn write_registration_marker(ident: &str, gateway_url: &str, channel_name: &str) -> Result<()> {
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

/// Register the project with the gateway if we haven't already for this URL.
/// Returns the channel name (cached or freshly registered).
///
/// The marker is keyed on the canonical ident (git URL / full path) so two
/// different repositories that collapse to the same short slug still get
/// distinct registration state.
async fn ensure_registered(ctx: &CommsContext, channel_override: Option<&str>) -> Result<String> {
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

// -- whoami ------------------------------------------------------------------

#[derive(Serialize)]
struct WhoamiOutput<'a> {
    project_ident: &'a str,
    canonical_ident: &'a str,
    agent_id: &'a str,
    gateway_url: Option<&'a str>,
    gateway_configured: bool,
}

fn cmd_whoami(json: bool) -> Result<()> {
    let canonical_ident = agent_core::project_ident_from_cwd().context("derive project ident")?;
    let ident = short_project_ident(&canonical_ident);
    let agent_id = load_or_generate_agent_id()?;
    let config = load_config();
    let gateway_url = config.gateway.url.as_deref();
    let configured = gateway_url.is_some() && config.gateway.api_key.is_some();

    if json {
        let out = WhoamiOutput {
            project_ident: &ident,
            canonical_ident: &canonical_ident,
            agent_id: &agent_id,
            gateway_url,
            gateway_configured: configured,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("project_ident:   {ident}");
        println!("canonical_ident: {canonical_ident}");
        println!("agent_id:        {agent_id}");
        match gateway_url {
            Some(u) => println!("gateway_url:     {u}"),
            None => println!("gateway_url:     (not configured)"),
        }
        println!(
            "gateway:         {}",
            if configured {
                "configured"
            } else {
                "NOT configured -- run `agent-tools setup gateway`"
            }
        );
    }
    Ok(())
}

// -- send --------------------------------------------------------------------

async fn cmd_send(
    content: String,
    agent_id: Option<String>,
    channel: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, channel.as_deref()).await?;

    let resp = ctx
        .gateway
        .send_message(&ctx.ident, &content, Some(&ctx.agent_id))
        .await
        .context("send message")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("sent (id={}, ident={})", resp.message_id, ctx.ident);
    }
    Ok(())
}

// -- recv --------------------------------------------------------------------

async fn cmd_recv(agent_id: Option<String>, json: bool) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .get_unread(&ctx.ident, Some(&ctx.agent_id))
        .await
        .context("fetch unread messages")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    if resp.messages.is_empty() {
        println!("no messages");
        return Ok(());
    }

    for m in &resp.messages {
        let source_tag = match (m.source.as_str(), m.agent_id.as_deref()) {
            ("agent", Some(aid)) => format!("[AGENT:{aid}]"),
            ("agent", None) => "[AGENT]".to_string(),
            _ => "[USER]".to_string(),
        };
        let type_tag = match m.message_type.as_deref() {
            Some("reply") => " [REPLY]",
            Some("action") => " [ACTION]",
            _ => "",
        };
        let parent = m
            .parent_message_id
            .map(|pid| format!(" (re: msg {pid})"))
            .unwrap_or_default();
        println!(
            "(id={}) {}{}{} {}",
            m.id, source_tag, type_tag, parent, m.content
        );
    }
    println!();
    println!(
        "Confirm each handled message with: agent-tools comms confirm <id>\n\
         Unconfirmed messages will reappear on the next recv."
    );
    Ok(())
}

// -- confirm -----------------------------------------------------------------

async fn cmd_confirm(message_id: i64, agent_id: Option<String>, json: bool) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let resp = ctx
        .gateway
        .confirm_read(&ctx.ident, message_id, Some(&ctx.agent_id))
        .await
        .context("confirm message")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.confirmed {
        println!("confirmed (id={message_id})");
    } else {
        println!("already confirmed or not found (id={message_id})");
    }
    Ok(())
}

// -- reply -------------------------------------------------------------------

async fn cmd_reply(
    message_id: i64,
    content: String,
    agent_id: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .reply_to(&ctx.ident, message_id, &content, Some(&ctx.agent_id))
        .await
        .context("reply to message")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "reply sent (id={}, parent={})",
            resp.message_id, resp.parent_message_id
        );
    }
    Ok(())
}

// -- action ------------------------------------------------------------------

async fn cmd_action(
    message_id: i64,
    message: String,
    agent_id: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .taking_action_on(&ctx.ident, message_id, &message, Some(&ctx.agent_id))
        .await
        .context("signal action")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "action signal sent (id={}, parent={})",
            resp.message_id, resp.parent_message_id
        );
    }
    Ok(())
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn marker_path_has_stable_shape() {
        let p = registration_marker_path("github.com/foo/bar.git");
        let file = p.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(file.len(), 64); // blake3 hex
        let parent = p.parent().unwrap();
        assert!(parent.ends_with(PathBuf::from(".agentic/agent-tools/registered")));
    }

    #[test]
    fn marker_round_trips() {
        // Use a unique ident so tests can run in parallel without stepping on
        // each other's marker files.
        let ident = format!("test-ident-{}", std::process::id());
        let url = "http://localhost:0";
        let path = registration_marker_path(&ident);
        // Ensure clean slate.
        let _ = std::fs::remove_file(&path);

        assert_eq!(read_registration_marker(&ident, url), None);

        write_registration_marker(&ident, url, "agent-test-channel").unwrap();
        assert_eq!(
            read_registration_marker(&ident, url),
            Some("agent-test-channel".to_string())
        );

        // Different URL -> miss.
        assert_eq!(read_registration_marker(&ident, "http://other"), None);

        // Cleanup.
        let _ = std::fs::remove_file(&path);
    }
}
