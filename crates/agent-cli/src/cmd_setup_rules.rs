//! `agent-tools setup rules` — inject the agent-tools usage protocols into
//! known agent rule files (e.g. `~/.claude/CLAUDE.md`, `~/.gemini/GEMINI.md`,
//! `~/.codex/AGENTS.md`).
//!
//! Detection is by *agent home directory* rather than rule-file existence, so
//! a fresh Codex install without an `AGENTS.md` still picks up the block on
//! the first setup run. Idempotent: uses
//! `<agent-tools-rules>...</agent-tools-rules>` markers so re-runs replace the
//! block in place rather than duplicating it. A `.bak` sibling is written
//! before each destructive modification so the user can recover.

use agent_comms::config::{home_dir, load_config};
use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const OPEN_MARKER: &str = "<agent-tools-rules>";
const CLOSE_MARKER: &str = "</agent-tools-rules>";

// The injected body is composed from these section constants. Code
// exploration is always included; the gateway-backed sections are only added
// when the local gateway config is present, so agents on un-configured
// machines still get the symbol/tree directives without false references to
// comms or tasks they can't actually reach.
//
// Keep these in lockstep with the README's "Manual rules install" section —
// `setup rules --print` is the single source of truth.

const HEADER: &str = r#"## Agent Tools — Mandatory Protocols
"#;

/// One-shot reminder printed to stdout after a successful inject. The same
/// information used to live inside the injected block itself, which wasted
/// agent context on every conversation; showing it here keeps the user aware
/// of the overwrite behavior without bloating the rules file.
const POST_INJECT_NOTE: &str =
    "Note: content between the <agent-tools-rules> markers is regenerated \
from this CLI on every run of `agent-tools setup rules` — edit the source, not the marker block.";

const CODE_EXPLORATION_SECTION: &str = r#"
### Code Exploration (token-efficient)

Prefer symbol-aware tools over raw file reads or shell text search.

```bash
agent-tools tree [path] --depth <n>            # directory tree
agent-tools symbols <file>                     # list a file's symbols
agent-tools symbol <name> --file <path>        # extract a symbol's source
agent-tools search <query> --type symbol|file  # project-wide index search
agent-tools summary [path]                     # compact project overview
agent-tools index --rebuild                    # refresh after large changes
```
"#;

const COMMS_SECTION: &str = r#"
### Comms (gateway-backed messaging)

Project ident auto-derives from the cwd git remote; agent id is machine-
persistent. Do NOT use the MCP comms tools — they are deprecated.

```bash
agent-tools comms recv                   # fetch unread (start of session)
agent-tools comms confirm <id>           # ack each handled message
agent-tools comms send "<body>"          # post to project channel
agent-tools comms reply <id> "<body>"    # threaded reply
agent-tools comms action <id> "<verb>"   # signal you are working on it
agent-tools comms whoami                 # show derived ident + agent-id
```
"#;

const TASKS_SECTION: &str = r#"
### Tasks (gateway-backed per-project board)

Use this as your TODO surface when the gateway is configured. For complex tasks
include a `--specification` with enough handoff context for a fresh agent to
resume; specifications survive crashes that local plan files do not.

```bash
agent-tools tasks list                   # TODO + IN PROGRESS for this project
agent-tools tasks get <id>               # full detail + comment thread
agent-tools tasks add --title "..." [--label x] [--description "..."] [--specification "..."]
agent-tools tasks add-delegated --target-project "..." --title "..." --description "..." --specification "..."
agent-tools tasks claim <id>             # take ownership (-> in_progress)
agent-tools tasks release <id>           # drop ownership (-> todo)
agent-tools tasks done <id>              # mark complete
agent-tools tasks comment <id> "<note>"  # append a note
agent-tools tasks rank <id> <n>          # set ordering within a column
```
"#;

const DOCS_SECTION: &str = r#"
### API Context Docs (gateway-backed)

Agent-first context for API intent, workflows, auth, safety, schemas, and
copyable examples. Look up existing context before searching code for API
behavior; after materially changing API files, publish updated context.

```bash
agent-tools docs search "<api-or-workflow>"
agent-tools docs list [--app APP] [--label LABEL] [--kind KIND] [--query Q]
agent-tools docs get <id>
agent-tools docs chunks --query "<api-or-workflow>" [--app APP] [--label LABEL]
agent-tools docs validate --file .agent/api/<app>.yaml
agent-tools docs publish --file .agent/api/<app>.yaml
```

If no docs exist for an app, propose adding `.agent/api/<app>.yaml` and track
the `docs publish` step as a task subtask so it isn't skipped.
"#;

