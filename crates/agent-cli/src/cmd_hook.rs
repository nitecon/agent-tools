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
use crate::hook_session::SessionMemory;
use agent_comms::docs::ApiDocFilters;
use agent_comms::patterns::PatternFilters;
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{IsTerminal, Read};
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

/// Session id from the hook payload, falling back to the env vars some CLIs
/// export instead. `None` disables per-session dedupe.
pub(crate) fn extract_session_id(payload: &Value) -> Option<String> {
    for key in &["session_id", "sessionId"] {
        if let Some(Value::String(s)) = payload.get(key) {
            if !s.trim().is_empty() {
                return Some(s.trim().to_owned());
            }
        }
    }
    ["CLAUDE_SESSION_ID", "GEMINI_SESSION_ID", "CODEX_SESSION_ID"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|s| !s.trim().is_empty()))
}

/// Prompts that are harness bookkeeping rather than the user speaking.
///
/// Background-task notifications and system reminders arrive through the same
/// hook as a real prompt. They never benefit from context injection, and they
/// are frequent enough in long sessions that answering them costs real tokens.
pub(crate) fn is_harness_notification(prompt: &str) -> bool {
    let head: String = prompt.trim_start().chars().take(400).collect();
    const MARKERS: &[&str] = &[
        "<task-notification>",
        "[SYSTEM NOTIFICATION",
        "<system-reminder>",
        "<local-command-stdout>",
        "<command-name>",
    ];
    MARKERS.iter().any(|marker| head.starts_with(marker))
        || head.contains("[SYSTEM NOTIFICATION - NOT USER INPUT]")
}

/// Stable dedupe key for a task: a status change makes it new again.
fn task_key(id: &str, status: &str) -> String {
    format!("{id}:{status}")
}

/// Longest excerpt injected for authored (repository/gateway) knowledge.
const KNOWLEDGE_SEGMENT_CHARS: usize = 320;
/// Derived concepts summarize code the agent can read directly; keep them short.
const DERIVED_SEGMENT_CHARS: usize = 220;
/// Upper bound on the whole knowledge section per prompt.
const KNOWLEDGE_CONTEXT_CHARS: usize = 2_000;

/// Prompt words that carry no signal for a knowledge lookup. Matching on these
/// is what made every prompt light up a handful of unrelated modules.
const STOPWORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "that",
    "this",
    "from",
    "are",
    "was",
    "were",
    "have",
    "has",
    "had",
    "not",
    "but",
    "you",
    "your",
    "our",
    "can",
    "will",
    "would",
    "should",
    "could",
    "into",
    "about",
    "then",
    "than",
    "them",
    "they",
    "what",
    "when",
    "where",
    "which",
    "how",
    "why",
    "also",
    "just",
    "make",
    "need",
    "want",
    "like",
    "use",
    "using",
    "used",
    "get",
    "got",
    "let",
    "lets",
    "please",
    "now",
    "here",
    "there",
    "some",
    "any",
    "all",
    "more",
    "most",
    "very",
    "its",
    "out",
    "over",
    "only",
    "does",
    "did",
    "doing",
    "done",
    "see",
    "look",
    "fix",
    "add",
    "run",
    "code",
    "file",
    "files",
    "new",
    "one",
    "way",
    "still",
    "sure",
    "think",
    "thing",
    "things",
    "something",
    "right",
    "okay",
    "yes",
    "going",
    "ahead",
    "actually",
    "really",
    "maybe",
    "much",
    "many",
    "each",
    "own",
    "same",
    "other",
    "because",
    "being",
    "before",
    "after",
    "again",
    "through",
    "under",
    "while",
    "both",
];

/// Prompt tokens worth searching the knowledge index for: distinct, at least
/// three characters, not a stopword, not a bare number, and not part of the
/// project's own name (which appears in nearly every concept title).
pub(crate) fn knowledge_query_tokens(prompt: &str, project_ident: &str) -> Vec<String> {
    let ident_tokens: BTreeSet<String> = prompt_tokens(project_ident).into_iter().collect();
    let mut seen = BTreeSet::new();
    prompt_tokens(prompt)
        .into_iter()
        .filter(|token| token.len() >= 3)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .filter(|token| !token.chars().all(|c| c.is_ascii_digit()))
        .filter(|token| !ident_tokens.contains(token))
        .filter(|token| seen.insert(token.clone()))
        .take(8)
        .collect()
}

/// Share of candidate titles a token may hit before it stops discriminating.
const UBIQUITOUS_TITLE_SHARE: f64 = 0.5;
/// Candidate sets smaller than this are too small to judge ubiquity.
const MIN_CANDIDATES_FOR_IDF: usize = 4;

