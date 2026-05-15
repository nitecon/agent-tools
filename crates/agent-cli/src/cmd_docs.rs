//! `agent-tools docs` subcommands for gateway-backed agent API context.

use crate::cmd_gateway_context::{read_registration_marker, write_registration_marker};
use agent_comms::config::load_config;
use agent_comms::docs::{ApiDoc, ApiDocChunk, ApiDocFilters, ApiDocSummary, PublishApiDocRequest};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const DEFAULT_KIND: &str = "agent_context";
const DEFAULT_SOURCE_FORMAT: &str = "agent_context";

#[derive(Subcommand)]
pub enum DocsCommands {
    /// List published agent-first API context documents.
    List {
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
        /// Override the project ident derived from cwd.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Search published agent-first API context documents.
    Search {
        query: String,
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Override the project ident derived from cwd.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch one full API context document.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Delete one API context document.
    #[command(alias = "remove")]
    Delete {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch RAG-ready chunks from the API context registry.
    Chunks {
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Publish a docs-first JSON/YAML file as agent API context.
    Publish {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "source-format")]
        source_format: Option<String>,
        #[arg(long = "source-ref")]
        source_ref: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Validate a docs-first JSON/YAML file without contacting the gateway.
    Validate {
        #[arg(long)]
        file: PathBuf,
    },

    /// Render the publish payload that would be sent to the gateway.
    Preview {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long = "source-format")]
        source_format: Option<String>,
        #[arg(long = "source-ref")]
        source_ref: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long = "label")]
        label: Vec<String>,
        #[arg(long)]
        author: Option<String>,
    },

    /// Create a starter docs-first file from an OpenAPI/Swagger file or template.
    Bootstrap {
        #[arg(long)]
        app: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long = "openapi")]
        openapi: Option<PathBuf>,
    },

    /// Export one artifact-backed API context to a source-adjacent docs file.
    Export {
        /// API context id/artifact id. If omitted, --app must match exactly one doc.
        id: Option<String>,
        #[arg(long)]
        app: Option<String>,
        #[arg(long, default_value = ".agent/api")]
        output_dir: PathBuf,
        #[arg(long = "manifest-file")]
        manifest_file: Option<PathBuf>,
        /// Write over an existing changed file instead of proposing a sibling file.
        #[arg(long)]
        overwrite: bool,
        /// Permit export when the docs artifact has no accepted version.
        #[arg(long = "current-version")]
        current_version: bool,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

struct DocsContext {
    ident: String,
    canonical_ident: String,
    agent_id: String,
    gateway: GatewayClient,
    gateway_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DocsFile {
    app: String,
    title: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    source_format: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    author: Option<String>,
    content: Value,
}

#[derive(Debug, Clone, Default)]
struct PublishOverrides {
    app: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    kind: Option<String>,
    source_format: Option<String>,
    source_ref: Option<String>,
    version: Option<String>,
    labels: Vec<String>,
    author: Option<String>,
}

pub fn dispatch(cmd: DocsCommands) -> Result<()> {
    if matches!(
        cmd,
        DocsCommands::List { .. }
            | DocsCommands::Search { .. }
            | DocsCommands::Get { .. }
            | DocsCommands::Delete { .. }
            | DocsCommands::Chunks { .. }
            | DocsCommands::Publish { .. }
            | DocsCommands::Export { .. }
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
            "API context docs are not available - agent-gateway is not configured.\n\
             Docs require a running agent-gateway connection. Ask the user to run\n\
             `agent-tools setup gateway` to enable the agent-first API context registry."
        );
    }
    Ok(())
}

async fn run(cmd: DocsCommands) -> Result<()> {
    match cmd {
        DocsCommands::List {
            app,
            label,
            kind,
            query,
            project,
            agent_id,
        } => cmd_list(app, label, kind, query, project, agent_id).await,
        DocsCommands::Search {
            query,
            app,
            label,
            kind,
            project,
            agent_id,
        } => cmd_list(app, label, kind, Some(query), project, agent_id).await,
        DocsCommands::Get {
            id,
            project,
            agent_id,
        } => cmd_get(id, project, agent_id).await,
        DocsCommands::Delete {
            id,
            project,
            agent_id,
        } => cmd_delete(id, project, agent_id).await,
        DocsCommands::Chunks {
            app,
            label,
            kind,
            query,
            project,
            agent_id,
        } => cmd_chunks(app, label, kind, query, project, agent_id).await,
        DocsCommands::Publish {
            file,
            app,
            title,
            summary,
            kind,
            source_format,
            source_ref,
            version,
            label,
            author,
            project,
            agent_id,
        } => {
            let overrides = PublishOverrides {
                app,
                title,
                summary,
                kind,
                source_format,
                source_ref,
                version,
                labels: label,
                author,
            };
            cmd_publish(file, overrides, project, agent_id).await
        }
        DocsCommands::Validate { file } => cmd_validate(file),
        DocsCommands::Preview {
            file,
            app,
            title,
            summary,
            kind,
            source_format,
            source_ref,
            version,
            label,
            author,
        } => {
            let overrides = PublishOverrides {
                app,
                title,
                summary,
                kind,
                source_format,
                source_ref,
                version,
                labels: label,
                author,
            };
            cmd_preview(file, overrides)
        }
        DocsCommands::Bootstrap {
            app,
            title,
            output,
            openapi,
        } => cmd_bootstrap(app, title, output, openapi),
        DocsCommands::Export {
            id,
            app,
            output_dir,
            manifest_file,
            overwrite,
            current_version,
            project,
            agent_id,
        } => {
            cmd_export(
                id,
                app,
                output_dir,
                manifest_file,
                overwrite,
                current_version,
                project,
                agent_id,
            )
            .await
        }
    }
}

async fn cmd_list(
    app: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    query: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let filters = filters(
        query.as_deref(),
        app.as_deref(),
        label.as_deref(),
        kind.as_deref(),
    );
    let docs = ctx
        .gateway
        .list_api_docs(&ctx.ident, &filters, Some(&ctx.agent_id))
        .await
        .context("list API context docs")?;
    print_doc_list(&ctx.ident, &docs);
    Ok(())
}

async fn cmd_get(id: String, project: Option<String>, agent_id: Option<String>) -> Result<()> {
    require_nonempty("--id", &id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let doc = ctx
        .gateway
        .get_api_doc(&ctx.ident, &id, Some(&ctx.agent_id))
        .await
        .context("fetch API context doc")?;
    print_doc_detail(&doc);
    Ok(())
}

async fn cmd_delete(id: String, project: Option<String>, agent_id: Option<String>) -> Result<()> {
    require_nonempty("--id", &id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    ctx.gateway
        .delete_api_doc(&ctx.ident, &id, Some(&ctx.agent_id))
        .await
        .with_context(|| {
            format!(
                "delete API context doc {id}; if this was a short or stale id, run `agent-tools docs list` and retry with the full id"
            )
        })?;
    print_delete_success(&id, &ctx.ident);
    Ok(())
}

async fn cmd_chunks(
    app: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    query: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let filters = filters(
        query.as_deref(),
        app.as_deref(),
        label.as_deref(),
        kind.as_deref(),
    );
    let chunks = ctx
        .gateway
        .api_doc_chunks(&ctx.ident, &filters, Some(&ctx.agent_id))
        .await
        .context("fetch API context chunks")?;
    print_chunks(&ctx.ident, &chunks);
    Ok(())
}

async fn cmd_publish(
    file: PathBuf,
    overrides: PublishOverrides,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let prepared = load_and_prepare_file(&file, overrides)?;
    validate_docs_file(&prepared)?;

    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let labels = labels_slice(&prepared.labels);
    let req = PublishApiDocRequest {
        app: &prepared.app,
        title: &prepared.title,
        content: &prepared.content,
        summary: prepared.summary.as_deref(),
        kind: prepared.kind.as_deref().unwrap_or(DEFAULT_KIND),
        source_format: prepared
            .source_format
            .as_deref()
            .unwrap_or(DEFAULT_SOURCE_FORMAT),
        source_ref: prepared.source_ref.as_deref(),
        version: prepared.version.as_deref(),
        labels,
        author: prepared.author.as_deref(),
    };
    let doc = ctx
        .gateway
        .publish_api_doc(&ctx.ident, &req, Some(&ctx.agent_id))
        .await
        .context("publish API context doc")?;

    println!(
        "published agent API context [{}] {} ({})",
        doc.summary.id, doc.summary.title, doc.summary.app
    );
    println!(
        "kind: {}",
        doc.summary.kind.as_deref().unwrap_or(DEFAULT_KIND)
    );
    println!(
        "source_format: {}",
        doc.summary
            .source_format
            .as_deref()
            .unwrap_or(DEFAULT_SOURCE_FORMAT)
    );
    Ok(())
}

fn cmd_validate(file: PathBuf) -> Result<()> {
    let prepared = load_and_prepare_file(&file, PublishOverrides::default())?;
    validate_docs_file(&prepared)?;
    println!(
        "valid agent API context file: {} ({})",
        prepared.title, prepared.app
    );
    print_content_guidance(&prepared.content);
    Ok(())
}

fn cmd_preview(file: PathBuf, overrides: PublishOverrides) -> Result<()> {
    let prepared = load_and_prepare_file(&file, overrides)?;
    validate_docs_file(&prepared)?;
    let value = serde_json::to_value(&prepared).context("serialize preview")?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn cmd_bootstrap(
    app: String,
    title: Option<String>,
    output: PathBuf,
    openapi: Option<PathBuf>,
) -> Result<()> {
    require_nonempty("--app", &app)?;
    if output.exists() {
        anyhow::bail!("{} already exists", output.display());
    }
    let docs = if let Some(path) = openapi {
        bootstrap_from_openapi(&app, title, &path)?
    } else {
        starter_docs_file(&app, title)
    };
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let body = serde_yaml::to_string(&docs).context("serialize starter docs file")?;
    fs::write(&output, body).with_context(|| format!("write {}", output.display()))?;
    println!(
        "created starter agent API context file {}",
        output.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_export(
    id: Option<String>,
    app: Option<String>,
    output_dir: PathBuf,
    manifest_file: Option<PathBuf>,
    overwrite: bool,
    current_version: bool,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let doc = match id {
        Some(id) => {
            require_nonempty("id", &id)?;
            ctx.gateway
                .get_api_doc(&ctx.ident, &id, Some(&ctx.agent_id))
                .await
                .context("fetch API context doc for export")?
        }
        None => {
            let app = app.context("provide an id or --app")?;
            require_nonempty("--app", &app)?;
            let filters = filters(None, Some(&app), None, None);
            let docs = ctx
                .gateway
                .list_api_docs(&ctx.ident, &filters, Some(&ctx.agent_id))
                .await
                .context("find API context doc for export")?;
            if docs.len() != 1 {
                anyhow::bail!(
                    "--app {app} matched {} docs; pass the exact id from `agent-tools docs list --app {app}`",
                    docs.len()
                );
            }
            ctx.gateway
                .get_api_doc(&ctx.ident, &docs[0].id, Some(&ctx.agent_id))
                .await
                .context("fetch API context doc for export")?
        }
    };
    let summary = &doc.summary;
    if summary.artifact_version_id.is_none() && !current_version {
        anyhow::bail!(
            "docs export requires an accepted artifact version; rerun with --current-version to export the current compatibility row"
        );
    }

    fs::create_dir_all(&output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    let target = export_target_path(&output_dir, summary);
    let export_file = DocsFile {
        app: summary.app.clone(),
        title: summary.title.clone(),
        summary: summary.summary.clone(),
        kind: summary.kind.clone(),
        source_format: summary.source_format.clone(),
        source_ref: summary.source_ref.clone().or_else(|| {
            summary
                .artifact_id
                .as_ref()
                .map(|id| format!("gateway-artifact:{id}"))
        }),
        version: summary.version.clone(),
        labels: summary.labels.clone(),
        author: summary.author.clone(),
        content: doc.content.clone(),
    };
    let body = serde_yaml::to_string(&export_file).context("serialize exported docs file")?;
    let body_hash = sha256_hex(&body);
    let (written_path, overwrite_policy) = write_export_file(&target, &body, overwrite)?;
    let manifest_path = manifest_file.unwrap_or_else(|| output_dir.join("export-manifest.json"));
    let manifest = serde_json::json!({
        "artifact_id": summary.artifact_id,
        "artifact_version_id": summary.artifact_version_id,
        "accepted_version_id": summary.accepted_version_id,
        "doc_id": summary.id,
        "app": summary.app,
        "source_ref": summary.source_ref,
        "source_path": written_path.display().to_string(),
        "content_hash": body_hash,
        "generated_at": OffsetDateTime::now_utc().unix_timestamp(),
        "overwrite_policy": overwrite_policy,
    });
    if let Some(parent) = manifest_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    println!("exported API context [{}] {}", summary.id, summary.title);
    println!(
        "artifact_id: {}",
        summary.artifact_id.as_deref().unwrap_or("-")
    );
    println!(
        "artifact_version_id: {}",
        summary.artifact_version_id.as_deref().unwrap_or("-")
    );
    println!("source_path: {}", written_path.display());
    println!("manifest: {}", manifest_path.display());
    println!("overwrite_policy: {overwrite_policy}");
    Ok(())
}

fn resolve_context(
    project: Option<String>,
    agent_id_override: Option<String>,
) -> Result<DocsContext> {
    let canonical_ident = match project {
        Some(project) => project,
        None => agent_core::project_ident_from_cwd().context("derive project ident from cwd")?,
    };
    let ident = short_project_ident(&canonical_ident);
    if ident.is_empty() {
        anyhow::bail!("could not derive a project ident; pass --project");
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
    Ok(DocsContext {
        ident,
        canonical_ident,
        agent_id,
        gateway,
        gateway_url,
    })
}

async fn ensure_registered(ctx: &DocsContext) -> Result<()> {
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

fn filters<'a>(
    query: Option<&'a str>,
    app: Option<&'a str>,
    label: Option<&'a str>,
    kind: Option<&'a str>,
) -> ApiDocFilters<'a> {
    ApiDocFilters {
        query,
        app,
        label,
        kind,
    }
}

fn load_and_prepare_file(path: &Path, overrides: PublishOverrides) -> Result<DocsFile> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut file = parse_docs_file(path, &text)?;
    apply_overrides(&mut file, overrides);
    Ok(file)
}

fn parse_docs_file(path: &Path, text: &str) -> Result<DocsFile> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => {
            serde_json::from_str(text).with_context(|| format!("parse JSON {}", path.display()))
        }
        "yaml" | "yml" | "" => {
            serde_yaml::from_str(text).with_context(|| format!("parse YAML {}", path.display()))
        }
        other => anyhow::bail!("unsupported docs file extension .{other}; use JSON, YAML, or YML"),
    }
}

fn apply_overrides(file: &mut DocsFile, overrides: PublishOverrides) {
    if let Some(v) = overrides.app {
        file.app = v;
    }
    if let Some(v) = overrides.title {
        file.title = v;
    }
    if overrides.summary.is_some() {
        file.summary = overrides.summary;
    }
    if overrides.kind.is_some() {
        file.kind = overrides.kind;
    }
    if overrides.source_format.is_some() {
        file.source_format = overrides.source_format;
    }
    if overrides.source_ref.is_some() {
        file.source_ref = overrides.source_ref;
    }
    if overrides.version.is_some() {
        file.version = overrides.version;
    }
    if !overrides.labels.is_empty() {
        file.labels = overrides.labels;
    }
    if overrides.author.is_some() {
        file.author = overrides.author;
    }
}

fn validate_docs_file(file: &DocsFile) -> Result<()> {
    require_nonempty("app", &file.app)?;
    require_nonempty("title", &file.title)?;
    if file.content.is_null() {
        anyhow::bail!("content must be present and non-null");
    }
    if !file.content.is_object() {
        anyhow::bail!("content should be an object with agent context sections");
    }
    Ok(())
}

fn require_nonempty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(())
}

fn labels_slice(labels: &[String]) -> Option<&[String]> {
    if labels.is_empty() {
        None
    } else {
        Some(labels)
    }
}

fn print_doc_list(project_ident: &str, docs: &[ApiDocSummary]) {
    println!("Agent API context docs for project {project_ident}");
    if docs.is_empty() {
        println!("(none)");
        println!("hint: create .agent/api/<app>.yaml or agent-api.yaml, then run `agent-tools docs validate --file PATH` and `agent-tools docs publish --file PATH`.");
        return;
    }
    for doc in docs {
        let labels = if doc.labels.is_empty() {
            String::new()
        } else {
            format!(" labels={}", doc.labels.join(","))
        };
        println!(
            "  [{}] {} / {} kind={}{}",
            doc.id,
            doc.app,
            doc.title,
            doc.kind.as_deref().unwrap_or(DEFAULT_KIND),
            labels
        );
        print_doc_artifact_metadata(
            &doc.artifact_id,
            doc.artifact_version_id.as_deref(),
            doc.accepted_version_id.as_deref(),
            doc.chunking_status.as_deref(),
            7,
        );
        if let Some(summary) = doc.summary.as_deref().filter(|s| !s.trim().is_empty()) {
            println!("       {summary}");
        }
    }
}

fn print_doc_detail(doc: &ApiDoc) {
    let summary = &doc.summary;
    println!("[{}] agent API context", summary.id);
    println!("app: {}", summary.app);
    println!("title: {}", summary.title);
    println!("kind: {}", summary.kind.as_deref().unwrap_or(DEFAULT_KIND));
    println!(
        "source_format: {}",
        summary
            .source_format
            .as_deref()
            .unwrap_or(DEFAULT_SOURCE_FORMAT)
    );
    if let Some(version) = summary.version.as_deref() {
        println!("version: {version}");
    }
    if !summary.labels.is_empty() {
        println!("labels: {}", summary.labels.join(", "));
    }
    print_doc_artifact_metadata(
        &summary.artifact_id,
        summary.artifact_version_id.as_deref(),
        summary.accepted_version_id.as_deref(),
        summary.chunking_status.as_deref(),
        0,
    );
    if let Some(scope) = summary.retrieval_scope.as_deref() {
        println!("retrieval_scope: {scope}");
    }
    if !summary.linked_ids.is_empty() {
        println!("linked_ids: {}", summary.linked_ids.join(", "));
    }
    if let Some(summary_text) = summary.summary.as_deref() {
        println!("summary: {summary_text}");
    }
    println!();
    println!(
        "{}",
        serde_json::to_string_pretty(&doc.content).unwrap_or_else(|_| doc.content.to_string())
    );
}

fn print_delete_success(id: &str, project_ident: &str) {
    println!("{}", render_delete_success(id, project_ident));
}

fn render_delete_success(id: &str, project_ident: &str) -> String {
    format!("deleted agent API context [{id}] from project {project_ident}")
}

fn print_chunks(project_ident: &str, chunks: &[ApiDocChunk]) {
    println!("Agent API context chunks for project {project_ident}");
    if chunks.is_empty() {
        println!("(none)");
        println!("hint: publish agent API context docs first, then retry with --query, --app, --label, or --kind filters.");
        return;
    }
    for chunk in chunks {
        let id = chunk
            .doc_id
            .as_deref()
            .or(chunk.id.as_deref())
            .unwrap_or("unknown");
        let app = chunk.app.as_deref().unwrap_or("unknown-app");
        let title = chunk.title.as_deref().unwrap_or("untitled");
        println!("  [{id}] {app} / {title}");
        if let Some(score) = chunk.score {
            println!("       score={score:.3}");
        }
        print_doc_artifact_metadata(
            &chunk.artifact_id,
            chunk.artifact_version_id.as_deref(),
            chunk.accepted_version_id.as_deref(),
            chunk.chunking_status.as_deref(),
            7,
        );
        if let Some(freshness) = chunk.freshness.as_deref() {
            println!("       freshness={freshness}");
        }
        if let Some(scope) = chunk.retrieval_scope.as_deref() {
            println!("       retrieval_scope={scope}");
        }
        if let Some(address) = chunk.child_address.as_deref() {
            println!("       child_address={address}");
        }
        let text = chunk
            .text
            .as_deref()
            .map(str::to_string)
            .or_else(|| chunk.content.as_ref().map(render_value))
            .unwrap_or_else(|| "(empty chunk)".to_string());
        print_indented_lines(&text, 7);
    }
}

fn print_content_guidance(content: &Value) {
    let Some(obj) = content.as_object() else {
        return;
    };
    let expected = [
        "purpose",
        "workflows",
        "endpoints",
        "auth",
        "safety",
        "relationships",
        "examples",
        "operations",
        "schemas",
    ];
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|key| !obj.contains_key(*key))
        .collect();
    if !missing.is_empty() {
        println!(
            "hint: optional agent-context sections not present: {}",
            missing.join(", ")
        );
    }
}

fn print_doc_artifact_metadata(
    artifact_id: &Option<String>,
    artifact_version_id: Option<&str>,
    accepted_version_id: Option<&str>,
    chunking_status: Option<&str>,
    indent: usize,
) {
    if artifact_id.is_none()
        && artifact_version_id.is_none()
        && accepted_version_id.is_none()
        && chunking_status.is_none()
    {
        return;
    }
    let pad = " ".repeat(indent);
    if let Some(id) = artifact_id.as_deref() {
        println!("{pad}artifact_id: {id}");
    }
    if let Some(id) = artifact_version_id {
        println!("{pad}artifact_version_id: {id}");
    }
    if let Some(id) = accepted_version_id {
        println!("{pad}accepted_version_id: {id}");
    }
    if let Some(status) = chunking_status {
        println!("{pad}chunking_status: {status}");
    }
}

fn print_indented_lines(text: &str, indent: usize) {
    let pad = " ".repeat(indent);
    for line in text.lines().take(40) {
        println!("{pad}{line}");
    }
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_string())
        }
    }
}

