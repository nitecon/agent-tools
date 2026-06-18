//! `agent-tools hook` — runtime hooks called by agent CLIs.
//!
//! Invoked by the hook entries installed via `agent-tools setup hooks`. Reads
//! context from the gateway and emits a `hookSpecificOutput` envelope on stdout
//! so the calling agent CLI injects it as `additionalContext`.
//!
//! MASTER RULE — fail-soft: this command MUST always exit 0 and never panic.
//! A non-zero exit on UserPromptSubmit blocks the user's prompt in Claude.
//! Every Err path silently returns Ok(()). Unconfigured gateway => silent.

use crate::cmd_gateway_context::resolve_context;
use agent_comms::patterns::PatternFilters;
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;
use std::io::Read;

#[derive(Subcommand)]
pub enum HookCommands {
    /// Hook for agent session start — injects open tasks as context.
    SessionStart {
        /// Agent name (claude, codex, gemini). Defaults to claude.
        #[arg(long, default_value = "claude")]
        agent: Option<String>,
    },
    /// Hook for user prompt submit — injects relevant patterns and tasks.
    UserPromptSubmit {
        /// Agent name (claude, codex, gemini). Defaults to claude.
        #[arg(long, default_value = "claude")]
        agent: Option<String>,
    },
}

/// Dispatch hook subcommands. Always returns Ok(()) — fail-soft.
pub fn dispatch(cmd: HookCommands) -> Result<()> {
    // Top-level env toggle: AGENT_TOOLS_HOOK=off => silent noop.
    if is_hook_disabled() {
        return Ok(());
    }

    match cmd {
        HookCommands::SessionStart { agent } => {
            let agent_str = agent.as_deref().unwrap_or("claude");
            if !is_known_agent(agent_str) {
                return Ok(());
            }
            // Fail-soft: any error => silent.
            let _ = run_session_start(agent_str);
            Ok(())
        }
        HookCommands::UserPromptSubmit { agent } => {
            let agent_str = agent.as_deref().unwrap_or("claude");
            if !is_known_agent(agent_str) {
                return Ok(());
            }
            // Fail-soft: any error => silent.
            let _ = run_user_prompt_submit(agent_str);
            Ok(())
        }
    }
}

// -- env helpers (pure, testable) --------------------------------------------

/// True when `AGENT_TOOLS_HOOK=off`.
fn is_hook_disabled() -> bool {
    std::env::var("AGENT_TOOLS_HOOK").as_deref() == Ok("off")
}

/// Parse `AGENT_TOOLS_HOOK_LIMIT` (default 5, invalid => 5).
fn hook_limit() -> usize {
    std::env::var("AGENT_TOOLS_HOOK_LIMIT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(5)
}

fn is_known_agent(agent: &str) -> bool {
    matches!(agent, "claude" | "codex" | "gemini")
}

// -- pure mapping helpers ----------------------------------------------------

/// Map (command kind, agent) to the event name for the envelope.
///
/// session-start => "SessionStart" always.
/// user-prompt-submit => "UserPromptSubmit" for claude/codex, "BeforeAgent" for gemini.
pub(crate) fn event_name(is_session_start: bool, agent: &str) -> &'static str {
    if is_session_start {
        "SessionStart"
    } else if agent == "gemini" {
        "BeforeAgent"
    } else {
        "UserPromptSubmit"
    }
}

/// Extract prompt from a JSON payload trying multiple keys in order.
/// Returns None if all keys are missing, non-string, or whitespace-only.
pub(crate) fn extract_prompt(payload: &Value) -> Option<String> {
    for key in &["prompt", "user_prompt", "userPrompt", "message", "input", "text"] {
        if let Some(Value::String(s)) = payload.get(key) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Build the hookSpecificOutput envelope JSON.
pub(crate) fn render_envelope(event: &str, additional_context: &str) -> String {
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": additional_context
        }
    });
    envelope.to_string()
}