/// Drop tokens that hit the titles of most candidates. A word that names half
/// the result set (a crate name, a common noun in this codebase) cannot be
/// evidence that the prompt means any one of them.
pub(crate) fn discriminative_tokens(tokens: &[String], titles: &[String]) -> Vec<String> {
    if titles.len() < MIN_CANDIDATES_FOR_IDF {
        return tokens.to_vec();
    }
    let lowered: Vec<String> = titles.iter().map(|t| t.to_ascii_lowercase()).collect();
    tokens
        .iter()
        .filter(|token| {
            let hits = lowered
                .iter()
                .filter(|t| t.contains(token.as_str()))
                .count();
            (hits as f64) / (titles.len() as f64) < UBIQUITOUS_TITLE_SHARE
        })
        .cloned()
        .collect()
}

/// Shortest derived excerpt worth injecting. Anything shorter is a bare
/// heading or a symbol-count line, which tells the agent nothing.
const MIN_DERIVED_TEXT_CHARS: usize = 60;

/// How many distinct query tokens hit a concept's title and how many hit the
/// matched excerpt. Title hits are the strong signal: they mean the prompt is
/// naming the thing rather than sharing vocabulary with its description.
pub(crate) fn knowledge_hits(tokens: &[String], title: &str, text: &str) -> (usize, usize) {
    let title = title.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    let title_hits = tokens.iter().filter(|t| title.contains(t.as_str())).count();
    let text_hits = tokens
        .iter()
        .filter(|t| !title.contains(t.as_str()) && text.contains(t.as_str()))
        .count();
    (title_hits, text_hits)
}

/// Segments that list a concept's relationships or exports are indexes, not
/// knowledge. A hit there means a call-site name matched, which says nothing
/// about the prompt.
fn is_index_segment(heading_path: Option<&str>, text: &str) -> bool {
    let heading = heading_path.unwrap_or("");
    let head: String = text.trim_start().chars().take(40).collect();
    ["Relationships", "Exported symbols"]
        .iter()
        .any(|marker| heading.contains(marker) || head.contains(marker))
}

/// Decide whether a matched concept is worth injecting without being asked.
///
/// Authored knowledge (repository or gateway authority) carries intent the
/// source cannot express, so any genuine hit is worth surfacing. Derived
/// concepts summarize code the agent can read for itself; they only earn
/// transparent injection when the prompt names them (a title hit) and either
/// corroborates that with a second token or the agent has read them before.
pub(crate) fn passes_value_gate(
    authority: &str,
    heading_path: Option<&str>,
    text: &str,
    title_hits: usize,
    text_hits: usize,
    accesses: u64,
) -> bool {
    if is_index_segment(heading_path, text) {
        return false;
    }
    if matches!(authority, "repository" | "gateway") {
        return title_hits + text_hits >= 1;
    }
    title_hits >= 1 && (title_hits + text_hits >= 2 || accesses > 0)
}

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

