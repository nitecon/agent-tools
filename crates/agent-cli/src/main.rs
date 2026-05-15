mod cmd_comms;
mod cmd_docs;
mod cmd_docs_artifacts;
mod cmd_gateway_context;
mod cmd_patterns;
mod cmd_read;
mod cmd_setup_menu;
mod cmd_setup_perms;
mod cmd_setup_rules;
mod cmd_setup_skill;
mod cmd_tasks;
mod cmd_text;
mod nudge;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "agent-tools",
    about = "Token-efficient tools for AI coding agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Token-efficient directory tree view
    Tree {
        /// Directory to display (default: current directory)
        path: Option<PathBuf>,
        /// Maximum depth (default: 3)
        #[arg(short, long, default_value = "3")]
        depth: usize,
        /// Maximum files per directory before truncation (default: 20)
        #[arg(short, long, default_value = "20")]
        max_files: usize,
    },

    /// Smart directory listing
    List {
        /// Directory to list (default: current directory)
        path: Option<PathBuf>,
        /// Show file sizes
        #[arg(short, long)]
        sizes: bool,
        /// Show hidden files
        #[arg(short = 'a', long)]
        all: bool,
    },

    /// Read a UTF-8 file, optionally limited to a 1-based line range
    Read {
        /// File to read
        file: PathBuf,
        /// Inclusive line or line range: N, START:END, START:, :END, or START,END
        #[arg(long, value_name = "RANGE")]
        lines: Option<String>,
    },

    /// Extract a symbol's source code by name
    Symbol {
        /// Symbol name to extract
        name: String,
        /// File to search in (if not specified, searches index)
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Symbol type filter (function, class, struct, etc.)
        #[arg(short = 't', long = "type")]
        kind: Option<String>,
    },

    /// List all symbols in a file
    Symbols {
        /// File to list symbols from
        file: PathBuf,
        /// Symbol type filter
        #[arg(short = 't', long = "type")]
        kind: Option<String>,
    },

    /// Search the project-wide symbol index
    Search {
        /// Search query
        query: String,
        /// Search type: "symbol" or "file"
        #[arg(short = 't', long = "type", default_value = "symbol")]
        search_type: String,
        /// File pattern filter
        #[arg(short, long)]
        file: Option<String>,
        /// Maximum results (default: 20)
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Build or update the project index
    Index {
        /// Directory to index (default: current directory)
        path: Option<PathBuf>,
        /// Force rebuild (ignore cached data)
        #[arg(long)]
        rebuild: bool,
    },

    /// Show a compact project summary
    Summary {
        /// Directory to summarize (default: current directory)
        path: Option<PathBuf>,
    },

    /// Portable, deterministic text search
    #[command(
        after_help = "Detailed docs: docs/grep-sed.md\nStable contract: docs/grep-sed-contract.md"
    )]
    Grep {
        /// Regex pattern to search for, unless --fixed is set
        pattern: Option<String>,
        /// Files or directories to search (default: current directory)
        paths: Vec<PathBuf>,
        /// Treat the pattern as a literal string
        #[arg(long, conflicts_with = "regex")]
        fixed: bool,
        /// Treat the pattern as a Rust regex (default)
        #[arg(long)]
        regex: bool,
        /// Read the pattern from a UTF-8 file
        #[arg(long, value_name = "FILE", conflicts_with = "pattern")]
        pattern_file: Option<PathBuf>,
        /// Case-insensitive matching
        #[arg(short = 'i', long)]
        ignore_case: bool,
        /// Include only paths matching this glob-like pattern; repeatable
        #[arg(long = "include", value_name = "GLOB")]
        include_globs: Vec<String>,
        /// Exclude paths matching this glob-like pattern; repeatable
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude_globs: Vec<String>,
        /// Include paths matching this glob-like pattern; repeatable
        #[arg(long = "glob", value_name = "GLOB")]
        glob_globs: Vec<String>,
        /// Show this many lines of leading and trailing context
        #[arg(short = 'C', long, default_value_t = 0)]
        context: usize,
        /// Show this many lines of leading context
        #[arg(short = 'B', long = "before-context")]
        before_context: Option<usize>,
        /// Show this many lines of trailing context
        #[arg(short = 'A', long = "after-context")]
        after_context: Option<usize>,
        /// Print per-file match counts instead of match records
        #[arg(short = 'c', long = "count")]
        count_only: bool,
        /// Print only files with at least one match
        #[arg(short = 'l', long = "files-with-matches")]
        files_with_matches: bool,
        /// Print only files without a match
        #[arg(short = 'L', long = "files-without-match")]
        files_without_match: bool,
        /// Alias for --files-with-matches with path-match records
        #[arg(long = "paths-only")]
        paths_only: bool,
        /// Emit NUL-delimited raw paths for path-family modes
        #[arg(short = '0', long = "null")]
        null: bool,
        /// Maximum output records before a resume hint is emitted
        #[arg(long, default_value_t = 1000)]
        limit: usize,
        /// Skip this many output records before rendering
        #[arg(long, default_value_t = 0)]
        skip: usize,
        /// Deferred v1 feature: null-delimited input path lists
        #[arg(long = "files0-from", value_name = "FILE")]
        files0_from: Option<PathBuf>,
        /// Deferred v1 feature: stdin-sourced pattern payloads
        #[arg(long = "pattern-stdin")]
        pattern_stdin: bool,
    },

    /// Portable, deterministic stream-editor preview/rewrite
    #[command(
        after_help = "Detailed docs: docs/grep-sed.md\nStable contract: docs/grep-sed-contract.md"
    )]
    Sed {
        /// Sed-like substitution expression, e.g. `s/foo/bar/g`. Mutually exclusive with --regex/--fixed.
        expression: Option<String>,
        /// Files or directories to operate on (default: current directory)
        paths: Vec<PathBuf>,
        /// Provide pattern and replacement explicitly via argv (regex mode)
        #[arg(long, value_name = "PATTERN", allow_hyphen_values = true)]
        regex: Option<String>,
        /// Regex replacement, expanded with Rust `regex::Captures::expand`
        #[arg(long, value_name = "REPLACEMENT", allow_hyphen_values = true)]
        replace: Option<String>,
        /// Provide a fixed (literal) old payload via argv. Pairs with the next positional as the new payload.
        #[arg(long = "fixed", num_args = 2, value_names = ["OLD", "NEW"], allow_hyphen_values = true)]
        fixed: Vec<String>,
        /// Read the pattern from a UTF-8 file
        #[arg(long, value_name = "FILE")]
        pattern_file: Option<PathBuf>,
        /// Read the replacement from a UTF-8 file
        #[arg(long, value_name = "FILE", conflicts_with = "replace")]
        replacement_file: Option<PathBuf>,
        /// Case-insensitive matching (mirrors the sed-like `i` flag)
        #[arg(short = 'i', long)]
        ignore_case: bool,
        /// Replace all non-overlapping matches per line (mirrors the sed-like `g` flag)
        #[arg(short = 'g', long)]
        global: bool,
        /// Include only paths matching this glob-like pattern; repeatable
        #[arg(long = "include", value_name = "GLOB")]
        include_globs: Vec<String>,
        /// Exclude paths matching this glob-like pattern; repeatable
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude_globs: Vec<String>,
        /// Include paths matching this glob-like pattern; repeatable
        #[arg(long = "glob", value_name = "GLOB")]
        glob_globs: Vec<String>,
        /// Inclusive 1-based line range per file, e.g. `--line 20:60`, `--line 20:`, `--line :60`
        #[arg(long = "line", value_name = "START:END")]
        line: Option<String>,
        /// Default preview mode (this is the default; the flag exists for clarity)
        #[arg(long, conflicts_with = "write")]
        preview: bool,
        /// Apply the substitution by rewriting files in place using per-file
        /// atomic temp+rename with drift detection. See docs/grep-sed-contract.md.
        #[arg(long, conflicts_with = "preview")]
        write: bool,
        /// Maximum output records before a resume hint is emitted
        #[arg(long, default_value_t = 1000)]
        limit: usize,
        /// Skip this many output records before rendering
        #[arg(long, default_value_t = 0)]
        skip: usize,
        /// Deferred v1 feature: stdin-sourced pattern payloads
        #[arg(long = "pattern-stdin")]
        pattern_stdin: bool,
        /// Deferred v1 feature: stdin-sourced replacement payloads
        #[arg(long = "replacement-stdin")]
        replacement_stdin: bool,
        /// GNU/BSD sed compatibility hint: agent-tools sed is replacement-only.
        #[arg(short = 'n', long = "quiet", alias = "silent", hide = true)]
        quiet: bool,
    },

    /// Copy a file or directory
    Cp {
        /// Source path
        src: PathBuf,
        /// Destination path
        dst: PathBuf,
    },

    /// Move a file or directory
    Mv {
        /// Source path
        src: PathBuf,
        /// Destination path
        dst: PathBuf,
    },

    /// Create directories recursively
    Mkdir {
        /// Directory path to create
        path: PathBuf,
    },

    /// Remove a file or directory
    Rm {
        /// Path to remove
        path: PathBuf,
    },

    /// Markdown reading helpers — outline + section extraction
    Doc {
        #[command(subcommand)]
        command: DocCommands,
    },

    /// Start MCP stdio server
    Serve,

    /// Setup and configuration commands (run with no subcommand for an
    /// interactive menu)
    Setup {
        #[command(subcommand)]
        command: Option<SetupCommands>,
    },

    /// Configure gateway connection (alias for `setup gateway`)
    Init,

    /// Send / receive messages via the gateway (project ident auto-derived from cwd)
    Comms {
        #[command(subcommand)]
        command: cmd_comms::CommsCommands,
    },

    /// Per-project task board: list, claim, comment, complete (gateway-backed)
    Tasks {
        #[command(subcommand)]
        command: cmd_tasks::TasksCommands,
    },

    /// Agent-first API context registry (gateway-backed)
    Docs {
        #[command(subcommand)]
        command: cmd_docs::DocsCommands,
    },

    /// Agent-facing artifact substrate for docs, reviews, specs, and handoffs
    Artifacts {
        #[command(subcommand)]
        command: cmd_docs_artifacts::ArtifactsCommands,
    },

    /// Design-review artifact workflows (gateway-backed)
    Reviews {
        #[command(subcommand)]
        command: cmd_docs_artifacts::ReviewsCommands,
    },

    /// Spec artifact workflows and task generation (gateway-backed)
    Specs {
        #[command(subcommand)]
        command: cmd_docs_artifacts::SpecsCommands,
    },

    /// Global pattern library and repository `.patterns` tracking (gateway-backed)
    Patterns {
        #[command(subcommand)]
        command: cmd_patterns::PatternsCommands,
    },

    /// Check for updates and install the latest version
    Update,

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum SetupCommands {
    /// Configure gateway connection (creates ~/.agentic/agent-tools/gateway.conf)
    Gateway,

    /// Inject the agent-tools usage protocols into known agent rule files
    /// (e.g. ~/.claude/CLAUDE.md). Idempotent — re-runs replace the existing
    /// `<agent-tools-rules>` block in place.
    Rules {
        /// Update a specific file instead of running detection.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Update every detected file without prompting.
        #[arg(long)]
        all: bool,
        /// Show the resulting file content without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Print the rules block to stdout and exit (no file IO, no gateway check).
        #[arg(long)]
        print: bool,
    },

    /// Install a Claude Code skill at ~/.claude/skills/agent-tools/SKILL.md
    /// so the agent-tools CLI is auto-advertised to sessions.
    Skill {
        /// Show the resulting file content without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Print the SKILL.md to stdout and exit.
        #[arg(long)]
        print: bool,
    },

    /// Add (or remove) permission denies in ~/.claude/settings.json that
    /// block the native task system (TaskCreate/TaskUpdate/TaskList/TaskGet,
    /// plus the legacy TodoWrite) so agents are forced onto
    /// `agent-tools tasks`.
    Perms {
        /// Remove the denies instead of adding them.
        #[arg(long)]
        remove: bool,
        /// Show the resulting settings.json without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Print the resulting settings.json to stdout and exit.
        #[arg(long)]
        print: bool,
    },

    /// Run gateway → rules → skill → perms non-interactively.
    All {
        /// Skip the confirmation prompt.
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum DocCommands {
    /// Print just the heading outline of a markdown file (no body)
    Outline {
        /// Markdown file to inspect
        file: PathBuf,
    },
    /// Extract a single section by heading text (case-insensitive)
    Section {
        /// Markdown file to inspect
        file: PathBuf,
        /// Heading text of the section to return
        section: String,
    },
}

#[cfg(windows)]
fn main() -> Result<()> {
    let handle = std::thread::Builder::new()
        .name("agent-tools-main".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(main_inner)?;

    match handle.join() {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(not(windows))]
fn main() -> Result<()> {
    main_inner()
}

fn main_inner() -> Result<()> {
    let cli = Cli::parse();

    // Auto-update check on every invocation (rate-limited, non-blocking for most calls)
    // Skip for update/version/init/comms commands to avoid double-checking,
    // blocking interactive prompts, or slowing down tight comms polling loops.
    if !matches!(
        cli.command,
        Commands::Update
            | Commands::Version
            | Commands::Init
            | Commands::Setup { .. }
            | Commands::Comms { .. }
            | Commands::Tasks { .. }
            | Commands::Docs { .. }
            | Commands::Artifacts { .. }
            | Commands::Reviews { .. }
            | Commands::Specs { .. }
            | Commands::Patterns { .. }
    ) {
        agent_updater::auto_update_blocking();
    }

    // Capture nudge eligibility before the match consumes `cli.command`.
    // The actual emit happens after dispatch so we only nudge on success and
    // never compete with the command's own output.
    let nudge_after = nudge::should_nudge(&cli.command);

    let result = match cli.command {
        Commands::Tree {
            path,
            depth,
            max_files,
        } => cmd_tree(path, depth, max_files),

        Commands::List { path, sizes, all } => cmd_list(path, sizes, all),

        Commands::Read { file, lines } => cmd_read::run(&file, lines.as_deref()),

        Commands::Symbol { name, file, kind } => cmd_symbol(&name, file, kind),

        Commands::Symbols { file, kind } => cmd_symbols(&file, kind),

        Commands::Search {
            query,
            search_type,
            file,
            limit,
        } => cmd_search(&query, &search_type, file, limit),

        Commands::Index { path, rebuild } => cmd_index(path, rebuild),

        Commands::Summary { path } => cmd_summary(path),

        Commands::Grep {
            pattern,
            paths,
            fixed,
            regex,
            pattern_file,
            ignore_case,
            include_globs,
            exclude_globs,
            glob_globs,
            context,
            before_context,
            after_context,
            count_only,
            files_with_matches,
            files_without_match,
            paths_only,
            null,
            limit,
            skip,
            files0_from,
            pattern_stdin,
        } => cmd_text::cmd_grep(cmd_text::GrepArgs {
            pattern,
            paths,
            fixed,
            regex,
            pattern_file,
            ignore_case,
            include_globs,
            exclude_globs,
            glob_globs,
            context,
            before_context,
            after_context,
            count_only,
            files_with_matches,
            files_without_match,
            paths_only,
            null,
            limit,
            skip,
            files0_from,
            pattern_stdin,
        }),

        Commands::Sed {
            expression,
            mut paths,
            regex,
            replace,
            fixed,
            pattern_file,
            replacement_file,
            ignore_case,
            global,
            include_globs,
            exclude_globs,
            glob_globs,
            line,
            preview,
            write,
            limit,
            skip,
            pattern_stdin,
            replacement_stdin,
            quiet,
        } => {
            // `expression` is a positional; if an explicit payload channel is
            // active (--fixed/--regex/--pattern-file/--pattern-stdin), the
            // first non-flag operand is really a path, not an expression.
            // Restore it to the front of `paths` before dispatch.
            let explicit_payload =
                !fixed.is_empty() || regex.is_some() || pattern_file.is_some() || pattern_stdin;
            let expression = if explicit_payload {
                if let Some(value) = expression.clone() {
                    paths.insert(0, PathBuf::from(value));
                }
                None
            } else {
                expression
            };
            cmd_text::cmd_sed(cmd_text::SedArgs {
                expression,
                paths,
                regex,
                replace,
                fixed,
                pattern_file,
                replacement_file,
                ignore_case,
                global,
                include_globs,
                exclude_globs,
                glob_globs,
                line,
                preview,
                write,
                limit,
                skip,
                pattern_stdin,
                replacement_stdin,
                quiet,
            })
        }

        Commands::Cp { src, dst } => {
            agent_fs::ops::copy(&src, &dst)?;
            println!("Copied {} -> {}", src.display(), dst.display());
            Ok(())
        }

        Commands::Mv { src, dst } => {
            agent_fs::ops::move_path(&src, &dst)?;
            println!("Moved {} -> {}", src.display(), dst.display());
            Ok(())
        }

        Commands::Mkdir { path } => {
            agent_fs::ops::mkdir(&path)?;
            println!("Created {}", path.display());
            Ok(())
        }

        Commands::Rm { path } => {
            agent_fs::ops::remove(&path)?;
            println!("Removed {}", path.display());
            Ok(())
        }

        Commands::Doc { command } => match command {
            DocCommands::Outline { file } => {
                let headings = agent_fs::markdown::extract_headings(&file)?;
                if headings.is_empty() {
                    eprintln!("No headings found in {}", file.display());
                } else {
                    print!("{}", agent_fs::markdown::render_outline_text(&headings));
                }
                Ok(())
            }
            DocCommands::Section { file, section } => {
                let body = agent_fs::markdown::extract_section(&file, &section)?;
                print!("{body}");
                Ok(())
            }
        },

        Commands::Serve => {
            eprintln!("Use `agent-tools-mcp` binary for MCP server");
            std::process::exit(1);
        }

        Commands::Setup { command } => match command {
            None => cmd_setup_menu::run_interactive(),
            Some(SetupCommands::Gateway) => agent_comms::config::run_setup_gateway(),
            Some(SetupCommands::Rules {
                target,
                all,
                dry_run,
                print,
            }) => cmd_setup_rules::run(target, all, dry_run, print),
            Some(SetupCommands::Skill { dry_run, print }) => cmd_setup_skill::run(dry_run, print),
            Some(SetupCommands::Perms {
                remove,
                dry_run,
                print,
            }) => cmd_setup_perms::run(remove, dry_run, print),
            Some(SetupCommands::All { yes }) => cmd_setup_menu::run_all(yes),
        },

        Commands::Init => agent_comms::config::run_setup_gateway(),

        Commands::Comms { command } => cmd_comms::dispatch(command),

        Commands::Tasks { command } => cmd_tasks::dispatch(command),

        Commands::Docs { command } => cmd_docs::dispatch(command),

        Commands::Artifacts { command } => cmd_docs_artifacts::dispatch(command),

        Commands::Reviews { command } => cmd_docs_artifacts::dispatch_reviews(command),

        Commands::Specs { command } => cmd_docs_artifacts::dispatch_specs(command),

        Commands::Patterns { command } => cmd_patterns::dispatch(command),

        Commands::Update => agent_updater::manual_update_blocking(),

        Commands::Version => {
            println!("agent-tools {}", env!("AGENT_TOOLS_VERSION"));
            Ok(())
        }
    };

    if nudge_after && result.is_ok() {
        nudge::emit_if_due();
    }

    result
}

/// Display a token-efficient directory tree.
fn cmd_tree(path: Option<PathBuf>, depth: usize, max_files: usize) -> Result<()> {
    let path = path.unwrap_or_else(|| PathBuf::from("."));
    let options = agent_fs::tree::TreeOptions {
        max_depth: depth,
        max_files_per_dir: max_files,
    };
    let tree = agent_fs::tree::tree(&path, &options)?;
    print!("{}", agent_fs::tree::render_tree_text(&tree, 0));
    Ok(())
}

/// List directory contents with optional file sizes and hidden file display.
fn cmd_list(path: Option<PathBuf>, sizes: bool, all: bool) -> Result<()> {
    let path = path.unwrap_or_else(|| PathBuf::from("."));
    let options = agent_fs::list::ListOptions {
        show_sizes: sizes,
        show_hidden: all,
    };
    let entries = agent_fs::list::list_dir(&path, &options)?;
    print!("{}", agent_fs::list::render_list_text(&entries));
    Ok(())
}

/// Extract a named symbol's source code, either from a specific file or the project index.
fn cmd_symbol(name: &str, file: Option<PathBuf>, kind: Option<String>) -> Result<()> {
    if let Some(file_path) = file {
        // Direct file extraction
        let mut parser = agent_symbols::SymbolParser::new();
        match parser.extract_symbol(&file_path, name)? {
            Some(source) => {
                println!("{source}");
            }
            None => {
                eprintln!("Symbol '{name}' not found in {}", file_path.display());
                std::process::exit(1);
            }
        }
    } else {
        // Search index
        let root = std::env::current_dir()?;
        let index = agent_symbols::SymbolIndex::open_for_project(&root)?;
        if index.is_ephemeral() {
            index.build(&root)?;
        }
        let results = index.search(name, kind.as_deref(), None, 10)?;

        if results.is_empty() {
            eprintln!("Symbol '{name}' not found in index. Run `agent-tools index` first.");
            std::process::exit(1);
        }

        // Extract source from the first match
        let first = &results[0];
        let mut parser = agent_symbols::SymbolParser::new();
        match parser.extract_symbol(&first.file, name)? {
            Some(source) => println!("{source}"),
            None => {
                // Fallback: just show location
                for r in &results {
                    println!(
                        "{} {} {}:{}-{}",
                        r.kind,
                        r.name,
                        r.file.display(),
                        r.start_line,
                        r.end_line
                    );
                }
            }
        }
    }
    Ok(())
}

/// List all symbols defined in a file, optionally filtered by kind.
fn cmd_symbols(file: &Path, kind: Option<String>) -> Result<()> {
    let mut parser = agent_symbols::SymbolParser::new();
    let symbols = parser.parse_file(file)?;

    for s in &symbols {
        if let Some(ref k) = kind {
            let kind_str = format!("{}", s.kind);
            if kind_str != *k {
                continue;
            }
        }
        let parent_info = s
            .parent
            .as_ref()
            .map(|p| format!(" (in {p})"))
            .unwrap_or_default();
        println!(
            "{:<10} {:<30} {}:{}-{}{}",
            format!("{}", s.kind),
            s.name,
            s.file.display(),
            s.start_line,
            s.end_line,
            parent_info
        );
    }
    Ok(())
}

/// Search the project-wide index by symbol name or file pattern.
fn cmd_search(query: &str, search_type: &str, file: Option<String>, limit: usize) -> Result<()> {
    let root = std::env::current_dir()?;

    match search_type {
        "symbol" => {
            let index = agent_symbols::SymbolIndex::open_for_project(&root)?;
            if index.is_ephemeral() {
                index.build(&root)?;
            }
            let results = index.search(query, None, file.as_deref(), limit)?;

            if results.is_empty() {
                eprintln!("No symbols found matching '{query}'");
                return Ok(());
            }

            for r in &results {
                println!(
                    "{:<10} {:<30} {}:{}-{}",
                    format!("{}", r.kind),
                    r.name,
                    r.file.display(),
                    r.start_line,
                    r.end_line
                );
            }
        }
        "file" => {
            let indexer = agent_search::indexer::FileIndexer::open_for_project(&root)?;
            if indexer.is_ephemeral() {
                indexer.build(&root, false)?;
            }
            let results =
                agent_search::query::find_files(&indexer, Some(query), None, None, None, limit)?;

            if results.is_empty() {
                eprintln!("No files found matching '{query}'");
                return Ok(());
            }

            for r in &results {
                println!("{}", r.path);
            }
        }
        _ => {
            eprintln!("Unknown search type: {search_type}. Use 'symbol' or 'file'.");
            std::process::exit(1);
        }
    }
    maybe_print_api_context_hint(query);
    Ok(())
}

fn maybe_print_api_context_hint(query: &str) {
    if !looks_api_related(query) {
        return;
    }
    eprintln!(
        "hint: API-related search detected. Also check agent API context with \
         `agent-tools docs search \"{query}\"` or \
         `agent-tools docs chunks --query \"{query}\"`. If no docs exist, ask \
         whether to create .agent/api/<app>.yaml or agent-api.yaml and publish \
         it with `agent-tools docs publish --file PATH` for future agents."
    );
}

fn looks_api_related(query: &str) -> bool {
    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| {
            let part = part.to_ascii_lowercase();
            matches!(
                part.as_str(),
                "api"
                    | "apis"
                    | "endpoint"
                    | "endpoints"
                    | "route"
                    | "routes"
                    | "openapi"
                    | "swagger"
                    | "graphql"
                    | "rest"
            )
        })
}

/// Build or rebuild the project file and symbol index.
fn cmd_index(path: Option<PathBuf>, rebuild: bool) -> Result<()> {
    let root =
        path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if rebuild {
        let data_dir = agent_core::project_data_dir(&root);
        if data_dir.exists() {
            match std::fs::remove_dir_all(&data_dir) {
                Ok(()) => println!("Cleared existing index at {}", data_dir.display()),
                Err(e) => eprintln!(
                    "Could not clear persistent index at {} ({e}); continuing with available storage",
                    data_dir.display()
                ),
            }
        }
    }

    // Build file index
    print!("Indexing files... ");
    let file_indexer = agent_search::indexer::FileIndexer::open_for_project(&root)?;
    let file_stats = file_indexer.build(&root, true)?;
    println!("{file_stats}");

    // Build symbol index
    print!("Indexing symbols... ");
    let symbol_index = agent_symbols::SymbolIndex::open_for_project(&root)?;
    let symbol_stats = symbol_index.build(&root)?;
    println!("{symbol_stats}");

    let (file_count, symbol_count) = symbol_index.stats()?;
    println!("\nTotal: {file_count} files, {symbol_count} symbols");

    Ok(())
}

/// Generate and display a compact project summary from the file index.
fn cmd_summary(path: Option<PathBuf>) -> Result<()> {
    let root =
        path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Ensure index exists
    let indexer = agent_search::indexer::FileIndexer::open_for_project(&root)?;
    if indexer.is_ephemeral() || indexer.file_count()? == 0 {
        println!("No index found. Building...");
        indexer.build(&root, false)?;
    }

    let summary = agent_search::query::project_summary(&indexer)?;
    print!("{}", agent_search::query::render_summary_text(&summary));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_hint_detects_api_search_terms() {
        assert!(looks_api_related("billing api"));
        assert!(looks_api_related("GET endpoints"));
        assert!(looks_api_related("openapi.yaml"));
        assert!(looks_api_related("GraphQL resolver"));
    }

    #[test]
    fn api_hint_ignores_non_api_words() {
        assert!(!looks_api_related("capitalization"));
        assert!(!looks_api_related("happier path"));
        assert!(!looks_api_related("config loader"));
    }
}
