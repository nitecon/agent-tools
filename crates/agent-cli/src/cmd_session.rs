//! Local CMUX discovery. Identity is resolved anew for each invocation, with no
//! provider invocation, screen reads, persistent cache or assignment policy.

use agent_comms::session::{is_uuid, SessionOrigin};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const TIMEOUT: Duration = Duration::from_secs(2);
const MAX_RESPONSE: u64 = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct AgentSession {
    #[serde(flatten)]
    pub(crate) origin: SessionOrigin,
    pub(crate) surface_id: String,
    pub(crate) workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
}

/// Both environment fields must be present in CMUX. Missing/broken explicit
/// context is an error rather than an unattributed mutation or stale identity.
struct LocalContext {
    endpoint: String,
    surface_id: String,
}

impl LocalContext {
    fn from_env() -> Result<Option<Self>> {
        Self::from_values(
            std::env::var("CMUX_SOCKET").ok(),
            std::env::var("CMUX_SOCKET_PATH").ok(),
            std::env::var("CMUX_SURFACE_ID").ok(),
        )
    }

    fn from_values(
        socket: Option<String>,
        socket_path: Option<String>,
        surface_id: Option<String>,
    ) -> Result<Option<Self>> {
        if socket.is_none() && socket_path.is_none() && surface_id.is_none() {
            return Ok(None);
        }
        let endpoint = socket
            .or(socket_path)
            .context("CMUX_SOCKET or CMUX_SOCKET_PATH is required for session identity")?;
        ensure!(!endpoint.is_empty(), "CMUX socket endpoint is empty");
        let surface_id = surface_id.context("CMUX_SURFACE_ID is required for session identity")?;
        ensure!(is_uuid(&surface_id), "invalid CMUX surface UUID");
        Ok(Some(Self {
            endpoint,
            surface_id,
        }))
    }

    async fn session(&self) -> Result<AgentSession> {
        let value = rpc(
            &self.endpoint,
            "gateway.session",
            json!({"surface_id": self.surface_id}),
        )
        .await?;
        self.decode_session(value)
    }

    fn decode_session(&self, value: Value) -> Result<AgentSession> {
        let session: AgentSession =
            serde_json::from_value(value).context("decode CMUX agent session")?;
        validate_session(&session)?;
        ensure!(
            session.surface_id == self.surface_id,
            "CMUX returned identity for a different surface"
        );
        Ok(session)
    }
}

fn validate_session(session: &AgentSession) -> Result<()> {
    session.origin.validate()?;
    ensure!(
        is_uuid(&session.surface_id) && is_uuid(&session.workspace_id),
        "invalid CMUX surface/workspace UUID"
    );
    Ok(())
}

pub(crate) async fn mutation_origin() -> Result<Option<SessionOrigin>> {
    match LocalContext::from_env()? {
        None => Ok(None),
        Some(context) => Ok(Some(
            context
                .session()
                .await
                .context("resolve current CMUX agent identity; task mutation was not sent")?
                .origin,
        )),
    }
}

pub(crate) fn run(peers: bool, json_output: bool) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(discover(peers, json_output))
}

async fn discover(peers: bool, json_output: bool) -> Result<()> {
    let context = LocalContext::from_env()?.context("not running in a CMUX agent surface")?;
    if peers {
        #[derive(Deserialize, Serialize)]
        struct Peers {
            sessions: Vec<AgentSession>,
        }
        let result: Peers =
            serde_json::from_value(rpc(&context.endpoint, "gateway.sessions", json!({})).await?)?;
        for session in &result.sessions {
            validate_session(session)?;
        }
        if json_output {
            println!("{}", serde_json::to_string(&result)?);
        } else {
            for session in result.sessions {
                print_session(&session);
            }
        }
    } else {
        let session = context.session().await?;
        if json_output {
            println!("{}", serde_json::to_string(&session)?);
        } else {
            print_session(&session);
        }
    }
    Ok(())
}

