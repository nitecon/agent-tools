//! `agent-tools setup skill` — install a Claude Code Agent Skill that
//! advertises the `agent-tools` CLI to the model via auto-loaded description.
//!
//! Skills published under `~/.claude/skills/<name>/SKILL.md` have their
//! frontmatter `description` injected into the session system prompt (~100
//! tokens each) at start. The full body is only loaded on-demand when the
//! model judges the skill relevant, so this file stays out of context until
//! it earns its place.
//!
//! Idempotent: overwrites SKILL.md in place. Writes a `.bak` sibling before
//! the first destructive overwrite so the user can recover prior content.

use agent_comms::config::home_dir;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Skill body. The frontmatter `description` is deliberately specific about
/// the **replacement** of the native Task* tools — Claude's built-in system
/// prompt still says "Use TaskCreate to plan work," so we need an explicit
/// override here for the model to route task tracking through `agent-tools`.
const SKILL_BODY: &str = r#"---
name: agent-tools
description: Token-efficient code exploration (tree/symbols/search), gateway-backed task board (REQUIRED — native TaskCreate/TaskUpdate/TaskList/TaskGet are disabled in this setup), cross-project comms, and persistent memory via the `agent-tools` and `memory` CLIs. Prefer for bulk exploration and ALWAYS for task tracking.
allowed-tools: Bash(agent-tools *), Bash(memory *)
---

# agent-tools

The `agent-tools` CLI and its companion `memory` CLI together replace several
built-in capabilities. The native Claude Code task system
(`TaskCreate`/`TaskUpdate`/`TaskList`/`TaskGet`) is **disabled** in this
environment — task tracking MUST go through `agent-tools tasks`.

## Code Exploration (token-efficient)

Prefer symbol-aware tools over raw file reads or shell text search for bulk
exploration. Built-in `Read` is still fine for a known file path.

```bash
agent-tools tree [path] --depth <n>            # directory tree
agent-tools symbols <file>                     # list a file's symbols
agent-tools symbol <name> --file <path>        # extract a symbol's source
agent-tools search <query> --type symbol|file  # project-wide index search
agent-tools summary [path]                     # compact project overview
agent-tools index --rebuild                    # refresh after large changes
```

## Task Board (replaces native Task tools)

Gateway-backed with three statuses, server-enforced ownership, 1h stale-claim
reclaim, and 7d done falloff. Use this as your TODO surface — `TaskCreate` and
friends will be refused.

```bash
agent-tools tasks list                   # TODO + IN PROGRESS for this project
agent-tools tasks get <id>               # full detail + comment thread
agent-tools tasks add --title "..." [--label x] [--description "..."]
agent-tools tasks claim <id>             # take ownership (-> in_progress)
agent-tools tasks release <id>           # drop ownership (-> todo)
agent-tools tasks done <id>              # mark complete
agent-tools tasks comment <id> "<note>"  # append a note
```

## Comms (gateway-backed messaging)

Project ident auto-derives from the cwd git remote; agent id is
machine-persistent. Do not use the deprecated MCP comms tools.

```bash
agent-tools comms recv                   # fetch unread at session start
agent-tools comms confirm <id>           # ack each handled message
agent-tools comms send "<body>"          # post to project channel
agent-tools comms reply <id> "<body>"    # threaded reply
agent-tools comms action <id> "<verb>"   # signal work-in-progress
agent-tools comms whoami                 # show derived ident + agent-id
```

## Memory (persistent across sessions)

Every task must begin with a `context` or `search` call and end with a `store`
call if functionality changed. Project auto-detects from the cwd git remote.

```bash
memory context "<task description>" -k <limit>   # pre-task recall
memory search "<query>" -k <limit>                # hybrid BM25 + vector search
memory store "<content>" -m <type> -t "<tags>"    # post-task save
memory get <uuid> [<uuid>...]                     # fetch full content by id
memory forget --id <uuid>                          # remove a memory
```

Types: `user`, `feedback`, `project`, `reference`.
"#;

/// Entry point invoked from `main.rs` for `agent-tools setup skill`.
pub fn run(dry_run: bool, print: bool) -> Result<()> {
    if print {
        print!("{SKILL_BODY}");
        return Ok(());
    }

    let target = skill_path();

    if dry_run {
        println!("--- DRY RUN: {} ---", target.display());
        print!("{SKILL_BODY}");
        println!("--- end preview ---");
        return Ok(());
    }

    let parent = target
        .parent()
        .context("skill path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create skill directory {}", parent.display()))?;

    if let Some(backup) = backup_if_exists(&target)? {
        println!("  backup: {}", backup.display());
    }

    std::fs::write(&target, SKILL_BODY)
        .with_context(|| format!("write skill file {}", target.display()))?;
    println!("Installed skill at {}", target.display());
    Ok(())
}

/// True iff the skill file is already installed at the global path.
pub fn is_installed() -> bool {
    skill_path().exists()
}

pub fn skill_path() -> PathBuf {
    home_dir()
        .join(".claude")
        .join("skills")
        .join("agent-tools")
        .join("SKILL.md")
}

fn backup_if_exists(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let existing = std::fs::read_to_string(path)
        .with_context(|| format!("read existing skill {}", path.display()))?;
    let backup = backup_path(path);
    std::fs::write(&backup, existing)
        .with_context(|| format!("write backup to {}", backup.display()))?;
    Ok(Some(backup))
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_valid_frontmatter_and_key_sections() {
        assert!(SKILL_BODY.starts_with("---\n"));
        // Name field drives discovery; must exist.
        assert!(SKILL_BODY.contains("name: agent-tools"));
        // Description is the always-loaded steering field; must mention the
        // disabled native task tools so Claude knows not to call them.
        assert!(SKILL_BODY.contains("disabled"));
        assert!(SKILL_BODY.contains("TaskCreate"));
        // Allowed-tools declaration must grant agent-tools + memory without
        // per-call prompting.
        assert!(SKILL_BODY.contains("allowed-tools: Bash(agent-tools *), Bash(memory *)"));
        // Body must cover the four capability domains.
        assert!(SKILL_BODY.contains("Code Exploration"));
        assert!(SKILL_BODY.contains("Task Board"));
        assert!(SKILL_BODY.contains("Comms"));
        assert!(SKILL_BODY.contains("Memory"));
    }

    #[test]
    fn backup_path_appends_bak() {
        let p = PathBuf::from("/tmp/foo/SKILL.md");
        assert_eq!(backup_path(&p), PathBuf::from("/tmp/foo/SKILL.md.bak"));
    }

    #[test]
    fn skill_path_lands_in_claude_skills() {
        let p = skill_path();
        assert!(p.ends_with(".claude/skills/agent-tools/SKILL.md"));
    }
}
