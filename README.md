# agent-tools

Token-efficient, cross-platform toolkit for AI coding agents. Provides symbol extraction, directory trees, file indexing, cross-platform file operations, and optional gateway communication — exposed as a **CLI**, **MCP stdio server**, and **sync CLI**.

## Why

AI coding agents' built-in tools have gaps when working with large codebases:

- **Bash assumes Unix** — breaks on Windows constantly
- **`ls`/`tree` waste tokens** — permissions, ownership, decorations you don't need
- **No symbol extraction** — reading a 500KB file to get one function destroys context
- **No file indexing** — every search is a cold filesystem walk

`agent-tools` fixes all of these with pure Rust, zero runtime dependencies.

## Installation

### Quick Install (recommended)

**macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-tools/refs/heads/main/install-macos.sh | sudo bash
```

**Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-tools/refs/heads/main/install.sh | sudo bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/nitecon/agent-tools/refs/heads/main/install.ps1 | iex
```

### Build from Source

**Prerequisites:** [Rust toolchain](https://rustup.rs/) (stable)

```bash
# Linux / macOS
./build.sh /usr/local/bin

# Windows
build.bat C:\Tools
```

This builds in release mode and copies `agent-tools` (CLI), `agent-tools-mcp` (MCP server), and `agent-sync` (sync CLI) to the specified path.

## Auto-Update

Both binaries check for new releases automatically (at most once per hour). When an update is available, it downloads and replaces the binaries in-place — symlinks are preserved since the update writes to the real binary location (e.g., `/opt/agentic/bin/`).

```bash
# Manual update check
agent-tools update

# Check current version
agent-tools version

# Disable auto-updates
export AGENT_TOOLS_NO_UPDATE=1
```

The rate-limit marker is stored at `~/.agentic/.agent-tools-update-check` and persists across reboots. Both the CLI and MCP server share the same marker, so an update from either resets the cooldown for both.

## Usage — CLI (Primary)

The primary way to use agent-tools is via the CLI binary, called directly from your AI agent's shell. Add the directive block below to your agent's system instructions to enable it.

```
agent-tools <COMMAND>

Commands:
  tree      Token-efficient directory tree view
  list      Smart directory listing
  symbol    Extract a symbol's source code by name
  symbols   List all symbols in a file
  search    Search the project-wide symbol index
  index     Build or update the project index
  summary   Show a compact project summary
  doc       Markdown reading helpers (outline / section extraction)
  cp        Copy a file or directory
  mv        Move a file or directory
  mkdir     Create directories recursively
  rm        Remove a file or directory
  comms     Send / receive messages via the gateway
  tasks     Per-project task board via the gateway
  patterns  Global pattern library and .patterns tracking via the gateway
  setup     Setup and configuration commands
  init      Configure gateway connection (alias for `setup gateway`)
  update    Check for updates and install the latest version
  version   Print version information
```

### Examples

```bash
# Compact tree view (default depth 3, max 20 files per dir)
agent-tools tree
agent-tools tree src/ --depth 5 --max-files 30

# List directory contents
agent-tools list
agent-tools list src/ --sizes

# Extract a single function from a file
agent-tools symbol ProcessDamage --file src/DamageSystem.cpp

# List all symbols in a file
agent-tools symbols src/main.rs

# Build the project index (files + symbols)
agent-tools index

# Search symbols across the project
agent-tools search MyClass
agent-tools search handle --type fn

# Search files by name
agent-tools search config --type file

# Project overview
agent-tools summary

# Markdown — show only the headings of a doc (no body — minimal tokens)
agent-tools doc outline docs/Architecture.md

# Markdown — pull just one section by heading name (case-insensitive)
agent-tools doc section docs/Architecture.md "Data Flow"
```

### Communication tools (CLI)

Once the gateway is configured (see [Gateway Integration](#gateway-integration)), agents can send and receive messages directly from the shell — no MCP required. The project identity is derived automatically from the current working directory (git `origin` URL, or the canonical path for non-git dirs), and a machine-stable agent id is generated once at `~/.agentic/agent-tools/agent-id` and reused on every subsequent call. Agents never pass either value explicitly.

```bash
# Send a message to the project's channel (auto-derives ident + agent-id)
agent-tools comms send "build green on main"

# Poll unread messages for this project + agent
agent-tools comms recv

# Acknowledge a message so it stops reappearing
agent-tools comms confirm 1234

# Threaded reply
agent-tools comms reply 1234 "fixed — see commit abc123"

# Signal that work is in progress on a message
agent-tools comms action 1234 "deploying to staging"

# Print what we'd send (debug)
agent-tools comms whoami
agent-tools comms whoami --json
```

All subcommands accept `--json` for machine-readable output, and `--agent-id <id>` when a per-invocation override is needed (e.g. running multiple distinct agents on one machine).

#### Structured rendering (Discord embeds & friends)

`send`, `reply`, and `action` now publish a structured payload to the gateway so messages render as a tidy Discord embed (and as clean Markdown on any other channel) instead of a single run-on line. The positional `<content>` argument still populates the message body — three new optional flags let you control the rest:

| Flag | Purpose | Default |
|------|---------|---------|
| `--subject <text>` | One-line headline used as the embed title | First non-empty line of the body, capped at 80 chars |
| `--hostname <host>` | Originating host shown in the byline | Local hostname (`gethostname`); pass `--hostname ""` to opt out and let the gateway fall back to the agent-id |
| `--event-at <time>` | Event time stamped on the embed; accepts RFC3339 (`2026-04-21T19:18:00Z`) or a bare epoch-ms integer | Gateway receipt time |

Examples:

```bash
# Fully structured send — bold subject, body in a code block, host + timestamp byline
agent-tools comms send \
  --subject "push.main on nitecon/agent-gateway · v0.9.5" \
  "commit 8814ff2 by Will Hattingh.

Two independent changes:
1. Versioning fix
2. ndesign theme"

# Threaded reply with an explicit timestamp (e.g. replaying a historical event)
agent-tools comms reply 1234 \
  --subject "deploy: prod rollout complete" \
  --event-at 2026-04-21T19:18:00Z \
  "All replicas healthy after 4m23s."

# Action signal with a custom subject (skips the auto `[ACTION] ` prefix)
agent-tools comms action 1234 \
  --subject "[CLAIM] migration 0042" \
  "running backfill in batches of 5k rows"
```

The gateway accepts either the new structured payload or the legacy single-`content` shape, so older `agent-tools` builds and the new structured CLI can coexist against the same gateway during rollout.

### Patterns (CLI)

The gateway-backed pattern library stores durable organization-wide guidance.
Repositories can opt into known guidance with a `.patterns` file in the current
working directory. The file is intentionally minimal: gateway pattern ids as
keys, with optional paths as values, and no comments.

```yaml
01JZEXAMPLEPATTERNID:
  - src/main.rs
  - crates/agent-cli/src/cmd_patterns.rs
```

```bash
# Search approved guidance before implementing established practices
agent-tools patterns search "systemd service" --version latest --state active

# Fetch the pattern body only; comments are opt-in review state
agent-tools patterns get 01JZEXAMPLEPATTERNID

# Iterating on a pattern: fetch both the body and comments first
agent-tools patterns get 01JZEXAMPLEPATTERNID
agent-tools patterns comments 01JZEXAMPLEPATTERNID
agent-tools patterns update 01JZEXAMPLEPATTERNID --body-file /tmp/pattern.md

# Validate $PWD/.patterns. Superseded patterns create migration tasks.
agent-tools patterns check

# Record use of a pattern with relevant paths
agent-tools patterns use 01JZEXAMPLEPATTERNID --path src/main.rs

# Create draft guidance from a markdown file
agent-tools patterns create \
  --title "Deploying Eventic Applications" \
  --version draft \
  --state active \
  --label deploy \
  --body-file pattern.md
```

## Agent Directives

Add the appropriate block below to your agent's global instructions file to enable CLI-based tool usage.

### Automated install: `agent-tools setup rules`

The fastest way to keep your agent rule files in sync with the latest agent-tools protocols is the built-in installer:

```bash
# Detect installed agents by home directory (~/.claude, ~/.gemini,
# ~/.codex or $CODEX_HOME, ~/.config/codex) and prompt for which to
# update. If the rule file doesn't exist yet it's created on the fly.
agent-tools setup rules

# Update every detected file without prompting:
agent-tools setup rules --all

# Target a specific file (skip detection):
agent-tools setup rules --target ~/.claude/CLAUDE.md

# Preview without writing:
agent-tools setup rules --dry-run --target ~/.claude/CLAUDE.md

# Dump the rules block to stdout (useful for piping or review):
agent-tools setup rules --print
```

Detection is by **agent home directory** rather than rule-file existence, so a fresh Codex install (with `~/.codex/` present but no `AGENTS.md`) still picks up the block on the first setup run. For Codex, `$CODEX_HOME` is honored with a fallback to `~/.codex`, matching Codex's own skill-install convention.

The injected block is wrapped in `<agent-tools-rules>...</agent-tools-rules>` markers; re-runs replace the block in place rather than duplicating it. A `<file>.bak` sibling is written before each destructive modification so changes are recoverable (brand-new files skip the backup to avoid zero-byte `.bak` clutter).

When the gateway is configured, the injected block includes code-exploration + comms + tasks + patterns. When the gateway is not configured, only the code-exploration section is injected (with a notice on stderr) — agents on unconfigured machines still get the symbol-aware tooling directives without false references to gateway-only surfaces.

### Automated install: `agent-tools setup skill`

Installs the agent-tools skill file (advertises the CLI to the session so the model picks it up automatically):

- Claude Code — `~/.claude/skills/agent-tools/SKILL.md` (includes `allowed-tools` frontmatter and flags the disabled native `TaskCreate*` tools).
- Codex CLI — `$CODEX_HOME/skills/agent-tools/SKILL.md` (defaults to `~/.codex/skills/agent-tools/SKILL.md`). Body is tuned for Codex: no `allowed-tools` frontmatter (Codex doesn't read it), no references to Claude-only disabled task tools.

```bash
agent-tools setup skill              # installs to every detected agent
agent-tools setup skill --dry-run    # preview per-target output
agent-tools setup skill --print      # dump the Claude body to stdout
```

### Automated install: `agent-tools setup perms`

Writes `TaskCreate*` / `TodoWrite` denies into `~/.claude/settings.json` so Claude-Code sessions can't route around the gateway-backed task board. **Claude-only** — Codex has no equivalent tool-deny facility in its config, so running `setup perms` is a no-op for Codex setups.

If you'd rather inspect and paste the content yourself, the manual blocks below are kept in sync with the installer's output.

### CLAUDE.md / Cline / Aider

Add this to your `CLAUDE.md` (or equivalent system instructions file):

````markdown
<code_exploration_protocol>
## Code Exploration Tools (MANDATORY)

**Binary:** `/opt/agentic/bin/agent-tools` — call directly via Bash (do NOT use MCP or skills for code exploration during normal workflow).

**The "Explore First" Rule:** Before modifying any file, use symbol-aware tools to understand the code. Prefer symbol extraction over full file reads to minimize token usage.

### 1. Pre-Task: Code Discovery
Before writing a single line of code, explore the relevant code.
- **Goal**: Understand the structure, symbols, and dependencies of the target code.
- **Action**: Use `tree`, `symbols`, and `symbol` to build a mental model before making changes.

### 2. Symbol-Aware Exploration
Prefer symbol-level tools over raw file reads whenever possible.
- **Discovery**: Use `tree` to understand structure; `summary` for the "big picture."
- **Analysis**: Use `symbols` to list a file's API; `symbol` to read specific implementation.
- **Search**: Use `search` (symbol-index) instead of `grep` (raw text) whenever possible.
- **Docs**: Use `doc outline` to scan a markdown file's headings, then `doc section "<heading>"` to read just the relevant part — never `cat` a long doc.

### CLI Commands (run via Bash):

```bash
# Tree — token-efficient directory tree (respects .gitignore)
/opt/agentic/bin/agent-tools tree [path] --depth <n> --max-files <n>

# List — smart directory listing (dirs first, minimal output)
/opt/agentic/bin/agent-tools list [path] --sizes

# Symbol — extract a symbol's complete source code by name
/opt/agentic/bin/agent-tools symbol <name> --file <path> --type <kind>

# Symbols — list all symbols in a file
/opt/agentic/bin/agent-tools symbols <file> --type <kind>

# Search — search the project-wide symbol index
/opt/agentic/bin/agent-tools search <query> --type symbol|file --limit <n>

# Index — build or update the project index
/opt/agentic/bin/agent-tools index [path] --rebuild

# Summary — compact project overview
/opt/agentic/bin/agent-tools summary [path]

# Doc outline — list only the headings of a markdown file (minimal tokens)
/opt/agentic/bin/agent-tools doc outline <file>

# Doc section — extract one section by heading text (case-insensitive)
/opt/agentic/bin/agent-tools doc section <file> "<heading>"

# File ops — cross-platform copy, move, mkdir, remove
/opt/agentic/bin/agent-tools cp <src> <dst>
/opt/agentic/bin/agent-tools mv <src> <dst>
/opt/agentic/bin/agent-tools mkdir <path>
/opt/agentic/bin/agent-tools rm <path>
```
</code_exploration_protocol>

<comms_protocol>
## Communication Tools (MANDATORY — CLI only)

**Binary:** `/opt/agentic/bin/agent-tools comms` — call directly via Bash. **Do NOT use the MCP comms tools; they are deprecated in favor of this CLI.**

**Zero-config identity:** The project ident is auto-derived from the git remote of the current working directory (normalized), or the canonical path for non-git dirs. The agent id is persisted at `~/.agentic/agent-tools/agent-id` and reused across invocations. You never pass either value unless explicitly overriding.

### The "Recv First / Confirm Always" Rule
1. Run `comms recv` at the start of a work session to pick up pending messages.
2. For every message returned, run `comms confirm <id>` once it has been handled — otherwise it will reappear on the next `recv`.
3. Use `comms action <id>` when claiming a task, `comms reply <id>` when reporting a result.

### CLI Commands (run via Bash):

```bash
# Send a message to the project's channel (auto-derives ident + agent-id)
/opt/agentic/bin/agent-tools comms send "<body>"

# Fetch unread messages for this project + agent
/opt/agentic/bin/agent-tools comms recv [--json]

# Confirm a message is handled (stops it reappearing on recv)
/opt/agentic/bin/agent-tools comms confirm <message_id>

# Threaded reply to a specific message
/opt/agentic/bin/agent-tools comms reply <message_id> "<body>"

# Signal that this agent is actively working on a message
/opt/agentic/bin/agent-tools comms action <message_id> "<what you're doing>"

# Show derived project ident + agent id (debug / verification)
/opt/agentic/bin/agent-tools comms whoami [--json]
```

### Structured rendering flags (send / reply / action)

For Discord embeds (and clean Markdown on Slack / email), enrich any send / reply / action with these optional flags. The positional argument still populates the body:

```bash
--subject "<one-line headline>"     # embed title; defaults to first line of body
--hostname "<host>"                  # byline host; defaults to local hostname (use "" to opt out)
--event-at <RFC3339|epoch-ms>        # event time; defaults to gateway receipt time
```

Use a structured `send` whenever the message has a clear headline + multi-line detail, e.g. a deploy summary, a build failure, or a PR-review verdict — it dramatically improves readability on Discord vs. a single run-on line.

### Notes
- All subcommands accept `--json` for machine-readable output.
- `--agent-id <id>` overrides the persisted agent id for a single invocation (rare — only needed when running multiple distinct agents on one machine).
- The first `send` from a new project auto-registers the channel with the gateway; subsequent calls hit a cached marker and skip the round-trip.
</comms_protocol>
````

### GEMINI.md / Google AI Studio

Add this to your `GEMINI.md` (or equivalent system instructions):

````markdown
<code_exploration_protocol>
## Code Exploration Tools (MANDATORY)

**Binary:** `/opt/agentic/bin/agent-tools` — call directly via shell execution.

**The "Explore First" Rule:** Before modifying any file, use symbol-aware tools to understand the code. Prefer symbol extraction over full file reads to minimize token usage.

### 1. Pre-Task: Code Discovery
Before writing a single line of code, explore the relevant code.
- **Goal**: Understand the structure, symbols, and dependencies of the target code.
- **Action**: Use `tree`, `symbols`, and `symbol` to build a mental model before making changes.

### 2. Symbol-Aware Exploration
Prefer symbol-level tools over raw file reads whenever possible.
- **Discovery**: Use `tree` to understand structure; `summary` for the "big picture."
- **Analysis**: Use `symbols` to list a file's API; `symbol` to read specific implementation.
- **Search**: Use `search` (symbol-index) instead of `grep` (raw text) whenever possible.
- **Docs**: Use `doc outline` to scan a markdown file's headings, then `doc section "<heading>"` to read just the relevant part — never `cat` a long doc.

### CLI Commands (run via shell):

```bash
# Tree — token-efficient directory tree (respects .gitignore)
/opt/agentic/bin/agent-tools tree [path] --depth <n> --max-files <n>

# List — smart directory listing (dirs first, minimal output)
/opt/agentic/bin/agent-tools list [path] --sizes

# Symbol — extract a symbol's complete source code by name
/opt/agentic/bin/agent-tools symbol <name> --file <path> --type <kind>

# Symbols — list all symbols in a file
/opt/agentic/bin/agent-tools symbols <file> --type <kind>

# Search — search the project-wide symbol index
/opt/agentic/bin/agent-tools search <query> --type symbol|file --limit <n>

# Index — build or update the project index
/opt/agentic/bin/agent-tools index [path] --rebuild

# Summary — compact project overview
/opt/agentic/bin/agent-tools summary [path]

# Doc outline — list only the headings of a markdown file (minimal tokens)
/opt/agentic/bin/agent-tools doc outline <file>

# Doc section — extract one section by heading text (case-insensitive)
/opt/agentic/bin/agent-tools doc section <file> "<heading>"

# File ops — cross-platform copy, move, mkdir, remove
/opt/agentic/bin/agent-tools cp <src> <dst>
/opt/agentic/bin/agent-tools mv <src> <dst>
/opt/agentic/bin/agent-tools mkdir <path>
/opt/agentic/bin/agent-tools rm <path>
```
</code_exploration_protocol>

<comms_protocol>
## Communication Tools (MANDATORY — CLI only)

**Binary:** `/opt/agentic/bin/agent-tools comms` — call directly via shell execution. **Do NOT use the MCP comms tools; they are deprecated in favor of this CLI.**

**Zero-config identity:** Project ident is auto-derived from the current working directory (git remote when available, canonical path otherwise); agent id is persisted per machine at `~/.agentic/agent-tools/agent-id`. No explicit identity arguments are needed.

### The "Recv First / Confirm Always" Rule
1. Run `comms recv` at the start of a work session to pick up pending messages.
2. For every message returned, run `comms confirm <id>` once it has been handled — otherwise it will reappear on the next `recv`.
3. Use `comms action <id>` when claiming a task, `comms reply <id>` when reporting a result.

### CLI Commands (run via shell):

```bash
/opt/agentic/bin/agent-tools comms send "<body>"
/opt/agentic/bin/agent-tools comms recv [--json]
/opt/agentic/bin/agent-tools comms confirm <message_id>
/opt/agentic/bin/agent-tools comms reply <message_id> "<body>"
/opt/agentic/bin/agent-tools comms action <message_id> "<what you're doing>"
/opt/agentic/bin/agent-tools comms whoami [--json]
```

### Structured rendering flags (send / reply / action)

For Discord embeds (and clean Markdown on Slack / email), enrich any send / reply / action with these optional flags:

```bash
--subject "<one-line headline>"     # embed title; defaults to first line of body
--hostname "<host>"                  # byline host; defaults to local hostname (use "" to opt out)
--event-at <RFC3339|epoch-ms>        # event time; defaults to gateway receipt time
```

Prefer a structured `send` whenever the message has a clear headline and multi-line detail; the body lands inside a code block so formatting is preserved.

### Notes
- All subcommands accept `--json` for machine-readable output.
- `--agent-id <id>` overrides the persisted agent id for a single invocation (rare).
- The first `send` from a new project auto-registers the channel with the gateway; subsequent calls use a cached marker.
</comms_protocol>
````

## Usage — MCP Server (Alternative)

If your AI agent supports MCP, you can also register agent-tools as an MCP stdio server:

```bash
# Code tools only (no gateway needed)
claude mcp add -s user agent-tools -- /opt/agentic/bin/agent-tools-mcp

# Code tools + communication tools (requires gateway)
claude mcp add -s user agent-tools -- /opt/agentic/bin/agent-tools-mcp --url https://your-gateway-host:7913
```

The `--url` flag connects the MCP server to your [agent-gateway](#gateway-integration) instance, enabling the communication tools (`set_identity`, `send_message`, `get_messages`, `confirm_read`, `reply_to`, `taking_action_on`) and sync tools (`sync_push`, `sync_pull`, `sync_list`, `sync_delete`, `sync_all`). Without it, only the code exploration tools are available.

Once registered, the following MCP tools become available:

**Code tools** (always available):

| MCP Tool | Description |
|----------|-------------|
| `tree` | Token-efficient directory tree (respects .gitignore) |
| `list` | Smart directory listing (dirs first, no bloat) |
| `file_ops` | Cross-platform copy, move, mkdir, remove |
| `extract_symbol` | Get a symbol's source code by name |
| `list_symbols` | List all symbols in a file |
| `search_symbols` | Search the project-wide symbol index |
| `build_index` | Build/update file and symbol indexes |
| `find_files` | Query the file index |
| `project_summary` | Compact project overview |
| `get_doc_outline` | List only the headings of a markdown file (level + text + line) |
| `get_doc_section` | Extract one section of a markdown file by heading (case-insensitive) |

**Communication tools** (require [gateway setup](#gateway-integration)) — **deprecated; prefer `agent-tools comms` CLI**:

| MCP Tool | Description |
|----------|-------------|
| `set_identity` | Set the project identity and optional agent ID for this session |
| `send_message` | Send a message to the user via the project's channel |
| `get_messages` | Poll for unread messages (per-agent when agent ID is set) |
| `confirm_read` | Acknowledge a message (per-agent scoping supported) |
| `reply_to` | Send a threaded reply to a specific message |
| `taking_action_on` | Signal that the agent is actively working on a message |

> These MCP comms tools are retained for backward compatibility but are no longer the recommended path. The `agent-tools comms` CLI auto-derives project identity from the working directory and the machine-persistent agent id file, so agents don't need to call `set_identity` first. New integrations should use the CLI.

**Sync tools** (require [gateway setup](#gateway-integration)):

| MCP Tool | Description |
|----------|-------------|
| `sync_push` | Push a skill, command, or agent to the gateway |
| `sync_pull` | Pull a resource from the gateway to the local machine |
| `sync_list` | List skills, commands, and agents stored on the gateway |
| `sync_delete` | Delete a resource from the gateway by name |
| `sync_all` | Bidirectional sync of all local and remote resources |

## Gateway Integration

The MCP server includes 6 communication tools (`set_identity`, `send_message`, `get_messages`, `confirm_read`, `reply_to`, `taking_action_on`), 5 sync tools (`sync_push`, `sync_pull`, `sync_list`, `sync_delete`, `sync_all`), and the `agent-sync` binary for sharing skills, commands, and agents across machines. These features require a running [agent-gateway](https://github.com/nitecon/agent-gateway) instance.

**If you only need code exploration tools, no gateway setup is needed.** The code tools (tree, symbols, search, etc.) work immediately with no configuration.

### Prerequisites

1. **Install and configure the gateway** — follow the [agent-gateway setup guide](https://github.com/nitecon/agent-gateway). The gateway is a single persistent service that handles Discord, Slack, email, and other channel integrations.

2. **Configure the client connection:**

   ```bash
   # Interactive setup — prompts for gateway URL, API key, etc.
   agent-tools setup gateway
   ```

   This writes `~/.agentic/agent-tools/gateway.conf`:

   ```
   GATEWAY_URL=http://your-gateway-host:7913
   GATEWAY_API_KEY=your-shared-secret
   GATEWAY_TIMEOUT_MS=5000
   ```

   You can also set these via environment variables (`GATEWAY_URL`, `GATEWAY_API_KEY`) or CLI flags.

3. **Verify the connection** — once configured, the MCP comms tools will connect automatically. Without configuration, they return a helpful error message instead of failing.

### Configuration hierarchy

Config is resolved in this order (highest priority wins):

| Priority | Source |
|----------|--------|
| 1 (highest) | CLI flags (`--url`, `--api-key`) |
| 2 | Environment variables (`GATEWAY_URL`, `GATEWAY_API_KEY`) |
| 3 | User config (`~/.agentic/agent-tools/gateway.conf`) |
| 4 | Global config (`/opt/agentic/agent-tools/gateway.conf`) |

### Syncing skills, commands, and agents

The `agent-sync` CLI manages shared resources on the gateway:

```bash
# Push a skill directory to the gateway
agent-sync skills push ./my-skill/

# Pull all shared resources
agent-sync sync --dir .

# List what's on the gateway
agent-sync skills list
agent-sync commands list
agent-sync agents list
```

## Supported Languages

Symbol extraction (via tree-sitter) supports:

- C / C++ (.c, .h, .cpp, .hpp, .cc, .cxx)
- Rust (.rs)
- Python (.py)
- TypeScript (.ts, .tsx)
- JavaScript (.js, .jsx, .mjs)
- C# (.cs)
- Go (.go)

## Architecture

```
crates/
  agent-core/       Shared types, error handling, path normalization
  agent-fs/         Tree view, directory listing, file operations
  agent-symbols/    Tree-sitter parsing, symbol extraction, SQLite index
  agent-search/     File indexing, cached search, project summaries
  agent-comms/      Gateway client library, config system, sanitization
  agent-updater/    Consolidated self-update mechanism (GitHub releases)
  agent-cli/        CLI binary (agent-tools)
  agent-mcp/        MCP stdio server (agent-tools-mcp) — 22 tools via rmcp
  agent-sync/       Sync CLI binary (agent-sync)
```

Three binaries are produced:

| Binary | Purpose |
|--------|---------|
| `agent-tools` | CLI for direct shell usage (code exploration + file ops) |
| `agent-tools-mcp` | MCP stdio server (code tools + comms tools in one server) |
| `agent-sync` | CLI for syncing skills, commands, and agents with the gateway |

Index data is stored centrally, with a writable-location resolution:

| Priority | Location | Scope |
|----------|----------|-------|
| 1 (highest) | `$AGENT_TOOLS_STATE_DIR/<hash>/` | Explicit override |
| 2 | `~/.agent-tools/<hash>/` | Per-user override |
| 3 | `/opt/agentic/tools/<hash>/` (Unix) or `%USERPROFILE%\.agentic\tools\<hash>\` (Windows) | Global / shared |

If the user-level directory (`~/.agent-tools/<hash>`) exists for a project and is writable, it takes precedence. Otherwise the writable global directory is used. For new projects, the global directory is preferred when it exists and is writable; otherwise the user-level directory is used automatically. If no persistent index database can be opened, `agent-tools` falls back to an in-memory index for that invocation; this is slower because the index is rebuilt on demand, but it keeps sandboxed agents working.

The `<hash>` is a blake3 digest of the normalized git remote origin URL (e.g., `github.com/nitecon/agent-tools.git`). For non-git directories, the hash is derived from the absolute path. This keeps index data out of your project tree (no `.gitignore` needed) and enables future cross-machine sync.

## Related Projects

The agentic tooling suite:

| Project | Purpose | Install scope |
|---------|---------|---------------|
| **[agent-tools](https://github.com/nitecon/agent-tools)** (this repo) | Code exploration, file ops, comms client, sync CLI | Every dev machine |
| **[agent-gateway](https://github.com/nitecon/agent-gateway)** | Communication hub — Discord, Slack, email channels + skill storage | Deploy once (server) |
| **[agent-memory](https://github.com/nitecon/agent-memory)** | Persistent memory — semantic search, context retrieval | Every dev machine |

Install all client-side tools for a complete agent toolkit:

```bash
# Install agent-tools (code exploration + comms client + sync)
# macOS:
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-tools/refs/heads/main/install-macos.sh | sudo bash
# Linux:
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-tools/refs/heads/main/install.sh | sudo bash

# Install agent-memory (persistent memory)
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-memory/refs/heads/main/install.sh | sudo bash

# Optional: install the gateway (deploy on one server)
curl -fsSL https://raw.githubusercontent.com/nitecon/agent-gateway/main/install-gateway.sh | sudo bash
```

All client tools follow the same patterns: installed to `/opt/agentic/bin/`, symlinked to `/usr/local/bin/`, auto-updating, and designed to be called directly from agent system instructions rather than requiring MCP registration.
