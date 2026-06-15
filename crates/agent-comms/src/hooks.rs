//! App-scoped hook registry client.

#![allow(dead_code)]

use crate::gateway::GatewayClient;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HookRecord {
    pub app: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

impl GatewayClient {
    pub async fn list_hooks(&self, app: &str, agent_id: Option<&str>) -> Result<Vec<HookRecord>> {
        let url = build_hooks_url(self.base_url(), app);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/hooks")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        decode_or_bail(resp).await
    }
}

async fn decode_or_bail<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context("decode hooks response")
}

fn build_hooks_url(base_url: &str, app: &str) -> String {
    format!("{base_url}/v1/hooks?app={}", encode_query_component(app))
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
    fn hooks_url_requires_app_query() {
        assert_eq!(
            build_hooks_url("https://gateway.example", "codex/beta"),
            "https://gateway.example/v1/hooks?app=codex%2Fbeta"
        );
    }

    #[test]
    fn hook_record_accepts_missing_optional_metadata() {
        let hook: HookRecord = serde_json::from_value(serde_json::json!({
            "app": "codex",
            "name": "session/start.sh",
            "content": "#!/bin/sh\n"
        }))
        .unwrap();
        assert_eq!(hook.app, "codex");
        assert_eq!(hook.size, None);
        assert_eq!(hook.checksum, None);
    }
}
