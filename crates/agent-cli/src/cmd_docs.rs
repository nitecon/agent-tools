//! `agent-tools docs` subcommands for gateway-backed agent API context.

use agent_comms::config::{home_dir, load_config};
use agent_comms::docs::{ApiDoc, ApiDocChunk, ApiDocFilters, ApiDocSummary, PublishApiDocRequest};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

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
            | DocsCommands::Get { .. }
            | DocsCommands::Chunks { .. }
            | DocsCommands::Publish { .. }
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
    let content = fs::read_to_string(&path).ok()?;
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
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    let body = format!("GATEWAY_URL={gateway_url}\nCHANNEL_NAME={channel_name}\n");
    fs::write(&path, body)
        .with_context(|| format!("write registration marker {}", path.display()))?;
    Ok(())
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
    if let Some(summary_text) = summary.summary.as_deref() {
        println!("summary: {summary_text}");
    }
    println!();
    println!(
        "{}",
        serde_json::to_string_pretty(&doc.content).unwrap_or_else(|_| doc.content.to_string())
    );
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
}
