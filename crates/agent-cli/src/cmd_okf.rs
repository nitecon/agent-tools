use crate::cmd_gateway_context::{ensure_registered, resolve_context_for};
use agent_comms::docs::{ApiDocFilters, ApiDocSummary, PublishApiDocRequest};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
struct OkfProjection {
    app: String,
    title: String,
    summary: Option<String>,
    kind: String,
    source_format: String,
    source_ref: String,
    version: String,
    labels: Vec<String>,
    content: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectionPlan {
    source_ref: String,
    version: String,
    action: String,
    existing_id: Option<String>,
}

pub(crate) fn publish(
    path: PathBuf,
    dry_run: bool,
    project: Option<String>,
    agent_id: Option<String>,
) -> Result<()> {
    let root = std::env::current_dir()?;
    let project_id = project
        .clone()
        .unwrap_or_else(|| agent_core::project_ident(&root));
    let bundle =
        agent_knowledge::okf::parse_bundle(&path, agent_knowledge::okf::OkfLimits::default())?;
    let origin = path
        .canonicalize()?
        .strip_prefix(root.canonicalize()?)
        .context("OKF publish bundle must be inside the repository")?
        .to_string_lossy()
        .replace('\\', "/");
    let projections = build_projections(&bundle, &project_id, &origin)?;
    if dry_run {
        for decision in plan_projection(&projections, &[]) {
            println!("{}", serde_json::to_string(&decision)?);
        }
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let ctx = resolve_context_for("docs", agent_id)?;
        ensure_registered(&ctx, None).await?;
        let existing = ctx
            .gateway
            .list_api_docs(
                &ctx.ident,
                &ApiDocFilters {
                    kind: Some("okf"),
                    ..ApiDocFilters::default()
                },
                Some(&ctx.agent_id),
            )
            .await
            .context("list existing OKF Documentation")?;
        let plans = plan_projection(&projections, &existing);
        for (projection, decision) in projections.iter().zip(plans) {
            if decision.action == "reuse" {
                println!(
                    "reused Documentation [{}] {}",
                    decision.existing_id.as_deref().unwrap_or("unknown"),
                    projection.source_ref
                );
                continue;
            }
            let request = PublishApiDocRequest {
                app: &projection.app,
                title: &projection.title,
                content: &projection.content,
                space: Some("OKF"),
                category: Some(&projection.kind),
                parent_page: None,
                parent_id: None,
                slug: None,
                order: None,
                sort_order: None,
                global_rank: None,
                global_descendants: None,
                summary: projection.summary.as_deref(),
                kind: "okf",
                source_format: &projection.source_format,
                source_ref: Some(&projection.source_ref),
                version: Some(&projection.version),
                labels: Some(&projection.labels),
                author: None,
            };
            let published = ctx
                .gateway
                .publish_api_doc(&ctx.ident, &request, Some(&ctx.agent_id))
                .await
                .with_context(|| format!("publish {}", projection.source_ref))?;
            println!(
                "published Documentation [{}] {}",
                published.summary.id, projection.source_ref
            );
        }
        Ok(())
    })
}

fn build_projections(
    bundle: &agent_knowledge::okf::OkfBundle,
    project_id: &str,
    origin: &str,
) -> Result<Vec<OkfProjection>> {
    let mut projections = Vec::new();
    for concept in &bundle.concepts {
        let source_ref = format!("okf://{project_id}/{origin}/{}", concept.id);
        let version =
            blake3::hash(format!("{}\0{}", concept.raw_frontmatter, concept.body).as_bytes())
                .to_hex()
                .to_string();
        projections.push(OkfProjection {
            app: "okf".to_owned(),
            title: concept.title.clone(),
            summary: concept.description.clone(),
            kind: concept.kind.clone(),
            source_format: format!("okf/{}", bundle.version),
            source_ref,
            version: version.clone(),
            labels: concept.tags.clone(),
            content: json!({
                "canonical_uri": format!("okf://{project_id}/{origin}/{}", concept.id),
                "path_id": concept.id,
                "content_hash": version,
                "raw_frontmatter": concept.raw_frontmatter,
                "metadata": concept.metadata,
                "body": concept.body,
                "links": concept.links,
                "authority": "repository",
                "projection": "one-way",
                "attested_computation_executed": false,
            }),
        });
    }
    projections.sort_by(|left, right| left.source_ref.cmp(&right.source_ref));
    Ok(projections)
}

fn plan_projection(
    projections: &[OkfProjection],
    existing: &[ApiDocSummary],
) -> Vec<ProjectionPlan> {
    let by_source: BTreeMap<&str, &ApiDocSummary> = existing
        .iter()
        .filter_map(|doc| doc.source_ref.as_deref().map(|source| (source, doc)))
        .collect();
    projections
        .iter()
        .map(|projection| {
            let current = by_source.get(projection.source_ref.as_str()).copied();
            ProjectionPlan {
                source_ref: projection.source_ref.clone(),
                version: projection.version.clone(),
                action: if current.and_then(|doc| doc.version.as_deref())
                    == Some(projection.version.as_str())
                {
                    "reuse"
                } else if current.is_some() {
                    "update"
                } else {
                    "create"
                }
                .to_owned(),
                existing_id: current.map(|doc| doc.id.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_lossless_sorted_and_idempotency_plans_create_reuse_update() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/knowledge_graph/okf/v02");
        let bundle =
            agent_knowledge::okf::parse_bundle(&root, agent_knowledge::okf::OkfLimits::default())
                .unwrap();
        let projections = build_projections(&bundle, "fixture", ".agents/knowledge").unwrap();
        assert_eq!(projections.len(), 3);
        assert!(projections[1].content["raw_frontmatter"]
            .as_str()
            .unwrap()
            .contains("type:"));
        assert_eq!(plan_projection(&projections, &[])[0].action, "create");
        let existing = ApiDocSummary {
            id: "doc-1".to_owned(),
            app: "okf".to_owned(),
            title: projections[0].title.clone(),
            source_ref: Some(projections[0].source_ref.clone()),
            version: Some(projections[0].version.clone()),
            space: None,
            category: None,
            parent_page: None,
            parent_id: None,
            slug: None,
            order: None,
            sort_order: None,
            breadcrumbs: vec![],
            page_id: None,
            section_id: None,
            summary: None,
            kind: Some("okf".to_owned()),
            source_format: Some("okf/0.2".to_owned()),
            labels: vec![],
            author: None,
            updated_at: None,
            artifact_id: None,
            artifact_version_id: None,
            accepted_version_id: None,
            subkind: None,
            manifest_chunk_count: None,
            chunking_status: None,
            scope: None,
            retrieval_scope: None,
            global_rank: None,
            global_descendants: None,
            owner_project: None,
            wiki_path: None,
            linked_ids: vec![],
        };
        assert_eq!(
            plan_projection(&projections, std::slice::from_ref(&existing))[0].action,
            "reuse"
        );
        let mut changed = existing;
        changed.version = Some("old".to_owned());
        assert_eq!(
            plan_projection(&projections, &[changed])[0].action,
            "update"
        );
    }
}