/// Extract prompt tokens for task ranking: split on non-alphanumeric,
/// lowercase, drop tokens shorter than 2 chars.
pub(crate) fn prompt_tokens(prompt: &str) -> Vec<String> {
    prompt
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Score a task by counting how many prompt tokens appear in its searchable text.
pub(crate) fn score_task(tokens: &[String], title: &str, labels: &[String]) -> usize {
    let haystack = format!(
        "{} {}",
        title.to_ascii_lowercase(),
        labels.join(" ").to_ascii_lowercase()
    );
    tokens
        .iter()
        .filter(|t| haystack.contains(t.as_str()))
        .count()
}

/// First 8 chars of an id (or the whole id when shorter) for compact display.
fn short_id(id: &str) -> &str {
    &id[..8.min(id.len())]
}

// -- session-start logic -----------------------------------------------------

fn run_session_start(agent: &str) -> Result<()> {
    let ctx = resolve_context(None)?;
    let k = hook_limit();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let tasks = rt.block_on(async {
        ctx.gateway
            .list_tasks(
                &ctx.ident,
                Some(&["todo", "in_progress"]),
                false,
                Some(&ctx.agent_id),
            )
            .await
    })?;

    if tasks.is_empty() {
        return Ok(());
    }

    let displayed: Vec<_> = tasks.iter().take(k).collect();

    let mut lines = vec!["Open tasks for this session:".to_string()];
    for t in &displayed {
        let owner = t.owner_agent_id.as_deref().unwrap_or("—");
        lines.push(format!(
            "[{}] {} ({}, owner={owner})",
            short_id(&t.id),
            t.title,
            t.status
        ));
    }
    lines.push("Pull full detail + spec before starting: agent-tools tasks get <id>".to_string());

    let additional_context = lines.join("\n");
    let event = event_name(true, agent);
    let envelope = render_envelope(event, &additional_context);
    println!("{envelope}");
    Ok(())
}

// -- user-prompt-submit logic ------------------------------------------------

fn run_user_prompt_submit(agent: &str) -> Result<()> {
    // Read all of stdin.
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;

    // Parse JSON; parse fail => silent.
    let payload: Value = serde_json::from_str(&raw)?;

    // Extract prompt; None => silent.
    let prompt = extract_prompt(&payload).ok_or_else(|| anyhow::anyhow!("no prompt"))?;

    let ctx = resolve_context(None)?;
    let k = hook_limit();
    let tokens = prompt_tokens(&prompt);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let (patterns, tasks) = rt.block_on(async {
        let filters = PatternFilters {
            query: Some(prompt.as_str()),
            state: Some("active"),
            version: Some("latest"),
            ..Default::default()
        };
        let p = ctx
            .gateway
            .list_patterns(&filters, Some(&ctx.agent_id))
            .await
            .unwrap_or_default();
        let t = ctx
            .gateway
            .list_tasks(
                &ctx.ident,
                Some(&["todo", "in_progress"]),
                false,
                Some(&ctx.agent_id),
            )
            .await
            .unwrap_or_default();
        (p, t)
    });

    let patterns: Vec<_> = patterns.into_iter().take(k).collect();

    // Rank tasks by prompt token overlap.
    let mut scored_tasks: Vec<_> = tasks
        .into_iter()
        .filter_map(|t| {
            let s = score_task(&tokens, &t.title, &t.labels);
            if s > 0 {
                Some((s, t))
            } else {
                None
            }
        })
        .collect();
    scored_tasks.sort_by(|a, b| b.0.cmp(&a.0));
    let top_tasks: Vec<_> = scored_tasks.into_iter().take(3).map(|(_, t)| t).collect();

    if patterns.is_empty() && top_tasks.is_empty() {
        return Ok(());
    }

    let mut sections = Vec::new();

    if !patterns.is_empty() {
        let mut lines = vec!["Relevant patterns:".to_string()];
        for p in &patterns {
            lines.push(format!("  {} [{}/{}] — {}", p.title, p.slug, p.id, p.summary));
            // `patterns get` accepts the slug or the id; the slug is the
            // stabler, human-readable handle so we surface it first.
            lines.push(format!("  fetch: agent-tools patterns get {}", p.slug));
        }
        sections.push(lines.join("\n"));
    }

    if !top_tasks.is_empty() {
        let mut lines = vec!["Possibly-relevant open tasks:".to_string()];
        for t in &top_tasks {
            lines.push(format!("  [{}] {}", short_id(&t.id), t.title));
            lines.push(format!("  agent-tools tasks get {}", t.id));
        }
        sections.push(lines.join("\n"));
    }

    let additional_context = sections.join("\n\n");
    let event = event_name(false, agent);
    let envelope = render_envelope(event, &additional_context);
    println!("{envelope}");
    Ok(())
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- env toggles ---------------------------------------------------------

    #[test]
    fn is_hook_disabled_only_on_off() {
        let prev = std::env::var("AGENT_TOOLS_HOOK").ok();
        std::env::remove_var("AGENT_TOOLS_HOOK");
        assert!(!is_hook_disabled());
        std::env::set_var("AGENT_TOOLS_HOOK", "off");
        assert!(is_hook_disabled());
        std::env::set_var("AGENT_TOOLS_HOOK", "1");
        assert!(!is_hook_disabled());
        match prev {
            Some(v) => std::env::set_var("AGENT_TOOLS_HOOK", v),
            None => std::env::remove_var("AGENT_TOOLS_HOOK"),
        }
    }

    #[test]
    fn hook_limit_default_and_parse() {
        let prev = std::env::var("AGENT_TOOLS_HOOK_LIMIT").ok();
        std::env::remove_var("AGENT_TOOLS_HOOK_LIMIT");
        assert_eq!(hook_limit(), 5);
        std::env::set_var("AGENT_TOOLS_HOOK_LIMIT", "3");
        assert_eq!(hook_limit(), 3);
        std::env::set_var("AGENT_TOOLS_HOOK_LIMIT", "0");
        assert_eq!(hook_limit(), 5); // invalid (zero) => default
        std::env::set_var("AGENT_TOOLS_HOOK_LIMIT", "abc");
        assert_eq!(hook_limit(), 5); // garbage => default
        match prev {
            Some(v) => std::env::set_var("AGENT_TOOLS_HOOK_LIMIT", v),
            None => std::env::remove_var("AGENT_TOOLS_HOOK_LIMIT"),
        }
    }

    // -- event mapping -------------------------------------------------------

    #[test]
    fn event_name_session_start_always_sessionstart() {
        assert_eq!(event_name(true, "claude"), "SessionStart");
        assert_eq!(event_name(true, "codex"), "SessionStart");
        assert_eq!(event_name(true, "gemini"), "SessionStart");
    }

    #[test]
    fn event_name_user_prompt_submit_by_agent() {
        assert_eq!(event_name(false, "claude"), "UserPromptSubmit");
        assert_eq!(event_name(false, "codex"), "UserPromptSubmit");
        assert_eq!(event_name(false, "gemini"), "BeforeAgent");
    }

    // -- prompt extraction ---------------------------------------------------

    #[test]
    fn extract_prompt_tries_all_keys_in_order() {
        let p = |k: &str, v: &str| extract_prompt(&json!({ k: v }));
        assert_eq!(p("prompt", "hello"), Some("hello".to_string()));
        assert_eq!(p("user_prompt", "hello"), Some("hello".to_string()));
        assert_eq!(p("userPrompt", "hello"), Some("hello".to_string()));
        assert_eq!(p("message", "hello"), Some("hello".to_string()));
        assert_eq!(p("input", "hello"), Some("hello".to_string()));
        assert_eq!(p("text", "hello"), Some("hello".to_string()));
    }

    #[test]
    fn extract_prompt_ignores_non_string_values() {
        let payload = json!({ "prompt": 42 });
        assert_eq!(extract_prompt(&payload), None);
    }

    #[test]
    fn extract_prompt_ignores_whitespace_only() {
        let payload = json!({ "prompt": "   " });
        assert_eq!(extract_prompt(&payload), None);
    }

    #[test]
    fn extract_prompt_trims_surrounding_whitespace() {
        let payload = json!({ "prompt": "  hello world  " });
        assert_eq!(extract_prompt(&payload), Some("hello world".to_string()));
    }

    #[test]
    fn extract_prompt_returns_none_when_no_key() {
        let payload = json!({ "other": "hello" });
        assert_eq!(extract_prompt(&payload), None);
    }

    // -- envelope rendering --------------------------------------------------

    #[test]
    fn render_envelope_is_valid_json_with_correct_keys() {
        let out = render_envelope("UserPromptSubmit", "some context");
        let parsed: Value = serde_json::from_str(&out).expect("should be valid JSON");
        let inner = &parsed["hookSpecificOutput"];
        assert_eq!(inner["hookEventName"], json!("UserPromptSubmit"));
        assert_eq!(inner["additionalContext"], json!("some context"));
    }

    #[test]
    fn render_envelope_escapes_special_chars() {
        let ctx = "line1\nline2\t\"quoted\"";
        let out = render_envelope("SessionStart", ctx);
        let parsed: Value = serde_json::from_str(&out).expect("should be valid JSON");
        assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], json!(ctx));
    }

    // -- task ranking --------------------------------------------------------

    #[test]
    fn prompt_tokens_splits_and_filters_short() {
        let tokens = prompt_tokens("Fix the auth bug");
        assert!(tokens.contains(&"fix".to_string()));
        assert!(tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"auth".to_string()));
        assert!(tokens.contains(&"bug".to_string()));
        // single-char tokens dropped
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn score_task_counts_matching_tokens() {
        let tokens: Vec<String> = vec!["auth".to_string(), "login".to_string()];
        let score = score_task(&tokens, "Fix auth login flow", &[]);
        assert_eq!(score, 2);
    }

    #[test]
    fn score_task_zero_for_no_overlap() {
        let tokens: Vec<String> = vec!["payment".to_string()];
        let score = score_task(&tokens, "Fix auth login flow", &[]);
        assert_eq!(score, 0);
    }

    #[test]
    fn score_task_includes_labels() {
        let tokens: Vec<String> = vec!["backend".to_string()];
        let score = score_task(&tokens, "Fix something", &["backend".to_string()]);
        assert_eq!(score, 1);
    }

    #[test]
    fn short_id_truncates_and_handles_short() {
        assert_eq!(short_id("019dbaf9-2527-7782"), "019dbaf9");
        assert_eq!(short_id("abc"), "abc");
    }

    // -- fail-soft -----------------------------------------------------------

    #[test]
    fn dispatch_returns_ok_when_hook_disabled() {
        let prev = std::env::var("AGENT_TOOLS_HOOK").ok();
        std::env::set_var("AGENT_TOOLS_HOOK", "off");
        let result = dispatch(HookCommands::SessionStart {
            agent: Some("claude".to_string()),
        });
        assert!(result.is_ok());
        match prev {
            Some(v) => std::env::set_var("AGENT_TOOLS_HOOK", v),
            None => std::env::remove_var("AGENT_TOOLS_HOOK"),
        }
    }

    #[test]
    fn dispatch_returns_ok_for_unknown_agent() {
        let result = dispatch(HookCommands::SessionStart {
            agent: Some("unknown-agent".to_string()),
        });
        assert!(result.is_ok());
    }
}
