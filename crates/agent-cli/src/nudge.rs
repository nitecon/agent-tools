//! Best-effort nudge layer.
//!
//! After most successful CLI invocations, emit a one-line stderr hint that
//! reminds the calling agent to keep work tracked in `agent-tools tasks`.
//! Throttled by a small persistent counter so the hint surfaces about every
//! Nth call rather than dominating output.
//!
//! Quiet by default when:
//! - the gateway isn't configured (no point pointing at an unreachable system);
//! - the invoking command is itself a `tasks ...` call or an admin command;
//! - the user has set `AGENT_TOOLS_NUDGE=off`.
//!
//! Rationale: avoids destructive scan/import of the user's local task files,
//! while still giving forgetful agents a periodic, in-band reminder to use the
//! gateway-backed task board.

use crate::Commands;
use agent_comms::config::{home_dir, load_config};
use std::path::PathBuf;

const HINT: &str =
    "[agent-tools] Tracking work? Make sure it has a task: `agent-tools tasks list`.";
const DEFAULT_INTERVAL: u64 = 5;

/// Decide whether a given command is eligible for nudging at all. Borrows
/// `cmd` so callers can still consume the value in a subsequent `match`.
pub fn should_nudge(cmd: &Commands) -> bool {
    !matches!(
        cmd,
        Commands::Tasks { .. }
            | Commands::Setup { .. }
            | Commands::Init
            | Commands::Version
            | Commands::Update
            | Commands::Serve
    )
}

/// Bump the counter and emit the hint when the throttle gate opens.
///
/// Silently no-ops when:
/// - `AGENT_TOOLS_NUDGE=off` is set;
/// - the gateway isn't configured;
/// - the modulo-N gate hasn't fired this call.
///
/// All errors are swallowed: a flaky counter file should never break the
/// command the user actually ran.
pub fn emit_if_due() {
    if std::env::var("AGENT_TOOLS_NUDGE").as_deref() == Ok("off") {
        return;
    }
    if !gateway_configured() {
        return;
    }
    let interval = read_interval();
    let count = bump_counter();
    if should_emit_at(count, interval) {
        eprintln!("{HINT}");
    }
}

// -- internals ---------------------------------------------------------------

fn gateway_configured() -> bool {
    let cfg = load_config();
    cfg.gateway.url.is_some() && cfg.gateway.api_key.is_some()
}

fn read_interval() -> u64 {
    std::env::var("AGENT_TOOLS_NUDGE_INTERVAL")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INTERVAL)
}

fn counter_path() -> PathBuf {
    home_dir()
        .join(".agentic")
        .join("agent-tools")
        .join("nudge-counter")
}

/// Persist the counter increment. Returns the new value (defaults to 1 when
/// the existing file is missing or unreadable). Write errors are swallowed.
fn bump_counter() -> u64 {
    let path = counter_path();
    let current = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let next = current.wrapping_add(1);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, next.to_string());
    next
}

/// Pure modulo gate, factored out so the throttle can be tested without
/// touching the filesystem or env.
fn should_emit_at(count: u64, interval: u64) -> bool {
    interval > 0 && count.is_multiple_of(interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_tasks::TasksCommands;
    use std::path::PathBuf;

    #[test]
    fn should_emit_at_respects_interval() {
        assert!(!should_emit_at(1, 5));
        assert!(!should_emit_at(4, 5));
        assert!(should_emit_at(5, 5));
        assert!(should_emit_at(10, 5));
        assert!(should_emit_at(0, 5)); // 0 % 5 == 0 — first-run case
    }

    #[test]
    fn should_emit_at_rejects_zero_interval() {
        // Zero interval should never fire; otherwise we'd divide by zero or
        // emit on every call once the env var is misconfigured.
        assert!(!should_emit_at(0, 0));
        assert!(!should_emit_at(5, 0));
    }

    #[test]
    fn should_nudge_skips_admin_and_tasks_family() {
        assert!(!should_nudge(&Commands::Version));
        assert!(!should_nudge(&Commands::Init));
        assert!(!should_nudge(&Commands::Update));
        assert!(!should_nudge(&Commands::Serve));
        assert!(!should_nudge(&Commands::Tasks {
            command: TasksCommands::List {
                status: "todo".into(),
                include_stale: false,
                agent_id: None,
                json: false,
            },
        }));
    }

    #[test]
    fn should_nudge_fires_for_general_commands() {
        assert!(should_nudge(&Commands::Tree {
            path: None,
            depth: 3,
            max_files: 20,
        }));
        assert!(should_nudge(&Commands::Mkdir {
            path: PathBuf::from("foo"),
        }));
    }

    // `read_interval` is exercised in a single test (rather than three) to
    // avoid env-var races between parallel test runs in the same binary.
    #[test]
    fn read_interval_handles_default_valid_and_garbage() {
        // Snapshot any pre-existing value so we don't perturb other tests.
        let prev = std::env::var("AGENT_TOOLS_NUDGE_INTERVAL").ok();

        std::env::remove_var("AGENT_TOOLS_NUDGE_INTERVAL");
        assert_eq!(read_interval(), DEFAULT_INTERVAL);

        std::env::set_var("AGENT_TOOLS_NUDGE_INTERVAL", "3");
        assert_eq!(read_interval(), 3);

        std::env::set_var("AGENT_TOOLS_NUDGE_INTERVAL", "0");
        assert_eq!(read_interval(), DEFAULT_INTERVAL);

        std::env::set_var("AGENT_TOOLS_NUDGE_INTERVAL", "abc");
        assert_eq!(read_interval(), DEFAULT_INTERVAL);

        match prev {
            Some(v) => std::env::set_var("AGENT_TOOLS_NUDGE_INTERVAL", v),
            None => std::env::remove_var("AGENT_TOOLS_NUDGE_INTERVAL"),
        }
    }
}
