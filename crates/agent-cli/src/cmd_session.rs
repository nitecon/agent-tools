//! Actor discovery and optional CMUX membership. A local terminal never supplies
//! actor identity; it can only validate/bind the independently derived origin.

use agent_comms::{
    actor::Actor,
    session::{is_uuid, SessionOrigin},
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const TIMEOUT: Duration = Duration::from_secs(2);
const MAX_RESPONSE: u64 = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Membership {
    origin: SessionOrigin,
    provider_session_id: String,
    base_id: String,
    session_slot: u32,
    actor_id: String,
    binding_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recipient_session_id: Option<String>,
}

fn decode_membership(actor: &Actor, value: Value) -> Result<Membership> {
    let membership: Membership =
        serde_json::from_value(value).context("decode CMUX actor membership")?;
    ensure!(
        membership.origin == actor.origin
            && membership.provider_session_id == actor.provider_session_id
            && membership.base_id == actor.base_id
            && membership.session_slot == actor.session_slot
            && membership.actor_id == actor.actor_id,
        "CMUX membership did not echo the verified actor"
    );
    match membership.binding_state.as_str() {
        "bound" => ensure!(
            [
                &membership.surface_id,
                &membership.workspace_id,
                &membership.recipient_session_id
            ]
            .iter()
            .all(|field| field.as_deref().is_some_and(is_uuid)),
            "CMUX bound membership lacks valid recipient context"
        ),
        "unbound" => ensure!(
            membership.surface_id.is_none()
                && membership.workspace_id.is_none()
                && membership.recipient_session_id.is_none(),
            "CMUX unbound membership contains recipient context"
        ),
        _ => bail!("unknown CMUX actor binding state"),
    }
    Ok(membership)
}

pub(crate) async fn mutation_origin() -> Result<Option<SessionOrigin>> {
    let actor = crate::actor_runtime::current()
        .context("register calling conversation; task mutation was not sent")?;
    if let Some(actor) = actor {
        // Optional membership is independent of hooks and cannot revoke provenance.
        // No RPC absence, old API, invalid binding or timeout revokes provenance.
        let _ = membership(&actor, "gateway.session.announce").await;
        Ok(Some(actor.origin))
    } else {
        Ok(None)
    }
}

pub(crate) fn run(peers: bool, json_output: bool) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(discover(peers, json_output))
}

async fn discover(peers: bool, json_output: bool) -> Result<()> {
    if peers {
        let peers = tokio::time::timeout(TIMEOUT, async {
            for endpoint in endpoints()? {
                if let Ok(value) = rpc(&endpoint, "gateway.sessions", json!({})).await {
                    validate_peers(&value)?;
                    return Ok(value);
                }
            }
            bail!("CMUX peer discovery unavailable")
        })
        .await
        .context("CMUX peer lookup timed out")??;
        if json_output {
            println!("{}", serde_json::to_string(&peers)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&peers)?);
        }
        return Ok(());
    }
    let actor = crate::actor_runtime::current()?
        .context("no provider-native actor context in this invocation")?;
    let bound = membership(&actor, "gateway.session.resolve").await.ok();
    if json_output {
        let mut value = serde_json::to_value(&actor)?;
        if let Some(bound) = bound {
            value["membership"] = serde_json::to_value(bound)?;
        }
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!(
            "{} {} {} session={} instance={} native={} binding={}",
            actor.actor_id,
            actor.origin.provider,
            actor.origin.os,
            actor.origin.session_id,
            actor.origin.instance_id,
            actor.provider_session_id,
            bound
                .as_ref()
                .map(|m| m.binding_state.as_str())
                .unwrap_or("unavailable")
        );
        if let Some(surface) = bound.and_then(|m| m.surface_id) {
            println!("surface={surface}");
        }
    }
    Ok(())
}

fn validate_peers(value: &Value) -> Result<()> {
    let sessions = value
        .get("sessions")
        .and_then(Value::as_array)
        .context("CMUX peers response missing sessions")?;
    for session in sessions {
        let origin: SessionOrigin = serde_json::from_value(session.clone())?;
        origin.validate()?;
        ensure!(
            ["surface_id", "workspace_id"].iter().all(|field| session
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(is_uuid)),
            "invalid CMUX peer surface/workspace UUID"
        );
    }
    Ok(())
}

