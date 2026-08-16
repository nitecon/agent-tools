mod cmd_comms;
mod cmd_docs;
mod cmd_docs_artifacts;
mod cmd_gateway_context;
mod cmd_hook;
mod cmd_okf;
mod cmd_patterns;
mod cmd_read;
mod cmd_setup_hooks;
mod cmd_setup_menu;
mod cmd_setup_perms;
mod cmd_setup_rules;
mod cmd_setup_skill;
mod cmd_tasks;
mod cmd_text;
mod codex_hooks_toml;
mod memory_reminder;
mod nudge;
mod settings_json;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "agent-tools",
    about = "Token-efficient tools for AI coding agents",
    version = env!("AGENT_TOOLS_VERSION")
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
        /// Search type
        #[arg(
            short = 't',
            long = "type",
            default_value = "symbol",
            value_parser = ["symbol", "file", "knowledge", "all"]
        )]
        search_type: String,
        /// File pattern filter
        #[arg(short, long)]
        file: Option<String>,
        /// Maximum results (default: 20)
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Resource namespace filter for knowledge/all search
        #[arg(long)]
        namespace: Option<String>,
        /// Resource kind filter for knowledge/all search
        #[arg(long)]
        kind: Option<String>,
        /// Lifecycle status filter for knowledge/all search
        #[arg(long)]
        status: Option<String>,
        /// Origin identifier or origin kind filter
        #[arg(long)]
        origin: Option<String>,
        /// Programming language filter
        #[arg(long)]
        language: Option<String>,
        /// Require a relationship type
        #[arg(long)]
        relation: Option<String>,
    },

    /// Get a resource and its current version, authority, lifecycle, and trust metadata
    Get {
        /// Resource URI, title, or external identifier
        resource: String,
    },

    /// Validate, import, or deterministically export an OKF bundle
    Okf {
        #[command(subcommand)]
        command: OkfCommands,
    },

    /// Traverse the project knowledge graph from a resource URI, title, or symbol
    Graph {
        /// Resource URI, title, or external identifier
        resource: String,
        /// Relationship type to follow (calls, imports, inherits, implements, ...)
        #[arg(short, long)]
        relation: Option<String>,
        /// Traversal direction: in, out, or both
        #[arg(short, long, default_value = "both")]
        direction: String,
        /// Maximum traversal depth
        #[arg(short = 'D', long, default_value = "1")]
        depth: usize,
        /// Maximum edges to return
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show callers and callees for a symbol
    Refs {
        /// Symbol URI or name
        symbol: String,
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show imports to and from a file or symbol
    Imports {
        /// Resource URI, path, title, or symbol
        resource: String,
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show inheritance and implementation relationships
    Impls {
        /// Symbol URI or name
        symbol: String,
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
        /// GNU grep compatibility no-op: agent-tools grep is recursive for directory operands
        #[arg(
            short = 'R',
            short_alias = 'r',
            long = "recursive",
            alias = "dereference-recursive",
            hide = true
        )]
        recursive: bool,
        /// GNU grep compatibility no-op: match records always include line numbers
        #[arg(short = 'n', long = "line-number", hide = true)]
        line_number: bool,
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

    /// Runtime context-injection hooks called by agent CLIs (installed via
    /// `setup hooks`). Fail-soft: always exits 0. Not for direct human use.
    Hook {
        #[command(subcommand)]
        command: cmd_hook::HookCommands,
    },

    /// Check for updates and install the latest version
    Update,

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum OkfCommands {
    /// Validate and summarize a bundle without changing the index
    Validate { path: PathBuf },
    /// Import a bundle into the shared project index
    Import { path: PathBuf },
    /// Normalize a bundle into a deterministic destination directory
    Export {
        path: PathBuf,
        #[arg(short, long)]
        destination: PathBuf,
    },
    /// Project a bundle one-way into gateway Documentation
    Publish {
        path: PathBuf,
        /// Print deterministic publish decisions without contacting a gateway
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum SetupCommands {
    /// Configure the default gateway or project-based upstream gateways
    Gateway {
        /// Add or update a project upstream profile (prompts for URL and credentials).
        #[arg(long, value_name = "PROFILE", conflicts_with_all = ["list", "remove_upstream"])]
        add_upstream: Option<String>,
        /// List the default gateway and current repository upstreams.
        #[arg(long, conflicts_with_all = ["add_upstream", "remove_upstream"])]
        list: bool,
        /// Remove an upstream declaration and its local credentials.
        #[arg(long, value_name = "PROFILE", conflicts_with_all = ["add_upstream", "list"])]
        remove_upstream: Option<String>,
        /// With --remove-upstream, preserve the repository declaration.
        #[arg(long, requires = "remove_upstream")]
        credentials_only: bool,
    },

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

    /// Sync app-scoped hooks from the gateway and install the local
    /// context-injection hook entries for detected agent clients.
    Hooks {
        /// Client app to sync. Repeatable. Defaults to every detected app.
        #[arg(long = "app")]
        app: Vec<String>,
        /// Show target paths without writing hook files.
        #[arg(long)]
        dry_run: bool,
        /// Remove the local context-injection hook entries instead of adding them.
        #[arg(long)]
        remove: bool,
    },

    /// Run gateway -> hooks -> rules -> skill -> perms non-interactively.
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
            namespace,
            kind,
            status,
            origin,
            language,
            relation,
        } => cmd_search(
            &query,
            &search_type,
            file,
            limit,
            namespace,
            kind,
            status,
            origin,
            language,
            relation,
        ),

        Commands::Get { resource } => cmd_get(&resource),

        Commands::Okf { command } => cmd_okf(command),

        Commands::Graph {
            resource,
            relation,
            direction,
            depth,
            limit,
        } => cmd_graph(&resource, relation.as_deref(), &direction, depth, limit),

        Commands::Refs { symbol, limit } => cmd_graph(&symbol, Some("calls"), "both", 1, limit),

        Commands::Imports { resource, limit } => {
            cmd_graph(&resource, Some("imports"), "both", 1, limit)
        }

        Commands::Impls { symbol, limit } => cmd_impls(&symbol, limit),

        Commands::Index { path, rebuild } => cmd_index(path, rebuild),

        Commands::Summary { path } => cmd_summary(path),

        Commands::Grep {
            pattern,
            paths,
            fixed,
            regex,
            pattern_file,
            ignore_case,
            recursive,
            line_number,
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
            recursive,
            line_number,
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
            Some(SetupCommands::Gateway {
                add_upstream,
                list,
                remove_upstream,
                credentials_only,
            }) => {
                if let Some(profile) = add_upstream {
                    agent_comms::config::run_add_project_gateway(Some(&profile))
                } else if list {
                    agent_comms::config::print_gateway_status()
                } else if let Some(profile) = remove_upstream {
                    agent_comms::config::run_remove_project_gateway(
                        Some(&profile),
                        credentials_only,
                    )
                } else {
                    agent_comms::config::run_setup_gateway()
                }
            }
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
            Some(SetupCommands::Hooks {
                app,
                dry_run,
                remove,
            }) => cmd_setup_hooks::run(app, dry_run, remove),
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

        Commands::Hook { command } => cmd_hook::dispatch(command),

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
        let mut index = agent_symbols::SymbolIndex::open_for_project(&root)?;
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
#[allow(clippy::too_many_arguments)]
fn cmd_search(
    query: &str,
    search_type: &str,
    file: Option<String>,
    limit: usize,
    namespace: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    origin: Option<String>,
    language: Option<String>,
    relation: Option<String>,
) -> Result<()> {
    let root = std::env::current_dir()?;

    match search_type {
        "symbol" => {
            let mut index = agent_symbols::SymbolIndex::open_for_project(&root)?;
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
        "knowledge" | "all" => {
            let project_id = agent_core::project_ident(&root);
            let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
            let filters = agent_knowledge::SearchFilter {
                namespace: namespace.as_deref(),
                kind: kind.as_deref(),
                status: status.as_deref(),
                origin: origin.as_deref(),
                path: file.as_deref(),
                language: language.as_deref(),
                relation: relation.as_deref(),
            };
            let results = index.search_segments_filtered(&project_id, query, &filters, limit)?;
            for result in &results {
                println!(
                    "{:<10} {:<18} {} [{}:{}] {}",
                    result.resource.namespace,
                    result.resource.kind,
                    result.resource.canonical_uri,
                    result.resource.status,
                    result.resource.authority,
                    result.heading_path.as_deref().unwrap_or("")
                );
            }
            if search_type == "all" && namespace.is_none() {
                let mut symbols = agent_symbols::SymbolIndex::open_for_project(&root)?;
                if symbols.is_ephemeral() {
                    symbols.build(&root)?;
                }
                for result in symbols.search(query, kind.as_deref(), file.as_deref(), limit)? {
                    println!(
                        "symbol     {:<18} {}:{} {}",
                        result.kind,
                        result.file.display(),
                        result.start_line,
                        result.name
                    );
                }
                let files = agent_search::indexer::FileIndexer::open_for_project(&root)?;
                for result in
                    agent_search::query::find_files(&files, Some(query), None, None, None, limit)?
                {
                    println!("file       file               {}", result.path);
                }
            }
            if results.is_empty() && search_type == "knowledge" {
                eprintln!("No knowledge resources found matching '{query}'");
            }
        }
        _ => {
            eprintln!(
                "Unknown search type: {search_type}. Use 'symbol', 'file', 'knowledge', or 'all'."
            );
            std::process::exit(1);
        }
    }
    maybe_print_api_context_hint(query);
    Ok(())
}

fn cmd_get(query: &str) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
    let project_id = agent_core::project_ident(&root);
    let matches = index.find_resources(&project_id, query, None, 20)?;
    let resource = match matches.as_slice() {
        [] => bail!("No resource found matching '{query}'"),
        [resource] => resource,
        resources => {
            let candidates = resources
                .iter()
                .map(|resource| format!("  {} ({})", resource.canonical_uri, resource.kind))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("Resource '{query}' is ambiguous:\n{candidates}")
        }
    };
    let detail = index
        .resource_detail(resource.id)?
        .context("resolved resource disappeared")?;
    let relationships = index.traverse(resource.id, None, "both", 1, 100)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "resource": detail,
            "relationships": relationships,
        }))?
    );
    Ok(())
}

fn cmd_okf(command: OkfCommands) -> Result<()> {
    let root = std::env::current_dir()?;
    match command {
        OkfCommands::Validate { path } => {
            let bundle = match agent_knowledge::okf::parse_bundle(
                &path,
                agent_knowledge::okf::OkfLimits::default(),
            ) {
                Ok(bundle) => bundle,
                Err(error) => {
                    eprintln!("invalid OKF bundle: {error:#}");
                    std::process::exit(2);
                }
            };
            println!(
                "OKF {}: {} concepts, {} diagnostics",
                bundle.version,
                bundle.concepts.len(),
                bundle.diagnostics.len()
            );
            for diagnostic in bundle.diagnostics {
                println!(
                    "{} {} {}: {}",
                    diagnostic.level, diagnostic.code, diagnostic.path, diagnostic.message
                );
            }
        }
        OkfCommands::Import { path } => {
            let project_id = agent_core::project_ident(&root);
            let mut index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
            let stats = agent_knowledge::knowledge::index_okf_bundle(
                &mut index,
                &project_id,
                &root,
                &path,
                agent_knowledge::okf::OkfLimits::default(),
            )?;
            println!(
                "Imported {} concepts ({} changed, {} unchanged, {} removed), {} segments, {} edges, {} unresolved, {} diagnostics",
                stats.resources_seen,
                stats.resources_indexed,
                stats.resources_unchanged,
                stats.resources_removed,
                stats.segments_indexed,
                stats.edges_indexed,
                stats.unresolved_edges,
                stats.diagnostics
            );
        }
        OkfCommands::Export { path, destination } => {
            let bundle = agent_knowledge::okf::parse_bundle(
                &path,
                agent_knowledge::okf::OkfLimits::default(),
            )?;
            agent_knowledge::okf::export_bundle(&bundle, &destination)?;
            println!(
                "Exported {} concepts to {}",
                bundle.concepts.len(),
                destination.display()
            );
        }
        OkfCommands::Publish {
            path,
            dry_run,
            project,
            agent_id,
        } => return cmd_okf::publish(path, dry_run, project, agent_id),
    }
    Ok(())
}

fn open_graph_index(root: &Path) -> Result<agent_symbols::SymbolIndex> {
    let mut index = agent_symbols::SymbolIndex::open_for_project(root)?;
    let (files, symbols) = index.stats()?;
    if index.is_ephemeral() || (files == 0 && symbols == 0) {
        index.build(root)?;
    }
    Ok(index)
}

fn resolve_graph_resource(
    index: &agent_symbols::SymbolIndex,
    root: &Path,
    query: &str,
) -> Result<agent_symbols::ResourceMatch> {
    let matches = index.find_graph_resources(root, query, None, 20)?;
    match matches.as_slice() {
        [] => bail!("No graph resource found matching '{query}'. Run `agent-tools index` first."),
        [resource] => Ok(resource.clone()),
        resources => {
            let candidates = resources
                .iter()
                .map(|resource| format!("  {} ({})", resource.canonical_uri, resource.kind))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("Graph resource '{query}' is ambiguous:\n{candidates}")
        }
    }
}

fn cmd_graph(
    query: &str,
    relation: Option<&str>,
    direction: &str,
    depth: usize,
    limit: usize,
) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = open_graph_index(&root)?;
    let resource = resolve_graph_resource(&index, &root, query)?;
    let edges = index.traverse_graph(resource.id, relation, direction, depth, limit)?;
    if edges.is_empty() {
        println!("No matching relationships from {}", resource.canonical_uri);
        return Ok(());
    }
    render_graph_edges(&edges);
    Ok(())
}

