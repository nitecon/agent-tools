// Response structs mirror the full server schema; not every field is consumed by
// the current tool set, but they should be kept for completeness / future use.
#![allow(dead_code)]

use crate::sanitize::validate_api_key;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// HTTP client for the agent-gateway API.
///
/// Manages authentication, timeouts, and endpoint routing for all
/// project / message operations against a running gateway instance.
#[derive(Clone)]
pub struct GatewayClient {
    client: Client,
    base_url: String,
    api_key: String,
}

// -- Request / response types -------------------------------------------------

#[derive(Serialize)]
struct RegisterProjectRequest<'a> {
    ident: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
    /// Git remote / canonical ident so the gateway can derive the repository
    /// mapping instead of guessing from the short ident.
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_url: Option<&'a str>,
}

/// Response returned after registering (or re-registering) a project.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterProjectResponse {
    pub ident: String,
    pub channel_name: String,
    pub room_id: String,
}

/// Optional metadata that enriches the structured message payload
/// (subject line, originating host, explicit event time). All fields are
/// optional; the gateway derives sensible defaults for anything left empty.
///
/// Borrowed form so callers can build a `MessageMeta` from existing strings
/// without forcing an allocation per send.
#[derive(Default, Debug, Clone, Copy)]
pub struct MessageMeta<'a> {
    /// One-line headline rendered as the embed title. Defaults server-side
    /// to the first non-empty line of the body, capped at 80 chars.
    pub subject: Option<&'a str>,
    /// Originating host of the message. Defaults server-side to the
    /// `X-Agent-Id` header value.
    pub hostname: Option<&'a str>,
    /// Event time in epoch milliseconds. Defaults server-side to receipt time.
    pub event_at_ms: Option<i64>,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    body: &'a str,
    /// Back-compat alias retained so this client also works against gateways
    /// that pre-date the structured payload. New gateways prefer `body`.
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_at: Option<i64>,
}

impl<'a> SendMessageRequest<'a> {
    fn from_meta(body: &'a str, meta: &MessageMeta<'a>) -> Self {
        Self {
            body,
            content: body,
            subject: meta.subject,
            hostname: meta.hostname,
            event_at: meta.event_at_ms,
        }
    }
}

/// Response returned after posting a message to a project channel.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SendMessageResponse {
    pub message_id: i64,
    pub external_message_id: String,
}

/// A single message retrieved from the gateway.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GatewayMessage {
    pub id: i64,
    pub project_ident: String,
    pub source: String,
    pub content: String,
    pub sent_at: i64,
    pub parent_message_id: Option<i64>,
    pub agent_id: Option<String>,
    pub message_type: Option<String>,
}

/// Response envelope for the unread-messages endpoint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetUnreadResponse {
    pub messages: Vec<GatewayMessage>,
    pub status: String,
}

/// Response returned after confirming a message as read.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfirmResponse {
    pub confirmed: bool,
}

/// One thread (root message) with gateway-derived state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ThreadSummary {
    pub id: i64,
    pub project_ident: String,
    pub source: String,
    pub author_kind: String,
    pub agent_id: Option<String>,
    pub hostname: Option<String>,
    pub subject: Option<String>,
    pub preview: String,
    pub sent_at: i64,
    pub last_activity_at: i64,
    pub reply_count: i64,
    pub ack_count: i64,
    /// open | acknowledged | answered | resolved
    pub state: String,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InboxCounts {
    pub human_open: i64,
    pub agent_open: i64,
    pub alerts_open: i64,
    pub system_open: i64,
    pub resolved: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListThreadsResponse {
    pub threads: Vec<ThreadSummary>,
    pub counts: InboxCounts,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolveResponse {
    pub changed: bool,
}

/// Response returned after replying to or acting on a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReplyResponse {
    pub message_id: i64,
    pub external_message_id: String,
    pub parent_message_id: i64,
}

#[derive(Serialize)]
struct ActionRequest<'a> {
    body: &'a str,
    /// Back-compat alias for action posts on older gateways. New gateways
    /// prefer `body`; this is ignored when `body` is set.
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_at: Option<i64>,
}

impl<'a> ActionRequest<'a> {
    fn from_meta(body: &'a str, meta: &MessageMeta<'a>) -> Self {
        Self {
            body,
            message: body,
            subject: meta.subject,
            hostname: meta.hostname,
            event_at: meta.event_at_ms,
        }
    }
}