const PATTERNS_SECTION: &str = r#"
### Patterns (gateway-backed global guidance)

Durable organization-wide implementation guidance. At the start of work that
may involve established practice, search latest active patterns; if
`$PWD/.patterns` exists, run `agent-tools patterns check` before relying on it.

```bash
agent-tools patterns search "<query>" --version latest --state active
agent-tools patterns get <id-or-slug>
agent-tools patterns comments <id-or-slug>      # only when iterating on a pattern
agent-tools patterns update <id-or-slug> --body-file /tmp/pattern.md
agent-tools patterns check                       # validate $PWD/.patterns
agent-tools patterns use <id-or-slug> --path src/main.rs
```

`.patterns` is minimal repo metadata: gateway pattern ids as keys, optional
file paths as values, no comments. When you use a pattern, ensure its id is
listed there with relevant paths.

When the user asks to iterate on a pattern, fetch its body and comments, edit
a local markdown draft, then `patterns update --body-file <draft.md>` —
preserve unrelated sections unless asked to change them. If you implement a
net-new approach worth reusing and no pattern exists, propose drafting one
with the user.
"#;

/// Entry point invoked from `main.rs` for `agent-tools setup rules`.
pub fn run(target: Option<PathBuf>, all: bool, dry_run: bool, print: bool) -> Result<()> {
    let gateway_on = gateway_configured();

    if print {
        print!("{}", build_block(gateway_on));
        return Ok(());
    }

    if !gateway_on {
        eprintln!(
            "Notice: agent-gateway is not configured.\n\
             Injecting code-exploration rules only. Run `agent-tools setup gateway`\n\
             then re-run `setup rules` to add the comms + tasks sections."
        );
    }

    let candidates = match &target {
        Some(t) => vec![t.clone()],
        None => detect_agent_files(),
    };

    if candidates.is_empty() {
        anyhow::bail!(
            "No agent home directories detected. Tried:\n  \
             ~/.claude (CLAUDE.md)\n  \
             ~/.gemini (GEMINI.md)\n  \
             ~/.codex or $CODEX_HOME (AGENTS.md)\n  \
             ~/.config/codex (AGENTS.md)\n\
             Install one of these agents first, or re-run with \
             `--target <path>` to point at a specific rule file."
        );
    }

    let chosen = if all || target.is_some() || candidates.len() == 1 {
        candidates
    } else {
        prompt_user_for_selection(&candidates)?
    };

    if chosen.is_empty() {
        println!("Cancelled — no files modified.");
        return Ok(());
    }

    let block = build_block(gateway_on);
    let mut any_failed = false;
    for path in &chosen {
        match inject(path, &block, dry_run) {
            Ok(InjectOutcome::DryRun(preview)) => {
                println!("--- DRY RUN: {} ---", path.display());
                print!("{preview}");
                println!("--- end preview ---");
            }
            Ok(InjectOutcome::Replaced { backup }) => {
                println!("Updated existing block in {}", path.display());
                if let Some(b) = backup {
                    println!("  backup: {}", b.display());
                }
            }
            Ok(InjectOutcome::Prepended { backup }) => {
                println!("Prepended new block to {}", path.display());
                if let Some(b) = backup {
                    println!("  backup: {}", b.display());
                }
            }
            Ok(InjectOutcome::Created) => {
                println!("Created new rule file at {}", path.display());
            }
            Err(e) => {
                eprintln!("Failed to update {}: {e:#}", path.display());
                any_failed = true;
            }
        }
    }
    if any_failed {
        anyhow::bail!("one or more files could not be updated");
    }
    if !dry_run {
        println!("\n{POST_INJECT_NOTE}");
    }
    Ok(())
}

// -- Internals ---------------------------------------------------------------

fn gateway_configured() -> bool {
    let cfg = load_config();
    cfg.gateway.url.is_some() && cfg.gateway.api_key.is_some()
}

