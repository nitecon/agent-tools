//! `agent-tools setup rules` — inject the agent-tools usage protocols into
//! known agent rule files (e.g. `~/.claude/CLAUDE.md`, `~/.gemini/GEMINI.md`).
//!
//! Idempotent: uses `<agent-tools-rules>...</agent-tools-rules>` markers so
//! re-runs replace the block in place rather than duplicating it. A `.bak`
//! sibling is written before each modification so the user can recover.

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

A real task board with three statuses, server-enforced ownership, 1h stale-
claim reclaim, and 7d done falloff. Use this as your TODO surface — do not
maintain task files locally when the gateway is configured.

```bash
agent-tools tasks list                   # TODO + IN PROGRESS for this project
agent-tools tasks get <id>               # full detail + comment thread
agent-tools tasks add --title "..." [--label x] [--description "..."]
agent-tools tasks claim <id>             # take ownership (-> in_progress)
agent-tools tasks release <id>           # drop ownership (-> todo)
agent-tools tasks done <id>              # mark complete
agent-tools tasks comment <id> "<note>"  # append a note
agent-tools tasks rank <id> <n>          # set ordering within a column
```
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
            "No agent rule files detected. Tried:\n  \
             ~/.claude/CLAUDE.md\n  \
             ~/.gemini/GEMINI.md\n  \
             ~/.codex/AGENTS.md\n  \
             ~/.config/codex/AGENTS.md\n\
             Re-run with `--target <path>` to point at a specific file."
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
                println!("  backup: {}", backup.display());
            }
            Ok(InjectOutcome::Prepended { backup }) => {
                println!("Prepended new block to {}", path.display());
                println!("  backup: {}", backup.display());
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
fn detect_agent_files() -> Vec<PathBuf> {
    let home = home_dir();
    let candidates = [
        home.join(".claude").join("CLAUDE.md"),
        home.join(".gemini").join("GEMINI.md"),
        home.join(".codex").join("AGENTS.md"),
        home.join(".config").join("codex").join("AGENTS.md"),
    ];
    candidates.into_iter().filter(|p| p.exists()).collect()
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
    }
    format!("{OPEN_MARKER}\n{body}{CLOSE_MARKER}\n")
}

enum InjectOutcome {
    DryRun(String),
    Replaced { backup: PathBuf },
    Prepended { backup: PathBuf },
}

fn inject(path: &Path, block: &str, dry_run: bool) -> Result<InjectOutcome> {
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

    let backup = backup_path(path);
    std::fs::write(&backup, &existing)
        .with_context(|| format!("write backup to {}", backup.display()))?;
    std::fs::write(path, &new_content)
        .with_context(|| format!("write updated file {}", path.display()))?;

    if already_present {
        Ok(InjectOutcome::Replaced { backup })
    } else {
        Ok(InjectOutcome::Prepended { backup })
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
        // Markers appear exactly once each — the body text never repeats them.
        assert_eq!(b.matches(OPEN_MARKER).count(), 1);
        assert_eq!(b.matches(CLOSE_MARKER).count(), 1);
    }

    #[test]
    fn build_block_without_gateway_omits_comms_and_tasks() {
        let b = build_block(false);
        assert!(b.starts_with(OPEN_MARKER));
        assert!(b.trim_end().ends_with(CLOSE_MARKER));
        assert!(b.contains("agent-tools symbol"));
        assert!(!b.contains("agent-tools comms"));
        assert!(!b.contains("agent-tools tasks"));
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
        // comms + tasks without leaving any of the old single-section block
        // behind.
        let bare = build_block(false);
        let full = build_block(true);
        let after_first = compute_new_content("# Header\n", &bare, false);
        let after_second = compute_new_content(&after_first, &full, true);
        assert!(after_second.contains("agent-tools comms recv"));
        assert!(after_second.contains("agent-tools tasks list"));
        assert_eq!(after_second.matches(OPEN_MARKER).count(), 1);
        assert_eq!(after_second.matches(CLOSE_MARKER).count(), 1);
    }

    #[test]
    fn backup_path_appends_bak() {
        let p = PathBuf::from("/tmp/foo/CLAUDE.md");
        assert_eq!(backup_path(&p), PathBuf::from("/tmp/foo/CLAUDE.md.bak"));
    }
}
