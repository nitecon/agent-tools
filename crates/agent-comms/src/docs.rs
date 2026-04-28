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
    pub kind: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub score: Option<f64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PublishApiDocRequest<'a> {
    pub app: &'a str,
    pub title: &'a str,
    pub content: &'a Value,
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
        let url = format!(
            "{}/v1/projects/{}/api-docs/{}",
            self.base_url(),
            ident,
            encode_query_component(doc_id)
        );
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
}

async fn decode_or_bail<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context("decode api-docs response")
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
    fn api_docs_url_encodes_filters() {
        let filters = ApiDocFilters {
            query: Some("billing auth"),
            app: Some("billing/api"),
            label: Some("internal"),
            kind: Some("agent_context"),
        };
        assert_eq!(
            build_api_docs_url("https://gateway.example", "agent-tools", None, &filters),
            "https://gateway.example/v1/projects/agent-tools/api-docs?q=billing%20auth&app=billing%2Fapi&label=internal&kind=agent_context"
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
}