// -- Client implementation ----------------------------------------------------

impl GatewayClient {
    /// Create a new `GatewayClient`.
    ///
    /// # Arguments
    /// * `base_url`   - Root URL of the gateway (e.g. `http://localhost:7913`).
    /// * `api_key`    - Bearer token used for all requests.
    /// * `timeout_ms` - Per-request timeout in milliseconds.
    ///
    /// # Errors
    /// Returns an error if the underlying `reqwest::Client` cannot be built.
    pub fn new(base_url: String, api_key: String, timeout_ms: u64) -> Result<Self> {
        // Trim outer whitespace defensively (env overrides / exotic shells can
        // leave stray \r or trailing spaces), then validate that the key can
        // safely flow into an `Authorization` header. Surfacing a clear error
        // here beats reqwest's opaque "failed to parse header value".
        let api_key = api_key.trim().to_string();
        validate_api_key(&api_key).map_err(anyhow::Error::msg)?;

        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    /// Construct the `Bearer <api-key>` `Authorization` header value.
    /// Exposed `pub(crate)` so sibling modules (e.g. `tasks`) can reuse it.
    pub(crate) fn auth(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    /// Borrow the underlying reqwest `Client` so sibling modules can build
    /// requests without re-creating connection pools.
    pub(crate) fn http_client(&self) -> &Client {
        &self.client
    }

    /// Borrow the gateway base URL so sibling modules can format endpoint
    /// paths without exposing the field directly.
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Conditionally attach the `X-Agent-Id` header to a request builder.
    pub(crate) fn add_agent_id(
        builder: reqwest::RequestBuilder,
        agent_id: Option<&str>,
    ) -> reqwest::RequestBuilder {
        if let Some(id) = agent_id {
            builder.header("X-Agent-Id", id)
        } else {
            builder
        }
    }

    /// Register (or re-register) a project with the gateway.
    ///
    /// `channel` selects the plugin; pass `None` to use the gateway's default.
    pub async fn register_project(
        &self,
        ident: &str,
        channel: Option<&str>,
    ) -> Result<RegisterProjectResponse> {
        self.register_project_with_remote(ident, channel, None)
            .await
    }

    /// Register a project and tell the gateway its git remote (or agent-tools
    /// canonical ident) so the repository mapping is derived server-side.
    pub async fn register_project_with_remote(
        &self,
        ident: &str,
        channel: Option<&str>,
        repo_url: Option<&str>,
    ) -> Result<RegisterProjectResponse> {
        let url = format!("{}/v1/projects", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth())
            .json(&RegisterProjectRequest {
                ident,
                channel,
                repo_url,
            })
            .send()
            .await
            .context("POST /v1/projects")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<RegisterProjectResponse>()
            .await
            .context("decode register response")
    }

    /// Post an agent message to the project's channel.
    ///
    /// `body` populates the structured `body` field (and the legacy `content`
    /// alias for back-compat with older gateways). `meta` carries the optional
    /// structured fields — subject, hostname, and explicit event time. Pass
    /// `&MessageMeta::default()` when you want the gateway to derive every
    /// optional field server-side.
    ///
    /// When `agent_id` is `Some`, the request includes an `X-Agent-Id` header so
    /// the gateway can attribute the message to a specific agent.
    pub async fn send_message(
        &self,
        ident: &str,
        body: &str,
        meta: &MessageMeta<'_>,
        agent_id: Option<&str>,
    ) -> Result<SendMessageResponse> {
        let url = format!("{}/v1/projects/{}/messages", self.base_url, ident);
        let builder = self
            .client
            .post(&url)
            .header("Authorization", self.auth())
            .json(&SendMessageRequest::from_meta(body, meta));
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/messages")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<SendMessageResponse>()
            .await
            .context("decode send message response")
    }

    /// Fetch unconfirmed messages for a project (peek -- no side effects).
    ///
    /// When `agent_id` is `Some`, the gateway returns only messages unconfirmed
    /// by that specific agent rather than the global unread set.
    pub async fn get_unread(
        &self,
        ident: &str,
        agent_id: Option<&str>,
    ) -> Result<GetUnreadResponse> {
        let url = format!("{}/v1/projects/{}/messages/unread", self.base_url, ident);
        let builder = self.client.get(&url).header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/messages/unread")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<GetUnreadResponse>()
            .await
            .context("decode unread response")
    }

    /// List threads (root messages) for a project with derived state.
    ///
    /// `state` is one of open | acknowledged | answered | resolved |
    /// unresolved | all; `kinds` filters by author kind (human, agent, bot,
    /// webhook, system).
    pub async fn list_threads(
        &self,
        ident: &str,
        state: Option<&str>,
        kinds: &[&str],
        limit: usize,
    ) -> Result<ListThreadsResponse> {
        let mut url = format!(
            "{}/v1/projects/{}/messages?limit={}",
            self.base_url, ident, limit
        );
        if let Some(state) = state {
            url.push_str(&format!("&state={state}"));
        }
        if !kinds.is_empty() {
            url.push_str(&format!("&kind={}", kinds.join(",")));
        }
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth())
            .send()
            .await
            .context("GET /v1/projects/:ident/messages")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }
        resp.json::<ListThreadsResponse>()
            .await
            .context("decode thread list")
    }

