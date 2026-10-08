use agent_comms::actor::{self, Actor};
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

async fn announce<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) -> Actor {
    let mut line = String::new();
    BufReader::new(&mut stream)
        .read_line(&mut line)
        .await
        .unwrap();
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["method"], "gateway.session.announce");
    assert!(request["params"].get("enrollment_token").is_none());
    let actor: Actor = serde_json::from_value(request["params"].clone()).unwrap();
    let mut result = serde_json::to_value(&actor).unwrap();
    result["binding_state"] = json!("unbound");
    let mut reply = serde_json::to_vec(&json!({"id":1,"result":result})).unwrap();
    reply.push(b'\n');
    stream.write_all(&reply).await.unwrap();
    actor
}

#[tokio::test]
async fn notification_first_prompt_announces_actual_hook_without_context_output() {
    let directory = tempfile::tempdir().unwrap();
    for provider in ["codex", "claude"] {
        #[cfg(unix)]
        let (endpoint, server) = {
            let endpoint = directory.path().join(format!("{provider}.sock"));
            let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
            let server =
                tokio::spawn(async move { announce(listener.accept().await.unwrap().0).await });
            (endpoint.to_string_lossy().into_owned(), server)
        };
        #[cfg(windows)]
        let (endpoint, server) = {
            let endpoint = format!(
                r"\\.\pipe\agent-tools-notification-{}-{provider}",
                std::process::id()
            );
            let pipe = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint)
                .unwrap();
            let server = tokio::spawn(async move {
                pipe.connect().await.unwrap();
                announce(pipe).await
            });
            (endpoint, server)
        };
        let native = format!("notification-first-{provider}");
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_agent-tools"));
        command
            .args(["hook", "user-prompt-submit", "--agent", provider])
            .current_dir(directory.path())
            .env("HOME", directory.path())
            .env("USERPROFILE", directory.path())
            .env("CMUX_SOCKET", endpoint)
            .env_remove("CMUX_SOCKET_PATH")
            .env_remove("AGENT_TOOLS_HOOK")
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
        let payload = json!({
            "session_id":native,
            "prompt":"<Start Agent Gateway Message Injection>\nDelegated task event\n</Stop AgentGateway Message injection>"
        });
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .await
            .unwrap();
        let (actor, output) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(server, child.wait_with_output())
        })
        .await
        .expect("notification hook must announce and exit within the deadline");
        let actor = actor.unwrap();
        let output = output.unwrap();
        assert!(output.status.success());
        assert!(
            output.stdout.is_empty(),
            "notification must not inject context"
        );
        assert!(output.stderr.is_empty(), "notification must stay silent");
        assert_eq!(actor.provider_session_id, native);
        assert_eq!(actor.origin.provider, provider);
        assert_eq!(actor.version, 2);
        assert!(actor.session_slot > 0);
        assert_eq!(
            actor,
            actor::derive(
                actor.origin.instance_id.parse().unwrap(),
                std::env::consts::OS,
                provider,
                &native,
                actor.base_id.parse().unwrap(),
                actor.session_slot,
            )
            .unwrap()
        );
    }
}
