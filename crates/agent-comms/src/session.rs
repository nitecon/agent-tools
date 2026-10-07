//! Exact originating agent identity, independent of the legacy machine agent ID.

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionOrigin {
    pub session_id: String,
    pub instance_id: String,
    pub provider: String,
    pub os: String,
}

impl SessionOrigin {
    /// Validate originating actor metadata before using it as HTTP provenance.
    pub fn validate(&self) -> Result<()> {
        ensure!(is_uuid(&self.session_id), "invalid actor session UUID");
        ensure!(is_uuid(&self.instance_id), "invalid actor instance UUID");
        ensure!(
            matches!(self.provider.as_str(), "codex" | "claude"),
            "unsupported actor provider"
        );
        ensure!(
            matches!(self.os.as_str(), "linux" | "windows" | "macos"),
            "unsupported actor OS"
        );
        Ok(())
    }
}

pub fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