    /// Mark a thread resolved so it leaves the human inbox.
    pub async fn resolve_message(
        &self,
        ident: &str,
        msg_id: i64,
        agent_id: Option<&str>,
    ) -> Result<ResolveResponse> {
        let url = format!(
            "{}/v1/projects/{}/messages/{}/resolve",
            self.base_url, ident, msg_id
        );
        let builder = self.client.post(&url).header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/messages/:id/resolve")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }
        resp.json::<ResolveResponse>()
            .await
            .context("decode resolve response")
    }

    /// Confirm a single message as read and acted upon.
    ///
    /// When `agent_id` is `Some`, the confirmation is scoped to that agent,
    /// leaving the message unconfirmed for other agents.
    pub async fn confirm_read(
        &self,
        ident: &str,
        msg_id: i64,
        agent_id: Option<&str>,
    ) -> Result<ConfirmResponse> {
        let url = format!(
            "{}/v1/projects/{}/messages/{}/confirm",
            self.base_url, ident, msg_id
        );
        let builder = self.client.post(&url).header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/messages/:id/confirm")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<ConfirmResponse>()
            .await
            .context("decode confirm response")
    }

    /// Reply to a specific message in a project's channel.
    ///
    /// Sends `body` as a threaded reply to the message identified by `msg_id`.
    /// `meta` populates the optional structured fields (subject, hostname,
    /// event time); pass `&MessageMeta::default()` to let the gateway derive
    /// them server-side.
    ///
    /// The gateway will attempt native threading (e.g. Discord message references)
    /// and falls back to a plain send if the parent has no external message id.
    pub async fn reply_to(
        &self,
        ident: &str,
        msg_id: i64,
        body: &str,
        meta: &MessageMeta<'_>,
        agent_id: Option<&str>,
    ) -> Result<ReplyResponse> {
        let url = format!(
            "{}/v1/projects/{}/messages/{}/reply",
            self.base_url, ident, msg_id
        );
        let builder = self
            .client
            .post(&url)
            .header("Authorization", self.auth())
            .json(&SendMessageRequest::from_meta(body, meta));
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/messages/:id/reply")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<ReplyResponse>()
            .await
            .context("decode reply response")
    }

    /// Signal that the agent is taking action on a message.
    ///
    /// Posts an action notification against `msg_id`. The payload uses the
    /// structured `body` field (with the legacy `message` alias for back-compat
    /// against older gateways). `meta` populates the optional structured fields
    /// — pass `&MessageMeta::default()` to defer everything to the gateway.
    ///
    /// When the gateway derives the subject server-side it prefixes `[ACTION] `
    /// so action posts stay visually distinct; supply your own subject (with
    /// the prefix if you want it) to override that behavior.
    pub async fn taking_action_on(
        &self,
        ident: &str,
        msg_id: i64,
        body: &str,
        meta: &MessageMeta<'_>,
        agent_id: Option<&str>,
    ) -> Result<ReplyResponse> {
        let url = format!(
            "{}/v1/projects/{}/messages/{}/action",
            self.base_url, ident, msg_id
        );
        let builder = self
            .client
            .post(&url)
            .header("Authorization", self.auth())
            .json(&ActionRequest::from_meta(body, meta));
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/messages/:id/action")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gateway error {status}: {body}");
        }

        resp.json::<ReplyResponse>()
            .await
            .context("decode action response")
    }
}
