//! Optional amux desktop membership. amux advertises its sibling CLI through
//! `AMUX_CLI_PATH`; the CLI validates the caller's ancestry itself. Like CMUX,
//! a failure here never revokes or replaces the independently derived actor.

use agent_comms::{
    actor::Actor,
    session::{is_uuid, SessionOrigin},
};
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TIMEOUT: Duration = Duration::from_secs(2);
const MAX_OUTPUT: u64 = 64 * 1024;
const MAX_PEERS: usize = 64;

/// The advertised amux CLI, only when it is an absolute path to a file.
fn cli() -> Option<PathBuf> {
    cli_from(std::env::var_os("AMUX_CLI_PATH"))
}

fn cli_from(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path = PathBuf::from(value?);
    (path.is_absolute() && path.as_os_str().len() <= 4096 && path.is_file()).then_some(path)
}

pub(crate) fn available() -> bool {
    cli().is_some()
}

/// Register the calling actor on its amux terminal and return the verified echo.
pub(crate) async fn announce(actor: &Actor) -> Result<Value> {
    let cli = cli().context("amux CLI unavailable")?;
    let value = invoke(&cli, "announce", Some(serde_json::to_vec(actor)?)).await?;
    decode_announce(actor, value)
}

/// List live amux terminals and their binding state.
pub(crate) async fn peers() -> Result<Value> {
    let cli = cli().context("amux CLI unavailable")?;
    let value = invoke(&cli, "peers", None).await?;
    validate_peers(&value)?;
    Ok(value)
}

/// Direct exec (no shell) with a deadline over spawn, stdin, bounded stdout and exit.
async fn invoke(cli: &PathBuf, operation: &str, input: Option<Vec<u8>>) -> Result<Value> {
    tokio::time::timeout(TIMEOUT, async {
        let mut child = tokio::process::Command::new(cli)
            .args(["actor", operation])
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("launch amux CLI")?;
        if let Some(input) = input {
            let mut stdin = child.stdin.take().context("amux CLI stdin unavailable")?;
            stdin.write_all(&input).await?;
            // Dropping stdin closes it so amux sees end of input.
        }
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .context("amux CLI stdout unavailable")?
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut output)
            .await?;
        ensure!(
            output.len() as u64 <= MAX_OUTPUT,
            "amux response exceeds 64 KiB"
        );
        // Never surface amux's stderr or output contents in errors.
        ensure!(child.wait().await?.success(), "amux rejected actor request");
        serde_json::from_slice(&output).context("decode amux response")
    })
    .await
    .context("amux lookup timed out after 2 seconds")?
}

fn valid_attachment(value: &Value) -> bool {
    value
        .get("workspace")
        .and_then(Value::as_str)
        .is_some_and(is_uuid)
        && value.get("terminal").is_some_and(Value::is_u64)
        && value
            .get("provider")
            .and_then(Value::as_str)
            .is_some_and(|p| matches!(p, "codex" | "claude"))
        && value.get("project").is_some_and(Value::is_string)
}

fn decode_announce(actor: &Actor, value: Value) -> Result<Value> {
    ensure!(
        value
            .get("instance")
            .and_then(Value::as_str)
            .is_some_and(is_uuid)
            && valid_attachment(&value),
        "invalid amux attachment"
    );
    let echoed = |field: &str, expected: Value| value.get(field) == Some(&expected);
    ensure!(
        echoed("version", actor.version.into())
            && echoed("origin", serde_json::to_value(&actor.origin)?)
            && echoed(
                "provider_session_id",
                actor.provider_session_id.clone().into()
            )
            && echoed("base_id", actor.base_id.clone().into())
            && echoed("session_slot", actor.session_slot.into())
            && echoed("actor_id", actor.actor_id.clone().into())
            && echoed("provider", actor.origin.provider.clone().into()),
        "amux membership did not echo the verified actor"
    );
    match value.get("binding_state").and_then(Value::as_str) {
        Some("bound" | "ambiguous") => Ok(value),
        _ => bail!("unknown amux actor binding state"),
    }
}