async fn membership(actor: &Actor, method: &str) -> Result<Membership> {
    tokio::time::timeout(TIMEOUT, async {
        let mut params = serde_json::to_value(actor)?;
        // Announce an actual remote only, never project_ident's cwd fallback.
        // Async git stays inside the membership deadline and is killed on drop.
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .kill_on_drop(true)
            .stderr(std::process::Stdio::null())
            .output()
            .await
        {
            if output.status.success() {
                if let Ok(remote) = std::str::from_utf8(&output.stdout) {
                    let repository = agent_core::storage::normalize_git_url(remote.trim());
                    let repository = repository.strip_suffix(".git").unwrap_or(&repository);
                    if !repository.is_empty() {
                        params["repository"] = json!(repository);
                    }
                }
            }
        }
        for endpoint in endpoints()? {
            // Try only conventional/hinted endpoints. A server error does not
            // authorize replacement provenance or logging request contents.
            if let Ok(result) = rpc(&endpoint, method, params.clone()).await {
                return decode_membership(actor, result);
            }
        }
        bail!("CMUX actor membership unavailable")
    })
    .await
    .context("CMUX membership lookup timed out")?
}

fn endpoints() -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for key in ["CMUX_SOCKET", "CMUX_SOCKET_PATH"] {
        if let Ok(path) = std::env::var(key) {
            if !path.is_empty() && path.len() <= 4096 && !paths.contains(&path) {
                #[cfg(unix)]
                if !PathBuf::from(&path).is_absolute() {
                    continue;
                }
                paths.push(path);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid has no parameters or memory effects.
        let uid = unsafe { libc::getuid() };
        let configured = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .filter(|p| {
                std::fs::symlink_metadata(p)
                    .is_ok_and(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
            });
        let runtime = configured.unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}")));
        let directory = runtime.join("cmux");
        paths.push(directory.join("cmux.sock").to_string_lossy().into_owned());
        let marker = directory.join("last-socket-path");
        if let Ok(metadata) = std::fs::symlink_metadata(&marker) {
            if metadata.is_file()
                && metadata.uid() == uid
                && metadata.mode() & 0o077 == 0
                && metadata.len() <= 4096
            {
                if let Ok(text) = std::fs::read_to_string(marker) {
                    let path = text.trim();
                    if PathBuf::from(path).is_absolute() && !paths.iter().any(|p| p == path) {
                        paths.push(path.into());
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    if let Ok(sid) = user_sid() {
        paths.push(format!(r"\\.\pipe\cmux-{sid}-control"));
    }
    // No well-known macOS membership endpoint is advertised by this release.
    Ok(paths)
}

#[cfg(windows)]
fn user_sid() -> Result<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = std::ptr::null_mut();
    // SAFETY: native handles and checked output buffers match the API; all
    // allocated resources are released, including failure paths.
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "current user token unavailable"
    );
    let result = (|| {
        let mut size = 0;
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size) };
        ensure!(size > 0 && size <= 65536, "invalid current user token size");
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        ensure!(
            unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut size,
                )
            } != 0,
            "current user SID unavailable"
        );
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut text = std::ptr::null_mut();
        ensure!(
            unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } != 0,
            "current user SID text unavailable"
        );
        let mut length = 0;
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        let sid = if length < 256 {
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
                .context("invalid SID text")
        } else {
            Err(anyhow::anyhow!("SID text exceeds bound"))
        };
        unsafe { LocalFree(text.cast()) };
        sid
    })();
    unsafe { CloseHandle(token) };
    result
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
    .context("CMUX lookup timed out after 2 seconds")?
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    method: &str,
    params: Value,
) -> Result<Value> {
    let mut request = serde_json::to_vec(&json!({"id":1,"method":method,"params":params}))?;
    request.push(b'\n');
    stream.write_all(&request).await?;
    let mut response = Vec::new();
    BufReader::new(stream.take(MAX_RESPONSE + 1))
        .read_until(b'\n', &mut response)
        .await?;
    ensure!(
        response.len() as u64 <= MAX_RESPONSE,
        "CMUX response exceeds 64 KiB"
    );
    ensure!(response.last() == Some(&b'\n'), "incomplete CMUX response");
    let response: Value = serde_json::from_slice(&response).context("decode CMUX RPC response")?;
    ensure!(
        response.get("id") == Some(&json!(1)),
        "CMUX RPC response ID mismatch"
    );
    if response.get("error").is_some_and(|value| !value.is_null()) {
        // Never print a server-controlled error object containing request contents.
        bail!("CMUX rejected actor membership request");
    }
    response
        .get("result")
        .cloned()
        .context("CMUX RPC response missing result")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Actor {
        let vectors: Value =
            serde_json::from_str(include_str!("../../../docs/actor-origin-vectors.json")).unwrap();
        let row = &vectors[0];
        serde_json::from_value(json!({"version":2,"origin":{"session_id":row["session_id"],"instance_id":row["instance_id"],"provider":row["provider"],"os":row["os"]},"provider_session_id":row["provider_session_id"],"base_id":row["base_id"],"session_slot":row["session_slot"],"actor_id":row["actor_id"]})).unwrap()
    }
    fn response(actor: &Actor) -> Value {
        let mut value = serde_json::to_value(actor).unwrap();
        value["binding_state"] = json!("unbound");
        value
    }
    #[test]
    fn membership_must_echo_actor_and_requires_exact_bound_context() {
        let actor = fixture();
        let value = response(&actor);
        assert!(decode_membership(&actor, value.clone()).is_ok());
        for field in [
            "origin",
            "provider_session_id",
            "base_id",
            "session_slot",
            "actor_id",
        ] {
            let mut broken = value.clone();
            broken[field] = Value::Null;
            assert!(decode_membership(&actor, broken).is_err());
        }
        let mut bound = value;
        bound["binding_state"] = json!("bound");
        assert!(decode_membership(&actor, bound.clone()).is_err());
        for field in ["surface_id", "workspace_id", "recipient_session_id"] {
            bound[field] = json!("00000000-0000-4000-8000-000000000001");
        }
        assert!(decode_membership(&actor, bound).is_ok());
    }
    #[test]
    fn peers_keep_existing_verified_native_session_schema() {
        let mut session = serde_json::to_value(fixture().origin).unwrap();
        session["surface_id"] = json!("00000000-0000-4000-8000-000000000001");
        session["workspace_id"] = session["surface_id"].clone();
        assert!(validate_peers(&json!({"sessions":[session.clone()]})).is_ok());
        session["provider"] = json!("unknown");
        assert!(validate_peers(&json!({"sessions":[session]})).is_err());
        assert!(validate_peers(&json!({})).is_err());
    }
    async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) {
        let mut request = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut request)
            .await
            .unwrap();
        let request: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "gateway.session.announce");
        assert_eq!(
            request["params"]["origin"],
            serde_json::to_value(&fixture().origin).unwrap()
        );
        let mut reply = serde_json::to_vec(&json!({"id":1,"result":response(&fixture())})).unwrap();
        reply.push(b'\n');
        stream.write_all(&reply).await.unwrap();
        let mut closed = [0u8; 1];
        let _ = stream.read(&mut closed).await;
    }
    #[tokio::test]
    async fn native_transport_announces_exact_actor() {
        #[cfg(unix)]
        let (endpoint, job, _directory) = {
            let directory = tempfile::tempdir().unwrap();
            let endpoint = directory.path().join("actor.sock");
            let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
            let job = tokio::spawn(async move { serve(listener.accept().await.unwrap().0).await });
            (endpoint.to_string_lossy().into_owned(), job, directory)
        };
        #[cfg(windows)]
        let (endpoint, job) = {
            let endpoint = format!(r"\\.\pipe\agent-tools-actor-{}", std::process::id());
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint)
                .unwrap();
            let job = tokio::spawn(async move {
                server.connect().await.unwrap();
                serve(server).await
            });
            (endpoint, job)
        };
        let actor = fixture();
        let value = rpc(
            &endpoint,
            "gateway.session.announce",
            serde_json::to_value(&actor).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            decode_membership(&actor, value).unwrap().binding_state,
            "unbound"
        );
        job.await.unwrap();
    }
    #[tokio::test]
    async fn rpc_rejects_errors_bad_framing_and_oversized_responses_without_token_logging() {
        for reply in [
            "{\"id\":1,\"error\":{\"token\":\"DO_NOT_LOG\"}}\n".to_owned(),
            "{\"id\":2,\"result\":{}}\n".into(),
            "{\"id\":1,\"result\":{}}".into(),
            "not JSON\n".into(),
            " ".repeat(MAX_RESPONSE as usize + 1),
        ] {
            let (client, mut server) = tokio::io::duplex(4096);
            let job = tokio::spawn(async move {
                let mut request = String::new();
                BufReader::new(&mut server)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let _ = server.write_all(reply.as_bytes()).await;
            });
            let error = exchange(client, "gateway.session.resolve", json!({}))
                .await
                .unwrap_err();
            assert!(!error.to_string().contains("DO_NOT_LOG"));
            job.await.unwrap();
        }
    }
}
