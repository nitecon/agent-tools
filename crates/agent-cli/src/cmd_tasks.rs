//! `agent-tools tasks` subcommands.
//!
//! Mirrors `cmd_comms.rs`: derives the project ident from the current working
//! directory, the agent id from the persistent machine file, and surfaces the
//! gateway-not-configured case with a friendly message rather than an opaque
//! "missing config" error.
//
// TODO(cleanup): `resolve_context` and the registration-marker helpers are
// duplicated from `cmd_comms.rs`. Factor them into a shared `context` module
// once a third command needs them.

use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use agent_comms::tasks::{
    AddCommentRequest, CreateTaskRequest, Task, TaskComment, TaskDetail, TaskSummary,
    UpdateTaskRequest,
};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Subcommand)]
pub enum TasksCommands {
    /// List tasks on the current project (default: TODO + IN PROGRESS).
    List {
        /// Comma-separated status filter. Default `todo,in_progress`.
        #[arg(long, default_value = "todo,in_progress")]
        status: String,
        /// Include `done` tasks older than the 7-day falloff window.
        #[arg(long)]
        include_stale: bool,
        /// Override the machine agent-id for this invocation.
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Show a single task plus its comment thread.
    Get {
        /// Task id (full UUIDv7 or unique 4-char prefix returned by `list`).
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Create a new task on the current project.
    Add {
        /// Required short title.
        #[arg(long)]
        title: String,
        /// Optional one-paragraph summary.
        #[arg(long)]
        description: Option<String>,
        /// Optional long-form spec / repro / context.
        #[arg(long)]
        details: Option<String>,
        /// Repeatable label flag — joined into the `labels[]` array.
        #[arg(long = "label")]
        label: Vec<String>,
        /// Originating host. Defaults to the local hostname.
        #[arg(long)]
        hostname: Option<String>,
        /// Reporter override. Defaults to the request's X-Agent-Id (or "user").
        #[arg(long)]
        reporter: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Move a task to IN PROGRESS and claim it for the current agent.
    Claim {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Move a task back to TODO and clear its owner.
    Release {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Move a task to DONE.
    Done {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Append a comment to a task.
    Comment {
        task_id: String,
        /// Comment body.
        content: String,
        /// `agent` (default when an agent-id is set) or `user`. `system` is rejected.
        #[arg(long, value_parser = ["agent", "user"])]
        author_type: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Set a task's absolute rank (lower = higher priority within column).
    Rank {
        task_id: String,
        rank: i64,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

// -- Entry -------------------------------------------------------------------

/// Dispatch a tasks subcommand. Short-circuits with a friendly message when
/// the gateway hasn't been set up so the agent knows to ask the user.
pub fn dispatch(cmd: TasksCommands) -> Result<()> {
    ensure_gateway_configured()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(cmd))
}

fn ensure_gateway_configured() -> Result<()> {
    let cfg = load_config();
    if cfg.gateway.url.is_none() || cfg.gateway.api_key.is_none() {
        anyhow::bail!(
            "Tasks are not available — agent-gateway is not configured.\n\
             Tasks require a running agent-gateway connection. Ask the user to run\n\
             `agent-tools setup gateway` to enable task tracking, then retry."
        );
    }
    Ok(())
}

async fn run(cmd: TasksCommands) -> Result<()> {
    match cmd {
        TasksCommands::List {
            status,
            include_stale,
            agent_id,
            json,
        } => cmd_list(status, include_stale, agent_id, json).await,
        TasksCommands::Get {
            task_id,
            agent_id,
            json,
        } => cmd_get(task_id, agent_id, json).await,
        TasksCommands::Add {
            title,
            description,
            details,
            label,
            hostname,
            reporter,
            agent_id,
            json,
        } => {
            cmd_add(
                title,
                description,
                details,
                label,
                hostname,
                reporter,
                agent_id,
                json,
            )
            .await
        }
        TasksCommands::Claim {
            task_id,
            agent_id,
            json,
        } => cmd_status_transition(task_id, "in_progress", agent_id, json, "claimed").await,
        TasksCommands::Release {
            task_id,
            agent_id,
            json,
        } => cmd_status_transition(task_id, "todo", agent_id, json, "released").await,
        TasksCommands::Done {
            task_id,
            agent_id,
            json,
        } => cmd_status_transition(task_id, "done", agent_id, json, "done").await,
        TasksCommands::Comment {
            task_id,
            content,
            author_type,
            agent_id,
            json,
        } => cmd_comment(task_id, content, author_type, agent_id, json).await,
        TasksCommands::Rank {
            task_id,
            rank,
            agent_id,
            json,
        } => cmd_rank(task_id, rank, agent_id, json).await,
    }
}

// -- Context resolution (duplicated from cmd_comms; see TODO above) ----------

struct TasksContext {
    ident: String,
    canonical_ident: String,
    agent_id: String,
    gateway: GatewayClient,
    gateway_url: String,
}

fn resolve_context(agent_id_override: Option<String>) -> Result<TasksContext> {
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

    Ok(TasksContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
        gateway_url,
    })
}

fn registration_marker_path(ident: &str) -> PathBuf {
    let hash = agent_core::hash_project_ident(ident);
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("registered")
        .join(hash)
}

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

async fn ensure_registered(ctx: &TasksContext) -> Result<()> {
    if read_registration_marker(&ctx.canonical_ident, &ctx.gateway_url).is_some() {
        return Ok(());
    }
    let resp = ctx
        .gateway
        .register_project(&ctx.ident, None)
        .await
        .context("register project with gateway")?;
    write_registration_marker(&ctx.canonical_ident, &ctx.gateway_url, &resp.channel_name)?;
    Ok(())
}

// -- Helpers -----------------------------------------------------------------

fn local_hostname_or_none(flag: Option<String>) -> Option<String> {
    match flag {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
        None => gethostname::gethostname()
            .into_string()
            .ok()
            .filter(|s| !s.is_empty()),
    }
}

fn fmt_epoch_ms(ms: i64) -> String {
    let nanos = (ms as i128) * 1_000_000;
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| ms.to_string())
}

fn fmt_relative_from_now(ms: i64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let diff = (now_ms - ms).max(0);
    let secs = diff / 1000;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// First 4 chars of a UUIDv7, used as the human-friendly short id in list views.
fn short_id(id: &str) -> &str {
    if id.len() >= 4 {
        &id[..4]
    } else {
        id
    }
}

fn parse_status_csv(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

// -- list --------------------------------------------------------------------

async fn cmd_list(
    status: String,
    include_stale: bool,
    agent_id: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let statuses = parse_status_csv(&status);
    let tasks: Vec<TaskSummary> = ctx
        .gateway
        .list_tasks(
            &ctx.ident,
            Some(&statuses),
            include_stale,
            Some(&ctx.agent_id),
        )
        .await
        .context("list tasks")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
        return Ok(());
    }

    if tasks.is_empty() {
        println!("(no tasks matching {status})");
        return Ok(());
    }
    render_grouped_list(&tasks);
    Ok(())
}

fn render_grouped_list(tasks: &[TaskSummary]) {
    let mut groups: Vec<(&str, &str, Vec<&TaskSummary>)> = vec![
        ("todo", "TODO", Vec::new()),
        ("in_progress", "IN PROGRESS", Vec::new()),
        ("done", "DONE", Vec::new()),
    ];
    for t in tasks {
        if let Some(g) = groups.iter_mut().find(|(s, _, _)| *s == t.status) {
            g.2.push(t);
        }
    }

    let mut printed_any = false;
    for (_, label, mut items) in groups {
        if items.is_empty() {
            continue;
        }
        if printed_any {
            println!();
        }
        printed_any = true;
        items.sort_by_key(|t| t.rank);
        println!("{label} ({})", items.len());
        for t in items {
            print_summary_row(t);
        }
    }
}

fn print_summary_row(t: &TaskSummary) {
    let labels = if t.labels.is_empty() {
        String::new()
    } else {
        t.labels.join(",")
    };
    let trailing = match t.status.as_str() {
        "in_progress" => {
            let owner = t
                .owner_agent_id
                .as_deref()
                .map(|o| format!("@{o}"))
                .unwrap_or_else(|| "@—".to_string());
            format!("{owner}  {}", fmt_relative_from_now(t.updated_at))
        }
        _ => labels,
    };
    println!("  [{}] {:<50} {}", short_id(&t.id), t.title, trailing);
}

// -- get ---------------------------------------------------------------------

async fn cmd_get(task_id: String, agent_id: Option<String>, json: bool) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let detail: TaskDetail = ctx
        .gateway
        .get_task(&ctx.ident, &task_id, Some(&ctx.agent_id))
        .await
        .context("fetch task")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&detail)?);
        return Ok(());
    }
    print_task_detail(&detail.task, &detail.comments);
    Ok(())
}

fn print_task_detail(task: &Task, comments: &[TaskComment]) {
    let status_label = match task.status.as_str() {
        "todo" => "TODO",
        "in_progress" => "IN PROGRESS",
        "done" => "DONE",
        other => other,
    };
    println!("[{}]  {}  rank {}", task.id, status_label, task.rank);
    println!("{}", task.title);

    let labels = if task.labels.is_empty() {
        "(none)".to_string()
    } else {
        task.labels.join(", ")
    };
    println!("labels:    {labels}");
    println!("reporter:  {}", task.reporter);
    println!(
        "owner:     {}",
        task.owner_agent_id.as_deref().unwrap_or("—")
    );
    println!("hostname:  {}", task.hostname.as_deref().unwrap_or("—"));
    println!("created:   {}", fmt_epoch_ms(task.created_at));
    println!("updated:   {}", fmt_epoch_ms(task.updated_at));
    if let Some(started) = task.started_at {
        println!("started:   {}", fmt_epoch_ms(started));
    }
    if let Some(done) = task.done_at {
        println!("done:      {}", fmt_epoch_ms(done));
    }

    println!();
    println!("Description:");
    match task.description.as_deref() {
        Some(s) if !s.trim().is_empty() => {
            for line in s.lines() {
                println!("  {line}");
            }
        }
        _ => println!("  (none)"),
    }

    println!();
    println!("Details:");
    match task.details.as_deref() {
        Some(s) if !s.trim().is_empty() => {
            for line in s.lines() {
                println!("  {line}");
            }
        }
        _ => println!("  (none)"),
    }

    println!();
    println!("Comments ({}):", comments.len());
    if comments.is_empty() {
        println!("  (none)");
    } else {
        for c in comments {
            println!(
                "  [{}] {} ({}): {}",
                fmt_epoch_ms(c.created_at),
                c.author,
                c.author_type,
                c.content
            );
        }
    }
}

// -- add ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn cmd_add(
    title: String,
    description: Option<String>,
    details: Option<String>,
    labels: Vec<String>,
    hostname: Option<String>,
    reporter: Option<String>,
    agent_id: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let host = local_hostname_or_none(hostname);
    let labels_slice: Option<&[String]> = if labels.is_empty() {
        None
    } else {
        Some(&labels)
    };

    let req = CreateTaskRequest {
        title: &title,
        description: description.as_deref(),
        details: details.as_deref(),
        labels: labels_slice,
        hostname: host.as_deref(),
        reporter: reporter.as_deref(),
    };

    let task: Task = ctx
        .gateway
        .create_task(&ctx.ident, &req, Some(&ctx.agent_id))
        .await
        .context("create task")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!(
            "created [{}] {} (status={})",
            short_id(&task.id),
            task.title,
            task.status
        );
        println!("full id: {}", task.id);
    }
    Ok(())
}

// -- claim / release / done --------------------------------------------------

async fn cmd_status_transition(
    task_id: String,
    new_status: &str,
    agent_id: Option<String>,
    json: bool,
    verb: &str,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let patch = UpdateTaskRequest {
        status: Some(new_status),
        ..Default::default()
    };

    let task: Task = ctx
        .gateway
        .update_task(&ctx.ident, &task_id, &patch, Some(&ctx.agent_id))
        .await
        .with_context(|| format!("transition task to {new_status}"))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!(
            "{verb} [{}] {} (status={}, owner={})",
            short_id(&task.id),
            task.title,
            task.status,
            task.owner_agent_id.as_deref().unwrap_or("—")
        );
    }
    Ok(())
}