fn export_target_path(output_dir: &Path, summary: &ApiDocSummary) -> PathBuf {
    if let Some(source_ref) = summary
        .source_ref
        .as_deref()
        .filter(|v| v.starts_with(".agent/api/") && !v.contains(".."))
    {
        return PathBuf::from(source_ref);
    }
    output_dir.join(format!("{}.yaml", safe_filename(&summary.app)))
}

fn write_export_file(
    target: &Path,
    body: &str,
    overwrite: bool,
) -> Result<(PathBuf, &'static str)> {
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if target.exists() {
        let existing = fs::read_to_string(target).unwrap_or_default();
        if existing == body {
            return Ok((target.to_path_buf(), "unchanged"));
        }
        if !overwrite {
            let proposed = proposed_export_path(target);
            fs::write(&proposed, body).with_context(|| format!("write {}", proposed.display()))?;
            return Ok((proposed, "proposed"));
        }
    }
    fs::write(target, body).with_context(|| format!("write {}", target.display()))?;
    Ok((
        target.to_path_buf(),
        if overwrite { "overwrite" } else { "write" },
    ))
}

fn proposed_export_path(target: &Path) -> PathBuf {
    let stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("doc");
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("yaml");
    target.with_file_name(format!("{stem}.proposed.{ext}"))
}

