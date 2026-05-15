//! `agent-tools comms` subcommands.
//!
//! Derives the project ident from the current working directory (via
//! `agent-core::project_ident_from_cwd`) and the agent id from a machine-wide
//! persistent file (`~/.agentic/agent-tools/agent-id`), so agents never need to
//! pass either explicitly. Registration with the gateway is cached per project
//! so repeat sends don't pay a register round-trip.

use crate::cmd_gateway_context::{ensure_registered, resolve_context};
use agent_comms::config::load_config;
use agent_comms::gateway::MessageMeta;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use clap::Subcommand;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Subcommand)]
pub enum CommsCommands {
    /// Send a message to the project channel (auto-derives project ident).
    Send {
        /// Message body. Rendered inside a fenced code block in Discord.
        content: String,
        /// One-line headline for the embed title (defaults to first line of body).
        #[arg(long)]
        subject: Option<String>,
        /// Originating host. Defaults to the local hostname; pass an empty
        /// string to leave the gateway to fall back to the agent-id.
        #[arg(long)]
        hostname: Option<String>,
        /// Event time. Accepts RFC3339 (e.g. `2026-04-21T19:18:00Z`) or epoch
        /// milliseconds. Defaults to gateway receipt time when omitted.
        #[arg(long)]
        event_at: Option<String>,
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
        /// Channel plugin (discord, slack, email); only used on first register.
        #[arg(long)]
        channel: Option<String>,
    },

    /// Fetch unread messages for this project and agent.
    Recv {
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Confirm a message as read.
    Confirm {
        /// Numeric message id returned by `recv`.
        message_id: i64,
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Send a threaded reply to a specific message.
    Reply {
        /// Numeric message id to reply to.
        message_id: i64,
        /// Reply body. Rendered inside a fenced code block in Discord.
        content: String,
        /// One-line headline for the embed title (defaults to first line of body).
        #[arg(long)]
        subject: Option<String>,
        /// Originating host. Defaults to the local hostname; pass an empty
        /// string to leave the gateway to fall back to the agent-id.
        #[arg(long)]
        hostname: Option<String>,
        /// Event time. Accepts RFC3339 or epoch milliseconds. Defaults to
        /// gateway receipt time when omitted.
        #[arg(long)]
        event_at: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Signal that this agent is actively working on a message.
    Action {
        /// Numeric message id being acted on.
        message_id: i64,
        /// Brief description of the action being taken (used as the body).
        message: String,
        /// One-line headline for the embed title. Auto-derived subjects are
        /// prefixed with `[ACTION] ` server-side; supply your own to override.
        #[arg(long)]
        subject: Option<String>,
        /// Originating host. Defaults to the local hostname; pass an empty
        /// string to leave the gateway to fall back to the agent-id.
        #[arg(long)]
        hostname: Option<String>,
        /// Event time. Accepts RFC3339 or epoch milliseconds. Defaults to
        /// gateway receipt time when omitted.
        #[arg(long)]
        event_at: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Print derived project ident + agent-id (debug / verification).
    Whoami,
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
        CommsCommands::Whoami => cmd_whoami(),
        CommsCommands::Send {
            content,
            subject,
            hostname,
            event_at,
            agent_id,
            channel,
        } => {
            let meta = ResolvedMeta::from_args(subject, hostname, event_at)?;
            cmd_send(content, meta, agent_id, channel).await
        }
        CommsCommands::Recv { agent_id } => cmd_recv(agent_id).await,
        CommsCommands::Confirm {
            message_id,
            agent_id,
        } => cmd_confirm(message_id, agent_id).await,
        CommsCommands::Reply {
            message_id,
            content,
            subject,
            hostname,
            event_at,
            agent_id,
        } => {
            let meta = ResolvedMeta::from_args(subject, hostname, event_at)?;
            cmd_reply(message_id, content, meta, agent_id).await
        }
        CommsCommands::Action {
            message_id,
            message,
            subject,
            hostname,
            event_at,
            agent_id,
        } => {
            let meta = ResolvedMeta::from_args(subject, hostname, event_at)?;
            cmd_action(message_id, message, meta, agent_id).await
        }
    }
}

// -- Structured-payload helpers ---------------------------------------------

/// Owned counterpart to `agent_comms::gateway::MessageMeta` so the CLI can
/// stage borrowed strings (host auto-detect, parsed event time) for the life
/// of one request before handing them to the gateway client.
struct ResolvedMeta {
    subject: Option<String>,
    hostname: Option<String>,
    event_at_ms: Option<i64>,
}

impl ResolvedMeta {
    fn from_args(
        subject: Option<String>,
        hostname: Option<String>,
        event_at: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            subject: subject.and_then(non_empty),
            hostname: resolve_hostname(hostname),
            event_at_ms: event_at.map(|raw| parse_event_at(&raw)).transpose()?,
        })
    }

    fn as_borrowed(&self) -> MessageMeta<'_> {
        MessageMeta {
            subject: self.subject.as_deref(),
            hostname: self.hostname.as_deref(),
            event_at_ms: self.event_at_ms,
        }
    }
}

