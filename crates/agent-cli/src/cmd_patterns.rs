//! `agent-tools patterns` subcommands.

use agent_comms::config::{home_dir, load_config};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::patterns::{
    AddPatternCommentRequest, CreatePatternRequest, Pattern, PatternComment, PatternFilters,
    PatternSummary, UpdatePatternRequest,
};
use agent_comms::sanitize::short_project_ident;
use agent_comms::tasks::{CreateTaskRequest, TaskSummary};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum PatternsCommands {
    /// List latest active patterns.
    List {
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, default_value = "latest")]
        version: String,
        #[arg(long, default_value = "active")]
        state: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Search the global pattern library.
    Search {
        query: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long = "superseded-by")]
        superseded_by: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch one pattern by gateway id or slug. Comments are not fetched.
    Get {
        id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Create a pattern from a markdown file.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long = "category")]
        category: Vec<String>,
        #[arg(long, default_value = "draft")]
        version: String,
        #[arg(long, default_value = "active")]
        state: String,
        #[arg(long = "body-file")]
        body_file: PathBuf,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Update pattern metadata or markdown body.
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long = "category")]
        category: Vec<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Delete a pattern.
    Delete {
        id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch comments for a pattern.
    Comments {
        id: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Add a comment to a pattern.
    Comment {
        id: String,
        content: String,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Publish one pattern file or a markdown catalog into the gateway.
    Publish {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long = "category")]
        category: Vec<String>,
        #[arg(long, default_value = "latest")]
        version: String,
        #[arg(long, default_value = "active")]
        state: String,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Validate a JSON/YAML pattern file or markdown pattern catalog.
    Validate {
        #[arg(long)]
        file: PathBuf,
    },

    /// Render the pattern publish payload that would be sent to the gateway.
    Preview {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long = "category")]
        category: Vec<String>,
        #[arg(long, default_value = "latest")]
        version: String,
        #[arg(long, default_value = "active")]
        state: String,
        #[arg(long)]
        author: Option<String>,
    },

    /// Validate the current directory's `.patterns` file.
    Check {
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Validate a pattern and add its canonical gateway id to `.patterns`.
    Use {
        id: String,
        #[arg(long = "path")]
        path: Vec<PathBuf>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

struct PatternsContext {
    ident: String,
    canonical_ident: String,
    agent_id: String,
    gateway: GatewayClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternUsage {
    id: String,
    paths: Vec<String>,
}

#[derive(Debug, Clone)]
struct PatternCheck {
    usage: PatternUsage,
    pattern: Pattern,
    replacement: Option<String>,
    task_created: Option<String>,
    task_existing: Option<String>,
}

struct CreatePatternArgs {
    title: String,
    slug: Option<String>,
    summary: Option<String>,
    labels: Vec<String>,
    categories: Vec<String>,
    version: String,
    state: String,
    body_file: PathBuf,
    agent_id: Option<String>,
}

struct UpdatePatternArgs {
    id: String,
    title: Option<String>,
    slug: Option<String>,
    summary: Option<String>,
    labels: Vec<String>,
    categories: Vec<String>,
    version: Option<String>,
    state: Option<String>,
    body_file: Option<PathBuf>,
    agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatternFile {
    title: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    body: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatternCatalogFile {
    patterns: Vec<PatternFile>,
}

#[derive(Debug, Clone, Default)]
struct PublishPatternOverrides {
    title: Option<String>,
    slug: Option<String>,
    summary: Option<String>,
    labels: Vec<String>,
    categories: Vec<String>,
    version: String,
    state: String,
    author: Option<String>,
}

pub fn dispatch(cmd: PatternsCommands) -> Result<()> {
    if !matches!(
        cmd,
        PatternsCommands::Validate { .. } | PatternsCommands::Preview { .. }
    ) {
        ensure_gateway_configured()?;
    }
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
            "Patterns are not available — agent-gateway is not configured.\n\
             Patterns require a running agent-gateway connection. Ask the user to run\n\
             `agent-tools setup gateway` to enable pattern guidance, then retry."
        );
    }
    Ok(())
}

async fn run(cmd: PatternsCommands) -> Result<()> {
    match cmd {
        PatternsCommands::List {
            label,
            category,
            version,
            state,
            agent_id,
        } => {
            cmd_search(
                None,
                label,
                category,
                Some(version),
                Some(state),
                None,
                agent_id,
            )
            .await
        }
        PatternsCommands::Search {
            query,
            label,
            category,
            version,
            state,
            superseded_by,
            agent_id,
        } => {
            cmd_search(
                Some(query),
                label,
                category,
                version,
                state,
                superseded_by,
                agent_id,
            )
            .await
        }
        PatternsCommands::Get { id, agent_id } => cmd_get(id, agent_id).await,
        PatternsCommands::Create {
            title,
            slug,
            summary,
            label,
            category,
            version,
            state,
            body_file,
            agent_id,
        } => {
            cmd_create(CreatePatternArgs {
                title,
                slug,
                summary,
                labels: label,
                categories: category,
                version,
                state,
                body_file,
                agent_id,
            })
            .await
        }
        PatternsCommands::Update {
            id,
            title,
            slug,
            summary,
            label,
            category,
            version,
            state,
            body_file,
            agent_id,
        } => {
            cmd_update(UpdatePatternArgs {
                id,
                title,
                slug,
                summary,
                labels: label,
                categories: category,
                version,
                state,
                body_file,
                agent_id,
            })
            .await
        }
        PatternsCommands::Delete { id, agent_id } => cmd_delete(id, agent_id).await,
        PatternsCommands::Comments { id, agent_id } => cmd_comments(id, agent_id).await,
        PatternsCommands::Comment {
            id,
            content,
            agent_id,
        } => cmd_comment(id, content, agent_id).await,
        PatternsCommands::Publish {
            file,
            title,
            slug,
            summary,
            label,
            category,
            version,
            state,
            author,
            agent_id,
        } => {
            cmd_publish(
                file,
                PublishPatternOverrides {
                    title,
                    slug,
                    summary,
                    labels: label,
                    categories: category,
                    version,
                    state,
                    author,
                },
                agent_id,
            )
            .await
        }
        PatternsCommands::Validate { file } => cmd_validate(file),
        PatternsCommands::Preview {
            file,
            title,
            slug,
            summary,
            label,
            category,
            version,
            state,
            author,
        } => cmd_preview(
            file,
            PublishPatternOverrides {
                title,
                slug,
                summary,
                labels: label,
                categories: category,
                version,
                state,
                author,
            },
        ),
        PatternsCommands::Check { agent_id } => cmd_check(agent_id).await,
        PatternsCommands::Use { id, path, agent_id } => cmd_use(id, path, agent_id).await,
    }
}

async fn cmd_search(
    query: Option<String>,
    label: Option<String>,
    category: Option<String>,
    version: Option<String>,
    state: Option<String>,
    superseded_by: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let request_label_storage = category.as_deref().map(category_marker);
    let request_label = label.as_deref().or(request_label_storage.as_deref());
    let filters = PatternFilters {
        query: query.as_deref(),
        label: request_label,
        category: category.as_deref(),
        version: version.as_deref(),
        state: state.as_deref(),
        superseded_by: superseded_by.as_deref(),
    };
    let mut patterns = ctx
        .gateway
        .list_patterns(&filters, Some(&ctx.agent_id))
        .await
        .context("list patterns")?;
    if let Some(category) = category.as_deref() {
        patterns.retain(|pattern| summary_matches_category(pattern, category));
    }
    if patterns.is_empty() {
        println!("(no patterns)");
    } else {
        for pattern in patterns {
            print_summary_row(&pattern);
        }
    }
    Ok(())
}

async fn cmd_get(id: String, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let pattern = ctx
        .gateway
        .get_pattern(&id, Some(&ctx.agent_id))
        .await
        .context("fetch pattern")?;
    print_pattern(&pattern);
    Ok(())
}

async fn cmd_create(args: CreatePatternArgs) -> Result<()> {
    let ctx = resolve_context(args.agent_id)?;
    let body = fs::read_to_string(&args.body_file)
        .with_context(|| format!("read body file {}", args.body_file.display()))?;
    let merged_labels = labels_with_category_markers(&args.labels, &args.categories);
    let labels_slice = labels_if_any(&merged_labels);
    let categories_slice = labels_if_any(&args.categories);
    let req = CreatePatternRequest {
        title: &args.title,
        slug: args.slug.as_deref(),
        summary: args.summary.as_deref(),
        body: &body,
        labels: labels_slice,
        categories: categories_slice,
        version: &args.version,
        state: &args.state,
        author: &ctx.agent_id,
    };
    let pattern = ctx
        .gateway
        .create_pattern(&req, Some(&ctx.agent_id))
        .await
        .context("create pattern")?;
    println!(
        "created pattern {} ({}, version={}, state={})",
        pattern.id, pattern.slug, pattern.version, pattern.state
    );
    Ok(())
}

async fn cmd_update(args: UpdatePatternArgs) -> Result<()> {
    let ctx = resolve_context(args.agent_id)?;
    let body = match args.body_file {
        Some(path) => Some(
            fs::read_to_string(&path)
                .with_context(|| format!("read body file {}", path.display()))?,
        ),
        None => None,
    };
    let merged_labels = labels_with_category_markers(&args.labels, &args.categories);
    let labels_slice = labels_if_any(&merged_labels);
    let categories_slice = labels_if_any(&args.categories);
    let req = UpdatePatternRequest {
        title: args.title.as_deref(),
        slug: args.slug.as_deref(),
        summary: args.summary.as_deref(),
        body: body.as_deref(),
        labels: labels_slice,
        categories: categories_slice,
        version: args.version.as_deref(),
        state: args.state.as_deref(),
    };
    let pattern = ctx
        .gateway
        .update_pattern(&args.id, &req, Some(&ctx.agent_id))
        .await
        .context("update pattern")?;
    println!(
        "updated pattern {} ({}, version={}, state={})",
        pattern.id, pattern.slug, pattern.version, pattern.state
    );
    Ok(())
}

async fn cmd_delete(id: String, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    ctx.gateway
        .delete_pattern(&id, Some(&ctx.agent_id))
        .await
        .context("delete pattern")?;
    println!("deleted pattern {id}");
    Ok(())
}

async fn cmd_comments(id: String, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let comments = ctx
        .gateway
        .list_pattern_comments(&id, Some(&ctx.agent_id))
        .await
        .context("list pattern comments")?;
    if comments.is_empty() {
        println!("(no comments)");
    } else {
        for comment in comments {
            print_comment(&comment);
        }
    }
    Ok(())
}

async fn cmd_comment(id: String, content: String, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let req = AddPatternCommentRequest {
        content: &content,
        author: &ctx.agent_id,
        author_type: "agent",
    };
    let comment = ctx
        .gateway
        .add_pattern_comment(&id, &req, Some(&ctx.agent_id))
        .await
        .context("add pattern comment")?;
    println!("comment added to pattern {}", comment.pattern_id);
    Ok(())
}

async fn cmd_publish(
    file: PathBuf,
    overrides: PublishPatternOverrides,
    agent_id: Option<String>,
) -> Result<()> {
    let mut patterns = load_and_prepare_patterns(&file, overrides)?;
    validate_pattern_files(&patterns)?;

    let ctx = resolve_context(agent_id)?;
    for pattern in &mut patterns {
        if pattern.author.is_none() {
            pattern.author = Some(ctx.agent_id.clone());
        }
    }

    let mut created = 0usize;
    let mut updated = 0usize;
    for pattern in patterns {
        let merged_labels = labels_with_category_markers(&pattern.labels, &pattern.categories);
        let labels = labels_if_any(&merged_labels);
        let categories = labels_if_any(&pattern.categories);
        if let Some(slug) = pattern.slug.as_deref() {
            if let Ok(existing) = ctx.gateway.get_pattern(slug, Some(&ctx.agent_id)).await {
                let req = UpdatePatternRequest {
                    title: Some(&pattern.title),
                    slug: Some(slug),
                    summary: pattern.summary.as_deref(),
                    body: Some(&pattern.body),
                    labels,
                    categories,
                    version: pattern.version.as_deref(),
                    state: pattern.state.as_deref(),
                };
                let saved = ctx
                    .gateway
                    .update_pattern(&existing.id, &req, Some(&ctx.agent_id))
                    .await
                    .with_context(|| format!("update pattern {slug}"))?;
                println!("updated pattern {} ({})", saved.id, saved.slug);
                updated += 1;
                continue;
            }
        }

        let version = pattern.version.as_deref().unwrap_or("latest");
        let state = pattern.state.as_deref().unwrap_or("active");
        let author = pattern.author.as_deref().unwrap_or(&ctx.agent_id);
        let req = CreatePatternRequest {
            title: &pattern.title,
            slug: pattern.slug.as_deref(),
            summary: pattern.summary.as_deref(),
            body: &pattern.body,
            labels,
            categories,
            version,
            state,
            author,
        };
        let saved = ctx
            .gateway
            .create_pattern(&req, Some(&ctx.agent_id))
            .await
            .with_context(|| format!("create pattern {}", pattern.title))?;
        println!("created pattern {} ({})", saved.id, saved.slug);
        created += 1;
    }

    println!("published patterns: created={created}, updated={updated}");
    Ok(())
}

fn cmd_validate(file: PathBuf) -> Result<()> {
    let patterns = load_and_prepare_patterns(&file, PublishPatternOverrides::default())?;
    validate_pattern_files(&patterns)?;
    println!("valid pattern publish file: {} pattern(s)", patterns.len());
    Ok(())
}

fn cmd_preview(file: PathBuf, overrides: PublishPatternOverrides) -> Result<()> {
    let patterns = load_and_prepare_patterns(&file, overrides)?;
    validate_pattern_files(&patterns)?;
    println!("{}", serde_json::to_string_pretty(&patterns)?);
    Ok(())
}

async fn cmd_check(agent_id: Option<String>) -> Result<()> {
    let file = patterns_file()?;
    if !file.exists() {
        println!("no .patterns file at {}", file.display());
        return Ok(());
    }
    let ctx = resolve_context(agent_id)?;
    let usages = read_patterns_file(&file)?;
    let checks = check_usages(&ctx, usages).await?;
    render_checks_text(&checks);
    Ok(())
}

async fn cmd_use(id: String, paths: Vec<PathBuf>, agent_id: Option<String>) -> Result<()> {
    let ctx = resolve_context(agent_id)?;
    let pattern = ctx
        .gateway
        .get_pattern(&id, Some(&ctx.agent_id))
        .await
        .context("fetch pattern")?;
    let file = patterns_file()?;
    let mut usages = if file.exists() {
        read_patterns_file(&file)?
    } else {
        Vec::new()
    };
    let path_strings: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    upsert_usage(&mut usages, &pattern.id, &path_strings);
    write_patterns_file(&file, &usages)?;

    println!("recorded pattern {} in {}", pattern.id, file.display());
    if !path_strings.is_empty() {
        println!("paths: {}", path_strings.join(", "));
    }
    Ok(())
}

async fn check_usages(
    ctx: &PatternsContext,
    usages: Vec<PatternUsage>,
) -> Result<Vec<PatternCheck>> {
    let mut checks = Vec::new();
    for usage in usages {
        let pattern = ctx
            .gateway
            .get_pattern(&usage.id, Some(&ctx.agent_id))
            .await
            .with_context(|| format!("fetch pattern {}", usage.id))?;
        let replacement = superseded_replacement(&pattern);
        let mut task_created = None;
        let mut task_existing = None;
        if let Some(next) = replacement.as_deref() {
            match ensure_superseded_task(ctx, &pattern, next, &usage).await? {
                SupersededTask::Created(id) => task_created = Some(id),
                SupersededTask::Existing(id) => task_existing = Some(id),
            }
        }
        checks.push(PatternCheck {
            usage,
            pattern,
            replacement,
            task_created,
            task_existing,
        });
    }
    Ok(checks)
}

enum SupersededTask {
    Created(String),
    Existing(String),
}

async fn ensure_superseded_task(
    ctx: &PatternsContext,
    pattern: &Pattern,
    replacement: &str,
    usage: &PatternUsage,
) -> Result<SupersededTask> {
    ensure_registered(ctx).await?;
    let title = format!("Migrate pattern {} to {}", pattern.id, replacement);
    let statuses = ["todo", "in_progress"];
    let existing: Vec<TaskSummary> = ctx
        .gateway
        .list_tasks(&ctx.ident, Some(&statuses), false, Some(&ctx.agent_id))
        .await
        .context("list tasks before creating superseded-pattern task")?;
    if let Some(task) = existing.iter().find(|task| task.title == title) {
        return Ok(SupersededTask::Existing(task.id.clone()));
    }

    let path_text = if usage.paths.is_empty() {
        "No paths are recorded in .patterns.".to_string()
    } else {
        format!("Recorded paths:\n{}", usage.paths.join("\n"))
    };
    let details = format!(
        "Pattern `{}` ({}) is superseded by `{}`.\n\n{}",
        pattern.id, pattern.title, replacement, path_text
    );
    let labels = vec!["patterns".to_string(), "migration".to_string()];
    let hostname = local_hostname();
    let req = CreateTaskRequest {
        title: &title,
        description: Some("Update repository usage from a superseded pattern to its replacement."),
        specification: Some(&details),
        details: None,
        labels: Some(&labels),
        hostname: hostname.as_deref(),
        reporter: Some(&ctx.agent_id),
    };
    let response = ctx
        .gateway
        .create_task(&ctx.ident, &req, Some(&ctx.agent_id))
        .await
        .context("create superseded-pattern migration task")?;
    Ok(SupersededTask::Created(response.task.id))
}

fn resolve_context(agent_id_override: Option<String>) -> Result<PatternsContext> {
    let canonical_ident =
        agent_core::project_ident_from_cwd().context("derive project ident from cwd")?;
    let ident = short_project_ident(&canonical_ident);
    if ident.is_empty() {
        anyhow::bail!(
            "could not derive a short project ident from {canonical_ident:?}; \
             set a git origin or run from a stable project directory"
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
    let gateway = GatewayClient::new(gateway_url, api_key, timeout_ms)?;
    Ok(PatternsContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
    })
}

async fn ensure_registered(ctx: &PatternsContext) -> Result<()> {
    let marker = registration_marker_path(&ctx.canonical_ident);
    if read_registration_marker(&marker).as_deref() == Some(&ctx.ident) {
        return Ok(());
    }
    ctx.gateway
        .register_project(&ctx.ident, None)
        .await
        .context("register project")?;
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&marker, &ctx.ident).ok();
    Ok(())
}

fn load_and_prepare_patterns(
    path: &Path,
    overrides: PublishPatternOverrides,
) -> Result<Vec<PatternFile>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut patterns = parse_publish_patterns_file(path, &text)?;
    apply_publish_overrides(&mut patterns, overrides)?;
    Ok(patterns)
}

fn parse_publish_patterns_file(path: &Path, text: &str) -> Result<Vec<PatternFile>> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => parse_markdown_pattern_catalog(text),
        "json" => parse_structured_patterns_file(path, text, |s| {
            serde_json::from_str::<serde_json::Value>(s).context("parse JSON pattern file")
        }),
        "yaml" | "yml" | "" => parse_structured_patterns_file(path, text, |s| {
            serde_yaml::from_str::<serde_json::Value>(s).context("parse YAML pattern file")
        }),
        other => {
            anyhow::bail!("unsupported pattern file extension .{other}; use MD, JSON, YAML, or YML")
        }
    }
}

