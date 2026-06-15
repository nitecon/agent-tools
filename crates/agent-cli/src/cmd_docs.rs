//! `agent-tools docs` subcommands for gateway-backed Documentation context.

use crate::cmd_gateway_context::{read_registration_marker, write_registration_marker};
use agent_comms::config::load_config;
use agent_comms::docs::{
    ApiDoc, ApiDocChunk, ApiDocFilters, ApiDocHierarchyFilters, ApiDocSummary,
    DocumentationHierarchy, DocumentationNode, DocumentationSpace, PublishApiDocRequest,
};
use agent_comms::gateway::GatewayClient;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::short_project_ident;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const DEFAULT_KIND: &str = "agent_context";
const DEFAULT_SOURCE_FORMAT: &str = "agent_context";

#[derive(Subcommand)]
pub enum DocsCommands {
    /// List published agent-facing Documentation entries.
    List {
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Documentation visibility scope: local, global, or all.
        #[arg(long, value_parser = ["local", "global", "all"])]
        scope: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
        /// Override the project ident derived from cwd.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Search published agent-facing Documentation entries.
    Search {
        query: String,
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Documentation visibility scope: local, global, or all.
        #[arg(long, value_parser = ["local", "global", "all"])]
        scope: Option<String>,
        /// Override the project ident derived from cwd.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch one full Documentation entry.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Delete one Documentation entry.
    #[command(alias = "remove")]
    Delete {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch RAG-ready chunks from Documentation.
    Chunks {
        #[arg(long)]
        app: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        /// Documentation visibility scope: local, global, or all.
        #[arg(long, value_parser = ["local", "global", "all"])]
        scope: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Publish a docs-first JSON/YAML file as agent-facing Documentation.
    Publish {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long = "parent-page")]
        parent_page: Option<String>,
        /// Parent Documentation node id for hierarchy placement.
        #[arg(long = "parent-id")]
        parent_id: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        order: Option<i64>,
        /// Sort order within the selected hierarchy parent.
        #[arg(long = "sort-order")]
        sort_order: Option<i64>,
        /// Gateway global visibility rank, for example 1 for Global 1.
        #[arg(long = "global-rank")]
        global_rank: Option<i64>,
        /// Apply global visibility to descendants when supported by the gateway.
        #[arg(long = "global-descendants", default_missing_value = "true", num_args = 0..=1)]
        global_descendants: Option<bool>,
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
        space: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long = "parent-page")]
        parent_page: Option<String>,
        /// Parent Documentation node id for hierarchy placement.
        #[arg(long = "parent-id")]
        parent_id: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        order: Option<i64>,
        /// Sort order within the selected hierarchy parent.
        #[arg(long = "sort-order")]
        sort_order: Option<i64>,
        /// Gateway global visibility rank, for example 1 for Global 1.
        #[arg(long = "global-rank")]
        global_rank: Option<i64>,
        /// Apply global visibility to descendants when supported by the gateway.
        #[arg(long = "global-descendants", default_missing_value = "true", num_args = 0..=1)]
        global_descendants: Option<bool>,
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

    /// Export one gateway-backed Documentation entry to a source-adjacent docs file.
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

    /// Browse the Documentation hierarchy for the current project and visible global docs.
    #[command(alias = "tree", alias = "list-tree", alias = "get-tree")]
    Hierarchy {
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        space: Option<String>,
        /// Documentation visibility scope: local, global, or all.
        #[arg(long, value_parser = ["local", "global", "all"])]
        scope: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
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
    space: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    parent_page: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    order: Option<i64>,
    #[serde(default)]
    sort_order: Option<i64>,
    #[serde(default)]
    global_rank: Option<i64>,
    #[serde(default)]
    global_descendants: Option<bool>,
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
    space: Option<String>,
    category: Option<String>,
    parent_page: Option<String>,
    parent_id: Option<String>,
    slug: Option<String>,
    order: Option<i64>,
    sort_order: Option<i64>,
    global_rank: Option<i64>,
    global_descendants: Option<bool>,
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
            | DocsCommands::Hierarchy { .. }
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
            "Documentation is not available - agent-gateway is not configured.\n\
             Docs require a running agent-gateway connection. Ask the user to run\n\
             `agent-tools setup gateway` to enable the agent-facing Documentation registry."
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
            scope,
            query,
            project,
            agent_id,
        } => cmd_list(app, label, kind, scope, query, project, agent_id).await,
        DocsCommands::Search {
            query,
            app,
            label,
            kind,
            scope,
            project,
            agent_id,
        } => {
            let scope = Some(scope.unwrap_or_else(|| "all".to_string()));
            cmd_list(app, label, kind, scope, Some(query), project, agent_id).await
        }
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
            scope,
            query,
            project,
            agent_id,
        } => {
            let scope = Some(scope.unwrap_or_else(|| "all".to_string()));
            cmd_chunks(app, label, kind, scope, query, project, agent_id).await
        }
        DocsCommands::Hierarchy {
            app,
            space,
            scope,
            query,
            project,
            agent_id,
        } => cmd_hierarchy(app, space, scope, query, project, agent_id).await,
        DocsCommands::Publish {
            file,
            app,
            space,
            category,
            parent_page,
            parent_id,
            slug,
            order,
            sort_order,
            global_rank,
            global_descendants,
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
                space,
                category,
                parent_page,
                parent_id,
                slug,
                order,
                sort_order,
                global_rank,
                global_descendants,
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
            space,
            category,
            parent_page,
            parent_id,
            slug,
            order,
            sort_order,
            global_rank,
            global_descendants,
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
                space,
                category,
                parent_page,
                parent_id,
                slug,
                order,
                sort_order,
                global_rank,
                global_descendants,
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
    scope: Option<String>,
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
        scope.as_deref(),
    );
    let docs = ctx
        .gateway
        .list_api_docs(&ctx.ident, &filters, Some(&ctx.agent_id))
        .await
        .context("list Documentation entries")?;
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
                "delete Documentation entry {id}; if this was a short or stale id, run `agent-tools docs list` and retry with the full id"
            )
        })?;
    print_delete_success(&id, &ctx.ident);
    Ok(())
}