fn safe_filename(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "api-context".to_string()
    } else {
        out
    }
}

fn sha256_hex(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    hex::encode(hasher.finalize())
}

fn starter_docs_file(app: &str, title: Option<String>) -> DocsFile {
    DocsFile {
        app: app.to_string(),
        title: title.unwrap_or_else(|| format!("{app} API context")),
        summary: Some(
            "Agent-first API context for service workflows, auth, safety, and examples."
                .to_string(),
        ),
        kind: Some(DEFAULT_KIND.to_string()),
        source_format: Some(DEFAULT_SOURCE_FORMAT.to_string()),
        source_ref: Some("docs-first".to_string()),
        version: None,
        labels: Vec::new(),
        author: None,
        content: serde_json::json!({
            "purpose": "",
            "workflows": [],
            "endpoints": [],
            "auth": {},
            "safety": [],
            "relationships": [],
            "examples": [],
            "operations": [],
            "schemas": {}
        }),
    }
}

fn bootstrap_from_openapi(app: &str, title: Option<String>, path: &Path) -> Result<DocsFile> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let spec = parse_value(path, &text)?;
    let inferred_title = title.or_else(|| {
        spec.pointer("/info/title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    let mut docs = starter_docs_file(app, inferred_title);
    docs.source_format = Some(
        if is_swagger(&spec) {
            "swagger"
        } else {
            "openapi"
        }
        .to_string(),
    );
    docs.source_ref = Some(path.display().to_string());
    docs.version = spec
        .pointer("/info/version")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    docs.summary = spec
        .pointer("/info/description")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or(docs.summary);
    docs.content = map_openapi_content(&spec);
    Ok(docs)
}

fn parse_value(path: &Path, text: &str) -> Result<Value> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => {
            serde_json::from_str(text).with_context(|| format!("parse JSON {}", path.display()))
        }
        "yaml" | "yml" | "" => {
            serde_yaml::from_str(text).with_context(|| format!("parse YAML {}", path.display()))
        }
        other => anyhow::bail!("unsupported source extension .{other}; use JSON, YAML, or YML"),
    }
}

