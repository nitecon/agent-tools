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
use crate::session::SessionOrigin;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// -- Domain types ------------------------------------------------------------

/// A full task record as returned by `GET /tasks/:id` and embedded in task
/// create responses.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Task {
    pub id: String,
    pub project_ident: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub specification: Option<String>,
    pub status: String,
    pub rank: i64,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub owner_agent_id: Option<String>,
    #[serde(default)]
    pub owner_origin: Option<SessionOrigin>,
    pub reporter: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub done_at: Option<i64>,
}

impl Task {
    pub fn specification_text(&self) -> Option<&str> {
        self.specification
            .as_deref()
            .or(self.details.as_deref())
            .filter(|s| !s.trim().is_empty())
    }
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
    pub owner_origin: Option<SessionOrigin>,
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
    #[serde(default)]
    pub origin: Option<SessionOrigin>,
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

/// Task create response returned by `POST /tasks`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskCreateResponse {
    #[serde(flatten)]
    pub task: Task,
    #[serde(default)]
    pub hint: Option<String>,
}

/// Server-owned linkage returned by the cross-project delegation endpoint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskDelegation {
    pub id: String,
    pub source_project_ident: String,
    pub source_task_id: String,
    pub target_project_ident: String,
    pub target_task_id: String,
    #[serde(default)]
    pub requester_agent_id: Option<String>,
    #[serde(default)]
    pub requester_hostname: Option<String>,
    pub created_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub completion_message_id: Option<i64>,
}

/// Response returned by `POST /tasks/delegate`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskDelegationResponse {
    pub delegation: TaskDelegation,
    pub source_task: Task,
    pub target_task: Task,
    pub message_id: i64,
}

// -- Request shapes ----------------------------------------------------------

/// Body for `POST /v1/projects/:ident/tasks`.
#[derive(Serialize, Default, Debug, Clone)]
pub struct CreateTaskRequest<'a> {
    pub title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specification: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporter: Option<&'a str>,
}

