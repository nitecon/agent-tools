//! Retired task API compatibility types and non-networking stubs.
//!
//! Task operations have moved to amux. Legacy wire types remain available;
//! client methods return a migration error before contacting a gateway.

// Response structs mirror the full server schema; not every field is consumed
// by the current tool set, but they should be kept for completeness.
#![allow(dead_code)]

use crate::gateway::GatewayClient;
use crate::session::SessionOrigin;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Retirement message shared by task compatibility entry points.
pub const TASK_MIGRATION: &str =
    "All task capabilities have migrated to amux; see https://github.com/nitecon/amux";

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
    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn list_tasks(
        &self,
        _ident: &str,
        _statuses: Option<&[&str]>,
        _include_stale: bool,
        _agent_id: Option<&str>,
    ) -> Result<Vec<TaskSummary>> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn get_task(
        &self,
        _ident: &str,
        _task_id: &str,
        _agent_id: Option<&str>,
    ) -> Result<TaskDetail> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn create_task(
        &self,
        _ident: &str,
        _req: &CreateTaskRequest<'_>,
        _agent_id: Option<&str>,
    ) -> Result<TaskCreateResponse> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn delegate_task(
        &self,
        _source_ident: &str,
        _req: &DelegateTaskRequest<'_>,
        _agent_id: Option<&str>,
    ) -> Result<TaskDelegationResponse> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn update_task(
        &self,
        _ident: &str,
        _task_id: &str,
        _patch: &UpdateTaskRequest<'_>,
        _agent_id: Option<&str>,
    ) -> Result<Task> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn add_task_comment(
        &self,
        _ident: &str,
        _task_id: &str,
        _req: &AddCommentRequest<'_>,
        _agent_id: Option<&str>,
    ) -> Result<TaskComment> {
        anyhow::bail!(TASK_MIGRATION)
    }

    /// Retired compatibility entry point; returns TASK_MIGRATION without I/O.
    pub async fn get_build_status(
        &self,
        _ident: &str,
        _repo: Option<&str>,
        _agent_id: Option<&str>,
    ) -> Result<Value> {
        anyhow::bail!(TASK_MIGRATION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retired_methods_return_migration_without_network_requests() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = GatewayClient::new(
            format!("http://{}", listener.local_addr().unwrap()),
            "test-key".into(),
            100,
        )
        .unwrap();
        let create = CreateTaskRequest {
            title: "t",
            ..Default::default()
        };
        let delegate = DelegateTaskRequest {
            target_project_ident: "other",
            title: "t",
            description: "d",
            specification: "s",
            reporter: None,
            labels: None,
            hostname: None,
        };
        let patch = UpdateTaskRequest {
            status: Some("done"),
            ..Default::default()
        };
        let comment = AddCommentRequest {
            content: "c",
            ..Default::default()
        };
        let errors = [
            client
                .list_tasks("demo", None, false, Some("agent"))
                .await
                .unwrap_err(),
            client
                .get_task("demo", "id", Some("agent"))
                .await
                .unwrap_err(),
            client
                .create_task("demo", &create, Some("agent"))
                .await
                .unwrap_err(),
            client
                .delegate_task("demo", &delegate, Some("agent"))
                .await
                .unwrap_err(),
            client
                .update_task("demo", "id", &patch, Some("agent"))
                .await
                .unwrap_err(),
            client
                .add_task_comment("demo", "id", &comment, Some("agent"))
                .await
                .unwrap_err(),
            client
                .get_build_status("demo", None, Some("agent"))
                .await
                .unwrap_err(),
        ];
        for error in errors {
            assert_eq!(error.to_string(), TASK_MIGRATION);
        }
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
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
}