fn cmd_impls(query: &str, limit: usize) -> Result<()> {
    let root = std::env::current_dir()?;
    let index = open_graph_index(&root)?;
    let resource = resolve_graph_resource(&index, &root, query)?;
    let mut edges = index.traverse_graph(resource.id, Some("implements"), "both", 1, limit)?;
    if edges.len() < limit {
        edges.extend(index.traverse_graph(
            resource.id,
            Some("inherits"),
            "both",
            1,
            limit - edges.len(),
        )?);
    }
    edges.sort_by(|left, right| {
        left.relation
            .cmp(&right.relation)
            .then_with(|| left.source_uri.cmp(&right.source_uri))
            .then_with(|| left.target_uri.cmp(&right.target_uri))
    });
    if edges.is_empty() {
        println!(
            "No inheritance or implementation relationships from {}",
            resource.canonical_uri
        );
    } else {
        render_graph_edges(&edges);
    }
    Ok(())
}

fn render_graph_edges(edges: &[agent_symbols::TraversedEdge]) {
    for edge in edges {
        let target = edge
            .target_uri
            .as_deref()
            .or(edge.unresolved_ref.as_deref())
            .unwrap_or("?");
        let unresolved = if edge.target_uri.is_none() { "?" } else { "" };
        let location = match (&edge.source_path, edge.start_line) {
            (Some(path), Some(line)) => format!(" {path}:{line}"),
            (Some(path), None) => format!(" {path}"),
            _ => String::new(),
        };
        println!(
            "d{} {} {} {} -> {}{} [{}]{}",
            edge.depth,
            edge.direction,
            edge.relation,
            edge.source_uri,
            unresolved,
            target,
            edge.confidence,
            location
        );
    }
}