fn print_session(session: &AgentSession) {
    println!(
        "{} {} {} surface={} workspace={} instance={}{}",
        session.origin.session_id,
        session.origin.provider,
        session.origin.os,
        session.surface_id,
        session.workspace_id,
        session.origin.instance_id,
        session
            .repository
            .as_deref()
            .map(|repository| format!(" repository={repository}"))
            .unwrap_or_default()
    );
}

/// Deadline covers connect, write and bounded newline-delimited response read.
async fn rpc(endpoint: &str, method: &str, params: Value) -> Result<Value> {
    tokio::time::timeout(TIMEOUT, async {
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(endpoint)
            .await
            .context("connect CMUX Unix socket")?;
        #[cfg(windows)]
        let stream = loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint) {
                Ok(stream) => break stream,
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(error) => return Err(error).context("connect CMUX named pipe"),
            }
        };
        #[cfg(any(unix, windows))]
        {
            exchange(stream, method, params).await
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (endpoint, method, params);
            bail!("CMUX local transport unsupported on this OS")
        }
    })
    .await
    .context("CMUX session lookup timed out after 2 seconds")?
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    method: &str,
    params: Value,
) -> Result<Value> {
    let mut request = serde_json::to_vec(&json!({"id": 1, "method": method, "params": params}))?;
    request.push(b'\n');
    stream.write_all(&request).await?;
    let mut response = Vec::new();
    BufReader::new(stream.take(MAX_RESPONSE + 1))
        .read_until(b'\n', &mut response)
        .await?;
    ensure!(
        response.len() as u64 <= MAX_RESPONSE,
        "CMUX session response exceeds 64 KiB"
    );
    ensure!(
        response.last() == Some(&b'\n'),
        "incomplete CMUX session response"
    );
    let response: Value = serde_json::from_slice(&response).context("decode CMUX RPC response")?;
    ensure!(
        response.get("id") == Some(&json!(1)),
        "CMUX RPC response ID mismatch"
    );
    if let Some(error) = response.get("error").filter(|value| !value.is_null()) {
        bail!("CMUX {method}: {error}");
    }
    response
        .get("result")
        .cloned()
        .context("CMUX RPC response missing result")
}

#[cfg(test)]
mod tests {
    use super::*;
    const SURFACE: &str = "00000000-0000-4000-8000-000000000001";

    fn session_value(surface: &str) -> Value {
        json!({"session_id":"00000000-0000-4000-8000-000000000002", "instance_id":"00000000-0000-4000-8000-000000000003",
            "provider":"codex", "os": std::env::consts::OS, "surface_id":surface, "workspace_id":"00000000-0000-4000-8000-000000000004"})
    }