fn validate_peers(value: &Value) -> Result<()> {
    ensure!(
        value
            .get("instance")
            .and_then(Value::as_str)
            .is_some_and(is_uuid),
        "invalid amux instance"
    );
    let peers = value
        .get("peers")
        .and_then(Value::as_array)
        .context("amux peers response missing peers")?;
    ensure!(peers.len() <= MAX_PEERS, "too many amux peers");
    for peer in peers {
        ensure!(valid_attachment(peer), "invalid amux peer attachment");
        match peer.get("binding_state").and_then(Value::as_str) {
            Some("bound") => {
                let origin: SessionOrigin = serde_json::from_value(
                    peer.get("origin")
                        .cloned()
                        .context("bound amux peer lacks origin")?,
                )?;
                origin.validate()?;
                ensure!(
                    peer.get("actor_id")
                        .is_some_and(|id| id.is_string() || id.is_null()),
                    "invalid amux peer actor"
                );
            }
            Some("unbound" | "ambiguous") => ensure!(
                peer.get("origin").is_none(),
                "unbound amux peer contains actor context"
            ),
            _ => bail!("unknown amux peer binding state"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Actor {
        let vectors: Value =
            serde_json::from_str(include_str!("../../../docs/actor-origin-vectors.json")).unwrap();
        let row = &vectors[0];
        serde_json::from_value(json!({"version":2,"origin":{"session_id":row["session_id"],"instance_id":row["instance_id"],"provider":row["provider"],"os":row["os"]},"provider_session_id":row["provider_session_id"],"base_id":row["base_id"],"session_slot":row["session_slot"],"actor_id":row["actor_id"]})).unwrap()
    }

    const UUID: &str = "00000000-0000-4000-8000-000000000001";

    fn announced(actor: &Actor, state: &str) -> Value {
        let mut value = serde_json::to_value(actor).unwrap();
        value["goal"] = Value::Null;
        value["instance"] = json!(UUID);
        value["workspace"] = json!(UUID);
        value["terminal"] = json!(3);
        value["provider"] = json!(actor.origin.provider);
        value["project"] = json!("/repo");
        value["binding_state"] = json!(state);
        value
    }

    #[test]
    fn announce_must_echo_actor_on_a_valid_attachment() {
        let actor = fixture();
        assert!(decode_announce(&actor, announced(&actor, "bound")).is_ok());
        assert!(decode_announce(&actor, announced(&actor, "ambiguous")).is_ok());
        assert!(decode_announce(&actor, announced(&actor, "unbound")).is_err());
        for field in [
            "version",
            "origin",
            "provider_session_id",
            "base_id",
            "session_slot",
            "actor_id",
            "provider",
            "instance",
            "workspace",
            "terminal",
            "project",
        ] {
            let mut broken = announced(&actor, "bound");
            broken[field] = Value::Null;
            assert!(decode_announce(&actor, broken).is_err(), "{field}");
        }
        let mut other = announced(&actor, "bound");
        other["provider"] = json!(if actor.origin.provider == "codex" {
            "claude"
        } else {
            "codex"
        });
        assert!(decode_announce(&actor, other).is_err());
    }

    #[test]
    fn peers_require_exact_binding_context() {
        let origin = serde_json::to_value(fixture().origin).unwrap();
        let peer = |state: &str| json!({"workspace":UUID,"terminal":1,"provider":"codex","project":"/repo","binding_state":state});
        let mut bound = peer("bound");
        bound["origin"] = origin.clone();
        bound["actor_id"] = json!("a-1");
        bound["goal"] = Value::Null;
        let ok =
            json!({"instance":UUID,"peers":[bound.clone(), peer("unbound"), peer("ambiguous")]});
        assert!(validate_peers(&ok).is_ok());
        assert!(validate_peers(&json!({"instance":UUID,"peers":[peer("bound")]})).is_err());
        let mut leaked = peer("ambiguous");
        leaked["origin"] = origin;
        assert!(validate_peers(&json!({"instance":UUID,"peers":[leaked]})).is_err());
        assert!(validate_peers(&json!({"instance":UUID,"peers":[peer("other")]})).is_err());
        assert!(validate_peers(&json!({"instance":UUID})).is_err());
        assert!(validate_peers(&json!({"peers":[]})).is_err());
        let many: Vec<Value> = (0..=MAX_PEERS).map(|_| peer("unbound")).collect();
        assert!(validate_peers(&json!({"instance":UUID,"peers":many})).is_err());
    }

    /// A fake amux CLI: a script that records its argv/stdin and replies verbatim.
    #[cfg(unix)]
    fn fake_cli(directory: &std::path::Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("amux");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_announces_exact_actor_over_stdin_without_shell_eval() {
        let directory = tempfile::tempdir().unwrap();
        let actor = fixture();
        let reply = directory.path().join("reply.json");
        std::fs::write(&reply, announced(&actor, "bound").to_string()).unwrap();
        let input = directory.path().join("input.json");
        let args = directory.path().join("args");
        let cli = fake_cli(
            directory.path(),
            &format!(
                "echo \"$@\" > '{}'; cat > '{}'; cat '{}'",
                args.display(),
                input.display(),
                reply.display()
            ),
        );
        let value = invoke(&cli, "announce", Some(serde_json::to_vec(&actor).unwrap()))
            .await
            .unwrap();
        assert_eq!(
            decode_announce(&actor, value).unwrap()["binding_state"],
            "bound"
        );
        assert_eq!(
            std::fs::read_to_string(args).unwrap().trim(),
            "actor announce"
        );
        let sent: Actor = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        assert_eq!(sent, actor);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_failures_stale_context_oversize_and_hangs_are_errors_without_leaks() {
        let directory = tempfile::tempdir().unwrap();
        for body in [
            // amux exits non-zero for absent or stale published context.
            "echo 'Published amux terminal context is stale DO_NOT_LOG' >&2; exit 1",
            "echo '{\"error\":\"DO_NOT_LOG\"}'; exit 1",
            "echo 'not JSON DO_NOT_LOG'",
            "head -c 70000 /dev/zero | tr '\\0' ' '",
            "exec sleep 10",
        ] {
            let cli = fake_cli(directory.path(), body);
            let started = std::time::Instant::now();
            let error = invoke(&cli, "peers", None).await.unwrap_err();
            assert!(started.elapsed() < Duration::from_secs(5));
            assert!(!format!("{error:#}").contains("DO_NOT_LOG"), "{body}");
        }
        let missing = directory.path().join("absent");
        assert!(invoke(&missing, "peers", None).await.is_err());
    }

    #[test]
    fn advertised_cli_must_be_an_absolute_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("amux");
        std::fs::write(&file, "").unwrap();
        assert_eq!(cli_from(Some(file.clone().into())), Some(file));
        assert!(cli_from(None).is_none());
        assert!(cli_from(Some("amux".into())).is_none());
        assert!(cli_from(Some(directory.path().into())).is_none());
        assert!(cli_from(Some(directory.path().join("absent").into())).is_none());
    }
}