fn maybe_print_api_context_hint(query: &str) {
    if !looks_api_related(query) {
        return;
    }
    eprintln!(
        "hint: API-related search detected. Also check Documentation with \
         `agent-tools docs search \"{query}\"`, \
         `agent-tools docs hierarchy`, or \
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
    let mut symbol_index = agent_symbols::SymbolIndex::open_for_project(&root)?;
    let symbol_stats = symbol_index.build(&root)?;
    println!("{symbol_stats}");

    let conventional_okf = root.join(".agents/knowledge");
    if conventional_okf.is_dir() {
        print!("Indexing OKF knowledge... ");
        let project_id = agent_core::project_ident(&root);
        let mut knowledge_index = agent_knowledge::ProjectIndex::open_for_project(&root)?;
        let stats = agent_knowledge::knowledge::index_okf_bundle(
            &mut knowledge_index,
            &project_id,
            &root,
            &conventional_okf,
            agent_knowledge::okf::OkfLimits::default(),
        )?;
        println!(
            "{} concepts, {} segments, {} edges ({} unresolved)",
            stats.resources_seen,
            stats.segments_indexed,
            stats.edges_indexed,
            stats.unresolved_edges
        );
    }

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
    use clap::error::ErrorKind;

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

    #[test]
    fn search_parser_advertises_and_enforces_every_supported_type() {
        let help = Cli::try_parse_from(["agent-tools", "search", "--help"])
            .err()
            .expect("help exits through clap");
        assert_eq!(help.kind(), ErrorKind::DisplayHelp);
        let rendered = help.to_string();
        assert!(rendered.contains("symbol"));
        assert!(rendered.contains("file"));
        assert!(rendered.contains("knowledge"));
        assert!(rendered.contains("all"));

        for search_type in ["symbol", "file", "knowledge", "all"] {
            assert!(Cli::try_parse_from(
                ["agent-tools", "search", "query", "--type", search_type,]
            )
            .is_ok());
        }

        let invalid =
            Cli::try_parse_from(["agent-tools", "search", "query", "--type", "unsupported"])
                .err()
                .expect("invalid search type must fail in clap");
        assert_eq!(invalid.kind(), ErrorKind::InvalidValue);
        let rendered = invalid.to_string();
        assert!(rendered.contains("possible values"));
        assert!(rendered.contains("knowledge"));
        assert!(rendered.contains("all"));
    }

    #[test]
    fn conventional_version_flag_matches_version_subcommand_value() {
        let version = Cli::try_parse_from(["agent-tools", "--version"])
            .err()
            .expect("version exits through clap");
        assert_eq!(version.kind(), ErrorKind::DisplayVersion);
        assert_eq!(
            version.to_string().trim(),
            format!("agent-tools {}", env!("AGENT_TOOLS_VERSION"))
        );
    }
}
