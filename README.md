# agent-tools

Token-efficient, cross-platform tools for AI coding agents. Provides symbol extraction, directory trees, file indexing, and cross-platform file operations — exposed as both a **CLI** and an **MCP stdio server**.

## Why

AI coding agents' built-in tools have gaps when working with large codebases:

- **Bash assumes Unix** — breaks on Windows constantly
- **`ls`/`tree` waste tokens** — permissions, ownership, decorations you don't need
- **No symbol extraction** — reading a 500KB file to get one function destroys context
- **No file indexing** — every search is a cold filesystem walk

`agent-tools` fixes all of these with pure Rust, zero runtime dependencies.

## Installation

### Quick Install (recommended)

**Linux / macOS:**

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

This builds in release mode and copies both `agent-tools` (CLI) and `agent-tools-mcp` (MCP server) to the specified path.

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
  cp        Copy a file or directory
  mv        Move a file or directory
  mkdir     Create directories recursively
  rm        Remove a file or directory
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
```

## Agent Directives

Add the appropriate block below to your agent's global instructions file to enable CLI-based tool usage.

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

# File ops — cross-platform copy, move, mkdir, remove
/opt/agentic/bin/agent-tools cp <src> <dst>
/opt/agentic/bin/agent-tools mv <src> <dst>
/opt/agentic/bin/agent-tools mkdir <path>
/opt/agentic/bin/agent-tools rm <path>
```
</code_exploration_protocol>
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

# File ops — cross-platform copy, move, mkdir, remove
/opt/agentic/bin/agent-tools cp <src> <dst>
/opt/agentic/bin/agent-tools mv <src> <dst>
/opt/agentic/bin/agent-tools mkdir <path>
/opt/agentic/bin/agent-tools rm <path>
```
</code_exploration_protocol>
````

## Usage — MCP Server (Alternative)

If your AI agent supports MCP, you can also register agent-tools as an MCP stdio server:

```bash
claude mcp add -s user agent-tools -- /opt/agentic/bin/agent-tools-mcp
```

Once registered, the following MCP tools become available:

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
  agent-cli/        CLI binary (agent-tools)
  agent-mcp/        MCP stdio server (agent-tools-mcp)
```

Index data is stored centrally, with a two-tier resolution:

| Priority | Location | Scope |
|----------|----------|-------|
| 1 (highest) | `~/.agent-tools/<hash>/` | Per-user override |
| 2 | `/opt/agentic/tools/<hash>/` (Unix) or `%USERPROFILE%\.agentic\tools\<hash>\` (Windows) | Global / shared |

If the user-level directory (`~/.agent-tools/<hash>`) exists for a project, it takes precedence. Otherwise the global directory is used. For new projects, the global directory is preferred when it exists and is writable; otherwise the user-level directory is used automatically.

The `<hash>` is a blake3 digest of the normalized git remote origin URL (e.g., `github.com/nitecon/agent-tools.git`). For non-git directories, the hash is derived from the absolute path. This keeps index data out of your project tree (no `.gitignore` needed) and enables future cross-machine sync.