/// Built-in detection list. Only home-dir global rule files; project-local
/// instruction files (e.g. `./CLAUDE.md`) are intentionally left alone since
/// they're per-repo content the user should edit directly.
///
/// Detection is by **agent home directory**, not rule-file existence. That
/// means a fresh Codex install (with `~/.codex/` present but no
/// `AGENTS.md`) still gets the rules injected on first run — `inject()`
/// creates the file if missing. The `~/.config/codex/AGENTS.md` variant is
/// only considered when that XDG-style directory actually exists, so we
/// don't pre-create it on macOS where only `~/.codex/` is used.
pub(crate) fn detect_agent_files() -> Vec<PathBuf> {
    let home = home_dir();
    let candidates = [
        (home.join(".claude"), home.join(".claude").join("CLAUDE.md")),
        (home.join(".gemini"), home.join(".gemini").join("GEMINI.md")),
        (codex_home(), codex_home().join("AGENTS.md")),
        (
            home.join(".config").join("codex"),
            home.join(".config").join("codex").join("AGENTS.md"),
        ),
    ];
    let mut out: Vec<PathBuf> = Vec::new();
    for (dir, file) in candidates {
        // The directory is the signal the agent is installed; the file itself
        // may or may not exist yet. Dedupe on file path so overlapping
        // CODEX_HOME + ~/.codex setups don't double-list.
        if dir.exists() && !out.iter().any(|p| p == &file) {
            out.push(file);
        }
    }
    out
}

/// Resolve the Codex home directory. Honors `CODEX_HOME` (Codex's own
/// override) before falling back to `~/.codex`, matching the behavior
/// documented in Codex's `skill-installer` skill.
pub(crate) fn codex_home() -> PathBuf {
    if let Ok(val) = std::env::var("CODEX_HOME") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    home_dir().join(".codex")
}

/// Compose the rules block. `include_gateway_sections` flips the comms +
/// tasks blocks on or off so the same code-exploration baseline ships even
/// when the gateway is absent.
fn build_block(include_gateway_sections: bool) -> String {
    let mut body = String::new();
    body.push_str(HEADER);
    body.push_str(CODE_EXPLORATION_SECTION);
    if include_gateway_sections {
        body.push_str(COMMS_SECTION);
        body.push_str(TASKS_SECTION);
        body.push_str(DOCS_SECTION);
        body.push_str(PATTERNS_SECTION);
    }
    format!("{OPEN_MARKER}\n{body}{CLOSE_MARKER}\n")
}

enum InjectOutcome {
    DryRun(String),
    Replaced { backup: Option<PathBuf> },
    Prepended { backup: Option<PathBuf> },
    Created,
}

fn inject(path: &Path, block: &str, dry_run: bool) -> Result<InjectOutcome> {
    let file_exists = path.exists();
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let already_present = existing.contains(OPEN_MARKER) && existing.contains(CLOSE_MARKER);
    let new_content = compute_new_content(&existing, block, already_present);

    if dry_run {
        return Ok(InjectOutcome::DryRun(new_content));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent of {}", path.display()))?;
    }

    // Only write a `.bak` when there's actual content to preserve. Brand-new
    // rule files (first-run Codex users whose AGENTS.md we just created)
    // don't need a zero-byte `.bak` cluttering the directory.
    let backup = if file_exists {
        let b = backup_path(path);
        std::fs::write(&b, &existing)
            .with_context(|| format!("write backup to {}", b.display()))?;
        Some(b)
    } else {
        None
    };
    std::fs::write(path, &new_content)
        .with_context(|| format!("write updated file {}", path.display()))?;

    match (file_exists, already_present) {
        (false, _) => Ok(InjectOutcome::Created),
        (true, true) => Ok(InjectOutcome::Replaced { backup }),
        (true, false) => Ok(InjectOutcome::Prepended { backup }),
    }
}

/// Pure helper: build the new file body from existing content + the rules
/// block. Factored out so unit tests can verify idempotency and ordering
/// without touching the filesystem.
fn compute_new_content(existing: &str, block: &str, already_present: bool) -> String {
    if already_present {
        replace_block(existing, block)
    } else if existing.is_empty() {
        block.to_string()
    } else {
        format!("{block}\n{existing}")
    }
}

