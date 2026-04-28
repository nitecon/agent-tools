//! `agent-tools setup skill` — install an agent skill that advertises the
//! `agent-tools` CLI to the model via an auto-loaded frontmatter description.
//!
//! Supported targets:
//!   * Claude Code — `~/.claude/skills/agent-tools/SKILL.md`
//!   * Codex CLI   — `$CODEX_HOME/skills/agent-tools/SKILL.md`
//!     (defaults to `~/.codex/skills/agent-tools/SKILL.md`)
//!
//! Both agents use the same SKILL.md format with YAML frontmatter, and in
//! both cases the `description` line is loaded into the session system
//! prompt (~100 tokens) while the body is only pulled in when the model
//! judges the skill relevant.
//!
//! Detection is by agent home directory — if `~/.claude` exists we install
//! the Claude variant, if `~/.codex` (or `$CODEX_HOME`) exists we install
//! the Codex variant. Both run independently so a machine with one agent
//! isn't punished for not having the other.
//!
//! Idempotent: overwrites SKILL.md in place. Writes a `.bak` sibling before
//! the first destructive overwrite so the user can recover prior content.

use crate::cmd_setup_rules::codex_home;
use agent_comms::config::home_dir;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Install target for the skill file. Each variant carries its own target
/// path resolver and body generator so divergent conventions (Claude's
/// `allowed-tools` field, Codex's lack of built-in task tools) are
/// expressed where they live, not behind runtime branching in `run()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTarget {
    Claude,
    Codex,
}

impl SkillTarget {
    pub const ALL: [SkillTarget; 2] = [SkillTarget::Claude, SkillTarget::Codex];

    pub fn label(self) -> &'static str {
        match self {
            SkillTarget::Claude => "Claude",
            SkillTarget::Codex => "Codex",
        }
    }

    /// Parent directory that must exist for the skill install to make sense.
    /// For Codex we respect `$CODEX_HOME`, falling back to `~/.codex`.
    pub fn agent_home(self) -> PathBuf {
        match self {
            SkillTarget::Claude => home_dir().join(".claude"),
            SkillTarget::Codex => codex_home(),
        }
    }

    /// Final SKILL.md path for this target.
    pub fn skill_path(self) -> PathBuf {
        self.agent_home()
            .join("skills")
            .join("agent-tools")
            .join("SKILL.md")
    }

    /// True when the agent this target serves is installed on this host.
    pub fn detected(self) -> bool {
        self.agent_home().exists()
    }

    /// Rendered SKILL.md body for this target.
    fn body(self) -> &'static str {
        match self {
            SkillTarget::Claude => SKILL_BODY_CLAUDE,
            SkillTarget::Codex => SKILL_BODY_CODEX,
        }
    }
}

/// Claude Code variant. Description explicitly flags the **replacement** of
/// the native Task* tools — Claude's built-in system prompt still says "Use
/// TaskCreate to plan work," so we need an explicit override here for the
/// model to route task tracking through `agent-tools`.
const SKILL_BODY_CLAUDE: &str = r#"---
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
friends will be refused. For complex tasks, add a specification with enough
handoff context for a disconnected or new agent to resume the work; gateway-
backed specifications are more durable than local plan files because they
survive full system crashes.

```bash
agent-tools tasks list                   # TODO + IN PROGRESS for this project
agent-tools tasks get <id>               # full detail + comment thread
agent-tools tasks add --title "..." [--label x] [--description "..."] [--specification "..."]
agent-tools tasks add-delegated --target-project "..." --title "..." --description "..." --specification "..."
agent-tools tasks claim <id>             # take ownership (-> in_progress)
agent-tools tasks release <id>           # drop ownership (-> todo)
agent-tools tasks done <id>              # mark complete
agent-tools tasks comment <id> "<note>"  # append a note
```

## API Context Docs (gateway-backed)

Before searching code for API behavior or implementing API-related work, check
the agent-first API context registry. If no context exists, tell the user that
future agents will work faster if a docs-first file is created, and ask whether
to add one.