fn parse_structured_patterns_file<F>(path: &Path, text: &str, parse: F) -> Result<Vec<PatternFile>>
where
    F: FnOnce(&str) -> Result<serde_json::Value>,
{
    let value = parse(text).with_context(|| format!("parse {}", path.display()))?;
    if value.is_array() {
        return serde_json::from_value(value).with_context(|| format!("decode {}", path.display()));
    }
    if value.get("patterns").is_some() {
        let catalog: PatternCatalogFile =
            serde_json::from_value(value).with_context(|| format!("decode {}", path.display()))?;
        return Ok(catalog.patterns);
    }
    let pattern: PatternFile =
        serde_json::from_value(value).with_context(|| format!("decode {}", path.display()))?;
    Ok(vec![pattern])
}

fn apply_publish_overrides(
    patterns: &mut [PatternFile],
    overrides: PublishPatternOverrides,
) -> Result<()> {
    let has_singleton_override =
        overrides.title.is_some() || overrides.slug.is_some() || overrides.summary.is_some();
    if has_singleton_override && patterns.len() != 1 {
        anyhow::bail!("--title, --slug, and --summary can only override single-pattern files");
    }
    for pattern in patterns {
        if let Some(title) = overrides.title.clone() {
            pattern.title = title;
        }
        if let Some(slug) = overrides.slug.clone() {
            pattern.slug = Some(slug);
        }
        if overrides.summary.is_some() {
            pattern.summary = overrides.summary.clone();
        }
        if !overrides.labels.is_empty() {
            pattern.labels = overrides.labels.clone();
        }
        if !overrides.categories.is_empty() {
            pattern.categories = overrides.categories.clone();
        }
        if pattern.version.is_none() {
            pattern.version = Some(if overrides.version.is_empty() {
                "latest".to_string()
            } else {
                overrides.version.clone()
            });
        }
        if pattern.state.is_none() {
            pattern.state = Some(if overrides.state.is_empty() {
                "active".to_string()
            } else {
                overrides.state.clone()
            });
        }
        if pattern.author.is_none() {
            pattern.author = overrides.author.clone();
        }
    }
    Ok(())
}