/// Auto-fill hostname when the caller didn't supply one. An explicit empty
/// string opts out — the gateway will then fall back to the agent-id.
fn resolve_hostname(flag: Option<String>) -> Option<String> {
    match flag {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
        None => gethostname::gethostname()
            .into_string()
            .ok()
            .and_then(non_empty),
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Parse `--event-at`. Accepts a bare epoch-ms integer or an RFC3339 timestamp
/// (`2026-04-21T19:18:00Z`). Returns epoch milliseconds.
fn parse_event_at(raw: &str) -> Result<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("--event-at value is empty");
    }
    if let Ok(ms) = trimmed.parse::<i64>() {
        return Ok(ms);
    }
    let dt = OffsetDateTime::parse(trimmed, &Rfc3339)
        .with_context(|| format!("parse --event-at {trimmed:?} as RFC3339 or epoch-ms"))?;
    let nanos = dt.unix_timestamp_nanos();
    let ms = nanos / 1_000_000;
    i64::try_from(ms).context("event_at out of range for i64 epoch-ms")
}

// -- whoami ------------------------------------------------------------------

fn cmd_whoami() -> Result<()> {
    let canonical_ident = agent_core::project_ident_from_cwd().context("derive project ident")?;
    let ident = short_project_ident(&canonical_ident);
    let agent_id = load_or_generate_agent_id()?;
    let config = load_config();
    let gateway_url = config.gateway.url.as_deref();
    let configured = gateway_url.is_some() && config.gateway.api_key.is_some();

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
    Ok(())
}

// -- send --------------------------------------------------------------------

async fn cmd_send(
    content: String,
    meta: ResolvedMeta,
    agent_id: Option<String>,
    channel: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, channel.as_deref()).await?;

    let resp = ctx
        .gateway
        .send_message(
            &ctx.ident,
            &content,
            &meta.as_borrowed(),
            Some(&ctx.agent_id),
        )
        .await
        .context("send message")?;

    println!("sent (id={}, ident={})", resp.message_id, ctx.ident);
    Ok(())
}

// -- recv --------------------------------------------------------------------

async fn cmd_recv(agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .get_unread(&ctx.ident, Some(&ctx.agent_id))
        .await
        .context("fetch unread messages")?;

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

async fn cmd_confirm(message_id: i64, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let resp = ctx
        .gateway
        .confirm_read(&ctx.ident, message_id, Some(&ctx.agent_id))
        .await
        .context("confirm message")?;

    if resp.confirmed {
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
    meta: ResolvedMeta,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .reply_to(
            &ctx.ident,
            message_id,
            &content,
            &meta.as_borrowed(),
            Some(&ctx.agent_id),
        )
        .await
        .context("reply to message")?;

    println!(
        "reply sent (id={}, parent={})",
        resp.message_id, resp.parent_message_id
    );
    Ok(())
}

// -- action ------------------------------------------------------------------

async fn cmd_action(
    message_id: i64,
    message: String,
    meta: ResolvedMeta,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx, None).await?;

    let resp = ctx
        .gateway
        .taking_action_on(
            &ctx.ident,
            message_id,
            &message,
            &meta.as_borrowed(),
            Some(&ctx.agent_id),
        )
        .await
        .context("signal action")?;

    println!(
        "action signal sent (id={}, parent={})",
        resp.message_id, resp.parent_message_id
    );
    Ok(())
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_at_accepts_epoch_ms() {
        assert_eq!(parse_event_at("1714000000000").unwrap(), 1_714_000_000_000);
    }

    #[test]
    fn parse_event_at_accepts_rfc3339_utc() {
        // 2024-04-25T00:26:40Z == 1714004800000 ms
        let ms = parse_event_at("2024-04-25T00:26:40Z").unwrap();
        assert_eq!(ms, 1_714_004_800_000);
    }

    #[test]
    fn parse_event_at_accepts_rfc3339_with_offset() {
        // 2024-04-25T02:26:40+02:00 is the same instant as the UTC case above.
        let ms = parse_event_at("2024-04-25T02:26:40+02:00").unwrap();
        assert_eq!(ms, 1_714_004_800_000);
    }

    #[test]
    fn parse_event_at_rejects_garbage() {
        assert!(parse_event_at("not-a-date").is_err());
        assert!(parse_event_at("   ").is_err());
    }

    #[test]
    fn resolve_hostname_explicit_empty_opts_out() {
        assert_eq!(resolve_hostname(Some(String::new())), None);
    }

    #[test]
    fn resolve_hostname_explicit_value_passes_through() {
        assert_eq!(
            resolve_hostname(Some("custom.host".into())),
            Some("custom.host".into())
        );
    }

    #[test]
    fn resolve_hostname_default_is_local_hostname() {
        // We don't assert a specific value (it varies by machine), only that
        // omitting the flag gives us *some* non-empty hostname back.
        let resolved = resolve_hostname(None);
        assert!(resolved.is_some_and(|h| !h.is_empty()));
    }
}
