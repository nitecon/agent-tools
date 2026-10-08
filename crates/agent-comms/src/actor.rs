//! Automatic local conversation registration, independent of provider processes.

use crate::session::SessionOrigin;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const CONTRACT: &str = "agent-tools-actor-v2";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub version: u32,
    pub origin: SessionOrigin,
    pub provider_session_id: String,
    pub base_id: String,
    pub session_slot: u32,
}

/// Identity initialization publishes a complete file without replacing a winner.
/// No display-id override or terminal metadata participates in this namespace.
pub fn instance_id() -> Result<Uuid> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("actor data home unavailable")?;
    ensure!(home.is_absolute(), "actor data home must be absolute");
    load_instance(&home.join(".agentic/agent-tools/actor-instance-id"))
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

pub fn base_id(
    instance: Uuid,
    os: &str,
    provider: &str,
    project_path: &str,
    git_identity: &str,
) -> Result<Uuid> {
    ensure!(
        instance.get_version_num() == 4,
        "actor instance must be a UUIDv4"
    );
    ensure!(
        matches!(os, "linux" | "windows" | "macos"),
        "unsupported actor OS"
    );
    ensure!(
        matches!(provider, "codex" | "claude"),
        "unsupported actor provider"
    );
    ensure!(!project_path.is_empty(), "actor project path unavailable");
    let name = serde_json::to_vec(&(CONTRACT, "base", os, project_path, git_identity, provider))?;
    Ok(Uuid::new_v5(&instance, &name))
}

pub fn derive(
    instance: Uuid,
    os: &str,
    provider: &str,
    native_id: &str,
    base: Uuid,
    session_slot: u32,
) -> Result<Actor> {
    ensure!(
        instance.get_version_num() == 4,
        "actor instance must be a UUIDv4"
    );
    ensure!(
        matches!(provider, "codex" | "claude"),
        "unsupported actor provider"
    );
    ensure!(base.get_version_num() == 5, "actor base must be a UUIDv5");
    ensure!(session_slot > 0, "actor session slot must be positive");
    let native_id = normalize_native_id(native_id)?;
    let name = serde_json::to_vec(&(CONTRACT, &native_id))?;
    let origin = SessionOrigin {
        session_id: Uuid::new_v5(&base, &name).to_string(),
        instance_id: instance.to_string(),
        provider: provider.into(),
        os: os.into(),
    };
    origin.validate()?;
    Ok(Actor {
        version: 2,
        origin,
        provider_session_id: native_id,
        base_id: base.to_string(),
        session_slot,
    })
}

/// Slots are display/registration metadata, never a replacement conversation key.
pub fn register(
    instance: Uuid,
    os: &str,
    provider: &str,
    native: &str,
    base: Uuid,
) -> Result<Actor> {
    let mut actor = derive(instance, os, provider, native, base, 1)?;
    let directory = std::env::temp_dir()
        .join("agent-tools-actors")
        .join(instance.to_string())
        .join(base.to_string());
    actor.session_slot = register_slot(&directory, &actor.origin.session_id)?;
    Ok(actor)
}

fn register_slot(directory: &Path, session_id: &str) -> Result<u32> {
    std::fs::create_dir_all(directory)?;
    for slot in 1..=65535 {
        let path = directory.join(slot.to_string());
        match std::fs::read_to_string(&path) {
            Ok(existing) => {
                let id =
                    Uuid::parse_str(existing.trim()).context("invalid local actor registration")?;
                ensure!(
                    id.get_version_num() == 5,
                    "invalid local actor registration"
                );
                if id.to_string() == session_id {
                    return Ok(slot);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut pending = tempfile::NamedTempFile::new_in(directory)?;
                writeln!(pending, "{session_id}")?;
                match pending.persist_noclobber(&path) {
                    Ok(_) => return Ok(slot),
                    Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                        // Read the complete winning registration before trying
                        // another slot, including concurrent calls by this actor.
                        let existing = std::fs::read_to_string(&path)?;
                        if existing.trim() == session_id {
                            return Ok(slot);
                        }
                        Uuid::parse_str(existing.trim())
                            .context("invalid local actor registration")?;
                    }
                    Err(error) => {
                        return Err(error.error).context("publish local actor registration")
                    }
                }
            }
            Err(error) => return Err(error).context("read local actor registration"),
        }
    }
    anyhow::bail!("local actor registration slots exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_uuid_conformance_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/actor-origin-vectors.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            let instance = Uuid::parse_str(vector["instance_id"].as_str().unwrap()).unwrap();
            let base = base_id(
                instance,
                vector["os"].as_str().unwrap(),
                vector["provider"].as_str().unwrap(),
                vector["project_path"].as_str().unwrap(),
                vector["git_identity"].as_str().unwrap(),
            )
            .unwrap();
            assert_eq!(base.to_string(), vector["base_id"]);
            let actor = derive(
                instance,
                vector["os"].as_str().unwrap(),
                vector["provider"].as_str().unwrap(),
                vector["provider_session_id"].as_str().unwrap(),
                base,
                vector["session_slot"].as_u64().unwrap() as u32,
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
    fn native_ids_and_slots_are_not_silently_repaired() {
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
            Uuid::new_v5(&id, b"base"),
            0,
        )
        .is_err());
    }

    #[test]
    fn concurrent_registration_reuses_conversations_without_slot_aliasing() {
        let directory = tempfile::tempdir().unwrap();
        let instance = Uuid::new_v4();
        let base = base_id(
            instance,
            "windows",
            "codex",
            "C:/repo",
            "github.com/user/repo",
        )
        .unwrap();
        let actor = derive(instance, "windows", "codex", "thread-a", base, 1).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let directory = directory.path().to_owned();
                let session = actor.origin.session_id.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    register_slot(&directory, &session).unwrap()
                })
            })
            .collect();
        assert!(handles.into_iter().all(|h| h.join().unwrap() == 1));
        let peer = derive(instance, "windows", "codex", "thread-b", base, 1).unwrap();
        assert_eq!(
            register_slot(directory.path(), &peer.origin.session_id).unwrap(),
            2
        );
        assert_eq!(
            register_slot(directory.path(), &actor.origin.session_id).unwrap(),
            1
        );
        assert_ne!(actor.origin.session_id, peer.origin.session_id);
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let directory = directory.path().to_owned();
                let session = derive(
                    instance,
                    "windows",
                    "codex",
                    &format!("peer-{index}"),
                    base,
                    1,
                )
                .unwrap()
                .origin
                .session_id;
                std::thread::spawn(move || {
                    let slot = register_slot(&directory, &session).unwrap();
                    assert_eq!(register_slot(&directory, &session).unwrap(), slot);
                    slot
                })
            })
            .collect();
        let slots: std::collections::BTreeSet<_> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(slots.len(), 8);
        assert!(slots.iter().all(|slot| *slot > 2));
        // A temp registry reset cannot make a peer reuse this conversation UUID.
        let reset = tempfile::tempdir().unwrap();
        assert_eq!(
            register_slot(reset.path(), &peer.origin.session_id).unwrap(),
            1
        );
        assert_ne!(actor.origin.session_id, peer.origin.session_id);
    }
}
