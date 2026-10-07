//! Client-owned actor identity. CMUX membership and cwd never select an actor.

use crate::{config::home_dir, session::SessionOrigin};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};
use uuid::Uuid;

pub const CONTRACT: &str = "agent-tools-actor-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub version: u32,
    pub origin: SessionOrigin,
    pub provider_session_id: String,
    pub executor_generation: Vec<String>,
}

/// Identity initialization publishes a complete file without replacing a winner.
/// No display-id override or terminal metadata participates in this namespace.
pub fn instance_id() -> Result<Uuid> {
    load_instance(&home_dir().join(".agentic/agent-tools/actor-instance-id"))
}

fn load_instance(path: &Path) -> Result<Uuid> {
    match std::fs::read_to_string(path) {
        Ok(text) => return parse_instance(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("read actor instance namespace"),
    }
    let directory = path.parent().context("actor instance directory")?;
    std::fs::create_dir_all(directory)?;
    let mut pending = tempfile::NamedTempFile::new_in(directory)?;
    let id = Uuid::new_v4();
    writeln!(pending, "{id}")?;
    pending.as_file().sync_all()?;
    match pending.persist_noclobber(path) {
        Ok(_) => Ok(id),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            parse_instance(&std::fs::read_to_string(path)?)
        }
        Err(error) => Err(error.error).context("publish actor instance namespace"),
    }
}

fn parse_instance(text: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(text.trim()).context("invalid actor instance namespace")?;
    ensure!(id.get_version_num() == 4, "actor instance must be a UUIDv4");
    Ok(id)
}

pub fn normalize_native_id(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value.bytes().all(|b| (0x21..=0x7e).contains(&b)),
        "invalid provider-native session ID"
    );
    Ok(match Uuid::parse_str(value) {
        Ok(id) => id.to_string(),
        Err(_) => value.to_owned(),
    })
}

pub fn derive(
    instance: Uuid,
    os: &str,
    provider: &str,
    native_id: &str,
    generation: Vec<String>,
) -> Result<Actor> {
    ensure!(
        instance.get_version_num() == 4,
        "actor instance must be a UUIDv4"
    );
    ensure!(
        matches!(provider, "codex" | "claude"),
        "unsupported actor provider"
    );
    validate_generation(os, &generation)?;
    let native_id = normalize_native_id(native_id)?;
    let name = serde_json::to_vec(&(CONTRACT, os, provider, &native_id, &generation))?;
    let origin = SessionOrigin {
        session_id: Uuid::new_v5(&instance, &name).to_string(),
        instance_id: instance.to_string(),
        provider: provider.into(),
        os: os.into(),
    };
    origin.validate()?;
    Ok(Actor {
        version: 1,
        origin,
        provider_session_id: native_id,
        executor_generation: generation,
    })
}

fn validate_generation(os: &str, fields: &[String]) -> Result<()> {
    let numeric = match os {
        "linux" => {
            ensure!(
                fields.len() == 4 && fields[0] == "linux-proc-v1",
                "invalid Linux executor generation"
            );
            let boot = Uuid::parse_str(&fields[1]).context("invalid Linux boot UUID")?;
            ensure!(
                boot.to_string() == fields[1],
                "noncanonical Linux boot UUID"
            );
            &fields[2..]
        }
        "windows" | "macos" => {
            let kind = if os == "windows" {
                "windows-process-v1"
            } else {
                "macos-proc-v1"
            };
            ensure!(
                fields.len() == 3 && fields[0] == kind,
                "invalid executor generation"
            );
            &fields[1..]
        }
        _ => anyhow::bail!("unsupported actor OS"),
    };
    for value in numeric {
        let number: u64 = value
            .parse()
            .context("invalid executor generation number")?;
        ensure!(
            number.to_string() == *value,
            "noncanonical executor generation number"
        );
    }
    let pid: u32 = numeric[0].parse().context("invalid executor PID")?;
    ensure!(pid > 0, "executor PID must be positive");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_uuid_conformance_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/actor-origin-vectors.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            let generation = serde_json::from_value(vector["executor_generation"].clone()).unwrap();
            let actor = derive(
                Uuid::parse_str(vector["instance_id"].as_str().unwrap()).unwrap(),
                vector["os"].as_str().unwrap(),
                vector["provider"].as_str().unwrap(),
                vector["provider_session_id"].as_str().unwrap(),
                generation,
            )
            .unwrap();
            assert_eq!(
                actor.origin.session_id, vector["session_id"],
                "{}",
                vector["name"]
            );
        }
    }

    #[test]
    fn atomic_first_use_has_one_winner_and_corruption_is_not_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actor-instance-id");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    load_instance(&path).unwrap()
                })
            })
            .collect();
        let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(ids.iter().all(|id| *id == ids[0]));
        std::fs::write(&path, "corrupt").unwrap();
        assert!(load_instance(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "corrupt");
    }

    #[test]
    fn native_ids_and_generation_are_not_silently_repaired() {
        for invalid in ["", " spaced", "has space", "newline\n", "\u{2603}"] {
            assert!(normalize_native_id(invalid).is_err());
        }
        let id = Uuid::new_v4();
        assert_eq!(
            normalize_native_id(&id.to_string().to_uppercase()).unwrap(),
            id.to_string()
        );
        assert!(derive(
            id,
            "windows",
            "codex",
            "thread-1",
            vec!["windows-process-v1".into(), "01".into(), "2".into()]
        )
        .is_err());
    }
}