fn is_swagger(spec: &Value) -> bool {
    spec.get("swagger").is_some()
}

fn map_openapi_content(spec: &Value) -> Value {
    let mut operations = Vec::new();
    if let Some(paths) = spec.get("paths").and_then(Value::as_object) {
        for (path, methods) in paths {
            let Some(methods) = methods.as_object() else {
                continue;
            };
            for (method, op) in methods {
                if !matches!(
                    method.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "options" | "head" | "trace"
                ) {
                    continue;
                }
                let mut row = Map::new();
                row.insert(
                    "method".to_string(),
                    Value::String(method.to_ascii_uppercase()),
                );
                row.insert("path".to_string(), Value::String(path.clone()));
                if let Some(v) = op.get("operationId").or_else(|| op.get("operation_id")) {
                    row.insert("operation_id".to_string(), v.clone());
                }
                if let Some(v) = op.get("summary") {
                    row.insert("summary".to_string(), v.clone());
                }
                if let Some(v) = op.get("description") {
                    row.insert("description".to_string(), v.clone());
                }
                operations.push(Value::Object(row));
            }
        }
    }
    serde_json::json!({
        "purpose": spec.pointer("/info/description").and_then(Value::as_str).unwrap_or(""),
        "workflows": [],
        "endpoints": operations,
        "auth": spec.get("security").cloned().unwrap_or_else(|| serde_json::json!({})),
        "safety": [],
        "relationships": [],
        "examples": [],
        "operations": [],
        "schemas": spec.pointer("/components/schemas")
            .or_else(|| spec.pointer("/definitions"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_required_agent_context_shape() {
        let docs = starter_docs_file("billing", None);
        assert!(validate_docs_file(&docs).is_ok());

        let mut missing_app = docs.clone();
        missing_app.app = " ".to_string();
        assert!(validate_docs_file(&missing_app).is_err());
    }

    #[test]
    fn parses_yaml_docs_first_file() {
        let file: DocsFile = serde_yaml::from_str(
            r#"
app: billing
title: Billing API
content:
  purpose: Handles invoices
"#,
        )
        .unwrap();
        assert_eq!(file.app, "billing");
        assert_eq!(file.title, "Billing API");
        assert_eq!(file.content["purpose"], "Handles invoices");
    }

    #[test]
    fn overrides_publish_metadata() {
        let mut file = starter_docs_file("billing", None);
        apply_overrides(
            &mut file,
            PublishOverrides {
                app: Some("payments".to_string()),
                labels: vec!["internal".to_string()],
                ..Default::default()
            },
        );
        assert_eq!(file.app, "payments");
        assert_eq!(file.labels, vec!["internal"]);
    }

    #[test]
    fn delete_success_message_names_doc_and_project() {
        let rendered = render_delete_success("doc-1", "agent-tools");
        assert!(rendered.contains("deleted agent API context [doc-1]"));
        assert!(rendered.contains("project agent-tools"));
    }

    #[test]
    fn maps_openapi_operations_to_agent_context() {
        let spec = serde_json::json!({
            "openapi": "3.0.0",
            "info": {"title": "Billing", "description": "Billing service", "version": "1"},
            "paths": {
                "/invoices": {
                    "get": {"operationId": "listInvoices", "summary": "List invoices"}
                }
            },
            "components": {"schemas": {"Invoice": {"type": "object"}}}
        });
        let content = map_openapi_content(&spec);
        assert_eq!(content["purpose"], "Billing service");
        assert_eq!(content["endpoints"][0]["method"], "GET");
        assert_eq!(content["schemas"]["Invoice"]["type"], "object");
    }

    #[test]
    fn export_target_prefers_safe_agent_api_source_ref() {
        let summary = ApiDocSummary {
            id: "doc-1".to_string(),
            app: "gateway".to_string(),
            title: "Gateway".to_string(),
            summary: None,
            kind: Some("agent_context".to_string()),
            source_format: Some("agent_context".to_string()),
            source_ref: Some(".agent/api/gateway.yaml".to_string()),
            version: None,
            labels: Vec::new(),
            author: None,
            updated_at: None,
            artifact_id: Some("art-1".to_string()),
            artifact_version_id: Some("ver-1".to_string()),
            accepted_version_id: Some("ver-1".to_string()),
            subkind: Some("api_context".to_string()),
            manifest_chunk_count: Some(1),
            chunking_status: Some("current".to_string()),
            retrieval_scope: None,
            linked_ids: Vec::new(),
        };
        assert_eq!(
            export_target_path(Path::new("out"), &summary),
            PathBuf::from(".agent/api/gateway.yaml")
        );
    }

    #[test]
    fn proposed_export_path_uses_sibling_file() {
        assert_eq!(
            proposed_export_path(Path::new(".agent/api/gateway.yaml")),
            PathBuf::from(".agent/api/gateway.proposed.yaml")
        );
    }
}
