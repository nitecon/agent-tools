//! Global pattern-library API client.

use crate::gateway::GatewayClient;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pattern {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub version: String,
    pub state: String,
    pub author: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PatternSummary {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub summary: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub version: String,
    pub state: String,
    pub author: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub comment_count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PatternComment {
    pub id: String,
    pub pattern_id: String,
    pub author: String,
    pub author_type: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Serialize, Debug, Clone)]
pub struct CreatePatternRequest<'a> {
    pub title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<&'a str>,
    pub body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<&'a [String]>,
    pub version: &'a str,
    pub state: &'a str,
    pub author: &'a str,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct UpdatePatternRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<&'a str>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AddPatternCommentRequest<'a> {
    pub content: &'a str,
    pub author: &'a str,
    pub author_type: &'a str,
}

#[derive(Default, Debug, Clone)]
pub struct PatternFilters<'a> {
    pub query: Option<&'a str>,
    pub label: Option<&'a str>,
    pub category: Option<&'a str>,
    pub version: Option<&'a str>,
    pub state: Option<&'a str>,
    pub superseded_by: Option<&'a str>,
}

impl GatewayClient {
    pub async fn list_patterns(
        &self,
        filters: &PatternFilters<'_>,
        agent_id: Option<&str>,
    ) -> Result<Vec<PatternSummary>> {
        let mut url = format!("{}/v1/patterns", self.base_url());
        let mut parts: Vec<String> = Vec::new();
        push_query_part(&mut parts, "q", filters.query);
        push_query_part(&mut parts, "label", filters.label);
        push_query_part(&mut parts, "category", filters.category);
        push_query_part(&mut parts, "version", filters.version);
        push_query_part(&mut parts, "state", filters.state);
        push_query_part(&mut parts, "superseded_by", filters.superseded_by);
        if !parts.is_empty() {
            url.push('?');
            url.push_str(&parts.join("&"));
        }

        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/patterns")?;
        decode_or_bail(resp).await
    }

    pub async fn get_pattern(&self, id: &str, agent_id: Option<&str>) -> Result<Pattern> {
        let url = format!("{}/v1/patterns/{}", self.base_url(), pct_encode(id));
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/patterns/:id")?;
        decode_or_bail(resp).await
    }

    pub async fn create_pattern(
        &self,
        req: &CreatePatternRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<Pattern> {
        let url = format!("{}/v1/patterns", self.base_url());
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/patterns")?;
        decode_or_bail(resp).await
    }

    pub async fn update_pattern(
        &self,
        id: &str,
        req: &UpdatePatternRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<Pattern> {
        let url = format!("{}/v1/patterns/{}", self.base_url(), pct_encode(id));
        let builder = self
            .http_client()
            .patch(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("PATCH /v1/patterns/:id")?;
        decode_or_bail(resp).await
    }

    pub async fn delete_pattern(&self, id: &str, agent_id: Option<&str>) -> Result<()> {
        let url = format!("{}/v1/patterns/{}", self.base_url(), pct_encode(id));
        let builder = self
            .http_client()
            .delete(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("DELETE /v1/patterns/:id")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }
        Ok(())
    }

    pub async fn list_pattern_comments(
        &self,
        id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<PatternComment>> {
        let url = format!(
            "{}/v1/patterns/{}/comments",
            self.base_url(),
            pct_encode(id)
        );
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/patterns/:id/comments")?;
        decode_or_bail(resp).await
    }

    pub async fn add_pattern_comment(
        &self,
        id: &str,
        req: &AddPatternCommentRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<PatternComment> {
        let url = format!(
            "{}/v1/patterns/{}/comments",
            self.base_url(),
            pct_encode(id)
        );
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/patterns/:id/comments")?;
        decode_or_bail(resp).await
    }
}

fn push_query_part(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        if !value.is_empty() {
            parts.push(format!("{key}={}", pct_encode(value)));
        }
    }
}

fn pct_encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn decode_or_bail<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context("decode gateway response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_encode_preserves_safe_ascii() {
        assert_eq!(pct_encode("abc-XYZ_123.~"), "abc-XYZ_123.~");
    }

    #[test]
    fn pct_encode_escapes_query_chars() {
        assert_eq!(pct_encode("deploy tags"), "deploy%20tags");
        assert_eq!(pct_encode("a/b?c=d"), "a%2Fb%3Fc%3Dd");
    }

    #[test]
    fn list_patterns_url_includes_category_filter() {
        let filters = PatternFilters {
            query: Some("go router"),
            label: Some("go"),
            category: Some("programming-language/golang"),
            version: Some("latest"),
            state: Some("active"),
            superseded_by: None,
        };
        let mut url = format!("{}/v1/patterns", "https://gateway.example");
        let mut parts = Vec::new();
        push_query_part(&mut parts, "q", filters.query);
        push_query_part(&mut parts, "label", filters.label);
        push_query_part(&mut parts, "category", filters.category);
        push_query_part(&mut parts, "version", filters.version);
        push_query_part(&mut parts, "state", filters.state);
        if !parts.is_empty() {
            url.push('?');
            url.push_str(&parts.join("&"));
        }
        assert_eq!(
            url,
            "https://gateway.example/v1/patterns?q=go%20router&label=go&category=programming-language%2Fgolang&version=latest&state=active"
        );
    }
}