fn validate_pattern_files(patterns: &[PatternFile]) -> Result<()> {
    if patterns.is_empty() {
        anyhow::bail!("pattern file does not contain any publishable patterns");
    }
    let mut slugs = BTreeSet::new();
    for (idx, pattern) in patterns.iter().enumerate() {
        let label = format!("patterns[{}]", idx + 1);
        require_nonempty(&format!("{label}.title"), &pattern.title)?;
        require_nonempty(&format!("{label}.body"), &pattern.body)?;
        if let Some(slug) = pattern.slug.as_deref() {
            require_nonempty(&format!("{label}.slug"), slug)?;
            if !slugs.insert(slug.to_string()) {
                anyhow::bail!("duplicate pattern slug {slug}");
            }
        }
        if let Some(version) = pattern.version.as_deref() {
            require_nonempty(&format!("{label}.version"), version)?;
        }
        if let Some(state) = pattern.state.as_deref() {
            require_nonempty(&format!("{label}.state"), state)?;
        }
    }
    Ok(())
}

fn require_nonempty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(())
}

fn parse_markdown_pattern_catalog(text: &str) -> Result<Vec<PatternFile>> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut fence: Option<char> = None;
    let mut starts = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        update_markdown_fence(trimmed, &mut fence);
        if fence.is_some() {
            continue;
        }
        if markdown_heading_level(trimmed) == 2 {
            starts.push(idx);
        }
    }

    starts.push(lines.len());
    for pair in starts.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let section = lines[start..end].join("\n");
        let Some(slug) = pattern_slug_from_section(&section) else {
            continue;
        };
        let title = markdown_heading_text(lines[start].trim_start(), 2);
        sections.push(PatternFile {
            title,
            slug: Some(slug.clone()),
            summary: scope_summary_from_section(&section),
            body: ensure_trailing_newline(section.trim_end()),
            labels: inferred_pattern_labels(&slug),
            categories: inferred_pattern_categories(&slug),
            version: Some("latest".to_string()),
            state: Some("active".to_string()),
            author: None,
        });
    }

    Ok(sections)
}