```bash
agent-tools docs search "<api-or-workflow>"
agent-tools docs list [--app APP] [--label LABEL] [--kind KIND] [--query Q]
agent-tools docs get <id>
agent-tools docs chunks --query "<api-or-workflow>" [--app APP] [--label LABEL]
agent-tools docs validate --file .agent/api/<app>.yaml
agent-tools docs publish --file .agent/api/<app>.yaml
```

When creating or materially changing API-related files, publish the
corresponding agent API context with `agent-tools docs publish`; for substantial
work, track that publish step as an `agent-tools tasks` subtask or checklist
item.

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

/// Codex variant. Codex has no built-in task tool to disable and doesn't
/// understand Claude's `allowed-tools` frontmatter, so both are dropped. The
/// description is tuned for Codex's skill-loading heuristic, which pulls the
/// full body in on relevance match.
const SKILL_BODY_CODEX: &str = r#"---
name: agent-tools
description: Token-efficient code exploration (tree/symbols/search), gateway-backed task board, cross-project comms, and persistent memory via the `agent-tools` and `memory` CLIs. Prefer for bulk exploration and use `agent-tools tasks` as the persistent task surface instead of an in-memory plan.
metadata:
  short-description: agent-tools + memory CLIs for exploration, tasks, comms, memory
---

# agent-tools

The `agent-tools` CLI and its companion `memory` CLI give Codex sessions a
persistent task board, cross-project messaging, and token-efficient symbol
search that outlives any single conversation.

## Code Exploration (token-efficient)

Prefer symbol-aware tools over raw file reads or shell text search for bulk
exploration. Reading a known file by path is still fine.

```bash
agent-tools tree [path] --depth <n>            # directory tree
agent-tools symbols <file>                     # list a file's symbols
agent-tools symbol <name> --file <path>        # extract a symbol's source
agent-tools search <query> --type symbol|file  # project-wide index search
agent-tools summary [path]                     # compact project overview
agent-tools index --rebuild                    # refresh after large changes
```

## Task Board (persistent across sessions)

Gateway-backed with three statuses, server-enforced ownership, 1h stale-claim
reclaim, and 7d done falloff. Use this as your TODO surface rather than an
in-message plan — tasks survive session turnover. For complex tasks, add a
specification with enough handoff context for a disconnected or new agent to
resume the work; gateway-backed specifications are more durable than local plan
files because they survive full system crashes.

```bash
agent-tools tasks list                   # TODO + IN PROGRESS for this project
agent-tools tasks get <id>               # full detail + comment thread
agent-tools tasks add --title "..." [--label x] [--description "..."] [--specification "..."]
agent-tools tasks add-delegated --target-project "..." --title "..." --description "..." --specification "..."
agent-tools tasks claim <id>             # take ownership (-> in_progress)
agent-tools tasks release <id>           # drop ownership (-> todo)
agent-tools tasks done <id>              # mark complete
agent-tools tasks comment <id> "<note>"  # append a note
```

## API Context Docs (gateway-backed)

Before searching code for API behavior or implementing API-related work, check
the agent-first API context registry. If no context exists, tell the user that
future agents will work faster if a docs-first file is created, and ask whether
to add one.

```bash
agent-tools docs search "<api-or-workflow>"
agent-tools docs list [--app APP] [--label LABEL] [--kind KIND] [--query Q]
agent-tools docs get <id>
agent-tools docs chunks --query "<api-or-workflow>" [--app APP] [--label LABEL]
agent-tools docs validate --file .agent/api/<app>.yaml
agent-tools docs publish --file .agent/api/<app>.yaml
```

When creating or materially changing API-related files, publish the
corresponding agent API context with `agent-tools docs publish`; for substantial
work, track that publish step as an `agent-tools tasks` subtask or checklist
item.

## Comms (gateway-backed messaging)

Project ident auto-derives from the cwd git remote; agent id is
machine-persistent.

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
        // `--print` predates multi-target; keep it deterministic by emitting
        // the Claude variant. Use `--target codex` (or the Codex-specific
        // body dump below) if you need the other one.
        print!("{}", SkillTarget::Claude.body());
        return Ok(());
    }

    let targets: Vec<SkillTarget> = SkillTarget::ALL
        .iter()
        .copied()
        .filter(|t| t.detected())
        .collect();

    if targets.is_empty() {
        anyhow::bail!(
            "No agent home directories detected for skill install. Tried:\n  \
             ~/.claude\n  \
             ~/.codex or $CODEX_HOME\n\
             Install Claude Code or Codex first, then re-run."
        );
    }

    if dry_run {
        for target in &targets {
            let path = target.skill_path();
            println!("--- DRY RUN [{}]: {} ---", target.label(), path.display());
            print!("{}", target.body());
            println!("--- end preview ---");
        }
        return Ok(());
    }

    for target in &targets {
        install_one(*target)?;
    }
    Ok(())
}

