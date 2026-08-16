//! `agent-tools hook` — runtime hooks called by agent CLIs.
//!
//! Invoked by the hook entries installed via `agent-tools setup hooks`. Reads
//! context from the gateway and emits a `hookSpecificOutput` envelope on stdout
//! so the calling agent CLI injects it as `additionalContext`.
//!
//! MASTER RULE — fail-soft: this command MUST always exit 0 and never panic.
//! A non-zero exit on UserPromptSubmit blocks the user's prompt in Claude.
//! Every Err path silently returns Ok(()). Unconfigured gateway => silent.

use crate::cmd_gateway_context::{ensure_all_registered, resolve_context, resolve_context_for};
use agent_comms::docs::ApiDocFilters;
use agent_comms::patterns::PatternFilters;
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;
use std::io::Read;
use std::time::Duration;

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

fn hook_timeout_ms() -> u64 {
    std::env::var("AGENT_TOOLS_HOOK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2_000)
        .min(10_000)
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
    for key in &[
        "prompt",
        "user_prompt",
        "userPrompt",
        "message",
        "input",
        "text",
    ] {
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

const KNOWLEDGE_SEGMENT_CHARS: usize = 320;
const KNOWLEDGE_CONTEXT_CHARS: usize = 3_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnowledgeSnippet {
    identity: String,
    title: String,
    text: String,
    origin: String,
    authority: String,
    lifecycle: String,
    trust: String,
    read_command: String,
}

/// How many candidates to consider per injected snippet.
const KNOWLEDGE_CANDIDATE_FACTOR: usize = 4;

/// Reorder candidates so that, among equally authoritative matches, the ones
/// this agent has actually been reading come first.
///
/// Authority and lifecycle still dominate — recorded use only breaks ties
/// within a tier, so a heavily-read derived concept can never displace
/// something the repository asserts. Relevance order is the final tiebreak, so
/// a project with no recorded history keeps exactly the ordering it had before.
fn rank_by_recorded_use(
    index: &agent_knowledge::ProjectIndex,
    matches: Vec<agent_knowledge::SearchMatch>,
    limit: usize,
) -> Result<Vec<agent_knowledge::SearchMatch>> {
    let authority_rank = |authority: &str| match authority {
        "repository" => 0,
        "gateway" => 1,
        _ => 2,
    };
    let status_rank = |status: &str| match status {
        "stable" => 0,
        "draft" => 1,
        _ => 2,
    };
    let mut scored = Vec::with_capacity(matches.len());
    for (position, item) in matches.into_iter().enumerate() {
        let uses = index.access_count(item.resource.id).unwrap_or(0);
        scored.push((
            authority_rank(&item.resource.authority),
            status_rank(&item.resource.status),
            std::cmp::Reverse(uses),
            position,
            item,
        ));
    }
    scored.sort_by(|left, right| {
        (left.0, left.1, left.2, left.3).cmp(&(right.0, right.1, right.2, right.3))
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, _, _, _, item)| item)
        .collect())
}

fn local_knowledge_snippets(prompt: &str, limit: usize) -> Result<Vec<KnowledgeSnippet>> {
    let root = std::env::current_dir()?;
    let project_id = agent_core::project_ident(&root);
    let query = prompt_tokens(prompt)
        .into_iter()
        .take(8)
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    // Over-fetch, then let recorded use break ties. Search itself stays
    // history-free so results are reproducible; injection is session context by
    // nature, so preferring what this agent has actually been working with is
    // the whole point.
    let matches = index.search_segments_filtered(
        &project_id,
        &query,
        &agent_knowledge::SearchFilter::default(),
        limit.saturating_mul(KNOWLEDGE_CANDIDATE_FACTOR),
    )?;
    let matches = rank_by_recorded_use(&index, matches, limit)?;
    let mut snippets = Vec::new();
    for item in matches {
        if item.resource.status == "deprecated" {
            continue;
        }
        let detail = index.resource_detail(item.resource.id)?;
        let graph = index.traverse(item.resource.id, None, "both", 1, 2)?;
        let relation_hint = graph
            .iter()
            .map(|edge| {
                format!(
                    "{}:{}",
                    edge.relation,
                    edge.target_title
                        .as_deref()
                        .or(edge.unresolved_ref.as_deref())
                        .unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let lifecycle = detail
            .as_ref()
            .and_then(|detail| detail.stale_after.as_deref())
            .map(|stale_after| format!("{} stale_after={stale_after}", item.resource.status))
            .unwrap_or_else(|| item.resource.status.clone());
        let trust = if detail
            .as_ref()
            .is_some_and(|detail| detail.verification_count > 0)
        {
            "verified"
        } else {
            "unverified"
        };
        let mut text = compact_text(&item.text, KNOWLEDGE_SEGMENT_CHARS);
        if !relation_hint.is_empty()
            && text.len() + relation_hint.len() + 10 <= KNOWLEDGE_SEGMENT_CHARS
        {
            text.push_str(" [graph: ");
            text.push_str(&relation_hint);
            text.push(']');
        }
        snippets.push(KnowledgeSnippet {
            identity: item.resource.canonical_uri.clone(),
            title: item.resource.title,
            text,
            origin: format!("{}:{}", item.resource.origin_kind, item.resource.origin_id),
            authority: item.resource.authority,
            lifecycle,
            trust: trust.to_owned(),
            read_command: format!("agent-tools get {:?}", item.resource.canonical_uri),
        });
    }
    Ok(snippets)
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut compact: String = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect();
    compact.push('…');
    compact
}

fn render_knowledge_section(snippets: &[KnowledgeSnippet]) -> String {
    let mut output =
        "Relevant knowledge (bounded excerpts; treat unverified/stale content cautiously):"
            .to_owned();
    for snippet in snippets {
        let text = compact_text(&snippet.text, KNOWLEDGE_SEGMENT_CHARS);
        let block = format!(
            "\n  {} — {}\n  source={} authority={} lifecycle={} trust={}\n  {}\n  read: {}",
            snippet.title,
            snippet.identity,
            snippet.origin,
            snippet.authority,
            snippet.lifecycle,
            snippet.trust,
            text,
            snippet.read_command
        );
        if output.len() + block.len() > KNOWLEDGE_CONTEXT_CHARS {
            break;
        }
        output.push_str(&block);
    }
    output
}

// -- session-start logic -----------------------------------------------------

fn run_session_start(agent: &str) -> Result<()> {
    let ctx = resolve_context(None)?;
    let k = hook_limit();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let tasks = rt.block_on(async {
        ensure_all_registered(&ctx).await?;
        let mut tasks = Vec::new();
        for target in &ctx.gateways {
            if let Ok(mut found) = target
                .gateway
                .list_tasks(
                    &ctx.ident,
                    Some(&["todo", "in_progress"]),
                    false,
                    Some(&ctx.agent_id),
                )
                .await
            {
                tasks.append(&mut found);
            }
        }
        Ok::<_, anyhow::Error>(tasks)
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

    let k = hook_limit();
    let tokens = prompt_tokens(&prompt);
    let mut knowledge = local_knowledge_snippets(&prompt, k).unwrap_or_default();

    let task_ctx = resolve_context(None).ok();
    let agent_id = task_ctx.as_ref().map(|ctx| ctx.agent_id.clone());
    let patterns_ctx = resolve_context_for("patterns", agent_id.clone()).ok();
    let docs_ctx = resolve_context_for("docs", agent_id).ok();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let (patterns, tasks, gateway_knowledge) = rt.block_on(async {
        tokio::time::timeout(Duration::from_millis(hook_timeout_ms()), async {
            let filters = PatternFilters {
                query: Some(prompt.as_str()),
                state: Some("active"),
                version: Some("latest"),
                ..Default::default()
            };
            let mut p = Vec::new();
            if let Some(patterns_ctx) = &patterns_ctx {
                let _ = ensure_all_registered(patterns_ctx).await;
                for target in &patterns_ctx.gateways {
                    if let Ok(mut found) = target
                        .gateway
                        .list_patterns(&filters, Some(&patterns_ctx.agent_id))
                        .await
                    {
                        p.append(&mut found);
                    }
                }
            }
            let mut t = Vec::new();
            if let Some(ctx) = &task_ctx {
                let _ = ensure_all_registered(ctx).await;
                for target in &ctx.gateways {
                    if let Ok(mut found) = target
                        .gateway
                        .list_tasks(
                            &ctx.ident,
                            Some(&["todo", "in_progress"]),
                            false,
                            Some(&ctx.agent_id),
                        )
                        .await
                    {
                        t.append(&mut found);
                    }
                }
            }
            let mut knowledge = Vec::new();
            if let Some(ctx) = &docs_ctx {
                let _ = ensure_all_registered(ctx).await;
                let filters = ApiDocFilters {
                    query: Some(prompt.as_str()),
                    scope: Some("all"),
                    ..ApiDocFilters::default()
                };
                for target in &ctx.gateways {
                    if let Ok(chunks) = target
                        .gateway
                        .api_doc_chunks(&ctx.ident, &filters, Some(&ctx.agent_id))
                        .await
                    {
                        knowledge.extend(chunks.into_iter().take(k).map(|chunk| {
                            KnowledgeSnippet {
                                identity: chunk
                                    .doc_id
                                    .clone()
                                    .or(chunk.id.clone())
                                    .unwrap_or_else(|| "unknown".to_owned()),
                                title: chunk
                                    .title
                                    .unwrap_or_else(|| "Gateway knowledge".to_owned()),
                                text: compact_text(
                                    chunk.text.as_deref().unwrap_or(""),
                                    KNOWLEDGE_SEGMENT_CHARS,
                                ),
                                origin: format!("gateway:{}", target.profile),
                                authority: "gateway".to_owned(),
                                lifecycle: chunk.freshness.unwrap_or_else(|| "current".to_owned()),
                                trust: if chunk.accepted_version_id.is_some() {
                                    "accepted"
                                } else {
                                    "unverified"
                                }
                                .to_owned(),
                                read_command: format!(
                                    "agent-tools docs get {}",
                                    chunk
                                        .doc_id
                                        .or(chunk.id)
                                        .unwrap_or_else(|| "<id>".to_owned())
                                ),
                            }
                        }));
                    }
                }
            }
            (p, t, knowledge)
        })
        .await
        .unwrap_or_default()
    });
    knowledge.extend(gateway_knowledge);
    knowledge.sort_by(|left, right| {
        left.origin
            .cmp(&right.origin)
            .then_with(|| left.identity.cmp(&right.identity))
    });
    knowledge
        .dedup_by(|left, right| left.identity == right.identity && left.origin == right.origin);
    knowledge.truncate(k);

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
    scored_tasks.sort_by_key(|b| std::cmp::Reverse(b.0));
    let top_tasks: Vec<_> = scored_tasks.into_iter().take(3).map(|(_, t)| t).collect();

    if patterns.is_empty() && top_tasks.is_empty() && knowledge.is_empty() {
        return Ok(());
    }

    let mut sections = Vec::new();

    if !patterns.is_empty() {
        let mut lines = vec!["Relevant patterns:".to_string()];
        for p in &patterns {
            lines.push(format!(
                "  {} [{}/{}] — {}",
                p.title, p.slug, p.id, p.summary
            ));
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

    if !knowledge.is_empty() {
        sections.push(render_knowledge_section(&knowledge));
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

    #[test]
    fn knowledge_rendering_is_deterministic_labelled_and_bounded() {
        let snippets = vec![KnowledgeSnippet {
            identity: "okf://fixture/runbook".to_owned(),
            title: "Recovery".to_owned(),
            text: "x".repeat(5_000),
            origin: "repository:.agents/knowledge".to_owned(),
            authority: "repository".to_owned(),
            lifecycle: "draft stale_after=2020-01-01".to_owned(),
            trust: "unverified".to_owned(),
            read_command: "agent-tools get okf://fixture/runbook".to_owned(),
        }];
        let first = render_knowledge_section(&snippets);
        let second = render_knowledge_section(&snippets);
        assert_eq!(first, second);
        assert!(first.len() <= KNOWLEDGE_CONTEXT_CHARS);
        assert!(first.contains("authority=repository"));
        assert!(first.contains("lifecycle=draft stale_after=2020-01-01"));
        assert!(first.contains("trust=unverified"));
        assert!(first.contains("read: agent-tools get"));
    }

    #[test]
    fn recorded_use_breaks_ties_without_outranking_authority() {
        let index = agent_knowledge::ProjectIndex::open_ephemeral().unwrap();
        let metadata = serde_json::json!({});
        let make = |uri: &str, authority: &str, status: &str| {
            index
                .ensure_resource(&agent_knowledge::ResourceInput {
                    project_id: "fixture",
                    namespace: "okf",
                    external_id: uri,
                    canonical_uri: uri,
                    kind: "CodeSymbol",
                    title: uri,
                    description: None,
                    origin_kind: if authority == "repository" {
                        "repository"
                    } else {
                        "local-derived"
                    },
                    origin_id: "fixture",
                    authority,
                    status: Some(status),
                    stale_after: None,
                    metadata: &metadata,
                })
                .unwrap()
        };
        let authored = make("okf://fixture/authored", "repository", "stable");
        let cold = make("okf://fixture/cold", "derived", "stable");
        let hot = make("okf://fixture/hot", "derived", "stable");
        let draft_hot = make("okf://fixture/draft-hot", "derived", "draft");

        for _ in 0..25 {
            index.record_access(hot, "read").unwrap();
            index.record_access(draft_hot, "read").unwrap();
        }

        // Relevance order deliberately puts the least-used first.
        let candidates: Vec<_> = [cold, draft_hot, hot, authored]
            .into_iter()
            .map(|id| stub_match(id, &index))
            .collect();
        let ranked = rank_by_recorded_use(&index, candidates, 4).unwrap();
        let order: Vec<i64> = ranked.iter().map(|item| item.resource.id).collect();

        // Authority first, then lifecycle, and only then recorded use — a
        // heavily-read derived concept never displaces what the repo asserts.
        assert_eq!(order, vec![authored, hot, cold, draft_hot]);
    }

    #[test]
    fn recorded_use_is_inert_without_history() {
        let index = agent_knowledge::ProjectIndex::open_ephemeral().unwrap();
        let metadata = serde_json::json!({});
        let ids: Vec<i64> = ["a", "b", "c"]
            .iter()
            .map(|name| {
                index
                    .ensure_resource(&agent_knowledge::ResourceInput {
                        project_id: "fixture",
                        namespace: "okf",
                        external_id: name,
                        canonical_uri: name,
                        kind: "CodeSymbol",
                        title: name,
                        description: None,
                        origin_kind: "local-derived",
                        origin_id: "fixture",
                        authority: "derived",
                        status: Some("stable"),
                        stale_after: None,
                        metadata: &metadata,
                    })
                    .unwrap()
            })
            .collect();
        let candidates: Vec<_> = ids.iter().map(|id| stub_match(*id, &index)).collect();
        let ranked = rank_by_recorded_use(&index, candidates, 3).unwrap();
        // With nothing recorded, relevance order is preserved exactly.
        assert_eq!(
            ranked
                .iter()
                .map(|item| item.resource.id)
                .collect::<Vec<_>>(),
            ids
        );
    }

    fn stub_match(
        resource_id: i64,
        index: &agent_knowledge::ProjectIndex,
    ) -> agent_knowledge::SearchMatch {
        let detail = index.resource_detail(resource_id).unwrap().unwrap();
        agent_knowledge::SearchMatch {
            resource: detail.resource,
            segment_id: resource_id,
            heading_path: None,
            text: String::new(),
            rank_micros: 0,
        }
    }

    #[test]
    fn compact_text_never_includes_unbounded_hostile_input() {
        let hostile = "<script>run()</script> ".repeat(10_000);
        let compact = compact_text(&hostile, 80);
        assert_eq!(compact.chars().count(), 80);
        assert!(compact.ends_with('…'));
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
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            json!(ctx)
        );
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
