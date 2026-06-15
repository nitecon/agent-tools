//! API docs registry client for agent-native gateway context.

#![allow(dead_code)]

use crate::gateway::GatewayClient;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiDocSummary {
    pub id: String,
    pub app: String,
    pub title: String,
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub parent_page: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub order: Option<i64>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub breadcrumbs: Vec<String>,
    #[serde(default)]
    pub page_id: Option<String>,
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub source_format: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub updated_at: Option<i64>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub artifact_version_id: Option<String>,
    #[serde(default)]
    pub accepted_version_id: Option<String>,
    #[serde(default)]
    pub subkind: Option<String>,
    #[serde(default)]
    pub manifest_chunk_count: Option<usize>,
    #[serde(default)]
    pub chunking_status: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub retrieval_scope: Option<String>,
    #[serde(default)]
    pub global_rank: Option<i64>,
    #[serde(default)]
    pub global_descendants: Option<bool>,
    #[serde(default)]
    pub owner_project: Option<String>,
    #[serde(default)]
    pub wiki_path: Option<String>,
    #[serde(default)]
    pub linked_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiDoc {
    #[serde(flatten)]
    pub summary: ApiDocSummary,
    pub content: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiDocChunk {
    #[serde(default)]
    pub doc_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub breadcrumbs: Vec<String>,
    #[serde(default)]
    pub page_id: Option<String>,
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub artifact_version_id: Option<String>,
    #[serde(default)]
    pub accepted_version_id: Option<String>,
    #[serde(default)]
    pub child_address: Option<String>,
    #[serde(default)]
    pub subkind: Option<String>,
    #[serde(default)]
    pub freshness: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub retrieval_scope: Option<String>,
    #[serde(default)]
    pub global_rank: Option<i64>,
    #[serde(default)]
    pub owner_project: Option<String>,
    #[serde(default)]
    pub wiki_path: Option<String>,
    #[serde(default)]
    pub chunking_status: Option<String>,
    #[serde(default)]
    pub linked_ids: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PublishApiDocRequest<'a> {
    pub app: &'a str,
    pub title: &'a str,
    pub content: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_page: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_rank: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_descendants: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<&'a str>,
    pub kind: &'a str,
    pub source_format: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<&'a str>,
}

#[derive(Default, Debug, Clone)]
pub struct ApiDocFilters<'a> {
    pub query: Option<&'a str>,
    pub app: Option<&'a str>,
    pub label: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub scope: Option<&'a str>,
}

#[derive(Default, Debug, Clone)]
pub struct ApiDocHierarchyFilters<'a> {
    pub query: Option<&'a str>,
    pub app: Option<&'a str>,
    pub space: Option<&'a str>,
    pub scope: Option<&'a str>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocumentationHierarchy {
    #[serde(default)]
    pub project_ident: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub spaces: Vec<DocumentationSpace>,
    #[serde(default)]
    pub pages: Vec<DocumentationNode>,
    #[serde(default)]
    pub placement_hints: Vec<String>,
    #[serde(default)]
    pub provenance: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocumentationSpace {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub order: Option<i64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub global_rank: Option<i64>,
    #[serde(default)]
    pub owner_project: Option<String>,
    #[serde(default)]
    pub wiki_path: Option<String>,
    #[serde(default)]
    pub pages: Vec<DocumentationNode>,
    #[serde(default)]
    pub placement_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocumentationNode {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub parent_page: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub order: Option<i64>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub global_rank: Option<i64>,
    #[serde(default)]
    pub global_descendants: Option<bool>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub owner_project: Option<String>,
    #[serde(default)]
    pub wiki_path: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub current_version_id: Option<String>,
    #[serde(default)]
    pub accepted_version_id: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub source_artifact_id: Option<String>,
    #[serde(default)]
    pub source_artifact_version_id: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub artifact_version_id: Option<String>,
    #[serde(default)]
    pub breadcrumbs: Vec<String>,
    #[serde(default)]
    pub sections: Vec<DocumentationNode>,
    #[serde(default)]
    pub children: Vec<DocumentationNode>,
    #[serde(default)]
    pub placement_hint: Option<String>,
}

impl GatewayClient {
    pub async fn list_api_docs(
        &self,
        ident: &str,
        filters: &ApiDocFilters<'_>,
        agent_id: Option<&str>,
    ) -> Result<Vec<ApiDocSummary>> {
        let url = build_api_docs_url(self.base_url(), ident, None, filters);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/api-docs")?;
        decode_or_bail(resp).await
    }

    pub async fn get_api_doc(
        &self,
        ident: &str,
        doc_id: &str,
        agent_id: Option<&str>,
    ) -> Result<ApiDoc> {
        let url = build_api_doc_url(self.base_url(), ident, doc_id);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/api-docs/:id")?;
        decode_or_bail(resp).await
    }

    pub async fn delete_api_doc(
        &self,
        ident: &str,
        doc_id: &str,
        agent_id: Option<&str>,
    ) -> Result<()> {
        let url = build_api_doc_url(self.base_url(), ident, doc_id);
        let builder = self
            .http_client()
            .delete(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("DELETE /v1/projects/:ident/api-docs/:id")?;
        empty_or_bail(resp).await
    }

    pub async fn publish_api_doc(
        &self,
        ident: &str,
        req: &PublishApiDocRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<ApiDoc> {
        let url = format!("{}/v1/projects/{}/api-docs", self.base_url(), ident);
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/api-docs")?;
        decode_or_bail(resp).await
    }

    pub async fn api_doc_chunks(
        &self,
        ident: &str,
        filters: &ApiDocFilters<'_>,
        agent_id: Option<&str>,
    ) -> Result<Vec<ApiDocChunk>> {
        let url = build_api_docs_url(self.base_url(), ident, Some("chunks"), filters);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/api-docs/chunks")?;
        decode_or_bail(resp).await
    }

    pub async fn api_doc_hierarchy(
        &self,
        ident: &str,
        filters: &ApiDocHierarchyFilters<'_>,
        agent_id: Option<&str>,
    ) -> Result<DocumentationHierarchy> {
        let url = build_api_doc_hierarchy_url(self.base_url(), ident, filters);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/api-docs/hierarchy")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(DocumentationHierarchy::default());
        }
        let value: Value = decode_or_bail(resp).await?;
        decode_hierarchy_payload(value, ident, filters)
    }
}

fn decode_hierarchy_payload(
    value: Value,
    ident: &str,
    filters: &ApiDocHierarchyFilters<'_>,
) -> Result<DocumentationHierarchy> {
    let payload = value.get("data").cloned().unwrap_or(value);
    if payload.is_array() {
        let pages: Vec<DocumentationNode> =
            serde_json::from_value(payload).context("decode api-docs hierarchy node array")?;
        return Ok(DocumentationHierarchy {
            project_ident: Some(ident.to_string()),
            app: filters.app.map(str::to_string),
            scope: filters.scope.map(str::to_string),
            pages,
            ..Default::default()
        });
    }
    serde_json::from_value(payload).context("decode api-docs hierarchy response")
}

async fn decode_or_bail<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context("decode api-docs response")
}

async fn empty_or_bail(resp: reqwest::Response) -> Result<()> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    Ok(())
}

fn build_api_docs_url(
    base_url: &str,
    ident: &str,
    suffix: Option<&str>,
    filters: &ApiDocFilters<'_>,
) -> String {
    let mut url = format!("{base_url}/v1/projects/{ident}/api-docs");
    if let Some(suffix) = suffix {
        url.push('/');
        url.push_str(suffix);
    }
    let mut parts = Vec::new();
    push_query(&mut parts, "q", filters.query);
    push_query(&mut parts, "app", filters.app);
    push_query(&mut parts, "label", filters.label);
    push_query(&mut parts, "kind", filters.kind);
    push_query(&mut parts, "scope", filters.scope);
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    url
}

fn build_api_doc_url(base_url: &str, ident: &str, doc_id: &str) -> String {
    format!(
        "{base_url}/v1/projects/{ident}/api-docs/{}",
        encode_query_component(doc_id)
    )
}

fn build_api_doc_hierarchy_url(
    base_url: &str,
    ident: &str,
    filters: &ApiDocHierarchyFilters<'_>,
) -> String {
    let mut url = format!("{base_url}/v1/projects/{ident}/api-docs/hierarchy");
    let mut parts = Vec::new();
    push_query(&mut parts, "q", filters.query);
    push_query(&mut parts, "app", filters.app);
    push_query(&mut parts, "space", filters.space);
    push_query(&mut parts, "scope", filters.scope);
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    url
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_request_defaults_to_agent_context_fields() {
        let content = serde_json::json!({"purpose": "Expose billing actions"});
        let req = PublishApiDocRequest {
            app: "billing",
            title: "Billing API context",
            content: &content,
            space: None,
            category: None,
            parent_page: None,
            parent_id: None,
            slug: None,
            order: None,
            sort_order: None,
            global_rank: None,
            global_descendants: None,
            summary: None,
            kind: "agent_context",
            source_format: "agent_context",
            source_ref: None,
            version: None,
            labels: None,
            author: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["app"], "billing");
        assert_eq!(json["title"], "Billing API context");
        assert_eq!(json["kind"], "agent_context");
        assert_eq!(json["source_format"], "agent_context");
        assert!(json.get("labels").is_none());
    }

    #[test]
    fn publish_request_serializes_hierarchy_and_global_fields() {
        let content = serde_json::json!({"purpose": "Expose billing actions"});
        let req = PublishApiDocRequest {
            app: "billing",
            title: "Billing API context",
            content: &content,
            space: Some("apis"),
            category: None,
            parent_page: None,
            parent_id: Some("page-1"),
            slug: Some("billing-api"),
            order: Some(10),
            sort_order: Some(20),
            global_rank: Some(2),
            global_descendants: Some(true),
            summary: None,
            kind: "agent_context",
            source_format: "agent_context",
            source_ref: None,
            version: None,
            labels: None,
            author: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["space"], "apis");
        assert_eq!(json["parent_id"], "page-1");
        assert_eq!(json["slug"], "billing-api");
        assert_eq!(json["order"], 10);
        assert_eq!(json["sort_order"], 20);
        assert_eq!(json["global_rank"], 2);
        assert_eq!(json["global_descendants"], true);
    }

    #[test]
    fn api_doc_summary_accepts_artifact_metadata_additively() {
        let json = serde_json::json!({
            "id": "doc-1",
            "project_ident": "agent-tools",
            "app": "gateway",
            "title": "Gateway",
            "kind": "agent_context",
            "source_format": "agent_context",
            "labels": [],
            "author": "tester",
            "space": "apis",
            "slug": "gateway",
            "breadcrumbs": ["Documentation", "APIs", "Gateway"],
            "artifact_id": "art-1",
            "artifact_version_id": "ver-1",
            "scope": "global",
            "global_rank": 1,
            "owner_project": "agent-tools",
            "wiki_path": "/Documentation/APIs/Gateway",
            "chunking_status": "current"
        });
        let summary: ApiDocSummary = serde_json::from_value(json).unwrap();
        assert_eq!(summary.space.as_deref(), Some("apis"));
        assert_eq!(summary.slug.as_deref(), Some("gateway"));
        assert_eq!(
            summary.breadcrumbs,
            vec!["Documentation", "APIs", "Gateway"]
        );
        assert_eq!(summary.artifact_id.as_deref(), Some("art-1"));
        assert_eq!(summary.artifact_version_id.as_deref(), Some("ver-1"));
        assert_eq!(summary.scope.as_deref(), Some("global"));
        assert_eq!(summary.global_rank, Some(1));
        assert_eq!(summary.owner_project.as_deref(), Some("agent-tools"));
        assert_eq!(
            summary.wiki_path.as_deref(),
            Some("/Documentation/APIs/Gateway")
        );
        assert_eq!(summary.chunking_status.as_deref(), Some("current"));
    }

    #[test]
    fn api_docs_url_encodes_filters() {
        let filters = ApiDocFilters {
            query: Some("billing auth"),
            app: Some("billing/api"),
            label: Some("internal"),
            kind: Some("agent_context"),
            scope: Some("global"),
        };
        assert_eq!(
            build_api_docs_url("https://gateway.example", "agent-tools", None, &filters),
            "https://gateway.example/v1/projects/agent-tools/api-docs?q=billing%20auth&app=billing%2Fapi&label=internal&kind=agent_context&scope=global"
        );
    }

    #[test]
    fn chunks_url_uses_chunks_endpoint() {
        let filters = ApiDocFilters {
            query: Some("refund"),
            ..Default::default()
        };
        assert_eq!(
            build_api_docs_url(
                "https://gateway.example",
                "agent-tools",
                Some("chunks"),
                &filters
            ),
            "https://gateway.example/v1/projects/agent-tools/api-docs/chunks?q=refund"
        );
    }

    #[test]
    fn api_doc_url_encodes_doc_id_for_lifecycle_calls() {
        assert_eq!(
            build_api_doc_url("https://gateway.example", "agent-tools", "doc/id#1"),
            "https://gateway.example/v1/projects/agent-tools/api-docs/doc%2Fid%231"
        );
    }

    #[test]
    fn hierarchy_url_encodes_filters() {
        let filters = ApiDocHierarchyFilters {
            query: Some("setup hooks"),
            app: Some("agent/tools"),
            space: Some("API Context"),
            scope: Some("all"),
        };
        assert_eq!(
            build_api_doc_hierarchy_url("https://gateway.example", "agent-tools", &filters),
            "https://gateway.example/v1/projects/agent-tools/api-docs/hierarchy?q=setup%20hooks&app=agent%2Ftools&space=API%20Context&scope=all"
        );
    }

    #[test]
    fn hierarchy_decoder_accepts_gateway_v1_13_node_array() {
        let value = serde_json::json!([
            {
                "id": "doc-1",
                "node_type": "page",
                "owner_project": "agent-gateway",
                "scope": "local",
                "app": "agent-gateway",
                "title": "Agent Gateway API context",
                "summary": "Gateway docs",
                "kind": "agent_context",
                "labels": ["gateway", "api-docs"],
                "parent_id": null,
                "slug": "agent-gateway-api-context",
                "sort_order": 0,
                "wiki_path": "/agent-gateway-api-context",
                "breadcrumbs": ["agent-gateway-api-context"],
                "global_rank": null,
                "direct_global_rank": null,
                "global_descendants": false,
                "artifact_id": "art-1",
                "artifact_version_id": "ver-1",
                "children": []
            }
        ]);
        let filters = ApiDocHierarchyFilters {
            app: Some("agent-gateway"),
            scope: Some("all"),
            ..Default::default()
        };
        let hierarchy = decode_hierarchy_payload(value, "agent-gateway", &filters).unwrap();
        assert_eq!(hierarchy.project_ident.as_deref(), Some("agent-gateway"));
        assert_eq!(hierarchy.app.as_deref(), Some("agent-gateway"));
        assert_eq!(hierarchy.scope.as_deref(), Some("all"));
        assert_eq!(hierarchy.pages.len(), 1);
        let node = &hierarchy.pages[0];
        assert_eq!(node.node_type.as_deref(), Some("page"));
        assert_eq!(node.kind.as_deref(), Some("agent_context"));
        assert_eq!(node.artifact_id.as_deref(), Some("art-1"));
        assert_eq!(node.artifact_version_id.as_deref(), Some("ver-1"));
        assert_eq!(
            node.wiki_path.as_deref(),
            Some("/agent-gateway-api-context")
        );
    }
}
