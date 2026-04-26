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
    AddCommentRequest, CreateTaskRequest, DelegateTaskRequest, Task, TaskComment,
    TaskCreateResponse, TaskDelegationResponse, TaskDetail, TaskSummary, UpdateTaskRequest,
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
    },

    /// Show a single task plus its comment thread.
    Get {
        /// Full task id returned by `list`.
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Create a new task on the current project.
    Add {
        /// Required short title.
        #[arg(long)]
        title: String,
        /// Optional one-paragraph summary.
        #[arg(long)]
        description: Option<String>,
        /// Optional long-form specification / repro / handoff context.
        #[arg(long, alias = "details")]
        specification: Option<String>,
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
    },

    /// Create a delegated task for another project and track it here.
    AddDelegated {
        /// Required target project ident.
        #[arg(long = "target-project")]
        target_project: String,
        /// Required short title.
        #[arg(long)]
        title: String,
        /// Required one-paragraph summary of why the work is needed.
        #[arg(long)]
        description: String,
        /// Required implementation contract / acceptance details for the target project.
        #[arg(long, alias = "details")]
        specification: String,
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
    },

    /// Move a task to IN PROGRESS and claim it for the current agent.
    Claim {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Move a task back to TODO and clear its owner.
    Release {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Move a task to DONE.
    Done {
        task_id: String,
        #[arg(long)]
        agent_id: Option<String>,
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
    },

    /// Set a task's absolute rank (lower = higher priority within column).
    Rank {
        task_id: String,
        rank: i64,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Show Eventic build status for the current project.
    Builds {
        /// Override the repo mapping to inspect.
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
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
        } => cmd_list(status, include_stale, agent_id).await,
        TasksCommands::Get { task_id, agent_id } => cmd_get(task_id, agent_id).await,
        TasksCommands::Add {
            title,
            description,
            specification,
            label,
            hostname,
            reporter,
            agent_id,
        } => {
            cmd_add(
                title,
                description,
                specification,
                label,
                hostname,
                reporter,
                agent_id,
            )
            .await
        }
        TasksCommands::AddDelegated {
            target_project,
            title,
            description,
            specification,
            label,
            hostname,
            reporter,
            agent_id,
        } => {
            cmd_add_delegated(
                target_project,
                title,
                description,
                specification,
                label,
                hostname,
                reporter,
                agent_id,
            )
            .await
        }
        TasksCommands::Claim { task_id, agent_id } => {
            cmd_status_transition(task_id, "in_progress", agent_id, "claimed").await
        }
        TasksCommands::Release { task_id, agent_id } => {
            cmd_status_transition(task_id, "todo", agent_id, "released").await
        }
        TasksCommands::Done { task_id, agent_id } => {
            cmd_status_transition(task_id, "done", agent_id, "done").await
        }
        TasksCommands::Comment {
            task_id,
            content,
            author_type,
            agent_id,
        } => cmd_comment(task_id, content, author_type, agent_id).await,
        TasksCommands::Rank {
            task_id,
            rank,
            agent_id,
        } => cmd_rank(task_id, rank, agent_id).await,
        TasksCommands::Builds { repo, agent_id } => cmd_builds(repo, agent_id).await,
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

fn parse_status_csv(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

fn require_nonempty_flag(flag: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{flag} must not be empty");
    }
    Ok(())
}

fn require_full_task_id(task_id: &str) -> Result<()> {
    let bytes = task_id.as_bytes();
    let looks_like_uuid = bytes.len() == 36
        && bytes[8] == b'-'
        && bytes[13] == b'-'
        && bytes[18] == b'-'
        && bytes[23] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, b)| matches!(idx, 8 | 13 | 18 | 23) || b.is_ascii_hexdigit());

    if !looks_like_uuid {
        anyhow::bail!(
            "short task IDs are no longer supported; run `agent-tools tasks list` and use the full task ID"
        );
    }

    Ok(())
}

// -- list --------------------------------------------------------------------

async fn cmd_list(status: String, include_stale: bool, agent_id: Option<String>) -> Result<()> {
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
    println!("  [{}] {:<50} {}", t.id, t.title, trailing);
}

// -- get ---------------------------------------------------------------------

async fn cmd_get(task_id: String, agent_id: Option<String>) -> Result<()> {
    require_full_task_id(&task_id)?;

    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let detail: TaskDetail = ctx
        .gateway
        .get_task(&ctx.ident, &task_id, Some(&ctx.agent_id))
        .await
        .context("fetch task")?;

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
    println!("Specification:");
    match task.specification_text() {
        Some(s) => {
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
    specification: Option<String>,
    labels: Vec<String>,
    hostname: Option<String>,
    reporter: Option<String>,
    agent_id: Option<String>,
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
        specification: specification.as_deref(),
        details: None,
        labels: labels_slice,
        hostname: host.as_deref(),
        reporter: reporter.as_deref(),
    };

    let response: TaskCreateResponse = ctx
        .gateway
        .create_task(&ctx.ident, &req, Some(&ctx.agent_id))
        .await
        .context("create task")?;
    let task = &response.task;

    println!(
        "created [{}] {} (status={})",
        task.id, task.title, task.status
    );
    if specification
        .as_deref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        println!(
            "hint: evaluate whether this task should include a specification. \
             Specs on gateway-backed tasks are more durable than local plan files \
             and survive full system crashes."
        );
    }
    Ok(())
}

// -- add-delegated -----------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn cmd_add_delegated(
    target_project: String,
    title: String,
    description: String,
    specification: String,
    labels: Vec<String>,
    hostname: Option<String>,
    reporter: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty_flag("--target-project", &target_project)?;
    require_nonempty_flag("--title", &title)?;
    require_nonempty_flag("--description", &description)?;
    require_nonempty_flag("--specification", &specification)?;

    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let host = local_hostname_or_none(hostname);
    let labels_slice: Option<&[String]> = if labels.is_empty() {
        None
    } else {
        Some(&labels)
    };

    let req = DelegateTaskRequest {
        target_project_ident: &target_project,
        title: &title,
        description: &description,
        specification: &specification,
        labels: labels_slice,
        hostname: host.as_deref(),
        reporter: reporter.as_deref(),
    };

    let response: TaskDelegationResponse = ctx
        .gateway
        .delegate_task(&ctx.ident, &req, Some(&ctx.agent_id))
        .await
        .context("create delegated task")?;

    println!(
        "delegated [{}] {} → {} [{}]",
        response.source_task.id,
        response.source_task.title,
        response.delegation.target_project_ident,
        response.target_task.id
    );
    Ok(())
}

// -- claim / release / done --------------------------------------------------

async fn cmd_status_transition(
    task_id: String,
    new_status: &str,
    agent_id: Option<String>,
    verb: &str,
) -> Result<()> {
    require_full_task_id(&task_id)?;

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

    println!(
        "{verb} [{}] {} (status={}, owner={})",
        task.id,
        task.title,
        task.status,
        task.owner_agent_id.as_deref().unwrap_or("—")
    );
    Ok(())
}

// -- comment -----------------------------------------------------------------

async fn cmd_comment(
    task_id: String,
    content: String,
    author_type: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_full_task_id(&task_id)?;

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

    println!(
        "comment added by {} ({}) on task {}",
        comment.author, comment.author_type, comment.task_id
    );
    Ok(())
}

// -- rank --------------------------------------------------------------------

async fn cmd_rank(task_id: String, rank: i64, agent_id: Option<String>) -> Result<()> {
    require_full_task_id(&task_id)?;

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

    println!("ranked [{}] {} → rank {}", task.id, task.title, task.rank);
    Ok(())
}

// -- builds ------------------------------------------------------------------

async fn cmd_builds(repo: Option<String>, agent_id: Option<String>) -> Result<()> {
    if let Some(repo) = repo.as_deref() {
        require_nonempty_flag("--repo", repo)?;
    }

    let ctx = resolve_context(agent_id)?;
    ensure_registered(&ctx).await?;

    let status = ctx
        .gateway
        .get_build_status(&ctx.ident, repo.as_deref(), Some(&ctx.agent_id))
        .await
        .context("fetch Eventic build status")?;

    print_build_status(&ctx.ident, repo.as_deref(), &status);
    Ok(())
}

fn print_build_status(project_ident: &str, repo_override: Option<&str>, status: &Value) {
    println!("Eventic builds for project {project_ident}");
    if let Some(repo) = repo_override {
        println!("repo override: {repo}");
    }

    if !status.is_object() {
        println!("status: {}", render_value(status));
        return;
    }

    print_field(status, "status", &["status", "current_status", "state"]);
    print_field(
        status,
        "eventic server",
        &["eventic_server", "server", "server_url", "eventic_url"],
    );
    print_field(status, "repo", &["repo", "repository", "repo_url"]);
    print_field(status, "ref", &["ref", "ref_name", "branch"]);
    print_field(status, "commit", &["commit", "commit_hash", "sha"]);

    if let Some(mapping) = find_first(status, &["repo_mapping", "mapping", "repository_mapping"]) {
        println!();
        println!("Repo mapping:");
        print_object_summary(
            mapping,
            &[
                "provider",
                "owner",
                "name",
                "repo",
                "repo_url",
                "default_ref",
            ],
        );
    }

    if let Some(current) = find_first(
        status,
        &["current", "current_build", "build", "latest_build"],
    ) {
        println!();
        println!("Current build:");
        print_object_summary(
            current,
            &[
                "status",
                "state",
                "event",
                "hook",
                "ref",
                "ref_name",
                "commit",
                "commit_hash",
                "sha",
                "started_at",
                "updated_at",
                "finished_at",
                "duration_ms",
            ],
        );
    }

    if let Some(output) = find_first(
        status,
        &["latest_output", "output", "log_tail", "latest_log"],
    ) {
        println!();
        println!("Latest output:");
        print_indented_lines(&render_value(output), 2);
    }

    print_array_section(status, "Recent events", &["recent_events", "events"]);
    print_array_section(
        status,
        "Configured hooks",
        &["hooks", "configured_hooks", "event_rows"],
    );

    if should_print_actionable_config_hint(status) {
        println!();
        println!(
            "hint: add a repo mapping for this project, or tell the user Eventic is not configured. Eventic provides build information."
        );
    } else if let Some(hint) = find_first(status, &["hint", "message"]) {
        println!();
        println!("hint: {}", render_value(hint));
    }
}

fn print_field(root: &Value, label: &str, keys: &[&str]) {
    if let Some(value) = find_first(root, keys) {
        println!("{label}: {}", render_value(value));
    }
}

fn print_object_summary(value: &Value, preferred_keys: &[&str]) {
    if let Some(obj) = value.as_object() {
        let mut printed = false;
        for key in preferred_keys {
            if let Some(v) = obj.get(*key).filter(|v| !v.is_null()) {
                println!("  {key}: {}", render_value(v));
                printed = true;
            }
        }
        if !printed {
            println!("  {}", render_value(value));
        }
    } else {
        println!("  {}", render_value(value));
    }
}

fn print_array_section(root: &Value, title: &str, keys: &[&str]) {
    let Some(value) = find_first(root, keys) else {
        return;
    };
    let Some(items) = value.as_array() else {
        return;
    };
    if items.is_empty() {
        return;
    }

    println!();
    println!("{title}:");
    for item in items.iter().take(10) {
        println!("  - {}", render_compact_row(item));
    }
    if items.len() > 10 {
        println!("  ... {} more", items.len() - 10);
    }
}

fn print_indented_lines(text: &str, indent: usize) {
    let pad = " ".repeat(indent);
    for line in text.lines().take(40) {
        println!("{pad}{line}");
    }
}

fn render_compact_row(value: &Value) -> String {
    if let Some(obj) = value.as_object() {
        let keys = [
            "at",
            "created_at",
            "updated_at",
            "event",
            "hook",
            "status",
            "state",
            "ref",
            "ref_name",
            "commit",
            "commit_hash",
            "sha",
            "message",
        ];
        let parts: Vec<String> = keys
            .iter()
            .filter_map(|key| obj.get(*key).map(|v| format!("{key}={}", render_value(v))))
            .collect();
        if !parts.is_empty() {
            return parts.join("  ");
        }
    }
    render_value(value)
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_string())
        }
    }
}

