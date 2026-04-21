//! Machine-stable agent identity.
//!
//! Agents need a stable per-instance id for gateway routing (per-agent unread
//! queues). The id is persisted at `~/.agentic/agent-tools/agent-id` on first
//! use and reused thereafter, so every agent process on the same machine shares
//! the same identity unless overridden with `--agent-id`.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::home_dir;

/// Path to the persistent machine agent-id file.
pub fn agent_id_path() -> PathBuf {
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("agent-id")
}

/// Load the machine agent-id, generating it on first call.
///
/// The id is written to `~/.agentic/agent-tools/agent-id` and shared by every
/// agent-tools invocation on the same machine.
///
/// # Errors
/// Returns an error if the file cannot be read or written.
pub fn load_or_generate_agent_id() -> Result<String> {
    let path = agent_id_path();

    if let Ok(content) = std::fs::read_to_string(&path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let id = generate_agent_id();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    std::fs::write(&path, &id).with_context(|| format!("write agent-id to {}", path.display()))?;

    Ok(id)
}

/// Build a fresh agent id: `<sanitized-hostname>-<8 hex chars>`.
fn generate_agent_id() -> String {
    let host = sanitize_hostname(&read_hostname());
    let suffix = random_suffix();
    if host.is_empty() {
        format!("m-{suffix}")
    } else {
        format!("{host}-{suffix}")
    }
}

/// Resolve the machine hostname via env vars, falling back to the `hostname`
/// command and finally the literal string `"machine"`.
fn read_hostname() -> String {
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    if let Ok(h) = std::env::var("COMPUTERNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "machine".to_string()
}

/// Sanitize a hostname: lowercase, drop domain suffix, keep only
/// `[a-z0-9-]`, collapse runs of hyphens, strip trailing hyphens.
fn sanitize_hostname(input: &str) -> String {
    let short = input
        .split('.')
        .next()
        .unwrap_or(input)
        .trim()
        .to_lowercase();
    let mut out = String::with_capacity(short.len());
    let mut last_was_hyphen = false;
    for c in short.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_hyphen = false;
        } else if !last_was_hyphen && !out.is_empty() {
            out.push('-');
            last_was_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 8-char hex suffix from SystemTime-nanos + pid. Not cryptographic; just
/// needs to be collision-resistant across one user's machines.
fn random_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update(pid.to_le_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_hostname_strips_domain_and_non_alnum() {
        assert_eq!(sanitize_hostname("Whattinghs-MBP.local"), "whattinghs-mbp");
        assert_eq!(
            sanitize_hostname("host_with_underscores"),
            "host-with-underscores"
        );
        assert_eq!(sanitize_hostname("trailing-dashes---"), "trailing-dashes");
        assert_eq!(sanitize_hostname(""), "");
        // Leading dots get dropped with the empty first segment (edge case).
        assert_eq!(sanitize_hostname("...leading-dots"), "");
    }

    #[test]
    fn generate_agent_id_matches_expected_shape() {
        let id = generate_agent_id();
        // Either "m-xxxxxxxx" or "<host>-xxxxxxxx"
        let (prefix, suffix) = id.rsplit_once('-').expect("has dash");
        assert!(!prefix.is_empty());
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn random_suffix_is_8_hex_chars() {
        let s = random_suffix();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn agent_id_path_ends_correctly() {
        let p = agent_id_path();
        assert!(p.ends_with(".agentic/agent-tools/agent-id"));
    }
}