fn install_one(target: SkillTarget) -> Result<()> {
    let path = target.skill_path();
    let parent = path
        .parent()
        .context("skill path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create skill directory {}", parent.display()))?;

    if let Some(backup) = backup_if_exists(&path)? {
        println!("  backup: {}", backup.display());
    }

    std::fs::write(&path, target.body())
        .with_context(|| format!("write skill file {}", path.display()))?;
    println!("Installed {} skill at {}", target.label(), path.display());
    Ok(())
}

/// All skill paths that currently exist on disk. Used by the setup menu's
/// probe to report "installed at X, Y" when partial or full.
pub fn installed_paths() -> Vec<PathBuf> {
    SkillTarget::ALL
        .iter()
        .map(|t| t.skill_path())
        .filter(|p| p.exists())
        .collect()
}

/// Expected skill paths across all detected agents. Used by the probe to
/// name the targets we'd write to if setup ran now.
pub fn expected_paths() -> Vec<PathBuf> {
    SkillTarget::ALL
        .iter()
        .copied()
        .filter(|t| t.detected())
        .map(|t| t.skill_path())
        .collect()
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
    fn claude_body_has_valid_frontmatter_and_key_sections() {
        let body = SkillTarget::Claude.body();
        assert!(body.starts_with("---\n"));
        assert!(body.contains("name: agent-tools"));
        // Description must mention the disabled native task tools so Claude
        // knows not to call them.
        assert!(body.contains("disabled"));
        assert!(body.contains("TaskCreate"));
        assert!(body.contains("allowed-tools: Bash(agent-tools *), Bash(memory *)"));
        assert!(body.contains("Code Exploration"));
        assert!(body.contains("Task Board"));
        assert!(body.contains("API Context Docs"));
        assert!(body.contains("agent-tools docs publish"));
        assert!(body.contains("Comms"));
        assert!(body.contains("Memory"));
    }

    #[test]
    fn codex_body_has_valid_frontmatter_and_omits_claude_specifics() {
        let body = SkillTarget::Codex.body();
        assert!(body.starts_with("---\n"));
        assert!(body.contains("name: agent-tools"));
        // Codex has no TaskCreate tool; claiming it's disabled would
        // confuse the model, so the Codex body must not include that wording.
        assert!(!body.contains("disabled"));
        assert!(!body.contains("TaskCreate"));
        // allowed-tools is a Claude-ism; Codex uses plugin-level permissions.
        assert!(!body.contains("allowed-tools:"));
        // Capability coverage still required.
        assert!(body.contains("Code Exploration"));
        assert!(body.contains("Task Board"));
        assert!(body.contains("API Context Docs"));
        assert!(body.contains("agent-tools docs publish"));
        assert!(body.contains("Comms"));
        assert!(body.contains("Memory"));
    }

    #[test]
    fn backup_path_appends_bak() {
        let p = PathBuf::from("/tmp/foo/SKILL.md");
        assert_eq!(backup_path(&p), PathBuf::from("/tmp/foo/SKILL.md.bak"));
    }

    #[test]
    fn claude_skill_path_lands_in_claude_skills() {
        let p = SkillTarget::Claude.skill_path();
        assert!(p.ends_with(".claude/skills/agent-tools/SKILL.md"));
    }

    #[test]
    fn codex_skill_path_lands_in_codex_skills() {
        // When CODEX_HOME is unset the path falls back to ~/.codex.
        let prev = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");
        let p = SkillTarget::Codex.skill_path();
        assert!(p.ends_with(".codex/skills/agent-tools/SKILL.md"));
        if let Some(v) = prev {
            std::env::set_var("CODEX_HOME", v);
        }
    }
}