fn find_first<'a>(root: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let obj = root.as_object()?;
    keys.iter()
        .filter_map(|key| obj.get(*key))
        .find(|value| !value.is_null())
}

fn should_print_actionable_config_hint(status: &Value) -> bool {
    let explicit_false = [
        "configured",
        "eventic_configured",
        "repo_mapped",
        "mapping_configured",
    ]
    .iter()
    .any(|key| {
        find_first(status, &[*key])
            .and_then(Value::as_bool)
            .map(|value| !value)
            .unwrap_or(false)
    });
    if explicit_false {
        return true;
    }

    ["status", "hint", "message", "error"].iter().any(|key| {
        find_first(status, &[*key])
            .and_then(Value::as_str)
            .map(|s| {
                let s = s.to_ascii_lowercase();
                s.contains("not configured")
                    || s.contains("no repo")
                    || s.contains("no mapping")
                    || s.contains("missing repo")
                    || s.contains("missing eventic")
            })
            .unwrap_or(false)
    })
}

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
    fn require_nonempty_flag_rejects_blank_values() {
        assert!(require_nonempty_flag("--title", "Useful title").is_ok());
        assert!(require_nonempty_flag("--title", "   ").is_err());
    }

    #[test]
    fn require_full_task_id_accepts_uuid() {
        assert!(require_full_task_id("019dbaf9-2527-7782-9b19-a7a2289bdb4e").is_ok());
    }

    #[test]
    fn require_full_task_id_rejects_short_prefix() {
        let err = require_full_task_id("019d").unwrap_err().to_string();
        assert!(err.contains("short task IDs are no longer supported"));
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