async fn cmd_chunks(
    app: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    scope: Option<String>,
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
        scope.as_deref(),
    );
    let chunks = ctx
        .gateway
        .api_doc_chunks(&ctx.ident, &filters, Some(&ctx.agent_id))
        .await
        .context("fetch API context chunks")?;
    print_chunks(&ctx.ident, &chunks);
    Ok(())
}

async fn cmd_hierarchy(
    app: Option<String>,
    space: Option<String>,
    scope: Option<String>,
    query: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let hierarchy_filters = ApiDocHierarchyFilters {
        query: query.as_deref(),
        app: app.as_deref(),
        space: space.as_deref(),
        scope: scope.as_deref(),
    };
    let mut hierarchy = ctx
        .gateway
        .api_doc_hierarchy(&ctx.ident, &hierarchy_filters, Some(&ctx.agent_id))
        .await
        .context("fetch Documentation hierarchy")?;
    if hierarchy.spaces.is_empty() && hierarchy.pages.is_empty() {
        let list_filters = filters(
            query.as_deref(),
            app.as_deref(),
            None,
            None,
            scope.as_deref(),
        );
        let docs = ctx
            .gateway
            .list_api_docs(&ctx.ident, &list_filters, Some(&ctx.agent_id))
            .await
            .context("fallback list Documentation entries for hierarchy")?;
        hierarchy =
            synthesize_hierarchy_from_docs(&ctx.ident, &docs, space.as_deref(), scope.as_deref());
    } else if hierarchy.scope.is_none() {
        hierarchy.scope = scope;
    }
    print_hierarchy(&ctx.ident, &hierarchy);
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
        space: prepared.space.as_deref(),
        category: prepared.category.as_deref(),
        parent_page: prepared.parent_page.as_deref(),
        parent_id: prepared.parent_id.as_deref(),
        slug: prepared.slug.as_deref(),
        order: prepared.order,
        sort_order: prepared.sort_order,
        global_rank: prepared.global_rank,
        global_descendants: prepared.global_descendants,
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
        "published Documentation [{}] {} ({})",
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
        "valid Documentation context file: {} ({})",
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
        "created starter Documentation context file {}",
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
            let filters = filters(None, Some(&app), None, None, None);
            let docs = ctx
                .gateway
                .list_api_docs(&ctx.ident, &filters, Some(&ctx.agent_id))
                .await
                .context("find Documentation entry for export")?;
            if docs.len() != 1 {
                anyhow::bail!(
                    "--app {app} matched {} docs; pass the exact id from `agent-tools docs list --app {app}`",
                    docs.len()
                );
            }
            ctx.gateway
                .get_api_doc(&ctx.ident, &docs[0].id, Some(&ctx.agent_id))
                .await
                .context("fetch Documentation entry for export")?
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
        space: summary.space.clone(),
        category: summary.category.clone(),
        parent_page: summary.parent_page.clone(),
        parent_id: summary.parent_id.clone(),
        slug: summary.slug.clone(),
        order: summary.order,
        sort_order: summary.sort_order,
        global_rank: summary.global_rank,
        global_descendants: summary.global_descendants,
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
    scope: Option<&'a str>,
) -> ApiDocFilters<'a> {
    ApiDocFilters {
        query,
        app,
        label,
        kind,
        scope,
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
    if overrides.space.is_some() {
        file.space = overrides.space;
    }
    if overrides.category.is_some() {
        file.category = overrides.category;
    }
    if overrides.parent_page.is_some() {
        file.parent_page = overrides.parent_page;
    }
    if overrides.parent_id.is_some() {
        file.parent_id = overrides.parent_id;
    }
    if overrides.slug.is_some() {
        file.slug = overrides.slug;
    }
    if overrides.order.is_some() {
        file.order = overrides.order;
    }
    if overrides.sort_order.is_some() {
        file.sort_order = overrides.sort_order;
    }
    if overrides.global_rank.is_some() {
        file.global_rank = overrides.global_rank;
    }
    if overrides.global_descendants.is_some() {
        file.global_descendants = overrides.global_descendants;
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
    println!("Documentation for project {project_ident}");
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
        print_doc_location(
            doc.space.as_deref(),
            doc.category.as_deref(),
            doc.slug.as_deref(),
            doc.parent_page.as_deref(),
            doc.order,
            &doc.breadcrumbs,
            doc.page_id.as_deref(),
            doc.section_id.as_deref(),
            7,
        );
        print_doc_provenance_metadata(
            &doc.artifact_id,
            doc.artifact_version_id.as_deref(),
            doc.accepted_version_id.as_deref(),
            doc.chunking_status.as_deref(),
            7,
        );
        print_doc_visibility_metadata(
            doc.scope.as_deref(),
            doc.retrieval_scope.as_deref(),
            doc.global_rank,
            doc.owner_project.as_deref(),
            doc.wiki_path.as_deref(),
            7,
        );
        if let Some(summary) = doc.summary.as_deref().filter(|s| !s.trim().is_empty()) {
            println!("       {summary}");
        }
    }
}