fn replace_block(existing: &str, new_block: &str) -> String {
    // Both markers are guaranteed present by the caller. Trim the trailing
    // newline after `</agent-tools-rules>` so the swap-in doesn't accumulate
    // blank lines on each refresh.
    let open_idx = existing.find(OPEN_MARKER).unwrap_or(0);
    let close_idx = existing.find(CLOSE_MARKER).unwrap_or(existing.len());
    let close_end = close_idx + CLOSE_MARKER.len();
    let after_start = if existing[close_end..].starts_with('\n') {
        close_end + 1
    } else {
        close_end
    };
    let before = &existing[..open_idx];
    let after = &existing[after_start..];
    format!("{before}{new_block}{after}")
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

fn prompt_user_for_selection(candidates: &[PathBuf]) -> Result<Vec<PathBuf>> {
    println!("Detected agent rule files:");
    for (i, p) in candidates.iter().enumerate() {
        println!("  {}) {}", i + 1, p.display());
    }
    print!(
        "Update [a]ll, [1-{}] specific, [c]ancel: ",
        candidates.len()
    );
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .lock()
        .read_line(&mut input)
        .context("read selection")?;
    let s = input.trim().to_ascii_lowercase();

    if s.is_empty() || s == "c" || s == "cancel" {
        return Ok(vec![]);
    }
    if s == "a" || s == "all" {
        return Ok(candidates.to_vec());
    }
    if let Ok(n) = s.parse::<usize>() {
        if n >= 1 && n <= candidates.len() {
            return Ok(vec![candidates[n - 1].clone()]);
        }
    }
    anyhow::bail!("invalid selection: {s:?}");
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_block_with_gateway_includes_all_sections() {
        let b = build_block(true);
        assert!(b.starts_with(OPEN_MARKER));
        assert!(b.trim_end().ends_with(CLOSE_MARKER));
        assert!(b.contains("agent-tools symbol"));
        assert!(b.contains("agent-tools comms recv"));
        assert!(b.contains("agent-tools tasks list"));
        assert!(b.contains("agent-tools docs search"));
        assert!(b.contains("publish updated context"));
        assert!(b.contains("agent-tools patterns check"));
        // Markers appear exactly once each — the body text never repeats them.
        assert_eq!(b.matches(OPEN_MARKER).count(), 1);
        assert_eq!(b.matches(CLOSE_MARKER).count(), 1);
    }

    #[test]
    fn build_block_without_gateway_omits_gateway_sections() {
        let b = build_block(false);
        assert!(b.starts_with(OPEN_MARKER));
        assert!(b.trim_end().ends_with(CLOSE_MARKER));
        assert!(b.contains("agent-tools symbol"));
        assert!(!b.contains("agent-tools comms"));
        assert!(!b.contains("agent-tools tasks"));
        assert!(!b.contains("agent-tools docs"));
        assert!(!b.contains("agent-tools patterns"));
    }

    #[test]
    fn compute_new_content_prepends_when_absent() {
        let existing = "# Existing instructions\n\nfoo bar\n";
        let block = build_block(true);
        let out = compute_new_content(existing, &block, false);
        assert!(out.starts_with(OPEN_MARKER));
        assert!(out.contains("# Existing instructions"));
        // Blank line separator between block and prior content for readability.
        assert!(out.contains(&format!("{CLOSE_MARKER}\n\n# Existing instructions")));
    }

    #[test]
    fn compute_new_content_replaces_in_place() {
        let block = build_block(true);
        let existing = format!("# Header\n\n{OPEN_MARKER}\nold body\n{CLOSE_MARKER}\n\n# Footer\n");
        let out = compute_new_content(&existing, &block, true);
        assert!(out.starts_with("# Header"));
        assert!(out.contains("# Footer"));
        assert!(!out.contains("old body"));
        assert!(out.contains("agent-tools tasks list"));
        // Block still bracketed exactly once after replacement.
        assert_eq!(out.matches(OPEN_MARKER).count(), 1);
        assert_eq!(out.matches(CLOSE_MARKER).count(), 1);
    }

    #[test]
    fn compute_new_content_is_idempotent() {
        let block = build_block(true);
        let existing = "# Header\n";
        let once = compute_new_content(existing, &block, false);
        // Second pass must detect the marker and be a true no-op.
        let twice = compute_new_content(&once, &block, true);
        assert_eq!(once, twice);
    }

    #[test]
    fn compute_new_content_handles_empty_file() {
        let block = build_block(true);
        let out = compute_new_content("", &block, false);
        assert_eq!(out, block);
    }

    #[test]
    fn re_running_with_different_gateway_state_swaps_block() {
        // Simulates: first run on a machine without gateway, then user
        // configures gateway and re-runs. The block should grow to include
        // gateway sections without leaving any of the old single-section block
        // behind.
        let bare = build_block(false);
        let full = build_block(true);
        let after_first = compute_new_content("# Header\n", &bare, false);
        let after_second = compute_new_content(&after_first, &full, true);
        assert!(after_second.contains("agent-tools comms recv"));
        assert!(after_second.contains("agent-tools tasks list"));
        assert!(after_second.contains("agent-tools docs search"));
        assert!(after_second.contains("agent-tools patterns check"));
        assert_eq!(after_second.matches(OPEN_MARKER).count(), 1);
        assert_eq!(after_second.matches(CLOSE_MARKER).count(), 1);
    }

    #[test]
    fn backup_path_appends_bak() {
        let p = PathBuf::from("/tmp/foo/CLAUDE.md");
        assert_eq!(backup_path(&p), PathBuf::from("/tmp/foo/CLAUDE.md.bak"));
    }
}
