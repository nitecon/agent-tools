//! Memory-save reminder for task completion.
//!
//! Completing a gateway task via `agent-tools tasks done <id>` is the natural
//! "save" moment for agent-memory: it mirrors the native Claude Code
//! `TaskCompleted` hook, which never fires when the native task tools are
//! disabled in favour of this gateway board. So on a *successful* `done`
//! transition we emit a short, deterministically-placed directive reminding
//! the agent to (1) store durable, non-obvious learnings and (2) save or clear
//! its WorkingContext via the `memory` CLI.
//!
//! Advisory only — the act of saving is still the agent's. Emitted to stderr so
//! the command's stdout stays the clean transition result. Suppressed when the
//! user sets `AGENT_TOOLS_MEMORY_REMINDER=off` (for setups not running
//! agent-memory). Fires only on `done`, never on `claim`/`release`/other verbs.

/// Reminder body, aligned with agent-memory's Rule B (scope classification) and
/// quality gate (reusable how/why only; no git/CI-derivable state).
const REMINDER: &str = "\
[memory] Task done — save durable learnings now (skip if nothing non-obvious):
  • `memory store \"<lesson>\" -m <user|feedback|project|reference> -t \"tags\"` — reusable how/why only; no git/CI-derivable state. Use --scope global for universal preferences; project scope is the default.
  • Handoff: `memory working set` if pausing active work, or `memory working clear` if this project thread is complete.";

/// Emit the completion reminder unless suppressed via env. Called only after a
/// successful `done` transition.
pub fn emit_done_reminder() {
    if is_suppressed() {
        return;
    }
    eprintln!("{REMINDER}");
}

/// Pure suppression gate, factored out so it can be tested without touching
/// stderr. The reminder is silenced when `AGENT_TOOLS_MEMORY_REMINDER=off`.
fn is_suppressed() -> bool {
    std::env::var("AGENT_TOOLS_MEMORY_REMINDER").as_deref() == Ok("off")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppressed_only_when_off() {
        let prev = std::env::var("AGENT_TOOLS_MEMORY_REMINDER").ok();

        std::env::remove_var("AGENT_TOOLS_MEMORY_REMINDER");
        assert!(!is_suppressed());

        std::env::set_var("AGENT_TOOLS_MEMORY_REMINDER", "off");
        assert!(is_suppressed());

        // Any value other than the exact "off" sentinel keeps the reminder on.
        std::env::set_var("AGENT_TOOLS_MEMORY_REMINDER", "on");
        assert!(!is_suppressed());
        std::env::set_var("AGENT_TOOLS_MEMORY_REMINDER", "1");
        assert!(!is_suppressed());

        match prev {
            Some(v) => std::env::set_var("AGENT_TOOLS_MEMORY_REMINDER", v),
            None => std::env::remove_var("AGENT_TOOLS_MEMORY_REMINDER"),
        }
    }

    #[test]
    fn reminder_mentions_both_save_paths() {
        // Guard the acceptance criteria: durable store + working context, via
        // the `memory` CLI, with scope guidance.
        assert!(REMINDER.contains("memory store"));
        assert!(REMINDER.contains("memory working set"));
        assert!(REMINDER.contains("memory working clear"));
        assert!(REMINDER.contains("--scope global"));
    }
}
