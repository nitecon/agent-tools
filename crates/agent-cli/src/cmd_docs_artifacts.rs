//! Agent-facing artifact substrate commands for docs/reviews/spec handoff.
//!
//! Keep this surface in the CLI/docs side of the tree. `agent-comms` is for
//! human communication primitives; artifact operations are shared document
//! substrate operations used by agents.

use crate::cmd_gateway_context::{read_registration_marker, write_registration_marker};
use agent_comms::config::load_config;
use agent_comms::identity::load_or_generate_agent_id;
use agent_comms::sanitize::{short_project_ident, validate_api_key};
use anyhow::{Context, Result};
use clap::Subcommand;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

const DEFAULT_TIMEOUT_MS: u64 = 5000;

#[derive(Subcommand)]
pub enum ArtifactsCommands {
    /// List gateway artifacts by kind, state, label, actor, or query.
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        subkind: Option<String>,
        #[arg(long, alias = "lifecycle-state")]
        status: Option<String>,
        #[arg(long = "label")]
        label: Option<String>,
        #[arg(long, alias = "actor-id")]
        actor: Option<String>,
        #[arg(long = "query", alias = "q")]
        query: Option<String>,
        /// Override the project ident derived from cwd.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch one artifact with current/accepted versions and chunk status.
    Get {
        artifact_id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// List immutable versions for one artifact.
    Versions {
        artifact_id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Print the unified diff between two artifact versions.
    Diff {
        artifact_id: String,
        from_version_id: String,
        to_version_id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// List/create/resolve/reopen artifact comments.
    Comments {
        artifact_id: String,
        #[command(subcommand)]
        command: ArtifactCommentCommands,
    },

    /// Create a comment on an artifact target.
    Comment {
        artifact_id: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long, default_value = "artifact")]
        target_kind: String,
        #[arg(long)]
        target_id: Option<String>,
        #[arg(long)]
        child_address: Option<String>,
        #[arg(long)]
        parent_comment_id: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ReviewsCommands {
    /// Create a design-review artifact, optionally with an initial source version.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long = "label")]
        labels: Vec<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long, default_value = "markdown")]
        body_format: String,
        #[arg(long)]
        source_artifact_id: Option<String>,
        #[arg(long)]
        source_artifact_version_id: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Start a design-review round and return its workflow_run_id.
    StartRound {
        artifact_id: String,
        #[arg(long)]
        source_artifact_version_id: String,
        #[arg(long)]
        round_id: Option<String>,
        #[arg(long = "participant-actor-id")]
        participant_actor_ids: Vec<String>,
        #[arg(long)]
        read_set: Option<String>,
        #[arg(long = "read-set-file")]
        read_set_file: Option<PathBuf>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Add a pass contribution to a design-review round.
    Contribute {
        artifact_id: String,
        workflow_run_id: String,
        #[arg(long)]
        phase: String,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        reviewed_version_id: Option<String>,
        #[arg(long)]
        read_set: Option<String>,
        #[arg(long = "read-set-file")]
        read_set_file: Option<PathBuf>,
        #[arg(long, default_value = "markdown")]
        body_format: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Store synthesis for a design-review round, optionally creating a version.
    Synthesize {
        artifact_id: String,
        workflow_run_id: String,
        #[arg(long)]
        reviewed_version_id: Option<String>,
        #[arg(long)]
        read_set: Option<String>,
        #[arg(long = "read-set-file")]
        read_set_file: Option<PathBuf>,
        #[arg(long, default_value = "markdown")]
        body_format: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long = "no-create-version")]
        no_create_version: bool,
        #[arg(long)]
        version_label: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Update design-review lifecycle/review state and record the transition.
    State {
        artifact_id: String,
        #[arg(long)]
        lifecycle_state: Option<String>,
        #[arg(long)]
        review_state: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// List design-review contributions with round/phase/read-set filters.
    Contributions {
        artifact_id: String,
        #[arg(long)]
        round_id: Option<String>,
        #[arg(long)]
        phase: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        reviewed_version_id: Option<String>,
        #[arg(long)]
        read_set_contains: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SpecsCommands {
    /// Import a spec directory or manifest file as a spec artifact.
    Import {
        spec_dir_or_manifest: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long = "label")]
        labels: Vec<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long)]
        source_doc: Option<String>,
        #[arg(long)]
        source_artifact_id: Option<String>,
        #[arg(long)]
        source_artifact_version_id: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Create a new immutable version for an existing spec artifact.
    Version {
        artifact_id: String,
        spec_dir_or_manifest: PathBuf,
        #[arg(long)]
        version_label: Option<String>,
        #[arg(long)]
        parent_version_id: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long)]
        source_doc: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Accept a spec artifact version as the implementation baseline.
    Accept {
        artifact_id: String,
        version_id: String,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch a spec manifest from accepted/current or an explicit version.
    Manifest {
        artifact_id: String,
        #[arg(long)]
        version_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Fetch one manifest item from accepted/current or an explicit version.
    ManifestItem {
        artifact_id: String,
        manifest_item_id: String,
        #[arg(long)]
        version_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Generate or reuse implementation tasks from an accepted spec artifact.
    GenerateTasks {
        artifact_id: String,
        #[arg(long = "manifest-item-id")]
        manifest_item_ids: Vec<String>,
        #[arg(long)]
        confirmed: bool,
        #[arg(long)]
        reporter: Option<String>,
        #[arg(long)]
        hostname: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Link an existing task to a spec manifest item.
    LinkTask {
        artifact_id: String,
        manifest_item_id: String,
        task_id: String,
        #[arg(long)]
        version_id: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ArtifactCommentCommands {
    /// List comments for an artifact.
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Create a comment on an artifact target.
    Add {
        #[arg(long)]
        body: Option<String>,
        #[arg(long = "body-file")]
        body_file: Option<PathBuf>,
        #[arg(long, default_value = "artifact")]
        target_kind: String,
        #[arg(long)]
        target_id: Option<String>,
        #[arg(long)]
        child_address: Option<String>,
        #[arg(long)]
        parent_comment_id: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Resolve a comment.
    Resolve {
        comment_id: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },

    /// Reopen a resolved comment.
    Reopen {
        comment_id: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        actor_display_name: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
        #[arg(long)]
        workflow_run_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct ArtifactContext {
    ident: String,
    canonical_ident: String,
    agent_id: String,
    client: Client,
    gateway_url: String,
    api_key: String,
    host: Option<String>,
    agent_system: String,
}

#[derive(Debug, Clone, Default)]
struct ArtifactFilters<'a> {
    kind: Option<&'a str>,
    subkind: Option<&'a str>,
    status: Option<&'a str>,
    label: Option<&'a str>,
    actor: Option<&'a str>,
    query: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct MutationOptions {
    idempotency_key: Option<String>,
    workflow_run_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PreparedSpecSource {
    title: String,
    manifest: Value,
    file_bodies: HashMap<String, String>,
    source_doc: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegisterProjectRequest<'a> {
    ident: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct RegisterProjectResponse {
    channel_name: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactReadResponse<T> {
    data: T,
    #[serde(default)]
    chunking_status: Option<ChunkingStatus>,
}

#[derive(Debug, Deserialize)]
struct ArtifactMutationResponse<T> {
    data: T,
    provenance: ArtifactProvenance,
}

#[derive(Debug, Deserialize)]
struct ArtifactSummary {
    artifact_id: String,
    project_ident: String,
    kind: String,
    #[serde(default)]
    subkind: Option<String>,
    title: String,
    #[serde(default)]
    labels: Vec<String>,
    lifecycle_state: String,
    review_state: String,
    implementation_state: String,
    #[serde(default)]
    current_version_id: Option<String>,
    #[serde(default)]
    accepted_version_id: Option<String>,
    created_by_actor_id: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Deserialize)]
struct ArtifactDetail {
    artifact: ArtifactSummary,
    #[serde(default)]
    current_version: Option<ArtifactVersion>,
    #[serde(default)]
    accepted_version: Option<ArtifactVersion>,
}

#[derive(Debug, Deserialize)]
struct ArtifactVersion {
    artifact_version_id: String,
    artifact_id: String,
    #[serde(default)]
    version_label: Option<String>,
    #[serde(default)]
    parent_version_id: Option<String>,
    body_format: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    structured_payload: Option<Value>,
    #[serde(default)]
    source_format: Option<String>,
    created_by_actor_id: String,
    #[serde(default)]
    created_via_workflow_run_id: Option<String>,
    version_state: String,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    body_purged_at: Option<i64>,
    created_at: i64,
    #[serde(default)]
    chunking_status: Option<ChunkingStatus>,
}

#[derive(Debug, Deserialize)]
struct ArtifactDiff {
    #[serde(default)]
    from_version_id: Option<String>,
    to_version_id: String,
    format: String,
    byte_delta: isize,
    diff: String,
    #[serde(default)]
    chunking_status: Option<ChunkingStatus>,
}

#[derive(Debug, Deserialize)]
struct ArtifactComment {
    comment_id: String,
    artifact_id: String,
    target_kind: String,
    target_id: String,
    #[serde(default)]
    child_address: Option<String>,
    #[serde(default)]
    parent_comment_id: Option<String>,
    actor_id: String,
    body: String,
    state: String,
    #[serde(default)]
    resolved_by_actor_id: Option<String>,
    #[serde(default)]
    resolved_by_workflow_run_id: Option<String>,
    #[serde(default)]
    resolved_at: Option<i64>,
    #[serde(default)]
    resolution_note: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Deserialize, Clone)]
struct ChunkingStatus {
    status: String,
    current_chunk_count: usize,
    stale_chunk_count: usize,
    superseded_chunk_count: usize,
    #[serde(default)]
    failed_addresses: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ArtifactProvenance {
    actor: Value,
    #[serde(default)]
    workflow_run_id: Option<String>,
    idempotency_key: String,
    request_id: String,
    created_at: i64,
    authorization: Value,
    generated_resources: Value,
    replay: bool,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CreateCommentRequest<'a> {
    target_kind: &'a str,
    target_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    child_address: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_comment_id: Option<&'a str>,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct ResolveCommentRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution_note: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct ReopenCommentRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    note_body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DesignReviewCreateRequest<'a> {
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    body_format: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_artifact_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_artifact_version_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DesignReviewRoundRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    round_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_actor_ids: Option<&'a [String]>,
    source_artifact_version_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_set: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DesignReviewContributionRequest<'a> {
    phase: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewed_version_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_set: Option<&'a Value>,
    body_format: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DesignReviewSynthesisRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewed_version_id: Option<&'a str>,
    read_set: &'a Value,
    body_format: &'a str,
    body: &'a str,
    create_version: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DesignReviewStateRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    lifecycle_state: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_state: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct SpecImportRequest<'a> {
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    manifest: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_bodies: Option<&'a HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_doc: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_artifact_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_artifact_version_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct SpecVersionRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    version_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_version_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    manifest: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_bodies: Option<&'a HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_doc: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct SpecAcceptRequest<'a> {
    version_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct GenerateSpecTasksRequest<'a> {
    confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_item_ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reporter: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct LinkSpecTaskRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    version_id: Option<&'a str>,
    manifest_item_id: &'a str,
    task_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_display_name: Option<&'a str>,
}

pub fn dispatch(cmd: ArtifactsCommands) -> Result<()> {
    ensure_gateway_configured()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(cmd))
}

pub fn dispatch_reviews(cmd: ReviewsCommands) -> Result<()> {
    ensure_gateway_configured()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run_reviews(cmd))
}

pub fn dispatch_specs(cmd: SpecsCommands) -> Result<()> {
    ensure_gateway_configured()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run_specs(cmd))
}

fn ensure_gateway_configured() -> Result<()> {
    let cfg = load_config();
    if cfg.gateway.url.is_none() || cfg.gateway.api_key.is_none() {
        anyhow::bail!(
            "Artifact commands are not available - agent-gateway is not configured.\n\
             Run `agent-tools setup gateway` to enable docs/reviews/spec artifact workflows."
        );
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn run_reviews(cmd: ReviewsCommands) -> Result<()> {
    match cmd {
        ReviewsCommands::Create {
            title,
            labels,
            body,
            body_file,
            body_format,
            source_artifact_id,
            source_artifact_version_id,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_review_create(
                title,
                labels,
                body,
                body_file,
                body_format,
                source_artifact_id,
                source_artifact_version_id,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        ReviewsCommands::StartRound {
            artifact_id,
            source_artifact_version_id,
            round_id,
            participant_actor_ids,
            read_set,
            read_set_file,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_review_start_round(
                artifact_id,
                source_artifact_version_id,
                round_id,
                participant_actor_ids,
                read_set,
                read_set_file,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        ReviewsCommands::Contribute {
            artifact_id,
            workflow_run_id,
            phase,
            role,
            reviewed_version_id,
            read_set,
            read_set_file,
            body_format,
            body,
            body_file,
            actor_display_name,
            idempotency_key,
            project,
            agent_id,
        } => {
            cmd_review_contribute(
                artifact_id,
                workflow_run_id,
                phase,
                role,
                reviewed_version_id,
                read_set,
                read_set_file,
                body_format,
                body,
                body_file,
                actor_display_name,
                idempotency_key,
                project,
                agent_id,
            )
            .await
        }
        ReviewsCommands::Synthesize {
            artifact_id,
            workflow_run_id,
            reviewed_version_id,
            read_set,
            read_set_file,
            body_format,
            body,
            body_file,
            no_create_version,
            version_label,
            actor_display_name,
            idempotency_key,
            project,
            agent_id,
        } => {
            cmd_review_synthesize(
                artifact_id,
                workflow_run_id,
                reviewed_version_id,
                read_set,
                read_set_file,
                body_format,
                body,
                body_file,
                !no_create_version,
                version_label,
                actor_display_name,
                idempotency_key,
                project,
                agent_id,
            )
            .await
        }
        ReviewsCommands::State {
            artifact_id,
            lifecycle_state,
            review_state,
            note,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_review_state(
                artifact_id,
                lifecycle_state,
                review_state,
                note,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        ReviewsCommands::Contributions {
            artifact_id,
            round_id,
            phase,
            role,
            reviewed_version_id,
            read_set_contains,
            project,
            agent_id,
        } => {
            cmd_review_contributions(
                artifact_id,
                round_id,
                phase,
                role,
                reviewed_version_id,
                read_set_contains,
                project,
                agent_id,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_specs(cmd: SpecsCommands) -> Result<()> {
    match cmd {
        SpecsCommands::Import {
            spec_dir_or_manifest,
            title,
            labels,
            body,
            body_file,
            source_doc,
            source_artifact_id,
            source_artifact_version_id,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_spec_import(
                spec_dir_or_manifest,
                title,
                labels,
                body,
                body_file,
                source_doc,
                source_artifact_id,
                source_artifact_version_id,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        SpecsCommands::Version {
            artifact_id,
            spec_dir_or_manifest,
            version_label,
            parent_version_id,
            body,
            body_file,
            source_doc,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_spec_version(
                artifact_id,
                spec_dir_or_manifest,
                version_label,
                parent_version_id,
                body,
                body_file,
                source_doc,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        SpecsCommands::Accept {
            artifact_id,
            version_id,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_spec_accept(
                artifact_id,
                version_id,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        SpecsCommands::Manifest {
            artifact_id,
            version_id,
            project,
            agent_id,
        } => cmd_spec_manifest(artifact_id, version_id, project, agent_id).await,
        SpecsCommands::ManifestItem {
            artifact_id,
            manifest_item_id,
            version_id,
            project,
            agent_id,
        } => {
            cmd_spec_manifest_item(artifact_id, manifest_item_id, version_id, project, agent_id)
                .await
        }
        SpecsCommands::GenerateTasks {
            artifact_id,
            manifest_item_ids,
            confirmed,
            reporter,
            hostname,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_spec_generate_tasks(
                artifact_id,
                manifest_item_ids,
                confirmed,
                reporter,
                hostname,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
        SpecsCommands::LinkTask {
            artifact_id,
            manifest_item_id,
            task_id,
            version_id,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_spec_link_task(
                artifact_id,
                manifest_item_id,
                task_id,
                version_id,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
    }
}

async fn run(cmd: ArtifactsCommands) -> Result<()> {
    match cmd {
        ArtifactsCommands::List {
            kind,
            subkind,
            status,
            label,
            actor,
            query,
            project,
            agent_id,
        } => {
            let filters = ArtifactFilters {
                kind: kind.as_deref(),
                subkind: subkind.as_deref(),
                status: status.as_deref(),
                label: label.as_deref(),
                actor: actor.as_deref(),
                query: query.as_deref(),
            };
            cmd_list(filters, project, agent_id).await
        }
        ArtifactsCommands::Get {
            artifact_id,
            project,
            agent_id,
        } => cmd_get(artifact_id, project, agent_id).await,
        ArtifactsCommands::Versions {
            artifact_id,
            project,
            agent_id,
        } => cmd_versions(artifact_id, project, agent_id).await,
        ArtifactsCommands::Diff {
            artifact_id,
            from_version_id,
            to_version_id,
            project,
            agent_id,
        } => {
            cmd_diff(
                artifact_id,
                from_version_id,
                to_version_id,
                project,
                agent_id,
            )
            .await
        }
        ArtifactsCommands::Comments {
            artifact_id,
            command,
        } => match command {
            ArtifactCommentCommands::List { project, agent_id } => {
                cmd_comments_list(artifact_id, project, agent_id).await
            }
            ArtifactCommentCommands::Add {
                body,
                body_file,
                target_kind,
                target_id,
                child_address,
                parent_comment_id,
                actor_display_name,
                idempotency_key,
                workflow_run_id,
                project,
                agent_id,
            } => {
                cmd_comment_add(
                    artifact_id,
                    body,
                    body_file,
                    target_kind,
                    target_id,
                    child_address,
                    parent_comment_id,
                    actor_display_name,
                    MutationOptions {
                        idempotency_key,
                        workflow_run_id,
                    },
                    project,
                    agent_id,
                )
                .await
            }
            ArtifactCommentCommands::Resolve {
                comment_id,
                note,
                actor_display_name,
                idempotency_key,
                workflow_run_id,
                project,
                agent_id,
            } => {
                cmd_comment_resolve(
                    artifact_id,
                    comment_id,
                    note,
                    actor_display_name,
                    MutationOptions {
                        idempotency_key,
                        workflow_run_id,
                    },
                    project,
                    agent_id,
                )
                .await
            }
            ArtifactCommentCommands::Reopen {
                comment_id,
                note,
                actor_display_name,
                idempotency_key,
                workflow_run_id,
                project,
                agent_id,
            } => {
                cmd_comment_reopen(
                    artifact_id,
                    comment_id,
                    note,
                    actor_display_name,
                    MutationOptions {
                        idempotency_key,
                        workflow_run_id,
                    },
                    project,
                    agent_id,
                )
                .await
            }
        },
        ArtifactsCommands::Comment {
            artifact_id,
            body,
            body_file,
            target_kind,
            target_id,
            child_address,
            parent_comment_id,
            actor_display_name,
            idempotency_key,
            workflow_run_id,
            project,
            agent_id,
        } => {
            cmd_comment_add(
                artifact_id,
                body,
                body_file,
                target_kind,
                target_id,
                child_address,
                parent_comment_id,
                actor_display_name,
                MutationOptions {
                    idempotency_key,
                    workflow_run_id,
                },
                project,
                agent_id,
            )
            .await
        }
    }
}

async fn cmd_list(
    filters: ArtifactFilters<'_>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = build_artifacts_url(&ctx.gateway_url, &ctx.ident, &filters);
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/artifacts")?;
    let data: ArtifactReadResponse<Vec<ArtifactSummary>> =
        decode_or_bail(resp, "decode artifacts response").await?;
    print_artifact_list(&ctx.ident, &data.data, data.chunking_status.as_ref());
    Ok(())
}

async fn cmd_get(
    artifact_id: String,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id);
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/artifacts/:artifact_id")?;
    let data: ArtifactReadResponse<ArtifactDetail> =
        decode_or_bail(resp, "decode artifact detail response").await?;
    print_artifact_detail(&data.data, data.chunking_status.as_ref());
    Ok(())
}

async fn cmd_versions(
    artifact_id: String,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/versions",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id)
    );
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/artifacts/:artifact_id/versions")?;
    let data: ArtifactReadResponse<Vec<ArtifactVersion>> =
        decode_or_bail(resp, "decode artifact versions response").await?;
    print_artifact_versions(&artifact_id, &data.data, data.chunking_status.as_ref());
    Ok(())
}

async fn cmd_diff(
    artifact_id: String,
    from_version_id: String,
    to_version_id: String,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("from_version_id", &from_version_id)?;
    require_nonempty("to_version_id", &to_version_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let mut url = format!(
        "{}/versions/{}/diff",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id),
        encode_query_component(&to_version_id)
    );
    url.push_str("?base_version_id=");
    url.push_str(&encode_query_component(&from_version_id));
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/artifacts/:artifact_id/versions/:version_id/diff")?;
    let data: ArtifactReadResponse<ArtifactDiff> =
        decode_or_bail(resp, "decode artifact diff response").await?;
    print_artifact_diff(&artifact_id, &data.data, data.chunking_status.as_ref());
    Ok(())
}

async fn cmd_comments_list(
    artifact_id: String,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/comments",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id)
    );
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/artifacts/:artifact_id/comments")?;
    let data: ArtifactReadResponse<Vec<ArtifactComment>> =
        decode_or_bail(resp, "decode artifact comments response").await?;
    print_artifact_comments(&artifact_id, &data.data, data.chunking_status.as_ref());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_create(
    title: String,
    labels: Vec<String>,
    body: Option<String>,
    body_file: Option<PathBuf>,
    body_format: String,
    source_artifact_id: Option<String>,
    source_artifact_version_id: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("--title", &title)?;
    require_nonempty("--body-format", &body_format)?;
    let body = resolve_optional_body(body, body_file)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/design-reviews",
        ctx.gateway_url, ctx.ident
    );
    let labels = if labels.is_empty() {
        None
    } else {
        Some(labels.as_slice())
    };
    let req = DesignReviewCreateRequest {
        title: &title,
        labels,
        body: body.as_deref(),
        body_format: &body_format,
        source_artifact_id: source_artifact_id.as_deref(),
        source_artifact_version_id: source_artifact_version_id.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "artifact.write artifact_version.create",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/design-reviews")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode design-review create response").await?;
    print_review_mutation("created design review", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_start_round(
    artifact_id: String,
    source_artifact_version_id: String,
    round_id: Option<String>,
    participant_actor_ids: Vec<String>,
    read_set: Option<String>,
    read_set_file: Option<PathBuf>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("--source-artifact-version-id", &source_artifact_version_id)?;
    let read_set = resolve_json_arg("--read-set", read_set, read_set_file)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/design-reviews/{}/rounds",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let participants = if participant_actor_ids.is_empty() {
        None
    } else {
        Some(participant_actor_ids.as_slice())
    };
    let req = DesignReviewRoundRequest {
        round_id: round_id.as_deref(),
        participant_actor_ids: participants,
        source_artifact_version_id: &source_artifact_version_id,
        read_set: read_set.as_ref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "workflow_run.start artifact.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/design-reviews/:artifact_id/rounds")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode design-review round response").await?;
    print_review_mutation("started design review round", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_contribute(
    artifact_id: String,
    workflow_run_id: String,
    phase: String,
    role: Option<String>,
    reviewed_version_id: Option<String>,
    read_set: Option<String>,
    read_set_file: Option<PathBuf>,
    body_format: String,
    body: Option<String>,
    body_file: Option<PathBuf>,
    actor_display_name: Option<String>,
    idempotency_key: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("workflow_run_id", &workflow_run_id)?;
    require_nonempty("--phase", &phase)?;
    require_nonempty("--body-format", &body_format)?;
    let body = resolve_body(body, body_file)?;
    let read_set = resolve_json_arg("--read-set", read_set, read_set_file)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/design-reviews/{}/rounds/{}/contributions",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id),
        encode_query_component(&workflow_run_id)
    );
    let req = DesignReviewContributionRequest {
        phase: &phase,
        role: role.as_deref(),
        reviewed_version_id: reviewed_version_id.as_deref(),
        read_set: read_set.as_ref(),
        body_format: &body_format,
        body: &body,
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "contribution.write",
        MutationOptions {
            idempotency_key,
            workflow_run_id: None,
        },
    )
    .send()
    .await
    .context(
        "POST /v1/projects/:ident/design-reviews/:artifact_id/rounds/:workflow_run_id/contributions",
    )?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode design-review contribution response").await?;
    print_review_mutation(
        "added design review contribution",
        &data.data,
        &data.provenance,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_synthesize(
    artifact_id: String,
    workflow_run_id: String,
    reviewed_version_id: Option<String>,
    read_set: Option<String>,
    read_set_file: Option<PathBuf>,
    body_format: String,
    body: Option<String>,
    body_file: Option<PathBuf>,
    create_version: bool,
    version_label: Option<String>,
    actor_display_name: Option<String>,
    idempotency_key: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("workflow_run_id", &workflow_run_id)?;
    require_nonempty("--body-format", &body_format)?;
    let body = resolve_body(body, body_file)?;
    let read_set = resolve_json_arg("--read-set", read_set, read_set_file)?
        .context("provide --read-set or --read-set-file")?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/design-reviews/{}/rounds/{}/synthesis",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id),
        encode_query_component(&workflow_run_id)
    );
    let req = DesignReviewSynthesisRequest {
        reviewed_version_id: reviewed_version_id.as_deref(),
        read_set: &read_set,
        body_format: &body_format,
        body: &body,
        create_version,
        version_label: version_label.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "contribution.write artifact_version.create artifact.write",
        MutationOptions {
            idempotency_key,
            workflow_run_id: None,
        },
    )
    .send()
    .await
    .context(
        "POST /v1/projects/:ident/design-reviews/:artifact_id/rounds/:workflow_run_id/synthesis",
    )?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode design-review synthesis response").await?;
    print_review_mutation("synthesized design review", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_state(
    artifact_id: String,
    lifecycle_state: Option<String>,
    review_state: Option<String>,
    note: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    if lifecycle_state.is_none() && review_state.is_none() {
        anyhow::bail!("provide --lifecycle-state or --review-state");
    }
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/design-reviews/{}/state",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let req = DesignReviewStateRequest {
        lifecycle_state: lifecycle_state.as_deref(),
        review_state: review_state.as_deref(),
        note: note.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "artifact.write contribution.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/design-reviews/:artifact_id/state")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode design-review state response").await?;
    print_review_mutation("updated design review state", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_review_contributions(
    artifact_id: String,
    round_id: Option<String>,
    phase: Option<String>,
    role: Option<String>,
    reviewed_version_id: Option<String>,
    read_set_contains: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let mut url = format!(
        "{}/v1/projects/{}/design-reviews/{}/contributions",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let mut parts = Vec::new();
    push_query(&mut parts, "round_id", round_id.as_deref());
    push_query(&mut parts, "phase", phase.as_deref());
    push_query(&mut parts, "role", role.as_deref());
    push_query(
        &mut parts,
        "reviewed_version_id",
        reviewed_version_id.as_deref(),
    );
    push_query(
        &mut parts,
        "read_set_contains",
        read_set_contains.as_deref(),
    );
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/design-reviews/:artifact_id/contributions")?;
    let data: ArtifactReadResponse<Vec<Value>> =
        decode_or_bail(resp, "decode design-review contributions response").await?;
    print_review_contributions(&artifact_id, &data.data, data.chunking_status.as_ref());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_spec_import(
    spec_dir_or_manifest: PathBuf,
    title: Option<String>,
    labels: Vec<String>,
    body: Option<String>,
    body_file: Option<PathBuf>,
    source_doc: Option<String>,
    source_artifact_id: Option<String>,
    source_artifact_version_id: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let mut prepared = load_spec_source(&spec_dir_or_manifest)?;
    if let Some(title) = title {
        require_nonempty("--title", &title)?;
        prepared.title = title;
    }
    if let Some(source_doc) = source_doc {
        require_nonempty("--source-doc", &source_doc)?;
        prepared.source_doc = Some(source_doc);
    }
    if body.is_some() || body_file.is_some() {
        prepared.body = resolve_optional_body(body, body_file)?;
    }
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!("{}/v1/projects/{}/specs", ctx.gateway_url, ctx.ident);
    let labels = if labels.is_empty() {
        None
    } else {
        Some(labels.as_slice())
    };
    let file_bodies = if prepared.file_bodies.is_empty() {
        None
    } else {
        Some(&prepared.file_bodies)
    };
    let req = SpecImportRequest {
        title: &prepared.title,
        labels,
        body: prepared.body.as_deref(),
        manifest: &prepared.manifest,
        file_bodies,
        source_doc: prepared.source_doc.as_deref(),
        source_artifact_id: source_artifact_id.as_deref(),
        source_artifact_version_id: source_artifact_version_id.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "artifact.write artifact_version.create",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/specs")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode spec import response").await?;
    print_spec_mutation("imported spec", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_spec_version(
    artifact_id: String,
    spec_dir_or_manifest: PathBuf,
    version_label: Option<String>,
    parent_version_id: Option<String>,
    body: Option<String>,
    body_file: Option<PathBuf>,
    source_doc: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let mut prepared = load_spec_source(&spec_dir_or_manifest)?;
    if let Some(source_doc) = source_doc {
        require_nonempty("--source-doc", &source_doc)?;
        prepared.source_doc = Some(source_doc);
    }
    if body.is_some() || body_file.is_some() {
        prepared.body = resolve_optional_body(body, body_file)?;
    }
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/specs/{}/versions",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let file_bodies = if prepared.file_bodies.is_empty() {
        None
    } else {
        Some(&prepared.file_bodies)
    };
    let req = SpecVersionRequest {
        version_label: version_label.as_deref(),
        parent_version_id: parent_version_id.as_deref(),
        body: prepared.body.as_deref(),
        manifest: &prepared.manifest,
        file_bodies,
        source_doc: prepared.source_doc.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "artifact_version.create",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/specs/:artifact_id/versions")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode spec version response").await?;
    print_spec_mutation("versioned spec", &data.data, &data.provenance);
    Ok(())
}

async fn cmd_spec_accept(
    artifact_id: String,
    version_id: String,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("version_id", &version_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/specs/{}/accept",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let req = SpecAcceptRequest {
        version_id: &version_id,
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "artifact_version.accept artifact.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/specs/:artifact_id/accept")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode spec accept response").await?;
    print_spec_mutation("accepted spec version", &data.data, &data.provenance);
    Ok(())
}

async fn cmd_spec_manifest(
    artifact_id: String,
    version_id: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let mut url = format!(
        "{}/v1/projects/{}/specs/{}/manifest",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    if let Some(version_id) = version_id.as_deref() {
        url.push_str("?version_id=");
        url.push_str(&encode_query_component(version_id));
    }
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/specs/:artifact_id/manifest")?;
    let data: ArtifactReadResponse<Value> =
        decode_or_bail(resp, "decode spec manifest response").await?;
    print_spec_read("spec manifest", &data.data, data.chunking_status.as_ref());
    Ok(())
}

async fn cmd_spec_manifest_item(
    artifact_id: String,
    manifest_item_id: String,
    version_id: Option<String>,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("manifest_item_id", &manifest_item_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let mut url = format!(
        "{}/v1/projects/{}/specs/{}/manifest/{}",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id),
        encode_query_component(&manifest_item_id)
    );
    if let Some(version_id) = version_id.as_deref() {
        url.push_str("?version_id=");
        url.push_str(&encode_query_component(version_id));
    }
    let resp = read_request(&ctx, ctx.client.get(&url), "artifact.read")
        .send()
        .await
        .context("GET /v1/projects/:ident/specs/:artifact_id/manifest/:manifest_item_id")?;
    let data: ArtifactReadResponse<Value> =
        decode_or_bail(resp, "decode spec manifest item response").await?;
    print_spec_read(
        "spec manifest item",
        &data.data,
        data.chunking_status.as_ref(),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_spec_generate_tasks(
    artifact_id: String,
    manifest_item_ids: Vec<String>,
    confirmed: bool,
    reporter: Option<String>,
    hostname: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/specs/{}/generate-tasks",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let selected = if manifest_item_ids.is_empty() {
        None
    } else {
        Some(manifest_item_ids.as_slice())
    };
    let req = GenerateSpecTasksRequest {
        confirmed,
        manifest_item_ids: selected,
        reporter: reporter.as_deref(),
        hostname: hostname.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "task.generate_from_spec workflow_run.start link.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/specs/:artifact_id/generate-tasks")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode spec task generation response").await?;
    print_spec_mutation("generated spec tasks", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_spec_link_task(
    artifact_id: String,
    manifest_item_id: String,
    task_id: String,
    version_id: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("manifest_item_id", &manifest_item_id)?;
    require_nonempty("task_id", &task_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/v1/projects/{}/specs/{}/link-task",
        ctx.gateway_url,
        ctx.ident,
        encode_query_component(&artifact_id)
    );
    let req = LinkSpecTaskRequest {
        version_id: version_id.as_deref(),
        manifest_item_id: &manifest_item_id,
        task_id: &task_id,
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "task.generate_from_spec workflow_run.start link.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/specs/:artifact_id/link-task")?;
    let data: ArtifactMutationResponse<Value> =
        decode_or_bail(resp, "decode spec task link response").await?;
    print_spec_mutation("linked spec task", &data.data, &data.provenance);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_comment_add(
    artifact_id: String,
    body: Option<String>,
    body_file: Option<PathBuf>,
    target_kind: String,
    target_id: Option<String>,
    child_address: Option<String>,
    parent_comment_id: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("target_kind", &target_kind)?;
    let target_id = target_id.unwrap_or_else(|| artifact_id.clone());
    require_nonempty("target_id", &target_id)?;
    let body = resolve_body(body, body_file)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/comments",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id)
    );
    let req = CreateCommentRequest {
        target_kind: &target_kind,
        target_id: &target_id,
        child_address: child_address.as_deref(),
        parent_comment_id: parent_comment_id.as_deref(),
        body: &body,
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "comment.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/artifacts/:artifact_id/comments")?;
    let data: ArtifactMutationResponse<ArtifactComment> =
        decode_or_bail(resp, "decode artifact comment response").await?;
    print_comment_mutation("commented", &data.data, &data.provenance);
    Ok(())
}

async fn cmd_comment_resolve(
    artifact_id: String,
    comment_id: String,
    note: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("comment_id", &comment_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/comments/{}/resolve",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id),
        encode_query_component(&comment_id)
    );
    let req = ResolveCommentRequest {
        resolution_note: note.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "comment.resolve",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/artifacts/:artifact_id/comments/:comment_id/resolve")?;
    let data: ArtifactMutationResponse<ArtifactComment> =
        decode_or_bail(resp, "decode artifact comment resolve response").await?;
    print_comment_mutation("resolved", &data.data, &data.provenance);
    Ok(())
}

async fn cmd_comment_reopen(
    artifact_id: String,
    comment_id: String,
    note: Option<String>,
    actor_display_name: Option<String>,
    mutation: MutationOptions,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    require_nonempty("artifact_id", &artifact_id)?;
    require_nonempty("comment_id", &comment_id)?;
    let ctx = resolve_context(project, agent_id)?;
    ensure_registered(&ctx).await?;
    let url = format!(
        "{}/comments/{}/reopen",
        artifact_url(&ctx.gateway_url, &ctx.ident, &artifact_id),
        encode_query_component(&comment_id)
    );
    let req = ReopenCommentRequest {
        note_body: note.as_deref(),
        actor_display_name: actor_display_name.as_deref(),
    };
    let resp = mutation_request(
        &ctx,
        ctx.client.post(&url).json(&req),
        "comment.write",
        mutation,
    )
    .send()
    .await
    .context("POST /v1/projects/:ident/artifacts/:artifact_id/comments/:comment_id/reopen")?;
    let data: ArtifactMutationResponse<ArtifactComment> =
        decode_or_bail(resp, "decode artifact comment reopen response").await?;
    print_comment_mutation("reopened", &data.data, &data.provenance);
    Ok(())
}

fn resolve_context(
    project: Option<String>,
    agent_id_override: Option<String>,
) -> Result<ArtifactContext> {
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
    let api_key = api_key.trim().to_string();
    validate_api_key(&api_key).map_err(anyhow::Error::msg)?;
    let timeout_ms = config.gateway.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .context("build reqwest client")?;

    Ok(ArtifactContext {
        ident,
        canonical_ident,
        agent_id,
        client,
        gateway_url,
        api_key,
        host: gethostname::gethostname().to_str().map(str::to_string),
        agent_system: detect_agent_system(),
    })
}

async fn ensure_registered(ctx: &ArtifactContext) -> Result<()> {
    if read_registration_marker(&ctx.canonical_ident, &ctx.gateway_url).is_some() {
        return Ok(());
    }
    let url = format!("{}/v1/projects", ctx.gateway_url);
    let resp = ctx
        .client
        .post(&url)
        .header("Authorization", auth_header(ctx))
        .json(&RegisterProjectRequest {
            ident: &ctx.ident,
            channel: None,
        })
        .send()
        .await
        .context("POST /v1/projects")?;
    let registered: RegisterProjectResponse =
        decode_or_bail(resp, "decode register response").await?;
    write_registration_marker(
        &ctx.canonical_ident,
        &ctx.gateway_url,
        &registered.channel_name,
    )?;
    Ok(())
}

fn read_request(ctx: &ArtifactContext, builder: RequestBuilder, scopes: &str) -> RequestBuilder {
    base_request_headers(ctx, builder, scopes)
}

fn mutation_request(
    ctx: &ArtifactContext,
    builder: RequestBuilder,
    scopes: &str,
    mutation: MutationOptions,
) -> RequestBuilder {
    let key = mutation
        .idempotency_key
        .unwrap_or_else(|| default_idempotency_key("artifact", &ctx.agent_id));
    let mut builder = base_request_headers(ctx, builder, scopes)
        .header("Idempotency-Key", key)
        .header("X-Actor-Type", "agent")
        .header("X-Agent-System", &ctx.agent_system);
    if let Some(run_id) = mutation.workflow_run_id {
        builder = builder.header("X-Workflow-Run-Id", run_id);
    }
    builder
}

fn base_request_headers(
    ctx: &ArtifactContext,
    builder: RequestBuilder,
    scopes: &str,
) -> RequestBuilder {
    let mut builder = builder
        .header("Authorization", auth_header(ctx))
        .header("X-Agent-Id", &ctx.agent_id)
        .header("X-Agent-Project", &ctx.ident)
        .header("X-Agent-Scopes", scopes);
    if let Some(host) = ctx.host.as_deref() {
        builder = builder.header("X-Host", host);
    }
    builder
}

fn auth_header(ctx: &ArtifactContext) -> String {
    format!("Bearer {}", ctx.api_key)
}

async fn decode_or_bail<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
    context: &str,
) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context(context.to_string())
}

fn build_artifacts_url(base_url: &str, ident: &str, filters: &ArtifactFilters<'_>) -> String {
    let mut url = format!("{base_url}/v1/projects/{ident}/artifacts");
    let mut parts = Vec::new();
    push_query(&mut parts, "kind", filters.kind);
    push_query(&mut parts, "subkind", filters.subkind);
    push_query(&mut parts, "lifecycle_state", filters.status);
    push_query(&mut parts, "label", filters.label);
    push_query(&mut parts, "actor_id", filters.actor);
    push_query(&mut parts, "q", filters.query);
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    url
}

fn artifact_url(base_url: &str, ident: &str, artifact_id: &str) -> String {
    format!(
        "{base_url}/v1/projects/{ident}/artifacts/{}",
        encode_query_component(artifact_id)
    )
}

fn push_query(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        if !value.trim().is_empty() {
            parts.push(format!("{key}={}", encode_query_component(value)));
        }
    }
}

fn encode_query_component(raw: &str) -> String {
    let mut out = String::new();
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn resolve_body(body: Option<String>, body_file: Option<PathBuf>) -> Result<String> {
    match (body, body_file) {
        (Some(_), Some(_)) => anyhow::bail!("use either --body or --body-file, not both"),
        (Some(body), None) => {
            require_nonempty("--body", &body)?;
            Ok(body)
        }
        (None, Some(path)) => {
            let body =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            require_nonempty("--body-file", &body)?;
            Ok(body)
        }
        (None, None) => anyhow::bail!("provide --body or --body-file"),
    }
}

fn resolve_optional_body(
    body: Option<String>,
    body_file: Option<PathBuf>,
) -> Result<Option<String>> {
    match (body, body_file) {
        (Some(_), Some(_)) => anyhow::bail!("use either --body or --body-file, not both"),
        (Some(body), None) => {
            require_nonempty("--body", &body)?;
            Ok(Some(body))
        }
        (None, Some(path)) => {
            let body =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            require_nonempty("--body-file", &body)?;
            Ok(Some(body))
        }
        (None, None) => Ok(None),
    }
}

fn resolve_json_arg(
    name: &str,
    raw: Option<String>,
    file: Option<PathBuf>,
) -> Result<Option<Value>> {
    match (raw, file) {
        (Some(_), Some(_)) => anyhow::bail!("use either {name} or {name}-file, not both"),
        (Some(raw), None) => {
            require_nonempty(name, &raw)?;
            parse_json_value(name, &raw).map(Some)
        }
        (None, Some(path)) => {
            let text =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            require_nonempty(name, &text)?;
            parse_json_value(name, &text).map(Some)
        }
        (None, None) => Ok(None),
    }
}

fn parse_json_value(name: &str, text: &str) -> Result<Value> {
    serde_json::from_str(text).with_context(|| format!("parse {name} as JSON"))
}

fn load_spec_source(path: &Path) -> Result<PreparedSpecSource> {
    let manifest_path = if path.is_dir() {
        find_manifest_file(path)?
    } else {
        path.to_path_buf()
    };
    let base_dir = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest = parse_manifest_value(&manifest_path, &text)?;
    let mut file_bodies = HashMap::new();
    collect_spec_file_bodies(&manifest, base_dir, &mut file_bodies)?;
    let title = manifest
        .pointer("/metadata/title")
        .and_then(Value::as_str)
        .or_else(|| manifest.get("title").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| {
            manifest_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("spec")
                .to_string()
        });
    let source_doc = manifest
        .get("source_doc")
        .and_then(Value::as_str)
        .map(str::to_string);
    let body = Some(format!(
        "Imported spec manifest from {} with {} embedded spec file(s).",
        manifest_path.display(),
        file_bodies.len()
    ));
    Ok(PreparedSpecSource {
        title,
        manifest,
        file_bodies,
        source_doc,
        body,
    })
}

fn find_manifest_file(dir: &Path) -> Result<PathBuf> {
    for name in ["manifest.yaml", "manifest.yml", "manifest.json"] {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "no manifest.yaml, manifest.yml, or manifest.json found in {}",
        dir.display()
    );
}

fn parse_manifest_value(path: &Path, text: &str) -> Result<Value> {
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
        other => anyhow::bail!("unsupported manifest extension .{other}; use JSON, YAML, or YML"),
    }
}

fn collect_spec_file_bodies(
    manifest: &Value,
    base_dir: &Path,
    out: &mut HashMap<String, String>,
) -> Result<()> {
    if let Some(items) = manifest.get("items").and_then(Value::as_array) {
        for item in items {
            collect_spec_file_body(item, base_dir, out)?;
        }
    }
    if let Some(phases) = manifest.get("phases").and_then(Value::as_array) {
        for phase in phases {
            if let Some(tasks) = phase.get("tasks").and_then(Value::as_array) {
                for task in tasks {
                    collect_spec_file_body(task, base_dir, out)?;
                }
            }
        }
    }
    Ok(())
}

fn collect_spec_file_body(
    item: &Value,
    base_dir: &Path,
    out: &mut HashMap<String, String>,
) -> Result<()> {
    let Some(spec_file) = item
        .get("spec_file")
        .or_else(|| item.get("file_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };
    if out.contains_key(spec_file) {
        return Ok(());
    }
    let path = base_dir.join(spec_file);
    if path.exists() {
        let body = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        out.insert(spec_file.to_string(), body);
    }
    Ok(())
}

fn require_nonempty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(())
}

fn detect_agent_system() -> String {
    env::var("AGENT_TOOLS_AGENT_SYSTEM")
        .or_else(|_| env::var("AGENT_SYSTEM"))
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "claude" | "codex" | "gemini" | "other"))
        .unwrap_or_else(|| "other".to_string())
}

fn default_idempotency_key(prefix: &str, agent_id: &str) -> String {
    let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos();
    format!("{prefix}-{agent_id}-{nanos}")
}

fn print_artifact_list(
    project_ident: &str,
    artifacts: &[ArtifactSummary],
    chunking_status: Option<&ChunkingStatus>,
) {
    println!("Artifacts for project {project_ident}");
    if artifacts.is_empty() {
        println!("(none)");
    }
    for artifact in artifacts {
        print_artifact_summary(artifact, 2);
    }
    print_chunking_status(chunking_status, 0);
}

fn print_artifact_detail(detail: &ArtifactDetail, chunking_status: Option<&ChunkingStatus>) {
    println!(
        "[{}] {}",
        detail.artifact.artifact_id, detail.artifact.title
    );
    print_artifact_summary(&detail.artifact, 0);
    print_chunking_status(chunking_status, 0);
    if let Some(version) = detail.current_version.as_ref() {
        println!();
        println!("current_version:");
        print_version(version, 2, true);
    }
    if let Some(version) = detail.accepted_version.as_ref() {
        if Some(version.artifact_version_id.as_str())
            != detail.artifact.current_version_id.as_deref()
        {
            println!();
            println!("accepted_version:");
            print_version(version, 2, true);
        }
    }
}

fn print_artifact_versions(
    artifact_id: &str,
    versions: &[ArtifactVersion],
    chunking_status: Option<&ChunkingStatus>,
) {
    println!("Versions for artifact {artifact_id}");
    if versions.is_empty() {
        println!("(none)");
    }
    for version in versions {
        print_version(version, 2, false);
    }
    print_chunking_status(chunking_status, 0);
}

fn print_artifact_diff(
    artifact_id: &str,
    diff: &ArtifactDiff,
    chunking_status: Option<&ChunkingStatus>,
) {
    println!("Artifact diff for {artifact_id}");
    println!(
        "from_version_id: {}",
        diff.from_version_id.as_deref().unwrap_or("(none)")
    );
    println!("to_version_id: {}", diff.to_version_id);
    println!("format: {}", diff.format);
    println!("byte_delta: {}", diff.byte_delta);
    print_chunking_status(diff.chunking_status.as_ref().or(chunking_status), 0);
    println!();
    print!("{}", diff.diff);
    if !diff.diff.ends_with('\n') {
        println!();
    }
}

fn print_artifact_comments(
    artifact_id: &str,
    comments: &[ArtifactComment],
    chunking_status: Option<&ChunkingStatus>,
) {
    println!("Comments for artifact {artifact_id}");
    if comments.is_empty() {
        println!("(none)");
    }
    for comment in comments {
        print_comment(comment, 2);
    }
    print_chunking_status(chunking_status, 0);
}

fn print_comment_mutation(
    action: &str,
    comment: &ArtifactComment,
    provenance: &ArtifactProvenance,
) {
    println!("{action} artifact comment [{}]", comment.comment_id);
    println!("artifact_id: {}", comment.artifact_id);
    println!("target: {} {}", comment.target_kind, comment.target_id);
    println!("state: {}", comment.state);
    print_provenance(provenance);
}

fn print_review_mutation(action: &str, data: &Value, provenance: &ArtifactProvenance) {
    println!("{action}");
    print_provenance(provenance);
    println!("data:");
    print_indented(&render_pretty_value(data), 2);
}

fn print_review_contributions(
    artifact_id: &str,
    contributions: &[Value],
    chunking_status: Option<&ChunkingStatus>,
) {
    println!("Design-review contributions for artifact {artifact_id}");
    if contributions.is_empty() {
        println!("(none)");
    }
    for contribution in contributions {
        let id = contribution
            .get("contribution_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let phase = contribution
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let role = contribution
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let target = contribution
            .get("target_id")
            .and_then(Value::as_str)
            .unwrap_or("-");
        println!("  [{id}] phase={phase} role={role} target={target}");
        print_indented(&render_pretty_value(contribution), 4);
    }
    print_chunking_status(chunking_status, 0);
}

fn print_spec_mutation(action: &str, data: &Value, provenance: &ArtifactProvenance) {
    println!("{action}");
    print_provenance(provenance);
    println!("data:");
    print_indented(&render_pretty_value(data), 2);
}

fn print_spec_read(label: &str, data: &Value, chunking_status: Option<&ChunkingStatus>) {
    println!("{label}:");
    print_chunking_status(chunking_status, 0);
    print_indented(&render_pretty_value(data), 2);
}

fn print_artifact_summary(artifact: &ArtifactSummary, indent: usize) {
    let pad = " ".repeat(indent);
    println!("{pad}[{}] {}", artifact.artifact_id, artifact.title);
    println!("{pad}project_ident: {}", artifact.project_ident);
    println!("{pad}kind: {}", artifact.kind);
    if let Some(subkind) = artifact.subkind.as_deref() {
        println!("{pad}subkind: {subkind}");
    }
    println!(
        "{pad}state: lifecycle={} review={} implementation={}",
        artifact.lifecycle_state, artifact.review_state, artifact.implementation_state
    );
    println!(
        "{pad}current_version_id: {}",
        artifact.current_version_id.as_deref().unwrap_or("-")
    );
    println!(
        "{pad}accepted_version_id: {}",
        artifact.accepted_version_id.as_deref().unwrap_or("-")
    );
    println!("{pad}created_by_actor_id: {}", artifact.created_by_actor_id);
    println!("{pad}created_at: {}", artifact.created_at);
    println!("{pad}updated_at: {}", artifact.updated_at);
    if !artifact.labels.is_empty() {
        println!("{pad}labels: {}", artifact.labels.join(", "));
    }
}

fn print_version(version: &ArtifactVersion, indent: usize, include_body: bool) {
    let pad = " ".repeat(indent);
    println!("{pad}[{}]", version.artifact_version_id);
    println!("{pad}artifact_id: {}", version.artifact_id);
    println!("{pad}version_state: {}", version.version_state);
    println!("{pad}body_format: {}", version.body_format);
    if let Some(label) = version.version_label.as_deref() {
        println!("{pad}version_label: {label}");
    }
    println!(
        "{pad}parent_version_id: {}",
        version.parent_version_id.as_deref().unwrap_or("-")
    );
    println!(
        "{pad}source_format: {}",
        version.source_format.as_deref().unwrap_or("-")
    );
    println!("{pad}created_by_actor_id: {}", version.created_by_actor_id);
    println!(
        "{pad}created_via_workflow_run_id: {}",
        version
            .created_via_workflow_run_id
            .as_deref()
            .unwrap_or("-")
    );
    println!(
        "{pad}idempotency_key: {}",
        version.idempotency_key.as_deref().unwrap_or("-")
    );
    println!(
        "{pad}body_purged_at: {}",
        version
            .body_purged_at
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("{pad}created_at: {}", version.created_at);
    print_chunking_status(version.chunking_status.as_ref(), indent);
    if include_body {
        if let Some(payload) = version.structured_payload.as_ref() {
            println!("{pad}structured_payload:");
            print_indented(
                &serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string()),
                indent + 2,
            );
        }
        if let Some(body) = version.body.as_deref() {
            println!("{pad}body:");
            print_indented(body, indent + 2);
        }
    }
}

fn print_comment(comment: &ArtifactComment, indent: usize) {
    let pad = " ".repeat(indent);
    println!("{pad}[{}] {}", comment.comment_id, comment.state);
    println!("{pad}target: {} {}", comment.target_kind, comment.target_id);
    if let Some(addr) = comment.child_address.as_deref() {
        println!("{pad}child_address: {addr}");
    }
    println!(
        "{pad}parent_comment_id: {}",
        comment.parent_comment_id.as_deref().unwrap_or("-")
    );
    println!("{pad}actor_id: {}", comment.actor_id);
    println!(
        "{pad}resolved_by_actor_id: {}",
        comment.resolved_by_actor_id.as_deref().unwrap_or("-")
    );
    println!(
        "{pad}resolved_by_workflow_run_id: {}",
        comment
            .resolved_by_workflow_run_id
            .as_deref()
            .unwrap_or("-")
    );
    println!(
        "{pad}resolved_at: {}",
        comment
            .resolved_at
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "{pad}idempotency_key: {}",
        comment.idempotency_key.as_deref().unwrap_or("-")
    );
    println!("{pad}created_at: {}", comment.created_at);
    println!("{pad}updated_at: {}", comment.updated_at);
    if let Some(note) = comment.resolution_note.as_deref() {
        println!("{pad}resolution_note:");
        print_indented(note, indent + 2);
    }
    println!("{pad}body:");
    print_indented(&comment.body, indent + 2);
}

fn print_chunking_status(status: Option<&ChunkingStatus>, indent: usize) {
    if let Some(status) = status {
        let pad = " ".repeat(indent);
        println!(
            "{pad}chunking_status: status={} current={} stale={} superseded={} failed={}",
            status.status,
            status.current_chunk_count,
            status.stale_chunk_count,
            status.superseded_chunk_count,
            status.failed_addresses.len()
        );
        if !status.failed_addresses.is_empty() {
            println!(
                "{pad}failed_addresses: {}",
                status.failed_addresses.join(", ")
            );
        }
    }
}

fn print_provenance(provenance: &ArtifactProvenance) {
    println!("provenance:");
    println!("  idempotency_key: {}", provenance.idempotency_key);
    println!("  request_id: {}", provenance.request_id);
    println!("  created_at: {}", provenance.created_at);
    println!("  replay: {}", provenance.replay);
    println!(
        "  workflow_run_id: {}",
        provenance.workflow_run_id.as_deref().unwrap_or("-")
    );
    println!("  actor: {}", render_value(&provenance.actor));
    println!(
        "  authorization: {}",
        render_value(&provenance.authorization)
    );
    println!(
        "  generated_resources: {}",
        render_value(&provenance.generated_resources)
    );
    if !provenance.warnings.is_empty() {
        println!("  warnings: {}", provenance.warnings.join(", "));
    }
}

fn print_indented(text: &str, indent: usize) {
    let pad = " ".repeat(indent);
    for line in text.lines() {
        println!("{pad}{line}");
    }
}

fn render_pretty_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_url_encodes_filters() {
        let filters = ArtifactFilters {
            kind: Some("documentation"),
            subkind: Some("api/context"),
            status: Some("active"),
            label: Some("agent docs"),
            actor: Some("actor#1"),
            query: Some("gateway smoke"),
        };
        assert_eq!(
            build_artifacts_url("https://gateway.example", "agent-tools", &filters),
            "https://gateway.example/v1/projects/agent-tools/artifacts?kind=documentation&subkind=api%2Fcontext&lifecycle_state=active&label=agent%20docs&actor_id=actor%231&q=gateway%20smoke"
        );
    }

    #[test]
    fn artifact_url_encodes_artifact_id() {
        assert_eq!(
            artifact_url("https://gateway.example", "agent-tools", "artifact/id#1"),
            "https://gateway.example/v1/projects/agent-tools/artifacts/artifact%2Fid%231"
        );
    }

    #[test]
    fn comment_request_omits_unset_optional_fields() {
        let req = CreateCommentRequest {
            target_kind: "artifact",
            target_id: "a1",
            child_address: None,
            parent_comment_id: None,
            body: "review note",
            actor_display_name: None,
        };
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["target_kind"], "artifact");
        assert_eq!(json["target_id"], "a1");
        assert_eq!(json["body"], "review note");
        assert!(json.get("child_address").is_none());
        assert!(json.get("parent_comment_id").is_none());
        assert!(json.get("actor_display_name").is_none());
    }

    #[test]
    fn review_create_request_omits_empty_labels_and_optional_source() {
        let req = DesignReviewCreateRequest {
            title: "Gateway review",
            labels: None,
            body: None,
            body_format: "markdown",
            source_artifact_id: None,
            source_artifact_version_id: None,
            actor_display_name: None,
        };
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["title"], "Gateway review");
        assert_eq!(json["body_format"], "markdown");
        assert!(json.get("labels").is_none());
        assert!(json.get("source_artifact_id").is_none());
    }

    #[test]
    fn review_contributions_query_uses_expected_filter_names() {
        let mut parts = Vec::new();
        push_query(&mut parts, "round_id", Some("round 1"));
        push_query(&mut parts, "phase", Some("pass_2"));
        push_query(&mut parts, "read_set_contains", Some("contrib/id"));
        assert_eq!(
            parts.join("&"),
            "round_id=round%201&phase=pass_2&read_set_contains=contrib%2Fid"
        );
    }

    #[test]
    fn spec_import_request_omits_empty_file_bodies_and_labels() {
        let manifest = serde_json::json!({"items": [{"id": "T001", "title": "Do it"}]});
        let req = SpecImportRequest {
            title: "Spec",
            labels: None,
            body: None,
            manifest: &manifest,
            file_bodies: None,
            source_doc: None,
            source_artifact_id: None,
            source_artifact_version_id: None,
            actor_display_name: None,
        };
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["title"], "Spec");
        assert!(json.get("labels").is_none());
        assert!(json.get("file_bodies").is_none());
    }

    #[test]
    fn spec_file_body_collection_reads_phase_task_specs() {
        let root = std::env::temp_dir().join(format!(
            "agent-tools-spec-test-{}",
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        fs::create_dir_all(root.join("backend")).unwrap();
        fs::write(root.join("backend/T001.md"), "task body").unwrap();
        let manifest = serde_json::json!({
            "phases": [{
                "tasks": [{
                    "id": "T001",
                    "title": "Implement",
                    "spec_file": "backend/T001.md"
                }]
            }]
        });
        let mut bodies = HashMap::new();
        collect_spec_file_bodies(&manifest, &root, &mut bodies).unwrap();
        assert_eq!(bodies["backend/T001.md"], "task body");
        fs::remove_file(root.join("backend/T001.md")).ok();
        fs::remove_dir(root.join("backend")).ok();
        fs::remove_dir(root).ok();
    }

    #[test]
    fn idempotency_key_is_namespaced() {
        let key = default_idempotency_key("artifact", "agent-1");
        assert!(key.starts_with("artifact-agent-1-"));
    }
}