/// Concepts from the local index that this prompt genuinely points at.
///
/// `seen` holds identities already injected this session; they are skipped so
/// the same concept is never sent twice.
fn local_knowledge_snippets(
    prompt: &str,
    limit: usize,
    seen: &BTreeSet<String>,
) -> Result<Vec<KnowledgeSnippet>> {
    let root = std::env::current_dir()?;
    let project_id = agent_core::project_ident(&root);
    let tokens = knowledge_query_tokens(prompt, &project_id);
    let query = tokens
        .iter()
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
    // Rank the full candidate set; the value gate below decides what survives,
    // so trimming to `limit` first would let a rejected candidate crowd out an
    // accepted one further down.
    let candidate_count = matches.len();
    let titles: Vec<String> = matches.iter().map(|m| m.resource.title.clone()).collect();
    let tokens = discriminative_tokens(&tokens, &titles);
    let matches = rank_by_recorded_use(&index, matches, candidate_count)?;
    let mut snippets = Vec::new();
    for item in matches {
        if snippets.len() >= limit {
            break;
        }
        if item.resource.status == "deprecated" || seen.contains(&item.resource.canonical_uri) {
            continue;
        }
        let accesses = index.access_count(item.resource.id).unwrap_or(0).max(0) as u64;
        let (title_hits, text_hits) = knowledge_hits(&tokens, &item.resource.title, &item.text);
        if !passes_value_gate(
            &item.resource.authority,
            item.heading_path.as_deref(),
            &item.text,
            title_hits,
            text_hits,
            accesses,
        ) {
            continue;
        }
        let detail = index.resource_detail(item.resource.id)?;
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
        let authored = matches!(item.resource.authority.as_str(), "repository" | "gateway");
        let budget = if authored {
            KNOWLEDGE_SEGMENT_CHARS
        } else {
            DERIVED_SEGMENT_CHARS
        };
        let text = compact_text(&item.text, budget);
        if !authored && text.chars().count() < MIN_DERIVED_TEXT_CHARS {
            continue;
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

/// Render the knowledge section: one labelled line per concept plus its
/// excerpt. Labels are authority, lifecycle, and trust, in that order, so an
/// agent can weigh a `derived unverified` excerpt differently from a
/// `repository verified` one without a per-line legend.
fn render_knowledge_section(snippets: &[KnowledgeSnippet]) -> String {
    let mut output = "Relevant knowledge [authority · lifecycle · trust] — full concept: \
                      agent-tools get <uri>"
        .to_owned();
    for snippet in snippets {
        let text = compact_text(&snippet.text, KNOWLEDGE_SEGMENT_CHARS);
        let block = format!(
            "\n- {} [{} · {} · {}] {}\n  {}",
            snippet.title,
            snippet.authority,
            snippet.lifecycle,
            snippet.trust,
            snippet.identity,
            text
        );
        if output.len() + block.len() > KNOWLEDGE_CONTEXT_CHARS {
            break;
        }
        output.push_str(&block);
    }
    output
}

// -- session-start logic -----------------------------------------------------

/// Read the hook payload when one is piped in. A terminal on stdin means a
/// human ran the command by hand; never block waiting on them.
fn read_payload() -> Option<Value> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut raw = String::new();
    stdin.lock().read_to_string(&mut raw).ok()?;
    serde_json::from_str(&raw).ok()
}

fn run_session_start(agent: &str) -> Result<()> {
    let payload = read_payload();
    let session_id = payload.as_ref().and_then(extract_session_id);
    let root = std::env::current_dir()?;
    let mut session = SessionMemory::open(&root, session_id.as_deref());

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
        // The prompt hook must not repeat what the session already opened with.
        session.state.tasks.insert(task_key(&t.id, &t.status));
    }
    lines.push("Pull full detail + spec before starting: agent-tools tasks get <id>".to_string());

    let additional_context = lines.join("\n");
    let event = event_name(true, agent);
    let envelope = render_envelope(event, &additional_context);
    println!("{envelope}");
    session.save();
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
    if is_harness_notification(&prompt) {
        return Ok(());
    }

    let root = std::env::current_dir()?;
    let mut session = SessionMemory::open(&root, extract_session_id(&payload).as_deref());

    let k = hook_limit();
    let tokens = prompt_tokens(&prompt);
    let mut knowledge =
        local_knowledge_snippets(&prompt, k, &session.state.knowledge).unwrap_or_default();

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
    knowledge.retain(|snippet| !session.state.knowledge.contains(&snippet.identity));
    knowledge.truncate(k);

    let patterns: Vec<_> = patterns
        .into_iter()
        .filter(|p| !session.state.patterns.contains(&p.id))
        .take(k)
        .collect();

    // Rank tasks by prompt token overlap, skipping ones this session has seen
    // in their current status.
    let mut scored_tasks: Vec<_> = tasks
        .into_iter()
        .filter(|t| !session.state.tasks.contains(&task_key(&t.id, &t.status)))
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

    for snippet in &knowledge {
        session.state.knowledge.insert(snippet.identity.clone());
    }
    for pattern in &patterns {
        session.state.patterns.insert(pattern.id.clone());
    }
    for task in &top_tasks {
        session.state.tasks.insert(task_key(&task.id, &task.status));
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
    session.save();
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
        assert!(first.contains("[repository · draft stale_after=2020-01-01 · unverified]"));
        assert!(first.contains("okf://fixture/runbook"));
        assert!(first.contains("agent-tools get <uri>"));
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

    // -- value gate ----------------------------------------------------------

    fn tokens(prompt: &str) -> Vec<String> {
        knowledge_query_tokens(prompt, "github.com/nitecon/agent-tools.git")
    }

    #[test]
    fn query_tokens_drop_stopwords_numbers_duplicates_and_project_name() {
        let got = tokens("Ok lets make it happen then, fix the hook dispatch 42 hook");
        assert_eq!(got, vec!["happen", "hook", "dispatch"]);
        assert!(tokens("the and for").is_empty());
        // "agent" and "tools" name the project; they would hit every title.
        assert_eq!(
            tokens("our agent tools have OKF built in"),
            vec!["okf", "built"]
        );
    }

    #[test]
    fn ubiquitous_title_tokens_stop_counting() {
        let toks = vec!["hook".to_owned(), "cli".to_owned()];
        let titles: Vec<String> = [
            "crates/agent-cli/src/cmd_hook.rs",
            "crates/agent-cli/src/cmd_tasks.rs",
            "crates/agent-cli/src/main.rs",
            "crates/agent-cli/src/nudge.rs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(discriminative_tokens(&toks, &titles), vec!["hook"]);
        // Too few candidates to judge: keep everything.
        assert_eq!(discriminative_tokens(&toks, &titles[..2]), toks);
    }

    #[test]
    fn hits_count_distinct_tokens_and_do_not_double_count_title_words() {
        let toks = tokens("refactor the hook dispatch");
        let (title, text) = knowledge_hits(&toks, "dispatch", "Dispatch hook subcommands.");
        assert_eq!((title, text), (1, 1));
        let (title, text) = knowledge_hits(&toks, "crates/agent-cli/src/cmd_hook.rs", "");
        assert_eq!((title, text), (1, 0));
        let (title, text) = knowledge_hits(&toks, "GatewayClient::new", "build a client");
        assert_eq!((title, text), (0, 0));
    }

    #[test]
    fn authored_knowledge_passes_on_any_hit() {
        assert!(passes_value_gate("repository", None, "runbook", 0, 1, 0));
        assert!(passes_value_gate("gateway", None, "api note", 1, 0, 0));
        assert!(!passes_value_gate("repository", None, "unrelated", 0, 0, 0));
    }

    #[test]
    fn derived_knowledge_needs_a_title_hit_plus_corroboration() {
        // Vocabulary overlap in the body alone is exactly the old noise.
        assert!(!passes_value_gate(
            "derived",
            None,
            "mentions hook twice",
            0,
            2,
            0
        ));
        // A title hit with nothing else is still too weak when never read.
        assert!(!passes_value_gate("derived", None, "…", 1, 0, 0));
        // Title hit + second token, or title hit + prior use, is a real signal.
        assert!(passes_value_gate("derived", None, "…", 1, 1, 0));
        assert!(passes_value_gate("derived", None, "…", 1, 0, 3));
        assert!(passes_value_gate("derived", None, "…", 2, 0, 0));
    }

    #[test]
    fn index_segments_never_pass() {
        assert!(!passes_value_gate(
            "repository",
            Some("Relationships"),
            "- calls `Ok`",
            2,
            2,
            9
        ));
        assert!(!passes_value_gate(
            "derived",
            None,
            "## Exported symbols - [HookCommands](...)",
            2,
            2,
            9
        ));
    }

    #[test]
    fn harness_notifications_are_recognised() {
        assert!(is_harness_notification(
            "<task-notification>\n<task-id>x</task-id>\n</task-notification>"
        ));
        assert!(is_harness_notification(
            "[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated event"
        ));
        assert!(is_harness_notification("  <system-reminder>\nreminder"));
        assert!(!is_harness_notification("fix the notification hook"));
        assert!(!is_harness_notification(
            "please read <task-notification> docs"
        ));
    }

    #[test]
    fn session_id_comes_from_payload_before_env() {
        let payload = json!({"session_id": " abc-123 ", "prompt": "x"});
        assert_eq!(extract_session_id(&payload).as_deref(), Some("abc-123"));
        let payload = json!({"sessionId": "camel"});
        assert_eq!(extract_session_id(&payload).as_deref(), Some("camel"));
        let prev = std::env::var("CLAUDE_SESSION_ID").ok();
        std::env::remove_var("CLAUDE_SESSION_ID");
        std::env::remove_var("GEMINI_SESSION_ID");
        std::env::remove_var("CODEX_SESSION_ID");
        assert!(extract_session_id(&json!({"prompt": "x"})).is_none());
        if let Some(v) = prev {
            std::env::set_var("CLAUDE_SESSION_ID", v);
        }
    }

    #[test]
    fn knowledge_section_is_compact_and_labelled() {
        let snippet = KnowledgeSnippet {
            identity: "okf://fixture/runbook".to_owned(),
            title: "Recovery".to_owned(),
            text: "Restore from the last snapshot.".to_owned(),
            origin: "repository:.agents/knowledge".to_owned(),
            authority: "repository".to_owned(),
            lifecycle: "stable".to_owned(),
            trust: "verified".to_owned(),
            read_command: String::new(),
        };
        let section = render_knowledge_section(&[snippet]);
        assert!(
            section.contains("- Recovery [repository · stable · verified] okf://fixture/runbook")
        );
        assert!(section.contains("\n  Restore from the last snapshot."));
        assert!(section.contains("agent-tools get <uri>"));
        assert!(section.len() < 260, "{}", section.len());
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
