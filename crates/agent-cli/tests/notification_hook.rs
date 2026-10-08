use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn hooks_do_not_block_legacy_enrollment_or_request_cmux_membership() {
    let directory = tempfile::tempdir().unwrap();
    let token = "a".repeat(64);
    let prompts = [
        "<Start Agent Gateway Message Injection>\nDelegated task event\n</Stop AgentGateway Message injection>".to_owned(),
        format!("<cmux-session-enrollment>{{\"version\":1,\"enrollment_token\":\"{token}\"}}</cmux-session-enrollment>"),
        "<cmux-session-enrollment>{invalid}</cmux-session-enrollment>".to_owned(),
        "Review the parser implementation".to_owned(),
    ];
    for provider in ["codex", "claude"] {
        for (index, prompt) in prompts.iter().enumerate() {
            #[cfg(unix)]
            let (endpoint, listener) = {
                let endpoint = directory.path().join(format!("{provider}-{index}.sock"));
                let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
                (endpoint.to_string_lossy().into_owned(), listener)
            };
            #[cfg(windows)]
            let (endpoint, listener) = {
                let endpoint = format!(
                    r"\\.\pipe\agent-tools-hook-{}-{provider}-{index}",
                    std::process::id()
                );
                let listener = tokio::net::windows::named_pipe::ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(&endpoint)
                    .unwrap();
                (endpoint, listener)
            };
            let native = format!("hook-{provider}");
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_agent-tools"));
            command
                .args(["hook", "user-prompt-submit", "--agent", provider])
                .current_dir(directory.path())
                .env("HOME", directory.path())
                .env("USERPROFILE", directory.path())
                .env("AGENT_TOOLS_STATE_DIR", directory.path().join("state"))
                .env("AGENT_TOOLS_HOOK_TIMEOUT_MS", "25")
                .env("AGENT_TOOLS_NUDGE", "off")
                .env("CMUX_SOCKET", endpoint)
                .env_remove("CMUX_SOCKET_PATH")
                .env_remove("AGENT_TOOLS_HOOK")
                .env_remove("GATEWAY_URL")
                .env_remove("GATEWAY_API_KEY")
                .env_remove("CODEX_THREAD_ID")
                .env_remove("CODEX_SESSION_ID")
                .env_remove("CLAUDE_CODE_SESSION_ID")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            if provider == "codex" {
                command
                    .env("CODEX_THREAD_ID", &native)
                    .env("CODEX_SESSION_ID", &native);
            } else {
                command.env("CLAUDE_CODE_SESSION_ID", &native);
            }
            let mut child = command.spawn().unwrap();
            let payload = json!({"session_id":native,"prompt":prompt});
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.to_string().as_bytes())
                .await
                .unwrap();
            let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
                .await
                .expect("hook must exit within the deadline")
                .unwrap();
            assert!(output.status.success(), "{provider} prompt {index}");
            assert!(output.stderr.is_empty(), "hook must stay fail-soft");
            if index == 0 {
                assert!(output.stdout.is_empty(), "notification must stay silent");
            } else {
                let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert!(envelope.get("decision").is_none());
                assert!(envelope["hookSpecificOutput"]
                    .get("suppressOriginalPrompt")
                    .is_none());
                let context = envelope["hookSpecificOutput"]["additionalContext"]
                    .as_str()
                    .unwrap();
                assert!(context.contains("Calling actor:"));
                assert!(context.contains(&format!("provider={provider}")));
                assert!(!context.contains(&token));
            }
            #[cfg(unix)]
            let connection = listener.accept();
            #[cfg(windows)]
            let connection = listener.connect();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), connection)
                    .await
                    .is_err(),
                "hooks must not initiate CMUX membership or enrollment"
            );
        }
    }
}
