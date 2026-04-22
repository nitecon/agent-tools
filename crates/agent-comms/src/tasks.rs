//! Tasks API client — companion to `gateway.rs`.
//!
//! Wire shapes mirror the gateway's per-project task board (see
//! `tasks-system.md` at the repo root for the full contract). Methods live
//! in an `impl GatewayClient` block so callers reuse the same client they
//! already use for `comms` traffic.

// Response structs mirror the full server schema; not every field is consumed
// by the current tool set, but they should be kept for completeness.
#![allow(dead_code)]

use crate::gateway::GatewayClient;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// -- Domain types ------------------------------------------------------------

/// A full task record as returned by `GET /tasks/:id` and `POST /tasks`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Task {
    pub id: String,
    pub project_ident: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    pub status: String,
    pub rank: i64,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub owner_agent_id: Option<String>,
    pub reporter: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub done_at: Option<i64>,
}

/// Slim task shape used by the list endpoint.
///
/// `project_ident` is intentionally `Option<String>` because the deployed
/// gateway omits it from list rows (it's implicit in the request URL).
/// Keeping the field optional lets us round-trip whatever future server
/// versions choose to include without breaking deserialization.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskSummary {
    pub id: String,
    #[serde(default)]
    pub project_ident: Option<String>,
    pub title: String,
    pub status: String,
    pub rank: i64,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub owner_agent_id: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    pub reporter: String,
    pub comment_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A single comment on a task.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskComment {
    pub id: String,
    pub task_id: String,
    pub author: String,
    pub author_type: String,
    pub content: String,
    pub created_at: i64,
}

// -- Composite shapes --------------------------------------------------------

/// Detailed task view returned by `GET /tasks/:id`. The deployed gateway
/// inlines `comments` as a sibling of the regular `Task` fields rather than
/// nesting them under a wrapper, so we use `#[serde(flatten)]` to merge the
/// two.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    #[serde(default)]
    pub comments: Vec<TaskComment>,
}

// -- Request shapes ----------------------------------------------------------

/// Body for `POST /v1/projects/:ident/tasks`.
#[derive(Serialize, Default, Debug, Clone)]
pub struct CreateTaskRequest<'a> {
    pub title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporter: Option<&'a str>,
}

/// Body for `PATCH /v1/projects/:ident/tasks/:id`.
///
/// Nullable string fields use `Option<serde_json::Value>` so callers can
/// distinguish the three patch states the server expects:
/// - field absent  → `None`                       — leave untouched
/// - explicit null → `Some(Value::Null)`          — clear
/// - string value  → `Some(Value::String(_))`     — set
#[derive(Serialize, Default, Debug, Clone)]
pub struct UpdateTaskRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<Value>,
}

/// Body for `POST /v1/projects/:ident/tasks/:id/comments`.
#[derive(Serialize, Default, Debug, Clone)]
pub struct AddCommentRequest<'a> {
    pub content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_type: Option<&'a str>,
}

// -- Client methods ----------------------------------------------------------

impl GatewayClient {
    /// List tasks on a project. `statuses` is comma-joined into a single
    /// `status` query param so callers stay decoupled from the gateway's
    /// repeat-vs-csv preference.
    pub async fn list_tasks(
        &self,
        ident: &str,
        statuses: Option<&[&str]>,
        include_stale: bool,
        agent_id: Option<&str>,
    ) -> Result<Vec<TaskSummary>> {
        // Build the URL by hand: reqwest is configured with
        // `default-features = false` so the optional `serde_urlencoded` based
        // `RequestBuilder::query` helper isn't available. The values we send
        // are short, ASCII-safe tokens (status names + booleans), so a manual
        // join keeps things simple without pulling extra deps.
        let mut url = format!("{}/v1/projects/{}/tasks", self.base_url(), ident);
        let mut parts: Vec<String> = Vec::new();
        if let Some(list) = statuses {
            if !list.is_empty() {
                parts.push(format!("status={}", list.join(",")));
            }
        }
        if include_stale {
            parts.push("include_stale=true".to_string());
        }
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
            .context("GET /v1/projects/:ident/tasks")?;
        decode_or_bail(resp).await
    }

    /// Fetch a single task with its comment thread.
    pub async fn get_task(
        &self,
        ident: &str,
        task_id: &str,
        agent_id: Option<&str>,
    ) -> Result<TaskDetail> {
        let url = format!(
            "{}/v1/projects/{}/tasks/{}",
            self.base_url(),
            ident,
            task_id
        );
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/tasks/:id")?;
        decode_or_bail(resp).await
    }

    /// Create a new task.
    pub async fn create_task(
        &self,
        ident: &str,
        req: &CreateTaskRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<Task> {
        let url = format!("{}/v1/projects/{}/tasks", self.base_url(), ident);
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/tasks")?;
        decode_or_bail(resp).await
    }

    /// Patch a task. The server enforces transition rules and reclaim logic.
    pub async fn update_task(
        &self,
        ident: &str,
        task_id: &str,
        patch: &UpdateTaskRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<Task> {
        let url = format!(
            "{}/v1/projects/{}/tasks/{}",
            self.base_url(),
            ident,
            task_id
        );
        let builder = self
            .http_client()
            .patch(&url)
            .header("Authorization", self.auth())
            .json(patch);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("PATCH /v1/projects/:ident/tasks/:id")?;
        decode_or_bail(resp).await
    }

    /// Append a comment to a task. `author_type = "system"` is rejected
    /// server-side; callers must use `agent` or `user`.
    pub async fn add_task_comment(
        &self,
        ident: &str,
        task_id: &str,
        req: &AddCommentRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<TaskComment> {
        let url = format!(
            "{}/v1/projects/{}/tasks/{}/comments",
            self.base_url(),
            ident,
            task_id
        );
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("POST /v1/projects/:ident/tasks/:id/comments")?;
        decode_or_bail(resp).await
    }
}

/// Shared response handler: bail with a useful message on non-2xx, decode JSON otherwise.
async fn decode_or_bail<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gateway error {status}: {body}");
    }
    resp.json::<T>().await.context("decode tasks response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_request_omits_absent_fields() {
        // None on every field → the server receives `{}`, leaving the task
        // entirely untouched. This is the contract for partial PATCHes.
        let req = UpdateTaskRequest::default();
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn update_request_emits_explicit_null_for_clear() {
        // Some(Value::Null) on a nullable string field must round-trip as
        // an explicit JSON null so the server clears the value.
        let req = UpdateTaskRequest {
            owner_agent_id: Some(Value::Null),
            description: Some(Value::String("new".into())),
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["owner_agent_id"], Value::Null);
        assert_eq!(json["description"], Value::String("new".into()));
        assert!(json.get("status").is_none());
    }

    #[test]
    fn create_request_includes_only_set_fields() {
        let req = CreateTaskRequest {
            title: "smoke",
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["title"], "smoke");
        assert!(json.get("description").is_none());
        assert!(json.get("labels").is_none());
    }

    #[test]
    fn comment_request_default_omits_author_metadata() {
        let req = AddCommentRequest {
            content: "hi",
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["content"], "hi");
        assert!(json.get("author").is_none());
        assert!(json.get("author_type").is_none());
    }
}