fn pattern_slug_from_section(section: &str) -> Option<String> {
    section.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("<!-- pattern-slug:")?;
        let slug = rest.strip_suffix("-->")?.trim();
        if slug.is_empty() || slug.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(slug.to_string())
        }
    })
}

fn scope_summary_from_section(section: &str) -> Option<String> {
    let mut in_scope = false;
    let mut parts = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if markdown_heading_level(trimmed) == 3 {
            if in_scope {
                break;
            }
            in_scope = markdown_heading_text(trimmed, 3).eq_ignore_ascii_case("Scope");
            continue;
        }
        if in_scope {
            if trimmed.is_empty() {
                if !parts.is_empty() {
                    break;
                }
                continue;
            }
            parts.push(trimmed.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn inferred_pattern_labels(slug: &str) -> Vec<String> {
    if slug.starts_with("go-") {
        vec!["go".to_string()]
    } else if slug.starts_with("ndesign-") {
        vec!["ndesign".to_string()]
    } else if slug.starts_with("fullstack-") {
        vec!["fullstack".to_string()]
    } else {
        Vec::new()
    }
}

fn inferred_pattern_categories(slug: &str) -> Vec<String> {
    if slug.starts_with("go-") {
        vec!["programming-language/golang".to_string()]
    } else if slug.starts_with("ndesign-") {
        vec!["frontend/ndesign".to_string()]
    } else if slug.starts_with("fullstack-") {
        vec!["application-architecture/fullstack".to_string()]
    } else {
        vec!["general".to_string()]
    }
}

fn ensure_trailing_newline(value: &str) -> String {
    let mut out = value.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn update_markdown_fence(trimmed: &str, fence: &mut Option<char>) {
    let Some(ch) = trimmed.chars().next() else {
        return;
    };
    if ch != '`' && ch != '~' {
        return;
    }
    if trimmed.chars().take_while(|c| *c == ch).count() < 3 {
        return;
    }
    match fence {
        Some(open) if *open == ch => *fence = None,
        None => *fence = Some(ch),
        _ => {}
    }
}

fn markdown_heading_level(trimmed: &str) -> usize {
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    let bytes = trimmed.as_bytes();
    if level > 0 && level <= 6 && level < bytes.len() && bytes[level] == b' ' {
        level
    } else {
        0
    }
}

fn markdown_heading_text(trimmed: &str, level: usize) -> String {
    trimmed[level + 1..].trim().to_string()
}

fn registration_marker_path(canonical_ident: &str) -> PathBuf {
    let hash = agent_core::hash_project_ident(canonical_ident);
    home_dir()
        .join(".agent-tools")
        .join(hash)
        .join("gateway-project")
}

fn read_registration_marker(path: &PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn patterns_file() -> Result<PathBuf> {
    Ok(std::env::current_dir()?.join(".patterns"))
}

fn read_patterns_file(path: &PathBuf) -> Result<Vec<PatternUsage>> {
    parse_patterns_file(
        &fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?,
    )
}

fn parse_patterns_file(input: &str) -> Result<Vec<PatternUsage>> {
    let mut usages: Vec<PatternUsage> = Vec::new();
    let mut current: Option<String> = None;

    for (idx, raw) in input.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            current = None;
            continue;
        }
        if trimmed.starts_with('#') {
            anyhow::bail!(".patterns comments are not supported (line {line_no})");
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            let Some(id) = current.as_deref() else {
                anyhow::bail!(".patterns path entry without a pattern id on line {line_no}");
            };
            let path = trimmed
                .strip_prefix("- ")
                .context(".patterns path entries must use `  - path` syntax")?
                .trim();
            validate_token(path, line_no, "path")?;
            append_paths(&mut usages, id, &[path.to_string()]);
            continue;
        }
        let (id, paths) = match trimmed.split_once(':') {
            Some((id, rest)) => {
                let id = id.trim();
                validate_token(id, line_no, "pattern id")?;
                let paths = rest
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                (id.to_string(), paths)
            }
            None => {
                validate_token(trimmed, line_no, "pattern id")?;
                (trimmed.to_string(), Vec::new())
            }
        };
        upsert_usage(&mut usages, &id, &paths);
        current = if trimmed.ends_with(':') {
            Some(id)
        } else {
            None
        };
    }
    Ok(usages)
}

fn write_patterns_file(path: &PathBuf, usages: &[PatternUsage]) -> Result<()> {
    let mut body = String::new();
    for usage in usages {
        if usage.paths.is_empty() {
            body.push_str(&usage.id);
            body.push('\n');
        } else {
            body.push_str(&usage.id);
            body.push_str(":\n");
            for path in &usage.paths {
                body.push_str("  - ");
                body.push_str(path);
                body.push('\n');
            }
        }
    }
    fs::write(path, body).with_context(|| format!("write {}", path.display()))
}

fn upsert_usage(usages: &mut Vec<PatternUsage>, id: &str, paths: &[String]) {
    if let Some(usage) = usages.iter_mut().find(|usage| usage.id == id) {
        append_path_values(&mut usage.paths, paths);
        return;
    }
    let mut usage = PatternUsage {
        id: id.to_string(),
        paths: Vec::new(),
    };
    append_path_values(&mut usage.paths, paths);
    usages.push(usage);
}

fn append_paths(usages: &mut [PatternUsage], id: &str, paths: &[String]) {
    if let Some(usage) = usages.iter_mut().find(|usage| usage.id == id) {
        append_path_values(&mut usage.paths, paths);
    }
}

fn append_path_values(existing: &mut Vec<String>, paths: &[String]) {
    let mut seen: BTreeSet<String> = existing.iter().cloned().collect();
    for path in paths {
        if !path.is_empty() && seen.insert(path.clone()) {
            existing.push(path.clone());
        }
    }
}

fn validate_token(value: &str, line_no: usize, label: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!(".patterns empty {label} on line {line_no}");
    }
    if value.starts_with('#') {
        anyhow::bail!(".patterns comments are not supported (line {line_no})");
    }
    Ok(())
}

fn superseded_replacement(pattern: &Pattern) -> Option<String> {
    if let Some(rest) = pattern.state.strip_prefix("superseded-by:") {
        return Some(rest.trim().to_string());
    }
    if pattern.version == "superseded" || pattern.state == "superseded" {
        return Some("(replacement not specified)".to_string());
    }
    None
}

fn render_checks_text(checks: &[PatternCheck]) {
    if checks.is_empty() {
        println!("(no patterns listed)");
        return;
    }
    for check in checks {
        let path_suffix = if check.usage.paths.is_empty() {
            String::new()
        } else {
            format!(" paths={}", check.usage.paths.join(","))
        };
        if let Some(replacement) = check.replacement.as_deref() {
            println!(
                "[superseded] {} ({}) -> {}{}",
                check.pattern.id, check.pattern.title, replacement, path_suffix
            );
            if let Some(task_id) = check.task_created.as_deref() {
                println!("  created migration task {task_id}");
            } else if let Some(task_id) = check.task_existing.as_deref() {
                println!("  migration task already exists {task_id}");
            }
        } else if check.pattern.state == "active" || check.pattern.version == "latest" {
            println!(
                "[active] {} ({}, version={}, state={}){}",
                check.pattern.id,
                check.pattern.title,
                check.pattern.version,
                check.pattern.state,
                path_suffix
            );
        } else {
            println!(
                "[review] {} ({}, version={}, state={}){}",
                check.pattern.id,
                check.pattern.title,
                check.pattern.version,
                check.pattern.state,
                path_suffix
            );
        }
    }
}

fn print_summary_row(pattern: &PatternSummary) {
    let visible_labels = visible_labels(&pattern.labels);
    let labels = if visible_labels.is_empty() {
        String::new()
    } else {
        format!(" [{}]", visible_labels.join(","))
    };
    let categories = effective_categories(&pattern.categories, &pattern.labels);
    let categories = if categories.is_empty() {
        String::new()
    } else {
        format!(" categories={}", categories.join(","))
    };
    println!(
        "[{}] {} ({}, version={}, state={}){}{}",
        pattern.id, pattern.title, pattern.slug, pattern.version, pattern.state, labels, categories
    );
    if !pattern.summary.trim().is_empty() {
        println!("  {}", pattern.summary);
    }
}

fn print_pattern(pattern: &Pattern) {
    let visible_labels = visible_labels(&pattern.labels);
    let categories = effective_categories(&pattern.categories, &pattern.labels);
    println!(
        "{} ({})\nid: {}\nversion: {}\nstate: {}\nlabels: {}\ncategories: {}\n",
        pattern.title,
        pattern.slug,
        pattern.id,
        pattern.version,
        pattern.state,
        visible_labels.join(", "),
        categories.join(", ")
    );
    print!("{}", pattern.body);
    if !pattern.body.ends_with('\n') {
        println!();
    }
}

fn print_comment(comment: &PatternComment) {
    println!(
        "[{}] {} ({}): {}",
        comment.created_at, comment.author, comment.author_type, comment.content
    );
}

fn labels_if_any(labels: &[String]) -> Option<&[String]> {
    if labels.is_empty() {
        None
    } else {
        Some(labels)
    }
}

fn labels_with_category_markers(labels: &[String], categories: &[String]) -> Vec<String> {
    let mut merged = Vec::new();
    append_unique(
        &mut merged,
        labels
            .iter()
            .filter(|label| !is_category_marker(label))
            .cloned(),
    );
    append_unique(
        &mut merged,
        categories.iter().map(|category| category_marker(category)),
    );
    merged
}

fn visible_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .filter(|label| !is_category_marker(label))
        .cloned()
        .collect()
}

fn effective_categories(categories: &[String], labels: &[String]) -> Vec<String> {
    let mut values = Vec::new();
    append_unique(&mut values, categories.iter().cloned());
    append_unique(
        &mut values,
        labels
            .iter()
            .filter_map(|label| label.strip_prefix("category:").map(ToString::to_string)),
    );
    values
}

fn summary_matches_category(pattern: &PatternSummary, category: &str) -> bool {
    effective_categories(&pattern.categories, &pattern.labels)
        .iter()
        .any(|value| value == category)
}

fn category_marker(category: &str) -> String {
    format!("category:{category}")
}

fn is_category_marker(label: &str) -> bool {
    label.starts_with("category:")
}

fn append_unique<I>(values: &mut Vec<String>, items: I)
where
    I: IntoIterator<Item = String>,
{
    let mut seen: BTreeSet<String> = values.iter().cloned().collect();
    for item in items {
        if !item.trim().is_empty() && seen.insert(item.clone()) {
            values.push(item);
        }
    }
}

fn local_hostname() -> Option<String> {
    let host = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_patterns_accepts_yaml_lists_and_bare_ids() {
        let parsed =
            parse_patterns_file("abc:\n  - src/main.rs\n  - /etc/app.py\n\ndef\n").unwrap();
        assert_eq!(
            parsed,
            vec![
                PatternUsage {
                    id: "abc".to_string(),
                    paths: vec!["src/main.rs".to_string(), "/etc/app.py".to_string()],
                },
                PatternUsage {
                    id: "def".to_string(),
                    paths: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn parse_patterns_accepts_colon_comma_shorthand() {
        let parsed = parse_patterns_file("abc:src/main.rs, crates/lib.rs\n").unwrap();
        assert_eq!(
            parsed[0],
            PatternUsage {
                id: "abc".to_string(),
                paths: vec!["src/main.rs".to_string(), "crates/lib.rs".to_string()],
            }
        );
    }

    #[test]
    fn parse_patterns_rejects_comments() {
        let err = parse_patterns_file("# note\n").unwrap_err().to_string();
        assert!(err.contains("comments are not supported"));
    }

    #[test]
    fn upsert_usage_deduplicates_paths() {
        let mut usages = vec![PatternUsage {
            id: "abc".to_string(),
            paths: vec!["a.rs".to_string()],
        }];
        upsert_usage(
            &mut usages,
            "abc",
            &["a.rs".to_string(), "b.rs".to_string()],
        );
        assert_eq!(usages[0].paths, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn markdown_catalog_extracts_pattern_sections() {
        let body = "# Guide\n\n## Intro\n\nnot publishable\n\n## Router\n<!-- pattern-slug: go-router-chi -->\n\n### Scope\n\nUse chi routers.\n\n### Body\n\nDetails.\n\n## Appendix A\n\nignore me\n";
        let patterns = parse_markdown_pattern_catalog(body).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].title, "Router");
        assert_eq!(patterns[0].slug.as_deref(), Some("go-router-chi"));
        assert_eq!(patterns[0].summary.as_deref(), Some("Use chi routers."));
        assert_eq!(patterns[0].labels, vec!["go"]);
        assert_eq!(patterns[0].categories, vec!["programming-language/golang"]);
        assert!(patterns[0].body.contains("### Body"));
    }

    #[test]
    fn publish_overrides_reject_single_pattern_metadata_for_catalogs() {
        let mut patterns = vec![
            PatternFile {
                title: "A".to_string(),
                slug: Some("a".to_string()),
                summary: None,
                body: "body".to_string(),
                labels: Vec::new(),
                categories: Vec::new(),
                version: None,
                state: None,
                author: None,
            },
            PatternFile {
                title: "B".to_string(),
                slug: Some("b".to_string()),
                summary: None,
                body: "body".to_string(),
                labels: Vec::new(),
                categories: Vec::new(),
                version: None,
                state: None,
                author: None,
            },
        ];
        let err = apply_publish_overrides(
            &mut patterns,
            PublishPatternOverrides {
                title: Some("Only One".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("single-pattern files"));
    }

    #[test]
    fn category_markers_are_hidden_from_labels_and_exposed_as_categories() {
        let labels = vec![
            "go".to_string(),
            "category:programming-language/golang".to_string(),
        ];
        assert_eq!(visible_labels(&labels), vec!["go"]);
        assert_eq!(
            effective_categories(&[], &labels),
            vec!["programming-language/golang"]
        );
        assert_eq!(
            labels_with_category_markers(
                &["go".to_string()],
                &["programming-language/golang".to_string()]
            ),
            vec!["go", "category:programming-language/golang"]
        );
    }
}
