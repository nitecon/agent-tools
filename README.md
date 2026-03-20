# claude-tools

Token-efficient, cross-platform tools for Claude Code. Provides symbol extraction, directory trees, file indexing, and cross-platform file operations — exposed as both a CLI and an MCP stdio server.

## Why

Claude Code's built-in tools have gaps when working with large codebases:

- **Bash assumes Unix** — breaks on Windows constantly
- **`ls`/`tree` waste tokens** — permissions, ownership, decorations you don't need
- **No symbol extraction** — reading a 500KB file to get one function destroys context
- **No file indexing** — every search is a cold filesystem walk

`claude-tools` fixes all of these with pure Rust, zero runtime dependencies.

## Building & Installing

**Prerequisites:** [Rust toolchain](https://rustup.rs/) (stable)

### Build only

```bash
# Linux / macOS
./build.sh

# Windows
build.bat
```

Binaries land in `target/release/`.

### Build and install to a directory

```bash
# Linux / macOS — copies to /usr/local/bin
./build.sh /usr/local/bin

# Linux / macOS — copies to ~/bin
./build.sh ~/bin

# Windows — copies to C:\Tools
build.bat C:\Tools
```

This builds in release mode and copies both `claude-tools` (CLI) and `claude-tools-mcp` (MCP server) to the specified path.

## Registering the MCP Server with Claude Code

After building, register the MCP server at the user level so it's available in all projects:

```bash
claude mcp add -s user claude-tools -- /path/to/claude-tools-mcp
```

Replace `/path/to/claude-tools-mcp` with the actual path to the binary. Examples:

```bash
# If you installed to /usr/local/bin
claude mcp add -s user claude-tools -- /usr/local/bin/claude-tools-mcp

# If you installed to ~/bin
claude mcp add -s user claude-tools -- ~/bin/claude-tools-mcp

# Windows — if you installed to C:\Tools
claude mcp add -s user claude-tools -- C:\Tools\claude-tools-mcp.exe

# Windows — using the build output directly
claude mcp add -s user claude-tools -- C:\path\to\claude-tools\target\release\claude-tools-mcp.exe
```

Once registered, the following tools become available to Claude Code:

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

## CLI Usage

```
claude-tools <COMMAND>

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
claude-tools tree
claude-tools tree src/ --depth 5 --max-files 30

# List directory contents
claude-tools list
claude-tools list src/ --sizes

# Extract a single function from a file
claude-tools symbol ProcessDamage --file src/DamageSystem.cpp

# List all symbols in a file
claude-tools symbols src/main.rs

# Build the project index (files + symbols)
claude-tools index

# Search symbols across the project
claude-tools search MyClass
claude-tools search handle --type fn

# Search files by name
claude-tools search config --type file

# Project overview
claude-tools summary
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
  claude-core/       Shared types, error handling, path normalization
  claude-fs/         Tree view, directory listing, file operations
  claude-symbols/    Tree-sitter parsing, symbol extraction, SQLite index
  claude-search/     File indexing, cached search, project summaries
  claude-cli/        CLI binary (claude-tools)
  claude-mcp/        MCP stdio server (claude-tools-mcp)
```

Index data is stored in `.claude-tools/` at the project root (gitignored).