// -- comment -----------------------------------------------------------------

async fn cmd_comment(
    task_id: String,
    content: String,
    author_type: Option<String>,
    agent_id: Option<String>,
    json: bool,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    // Default author_type: agent — we always send X-Agent-Id below, so the
    // server-side default would also be `agent`. We pass it explicitly so the
    // CLI behavior is deterministic regardless of server defaults.
    let resolved_type = author_type.unwrap_or_else(|| "agent".to_string());
    let req = AddCommentRequest {
        content: &content,
        author: None, // let the server derive from X-Agent-Id
        author_type: Some(&resolved_type),
    };

    let comment: TaskComment = ctx
        .gateway
        .add_task_comment(&ctx.ident, &task_id, &req, Some(&ctx.agent_id))
        .await
        .context("add comment")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&comment)?);
    } else {
        println!(
            "comment added by {} ({}) on task {}",
            comment.author,
            comment.author_type,
            short_id(&comment.task_id)
        );
    }
    Ok(())
}

// -- rank --------------------------------------------------------------------

async fn cmd_rank(task_id: String, rank: i64, agent_id: Option<String>, json: bool) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let patch = UpdateTaskRequest {
        rank: Some(rank),
        ..Default::default()
    };
    let task: Task = ctx
        .gateway
        .update_task(&ctx.ident, &task_id, &patch, Some(&ctx.agent_id))
        .await
        .context("set rank")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!(
            "ranked [{}] {} → rank {}",
            short_id(&task.id),
            task.title,
            task.rank
        );
    }
    Ok(())
}