/// Body for `POST /v1/projects/:source_ident/tasks/delegate`.
#[derive(Serialize, Default, Debug, Clone)]
pub struct DelegateTaskRequest<'a> {
    pub target_project_ident: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub specification: &'a str,
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
    pub specification: Option<Value>,
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
    ) -> Result<TaskCreateResponse> {
        let url = format!("{}/v1/projects/{}/tasks", self.base_url(), ident);
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = self
            .add_task_origin(Self::add_agent_id(builder, agent_id))
            .send()
            .await
            .context("POST /v1/projects/:ident/tasks")?;
        decode_or_bail(resp).await
    }

    /// Create a target-project task plus a source-project delegated tracker.
    pub async fn delegate_task(
        &self,
        source_ident: &str,
        req: &DelegateTaskRequest<'_>,
        agent_id: Option<&str>,
    ) -> Result<TaskDelegationResponse> {
        let url = format!(
            "{}/v1/projects/{}/tasks/delegate",
            self.base_url(),
            source_ident
        );
        let builder = self
            .http_client()
            .post(&url)
            .header("Authorization", self.auth())
            .json(req);
        let resp = self
            .add_task_origin(Self::add_agent_id(builder, agent_id))
            .send()
            .await
            .context("POST /v1/projects/:source_ident/tasks/delegate")?;
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
        let resp = self
            .add_task_origin(Self::add_agent_id(builder, agent_id))
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
        let resp = self
            .add_task_origin(Self::add_agent_id(builder, agent_id))
            .send()
            .await
            .context("POST /v1/projects/:ident/tasks/:id/comments")?;
        decode_or_bail(resp).await
    }

    /// Fetch Eventic build status for the project. The endpoint is project
    /// scoped, so it lives beside the task board API even though it surfaces
    /// build metadata rather than task rows.
    pub async fn get_build_status(
        &self,
        ident: &str,
        repo: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Value> {
        let url = build_eventic_status_url(self.base_url(), ident, repo);
        let builder = self
            .http_client()
            .get(&url)
            .header("Authorization", self.auth());
        let resp = Self::add_agent_id(builder, agent_id)
            .send()
            .await
            .context("GET /v1/projects/:ident/eventic")?;
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

fn build_eventic_status_url(base_url: &str, ident: &str, repo: Option<&str>) -> String {
    let mut url = format!("{base_url}/v1/projects/{ident}/eventic");
    if let Some(repo) = repo {
        if !repo.trim().is_empty() {
            url.push_str("?repo=");
            url.push_str(&encode_query_component(repo));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_task_mutation_sends_exact_origin_over_http() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let task = serde_json::json!({"id":"task-1","project_ident":"demo","title":"Demo","status":"todo","rank":1,"reporter":"agent","created_at":1,"updated_at":1});
        let delegated = serde_json::json!({"delegation":{"id":"delegation-1","source_project_ident":"demo","source_task_id":"source","target_project_ident":"other","target_task_id":"target","created_at":1},"source_task":task,"target_task":task,"message_id":1});
        let comment = serde_json::json!({"id":"comment-1","task_id":"task-1","author":"machine-agent","author_type":"agent","content":"result","created_at":1});
        let responses = [task.clone(), delegated, task, comment];
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/actor-origin-vectors.json")).unwrap();
        let row = &vectors[0];
        let origin = crate::actor::derive(
            uuid::Uuid::parse_str(row["instance_id"].as_str().unwrap()).unwrap(),
            row["os"].as_str().unwrap(),
            row["provider"].as_str().unwrap(),
            row["provider_session_id"].as_str().unwrap(),
            uuid::Uuid::parse_str(row["base_id"].as_str().unwrap()).unwrap(),
            row["session_slot"].as_u64().unwrap() as u32,
        )
        .unwrap()
        .origin;
        let expected_origin = origin.clone();
        let fixture = std::thread::spawn(move || {
            for (index, response) in responses.into_iter().enumerate() {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with(if index == 2 { "PATCH " } else { "POST " }));
                let mut headers = std::collections::HashMap::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let (key, value) = line.split_once(':').unwrap();
                    headers.insert(key.to_ascii_lowercase(), value.trim().to_string());
                }
                assert_eq!(headers["x-agent-id"], "machine-agent");
                assert_eq!(headers["x-agent-session-id"], expected_origin.session_id);
                assert_eq!(headers["x-agent-instance-id"], expected_origin.instance_id);
                assert_eq!(headers["x-agent-provider"], "codex");
                assert_eq!(headers["x-agent-os"], "linux");
                let mut request_body = vec![0; headers["content-length"].parse::<usize>().unwrap()];
                reader.read_exact(&mut request_body).unwrap();
                assert!(serde_json::from_slice::<serde_json::Value>(&request_body).is_ok());
                let body = serde_json::to_vec(&response).unwrap();
                let stream = reader.get_mut();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        let mut client =
            GatewayClient::new(format!("http://{address}"), "test-key".into(), 5000).unwrap();
        client.set_session_origin(origin).unwrap();
        client
            .create_task(
                "demo",
                &CreateTaskRequest {
                    title: "Demo",
                    ..Default::default()
                },
                Some("machine-agent"),
            )
            .await
            .unwrap();
        client
            .delegate_task(
                "demo",
                &DelegateTaskRequest {
                    target_project_ident: "other",
                    title: "Demo",
                    description: "description",
                    specification: "specification",
                    ..Default::default()
                },
                Some("machine-agent"),
            )
            .await
            .unwrap();
        client
            .update_task(
                "demo",
                "task-1",
                &UpdateTaskRequest {
                    status: Some("in_progress"),
                    ..Default::default()
                },
                Some("machine-agent"),
            )
            .await
            .unwrap();
        client
            .add_task_comment(
                "demo",
                "task-1",
                &AddCommentRequest {
                    content: "result",
                    ..Default::default()
                },
                Some("machine-agent"),
            )
            .await
            .unwrap();
        fixture.join().unwrap();
    }

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
        assert!(json.get("specification").is_none());
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
        assert!(json.get("specification").is_none());
        assert!(json.get("details").is_none());
        assert!(json.get("labels").is_none());
    }

    #[test]
    fn create_request_sends_specification_field() {
        let req = CreateTaskRequest {
            title: "smoke",
            specification: Some("handoff spec"),
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["specification"], "handoff spec");
        assert!(json.get("details").is_none());
    }

    #[test]
    fn delegate_request_requires_contract_fields_and_omits_unset_metadata() {
        let req = DelegateTaskRequest {
            target_project_ident: "other-project",
            title: "Update exported API",
            description: "Project A needs this API for integration.",
            specification: "Add the endpoint and document the response shape.",
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["target_project_ident"], "other-project");
        assert_eq!(json["title"], "Update exported API");
        assert_eq!(
            json["description"],
            "Project A needs this API for integration."
        );
        assert_eq!(
            json["specification"],
            "Add the endpoint and document the response shape."
        );
        assert!(json.get("labels").is_none());
        assert!(json.get("hostname").is_none());
        assert!(json.get("reporter").is_none());
    }

    #[test]
    fn task_specification_text_prefers_new_field_but_accepts_legacy_details() {
        let mut task = serde_json::from_value::<Task>(serde_json::json!({
            "id": "task-1",
            "project_ident": "demo",
            "title": "Demo",
            "specification": "new spec",
            "details": "legacy details",
            "status": "todo",
            "rank": 1,
            "reporter": "agent",
            "created_at": 1,
            "updated_at": 1
        }))
        .unwrap();
        assert_eq!(task.specification_text(), Some("new spec"));
        assert!(task.owner_origin.is_none());

        task.specification = None;
        assert_eq!(task.specification_text(), Some("legacy details"));
    }

    #[test]
    fn session_ownership_and_comment_origin_survive_decoding() {
        let value = serde_json::json!({"id":"task-1","project_ident":"demo","title":"Demo","status":"in_progress","rank":1,"reporter":"agent","created_at":1,"updated_at":1,"owner_agent_id":"shared-machine","owner_origin":{"session_id":"00000000-0000-4000-8000-000000000001","instance_id":"00000000-0000-4000-8000-000000000003","provider":"codex","os":"linux"}});
        let task: Task = serde_json::from_value(value).unwrap();
        assert_eq!(
            task.owner_origin
                .as_ref()
                .map(|origin| origin.session_id.as_str()),
            Some("00000000-0000-4000-8000-000000000001")
        );
        let origin = serde_json::json!({"session_id":"00000000-0000-4000-8000-000000000002","instance_id":"00000000-0000-4000-8000-000000000003","provider":"claude","os":"windows"});
        let comment: TaskComment = serde_json::from_value(serde_json::json!({"id":"comment-1","task_id":"task-1","author":"shared-machine","author_type":"agent","content":"result","created_at":1,"origin":origin})).unwrap();
        assert_eq!(comment.origin.unwrap().provider, "claude");
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

    #[test]
    fn encode_query_component_escapes_repo_override() {
        assert_eq!(
            encode_query_component("github.com/nitecon/agent-tools.git"),
            "github.com%2Fnitecon%2Fagent-tools.git"
        );
        assert_eq!(
            encode_query_component("owner/repo#main"),
            "owner%2Frepo%23main"
        );
    }

    #[test]
    fn build_eventic_status_url_uses_deployed_endpoint() {
        assert_eq!(
            build_eventic_status_url("https://gateway.example", "agent-tools", None),
            "https://gateway.example/v1/projects/agent-tools/eventic"
        );
        assert_eq!(
            build_eventic_status_url(
                "https://gateway.example",
                "agent-tools",
                Some("nitecon/agent-tools")
            ),
            "https://gateway.example/v1/projects/agent-tools/eventic?repo=nitecon%2Fagent-tools"
        );
    }
}