fn print_doc_detail(doc: &ApiDoc) {
    let summary = &doc.summary;
    println!("[{}] Documentation", summary.id);
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
    print_doc_location(
        summary.space.as_deref(),
        summary.category.as_deref(),
        summary.slug.as_deref(),
        summary.parent_page.as_deref(),
        summary.order,
        &summary.breadcrumbs,
        summary.page_id.as_deref(),
        summary.section_id.as_deref(),
        0,
    );
    print_doc_provenance_metadata(
        &summary.artifact_id,
        summary.artifact_version_id.as_deref(),
        summary.accepted_version_id.as_deref(),
        summary.chunking_status.as_deref(),
        0,
    );
    print_doc_visibility_metadata(
        summary.scope.as_deref(),
        summary.retrieval_scope.as_deref(),
        summary.global_rank,
        summary.owner_project.as_deref(),
        summary.wiki_path.as_deref(),
        0,
    );
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
    format!("deleted Documentation entry [{id}] from project {project_ident}")
}

fn print_chunks(project_ident: &str, chunks: &[ApiDocChunk]) {
    println!("Documentation chunks for project {project_ident}");
    if chunks.is_empty() {
        println!("(none)");
        println!("hint: publish Documentation first, then retry with --query, --app, --label, or --kind filters.");
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
        print_doc_location(
            chunk.space.as_deref(),
            chunk.category.as_deref(),
            chunk.slug.as_deref(),
            None,
            None,
            &chunk.breadcrumbs,
            chunk.page_id.as_deref(),
            chunk.section_id.as_deref(),
            7,
        );
        print_doc_provenance_metadata(
            &chunk.artifact_id,
            chunk.artifact_version_id.as_deref(),
            chunk.accepted_version_id.as_deref(),
            chunk.chunking_status.as_deref(),
            7,
        );
        print_doc_visibility_metadata(
            chunk.scope.as_deref(),
            chunk.retrieval_scope.as_deref(),
            chunk.global_rank,
            chunk.owner_project.as_deref(),
            chunk.wiki_path.as_deref(),
            7,
        );
        if let Some(freshness) = chunk.freshness.as_deref() {
            println!("       freshness={freshness}");
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

fn synthesize_hierarchy_from_docs(
    project_ident: &str,
    docs: &[ApiDocSummary],
    space_filter: Option<&str>,
    scope_filter: Option<&str>,
) -> DocumentationHierarchy {
    let mut grouped: BTreeMap<String, DocumentationSpace> = BTreeMap::new();
    for doc in docs {
        let space_key = doc.space.clone().unwrap_or_else(|| "default".to_string());
        if let Some(filter) = space_filter {
            if space_key != filter {
                continue;
            }
        }
        let entry = grouped
            .entry(space_key.clone())
            .or_insert_with(|| DocumentationSpace {
                key: Some(space_key.clone()),
                title: Some(space_key.clone()),
                app: Some(doc.app.clone()),
                category: doc.category.clone(),
                scope: doc
                    .scope
                    .clone()
                    .or_else(|| doc.retrieval_scope.clone())
                    .or_else(|| scope_filter.map(str::to_string)),
                global_rank: doc.global_rank,
                owner_project: doc.owner_project.clone(),
                wiki_path: doc.wiki_path.clone(),
                placement_hint: Some(
                    "Gateway hierarchy endpoint is unavailable; this tree is synthesized from Documentation metadata."
                        .to_string(),
                ),
                ..Default::default()
            });
        entry.pages.push(DocumentationNode {
            id: Some(doc.id.clone()),
            kind: Some("page".to_string()),
            title: Some(doc.title.clone()),
            slug: doc.slug.clone(),
            path: doc.source_ref.clone(),
            app: Some(doc.app.clone()),
            space: doc.space.clone(),
            category: doc.category.clone(),
            parent_page: doc.parent_page.clone(),
            parent_id: doc.parent_id.clone(),
            order: doc.order,
            sort_order: doc.sort_order,
            global_rank: doc.global_rank,
            global_descendants: doc.global_descendants,
            scope: doc
                .scope
                .clone()
                .or_else(|| doc.retrieval_scope.clone())
                .or_else(|| scope_filter.map(str::to_string)),
            owner_project: doc.owner_project.clone(),
            wiki_path: doc.wiki_path.clone(),
            labels: doc.labels.clone(),
            current_version_id: doc.artifact_version_id.clone(),
            accepted_version_id: doc.accepted_version_id.clone(),
            source_ref: doc.source_ref.clone(),
            source_artifact_id: doc.artifact_id.clone(),
            source_artifact_version_id: doc.artifact_version_id.clone(),
            breadcrumbs: if doc.breadcrumbs.is_empty() {
                vec![space_key.clone(), doc.title.clone()]
            } else {
                doc.breadcrumbs.clone()
            },
            ..Default::default()
        });
    }
    DocumentationHierarchy {
        project_ident: Some(project_ident.to_string()),
        app: docs.first().map(|doc| doc.app.clone()),
        scope: scope_filter.map(str::to_string),
        spaces: grouped.into_values().collect(),
        pages: Vec::new(),
        placement_hints: vec![
            "Hierarchy endpoint not available; using docs list metadata fallback.".to_string(),
        ],
        provenance: None,
    }
}

fn print_hierarchy(project_ident: &str, hierarchy: &DocumentationHierarchy) {
    if let Some(scope) = hierarchy.scope.as_deref() {
        println!("Documentation hierarchy for project {project_ident} (scope={scope})");
    } else {
        println!("Documentation hierarchy for project {project_ident}");
    }
    if let Some(app) = hierarchy.app.as_deref() {
        println!("app: {app}");
    }
    if hierarchy.spaces.is_empty() && hierarchy.pages.is_empty() {
        println!("(none)");
        println!("hint: publish Documentation with space/slug metadata, then retry `agent-tools docs hierarchy`.");
        return;
    }
    for hint in &hierarchy.placement_hints {
        if !hint.trim().is_empty() {
            println!("hint: {hint}");
        }
    }
    for space in &hierarchy.spaces {
        print_hierarchy_space(space);
    }
    for page in &hierarchy.pages {
        print_hierarchy_node(page, 2);
    }
}

fn print_hierarchy_space(space: &DocumentationSpace) {
    let key = space
        .key
        .as_deref()
        .or(space.id.as_deref())
        .unwrap_or("default");
    let title = space.title.as_deref().unwrap_or(key);
    println!("  space {key}: {title}");
    if let Some(app) = space.app.as_deref() {
        println!("    app: {app}");
    }
    if let Some(category) = space.category.as_deref() {
        println!("    category: {category}");
    }
    if let Some(order) = space.order {
        println!("    order: {order}");
    }
    print_doc_visibility_metadata(
        space.scope.as_deref(),
        None,
        space.global_rank,
        space.owner_project.as_deref(),
        space.wiki_path.as_deref(),
        4,
    );
    if let Some(hint) = space
        .placement_hint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        println!("    placement_hint: {hint}");
    }
    for page in &space.pages {
        print_hierarchy_node(page, 4);
    }
}

fn print_hierarchy_node(node: &DocumentationNode, indent: usize) {
    let pad = " ".repeat(indent);
    let title = node
        .title
        .as_deref()
        .or(node.slug.as_deref())
        .or(node.id.as_deref())
        .unwrap_or("untitled");
    let kind = node
        .node_type
        .as_deref()
        .or(node.kind.as_deref())
        .unwrap_or("page");
    let id = node.id.as_deref().unwrap_or("-");
    println!("{pad}- {kind} [{id}] {title}");
    if node.node_type.is_some() {
        if let Some(doc_kind) = node.kind.as_deref() {
            println!("{pad}  kind: {doc_kind}");
        }
    }
    print_doc_location(
        node.space.as_deref(),
        node.category.as_deref(),
        node.slug.as_deref(),
        node.parent_page.as_deref(),
        node.order,
        &node.breadcrumbs,
        None,
        None,
        indent + 2,
    );
    if let Some(parent_id) = node.parent_id.as_deref() {
        println!("{pad}  parent_id: {parent_id}");
    }
    if let Some(sort_order) = node.sort_order {
        println!("{pad}  sort_order: {sort_order}");
    }
    if let Some(global_descendants) = node.global_descendants {
        println!("{pad}  global_descendants: {global_descendants}");
    }
    print_doc_visibility_metadata(
        node.scope.as_deref(),
        None,
        node.global_rank,
        node.owner_project.as_deref(),
        node.wiki_path.as_deref(),
        indent + 2,
    );
    if let Some(path) = node.path.as_deref() {
        println!("{pad}  path: {path}");
    }
    if let Some(current) = node.current_version_id.as_deref() {
        println!("{pad}  current_version_id: {current}");
    }
    if let Some(accepted) = node.accepted_version_id.as_deref() {
        println!("{pad}  accepted_version_id: {accepted}");
    }
    if let Some(source) = node.source_ref.as_deref() {
        println!("{pad}  source_ref: {source}");
    }
    let artifact_id = node
        .source_artifact_id
        .as_ref()
        .or(node.artifact_id.as_ref())
        .cloned();
    let artifact_version_id = node
        .source_artifact_version_id
        .as_deref()
        .or(node.artifact_version_id.as_deref());
    print_doc_provenance_metadata(
        &artifact_id,
        artifact_version_id,
        node.accepted_version_id.as_deref(),
        None,
        indent + 2,
    );
    if let Some(hint) = node
        .placement_hint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        println!("{pad}  placement_hint: {hint}");
    }
    for section in &node.sections {
        print_hierarchy_node(section, indent + 2);
    }
    for child in &node.children {
        print_hierarchy_node(child, indent + 2);
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

#[allow(clippy::too_many_arguments)]
fn print_doc_location(
    space: Option<&str>,
    category: Option<&str>,
    slug: Option<&str>,
    parent_page: Option<&str>,
    order: Option<i64>,
    breadcrumbs: &[String],
    page_id: Option<&str>,
    section_id: Option<&str>,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    if !breadcrumbs.is_empty() {
        println!("{pad}breadcrumbs: {}", breadcrumbs.join(" > "));
    }
    if let Some(space) = space {
        println!("{pad}space: {space}");
    }
    if let Some(category) = category {
        println!("{pad}category: {category}");
    }
    if let Some(parent) = parent_page {
        println!("{pad}parent_page: {parent}");
    }
    if let Some(slug) = slug {
        println!("{pad}slug: {slug}");
    }
    if let Some(order) = order {
        println!("{pad}order: {order}");
    }
    if let Some(id) = page_id {
        println!("{pad}page_id: {id}");
    }
    if let Some(id) = section_id {
        println!("{pad}section_id: {id}");
    }
}

fn print_doc_provenance_metadata(
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

fn print_doc_visibility_metadata(
    scope: Option<&str>,
    retrieval_scope: Option<&str>,
    global_rank: Option<i64>,
    owner_project: Option<&str>,
    wiki_path: Option<&str>,
    indent: usize,
) {
    let scope = scope.or(retrieval_scope);
    if scope.is_none() && global_rank.is_none() && owner_project.is_none() && wiki_path.is_none() {
        return;
    }
    let pad = " ".repeat(indent);
    let mut parts = Vec::new();
    if let Some(scope) = scope {
        parts.push(format!("scope={scope}"));
    }
    if let Some(rank) = global_rank {
        parts.push(format!("global_rank={rank}"));
    }
    if let Some(project) = owner_project {
        parts.push(format!("owner_project={project}"));
    }
    if let Some(path) = wiki_path {
        parts.push(format!("wiki_path={path}"));
    }
    println!("{pad}{}", parts.join(" "));
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
        space: None,
        category: None,
        parent_page: None,
        parent_id: None,
        slug: None,
        order: None,
        sort_order: None,
        global_rank: None,
        global_descendants: None,
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
                space: Some("apis".to_string()),
                parent_page: Some("Billing".to_string()),
                parent_id: Some("page-1".to_string()),
                slug: Some("payments-api".to_string()),
                order: Some(20),
                sort_order: Some(30),
                global_rank: Some(2),
                global_descendants: Some(true),
                labels: vec!["internal".to_string()],
                ..Default::default()
            },
        );
        assert_eq!(file.app, "payments");
        assert_eq!(file.space.as_deref(), Some("apis"));
        assert_eq!(file.parent_page.as_deref(), Some("Billing"));
        assert_eq!(file.parent_id.as_deref(), Some("page-1"));
        assert_eq!(file.slug.as_deref(), Some("payments-api"));
        assert_eq!(file.order, Some(20));
        assert_eq!(file.sort_order, Some(30));
        assert_eq!(file.global_rank, Some(2));
        assert_eq!(file.global_descendants, Some(true));
        assert_eq!(file.labels, vec!["internal"]);
    }

    #[test]
    fn delete_success_message_names_doc_and_project() {
        let rendered = render_delete_success("doc-1", "agent-tools");
        assert!(rendered.contains("deleted Documentation entry [doc-1]"));
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
            space: None,
            category: None,
            parent_page: None,
            parent_id: None,
            slug: None,
            order: None,
            sort_order: None,
            breadcrumbs: Vec::new(),
            page_id: None,
            section_id: None,
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
            scope: None,
            retrieval_scope: None,
            global_rank: None,
            global_descendants: None,
            owner_project: None,
            wiki_path: None,
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