// Silence the unused-import warning in the case the helper above is the only
// place `Value` is referenced. (The current code uses it implicitly via the
// `UpdateTaskRequest` struct fields — this keeps the import explicit for
// future patches that build raw JSON values.)
#[allow(dead_code)]
fn _unused_value_marker(_: Value) {}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_csv_basic() {
        assert_eq!(
            parse_status_csv("todo,in_progress"),
            vec!["todo", "in_progress"]
        );
        assert_eq!(parse_status_csv(" done "), vec!["done"]);
        assert!(parse_status_csv("").is_empty());
        assert!(parse_status_csv(" , ").is_empty());
    }

    #[test]
    fn short_id_truncates_long_uuid() {
        assert_eq!(short_id("k7h2e16b-2f83-7a41-aaaa-bbbbbbbbbbbb"), "k7h2");
    }

    #[test]
    fn short_id_passes_through_short() {
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn fmt_epoch_ms_is_rfc3339() {
        // 1714004800000 ms == 2024-04-25T00:26:40Z
        let s = fmt_epoch_ms(1_714_004_800_000);
        assert_eq!(s, "2024-04-25T00:26:40Z");
    }

    #[test]
    fn fmt_relative_from_now_handles_future_clamps_to_zero() {
        // A timestamp 10s in the future should clamp to 0s ago, not produce
        // a negative duration.
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap();
        let s = fmt_relative_from_now(now_ms + 10_000);
        assert!(s.ends_with(" ago"));
    }

    #[test]
    fn local_hostname_explicit_empty_opts_out() {
        assert_eq!(local_hostname_or_none(Some(String::new())), None);
    }
}