    async fn serve_session<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) {
        let mut request = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut request)
            .await
            .unwrap();
        let request: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "gateway.session");
        assert_eq!(request["params"]["surface_id"], SURFACE);
        let mut response =
            serde_json::to_vec(&json!({"id":request["id"],"result":session_value(SURFACE)}))
                .unwrap();
        response.push(b'\n');
        stream.write_all(&response).await.unwrap();
        // Keep the Windows server handle alive until the client consumes its
        // response; closing a named pipe can discard unread buffered bytes.
        let mut closed = [0u8; 1];
        let _ = stream.read(&mut closed).await;
    }

    #[tokio::test]
    #[cfg(any(unix, windows))]
    async fn native_local_transport_resolves_verified_session() {
        #[cfg(unix)]
        let (endpoint, fixture, _directory) = {
            let directory = tempfile::tempdir().unwrap();
            let endpoint = directory.path().join("session.sock");
            let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
            let fixture = tokio::spawn(async move {
                serve_session(listener.accept().await.unwrap().0).await;
            });
            (endpoint.to_string_lossy().into_owned(), fixture, directory)
        };
        #[cfg(windows)]
        let (endpoint, fixture) = {
            let endpoint = format!(r"\\.\pipe\agent-tools-session-{}", std::process::id());
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint)
                .unwrap();
            let fixture = tokio::spawn(async move {
                server.connect().await.unwrap();
                serve_session(server).await;
            });
            (endpoint, fixture)
        };
        let context = LocalContext {
            endpoint,
            surface_id: SURFACE.into(),
        };
        let session = context.session().await.unwrap();
        assert_eq!(session.origin.provider, "codex");
        assert_eq!(session.surface_id, SURFACE);
        fixture.await.unwrap();
    }

    #[test]
    fn identity_rejects_malformed_provenance() {
        for field in [
            "session_id",
            "instance_id",
            "provider",
            "os",
            "surface_id",
            "workspace_id",
        ] {
            let mut value = session_value(SURFACE);
            value[field] = json!("unknown");
            let session: AgentSession = serde_json::from_value(value).unwrap();
            assert!(
                validate_session(&session).is_err(),
                "accepted invalid {field}"
            );
        }
    }

    #[test]
    fn identity_is_surface_bound_and_never_reuses_an_old_generation() {
        let context = LocalContext {
            endpoint: "socket".into(),
            surface_id: SURFACE.into(),
        };
        assert!(context
            .decode_session(session_value("00000000-0000-4000-8000-000000000099"))
            .is_err());
        let first = context.decode_session(session_value(SURFACE)).unwrap();
        let mut replacement = session_value(SURFACE);
        replacement["session_id"] = json!("00000000-0000-4000-8000-000000000098");
        let next = context.decode_session(replacement).unwrap();
        assert_ne!(first.origin.session_id, next.origin.session_id);
        assert!(context.decode_session(Value::Null).is_err());
    }

    #[test]
    fn explicit_context_never_falls_back_to_legacy() {
        assert!(LocalContext::from_values(None, None, None)
            .unwrap()
            .is_none());
        assert!(LocalContext::from_values(None, None, Some(SURFACE.into())).is_err());
        assert!(
            LocalContext::from_values(Some(String::new()), None, Some(SURFACE.into())).is_err()
        );
        assert!(LocalContext::from_values(Some("socket".into()), None, None).is_err());
        assert!(
            LocalContext::from_values(Some("socket".into()), None, Some("1000".into())).is_err()
        );
        assert_eq!(
            LocalContext::from_values(None, Some("pipe".into()), Some(SURFACE.into()))
                .unwrap()
                .unwrap()
                .endpoint,
            "pipe"
        );
    }

    #[tokio::test]
    async fn rpc_checks_errors_and_framing() {
        for (reply, expected) in [
            ("{\"id\":1,\"result\":{\"ok\":true}}\n", true),
            (
                "{\"id\":1,\"error\":{\"message\":\"no verified session\"}}\n",
                false,
            ),
            ("{\"id\":2,\"result\":{}}\n", false),
            ("{\"id\":1,\"result\":{}}", false),
            ("not json\n", false),
        ] {
            let (client, mut server) = tokio::io::duplex(4096);
            let fixture = tokio::spawn(async move {
                let mut request = String::new();
                BufReader::new(&mut server)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                assert_eq!(request["method"], "gateway.session");
                assert_eq!(request["params"]["surface_id"], SURFACE);
                server.write_all(reply.as_bytes()).await.unwrap();
            });
            assert_eq!(
                exchange(client, "gateway.session", json!({"surface_id": SURFACE}))
                    .await
                    .is_ok(),
                expected
            );
            fixture.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rpc_rejects_oversized_response() {
        let (client, mut server) = tokio::io::duplex(4096);
        let fixture = tokio::spawn(async move {
            let mut request = String::new();
            BufReader::new(&mut server)
                .read_line(&mut request)
                .await
                .unwrap();
            let _ = server
                .write_all(&vec![b' '; MAX_RESPONSE as usize + 1])
                .await;
        });
        assert!(exchange(client, "gateway.sessions", json!({}))
            .await
            .unwrap_err()
            .to_string()
            .contains("64 KiB"));
        fixture.await.unwrap();
    }
}
