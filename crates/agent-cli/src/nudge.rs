//! Retired task nudge compatibility entry points. No output or disk writes.
use crate::Commands;

pub fn should_nudge(_cmd: &Commands) -> bool {
    false
}

pub fn emit_if_due() {}
