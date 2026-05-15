use agent_core::{
    classify_text_exit_code, render_null_path_records, render_text_records, GrepContextKind,
    ReplacementRecordId, TextErrorLabel, TextExitClassificationInput, TextOperationKind, TextPath,
    TextRecord, TextRenderOptions, TextSummaryCounters,
};
use agent_fs::text_ops::{
    atomic_write_bytes, collect_text_files, encode_text_with_bom, recheck_file_drift,
    relative_path_hash, stable_content_hash, DriftCheck, TextFile, TextFileClassification,
    TextFileSet, TextInput, TextTargetOptions,
};
use anyhow::Result;
use regex::{Regex, RegexBuilder};
use std::io::{self, Read, Write};
use std::path::PathBuf;

pub(crate) struct GrepArgs {
    pub(crate) pattern: Option<String>,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) fixed: bool,
    pub(crate) regex: bool,
    pub(crate) pattern_file: Option<PathBuf>,
    pub(crate) ignore_case: bool,
    pub(crate) recursive: bool,
    pub(crate) line_number: bool,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) glob_globs: Vec<String>,
    pub(crate) context: usize,
    pub(crate) before_context: Option<usize>,
    pub(crate) after_context: Option<usize>,
    pub(crate) count_only: bool,
    pub(crate) files_with_matches: bool,
    pub(crate) files_without_match: bool,
    pub(crate) paths_only: bool,
    pub(crate) null: bool,
    pub(crate) limit: usize,
    pub(crate) skip: usize,
    pub(crate) files0_from: Option<PathBuf>,
    pub(crate) pattern_stdin: bool,
}

struct TextPatternSource {
    inline: Option<String>,
    file: Option<PathBuf>,
    stdin_deferred: bool,
}

struct TextMatcherConfig {
    pattern: String,
    fixed: bool,
    ignore_case: bool,
}

struct GrepMatchRecord {
    line: usize,
    byte: usize,
    text: String,
}

struct TextCommandContext<'a> {
    path: TextPath,
    text: &'a str,
    source: TextCommandSource<'a>,
}

enum TextCommandSource<'a> {
    Stdin,
    File { file: &'a TextFile },
}

impl<'a> TextCommandContext<'a> {
    fn stdin(path: TextPath, text: &'a str) -> Self {
        Self {
            path,
            text,
            source: TextCommandSource::Stdin,
        }
    }

    fn file(path: TextPath, text: &'a str, file: &'a TextFile) -> Self {
        Self {
            path,
            text,
            source: TextCommandSource::File { file },
        }
    }

    fn path(&self) -> &TextPath {
        &self.path
    }

    fn text(&self) -> &str {
        self.text
    }

    fn text_file(&self) -> Option<&TextFile> {
        match self.source {
            TextCommandSource::Stdin => None,
            TextCommandSource::File { file } => Some(file),
        }
    }
}

struct TextCommandOutcome {
    operation: TextOperationKind,
    records: Vec<TextRecord>,
    matched: bool,
    changed: bool,
    replacements: usize,
    warnings: usize,
    errors: usize,
    no_op: bool,
    /// True when this file contributed a recoverable but non-success outcome
    /// (sed write drift / write failure) that must escalate the traversal
    /// exit class to 3 without aborting subsequent files.
    partial_failure: bool,
}

impl TextCommandOutcome {
    fn grep(records: Vec<TextRecord>, matched: bool) -> Self {
        Self {
            operation: TextOperationKind::Grep,
            records,
            matched,
            changed: false,
            replacements: 0,
            warnings: 0,
            errors: 0,
            no_op: !matched,
            partial_failure: false,
        }
    }

    fn sed_preview(records: Vec<TextRecord>, replacements: usize) -> Self {
        let changed = replacements > 0;
        Self {
            operation: TextOperationKind::SedPreview,
            records,
            matched: changed,
            changed,
            replacements,
            warnings: 0,
            errors: 0,
            no_op: !changed,
            partial_failure: false,
        }
    }

    fn sed_write(
        records: Vec<TextRecord>,
        replacements: usize,
        changed: bool,
        warnings: usize,
        errors: usize,
        partial_failure: bool,
    ) -> Self {
        Self {
            operation: TextOperationKind::SedWrite,
            records,
            matched: changed,
            changed,
            replacements,
            warnings,
            errors,
            no_op: !changed,
            partial_failure,
        }
    }
}

struct TextCommandTraversalOutcome {
    operation: TextOperationKind,
    records: Vec<TextRecord>,
    counters: TextSummaryCounters,
    matched_files: usize,
    changed_files: usize,
    #[allow(dead_code)]
    no_op_files: usize,
    partial_traversal_failure: bool,
}

enum TextSummaryMode {
    Diagnostics,
    #[allow(dead_code)]
    Always,
    #[allow(dead_code)]
    Never,
}

enum TextExitSuccessCriteria {
    GrepMatchedFiles,
    GrepPathNoMatchRecords,
    #[allow(dead_code)]
    ChangedFiles,
}

struct TextCommandFinalizeOptions {
    summary_mode: TextSummaryMode,
    success_criteria: TextExitSuccessCriteria,
    null_paths: bool,
    skip: usize,
    limit: usize,
}

struct TextCommandResult {
    stdout: String,
    stdout_bytes: Option<Vec<u8>>,
    stderr: String,
    exit_code: i32,
}

fn run_text_command<F>(operation: TextOperationKind, f: F) -> !
where
    F: FnOnce() -> Result<TextCommandResult>,
{
    let result = match f() {
        Ok(result) => result,
        Err(err) => render_text_fallback_error_result(operation, &err.to_string()),
    };
    exit_text_command_result(operation, result)
}

fn exit_text_command_result(operation: TextOperationKind, result: TextCommandResult) -> ! {
    let code = result.exit_code;
    if let Some(bytes) = result.stdout_bytes {
        if let Err(err) = io::stdout().write_all(&bytes) {
            let result = render_text_error_result(
                TextErrorLabel::InvalidInput,
                &err.to_string(),
                TextExitClassificationInput::invalid_input(operation),
            );
            if !result.stderr.is_empty() {
                eprint!("{}", result.stderr);
            }
            std::process::exit(result.exit_code);
        }
    } else {
        print!("{}", result.stdout);
    }
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }
    std::process::exit(code);
}

fn render_text_fallback_error_result(
    operation: TextOperationKind,
    message: &str,
) -> TextCommandResult {
    let (label, reason) = if message.starts_with("error: invalid-expression: ") {
        (
            TextErrorLabel::InvalidExpression,
            strip_error_label(message, "error: invalid-expression: "),
        )
    } else if message.starts_with("error: invalid-input: ") {
        (
            TextErrorLabel::InvalidInput,
            strip_error_label(message, "error: invalid-input: "),
        )
    } else if message.starts_with("error: invalid-path: ") {
        (
            TextErrorLabel::InvalidPath,
            strip_error_label(message, "error: invalid-path: "),
        )
    } else if message.starts_with("error: unsupported: ") {
        (
            TextErrorLabel::Unsupported,
            strip_error_label(message, "error: unsupported: "),
        )
    } else if message.starts_with("error: partial-traversal-failure: ") {
        (
            TextErrorLabel::PartialTraversalFailure,
            strip_error_label(message, "error: partial-traversal-failure: "),
        )
    } else if message.starts_with("error: write-failed: ") {
        (
            TextErrorLabel::WriteFailed,
            strip_error_label(message, "error: write-failed: "),
        )
    } else {
        (TextErrorLabel::InvalidInput, message)
    };
    render_text_error_result(
        label,
        reason,
        TextExitClassificationInput::invalid_input(operation),
    )
}

pub(crate) fn cmd_grep(args: GrepArgs) -> Result<()> {
    run_text_command(TextOperationKind::Grep, || run_grep(args))
}

fn run_grep(args: GrepArgs) -> Result<TextCommandResult> {
    let _compat_flags = (args.recursive, args.line_number);

    if args.files0_from.is_some() {
        return Ok(render_text_error_result(
            TextErrorLabel::Unsupported,
            "null-delimited input lists are deferred",
            TextExitClassificationInput::invalid_input(TextOperationKind::Grep),
        ));
    }

    let pattern = match resolve_text_pattern(
        TextOperationKind::Grep,
        TextPatternSource {
            inline: args.pattern.clone(),
            file: args.pattern_file.clone(),
            stdin_deferred: args.pattern_stdin,
        },
    ) {
        Ok(pattern) => pattern,
        Err(result) => return Ok(result),
    };

    if let Some(result) = validate_grep_output_modes(&args)? {
        return Ok(result);
    }

    let _regex_requested = args.regex;
    let matcher = match build_text_matcher(TextMatcherConfig {
        pattern,
        fixed: args.fixed,
        ignore_case: args.ignore_case,
    }) {
        Ok(matcher) => matcher,
        Err(err) => {
            return Ok(render_text_error_result(
                TextErrorLabel::InvalidExpression,
                strip_error_label(&err.to_string(), "error: invalid-expression: "),
                TextExitClassificationInput::invalid_expression(TextOperationKind::Grep),
            ));
        }
    };

    let cwd = std::env::current_dir()?;
    let options = text_target_options(
        args.include_globs.clone(),
        args.exclude_globs.clone(),
        args.glob_globs.clone(),
        true,
    );
    let files = match collect_text_files(&cwd, &args.paths, &options) {
        Ok(files) => files,
        Err(err) => return text_error_from_message(TextOperationKind::Grep, &err.to_string()),
    };

    let before = args.before_context.unwrap_or(args.context);
    let after = args.after_context.unwrap_or(args.context);
    let collected = collect_text_command_outcomes(TextOperationKind::Grep, &files, |context| {
        let matches = grep_line_matches(context.text(), &matcher);
        let matched = !matches.is_empty();
        let mut records = Vec::new();
        push_grep_records_for_matches(
            &mut records,
            context.path().clone(),
            context.text(),
            &matches,
            before,
            after,
            &args,
        );
        Ok(TextCommandOutcome::grep(records, matched))
    })?;

    Ok(finalize_text_command_output(
        collected,
        TextCommandFinalizeOptions {
            summary_mode: TextSummaryMode::Diagnostics,
            success_criteria: if args.files_without_match {
                TextExitSuccessCriteria::GrepPathNoMatchRecords
            } else {
                TextExitSuccessCriteria::GrepMatchedFiles
            },
            null_paths: args.null,
            skip: args.skip,
            limit: args.limit,
        },
    ))
}

fn resolve_text_pattern(
    operation: TextOperationKind,
    source: TextPatternSource,
) -> std::result::Result<String, TextCommandResult> {
    if source.stdin_deferred {
        return Err(render_text_error_result(
            TextErrorLabel::Unsupported,
            "stdin payload modes are deferred",
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    match (source.inline, source.file) {
        (Some(pattern), None) => Ok(pattern),
        (None, Some(path)) => resolve_text_payload_file(&path, "pattern-file", operation),
        (None, None) => Err(render_text_error_result(
            TextErrorLabel::InvalidExpression,
            "missing pattern",
            TextExitClassificationInput::invalid_expression(operation),
        )),
        (Some(_), Some(_)) => Err(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "pattern and pattern-file conflict",
            TextExitClassificationInput::invalid_input(operation),
        )),
    }
}

fn build_text_matcher(config: TextMatcherConfig) -> Result<Regex> {
    let source = if config.fixed {
        regex::escape(&config.pattern)
    } else {
        config.pattern
    };
    RegexBuilder::new(&source)
        .case_insensitive(config.ignore_case)
        .build()
        .map_err(|err| anyhow::anyhow!("error: invalid-expression: {err}"))
}

fn text_target_options(
    include_globs: Vec<String>,
    exclude_globs: Vec<String>,
    glob_globs: Vec<String>,
    allow_stdin: bool,
) -> TextTargetOptions {
    let mut include_globs = include_globs;
    include_globs.extend(glob_globs);
    TextTargetOptions {
        include_globs,
        exclude_globs,
        allow_stdin,
        ..TextTargetOptions::default()
    }
}

fn collect_text_command_outcomes<F>(
    operation: TextOperationKind,
    files: &TextFileSet,
    mut run_operation: F,
) -> Result<TextCommandTraversalOutcome>
where
    F: for<'a> FnMut(TextCommandContext<'a>) -> Result<TextCommandOutcome>,
{
    let mut records = Vec::new();
    let mut counters = TextSummaryCounters::default();
    let mut matched_files = 0usize;
    let mut changed_files = 0usize;
    let mut no_op_files = 0usize;
    let mut partial_traversal_failure = false;

    if matches!(files.inputs.as_slice(), [TextInput::Stdin]) {
        let mut buffer = Vec::new();
        io::stdin().read_to_end(&mut buffer)?;
        let input = String::from_utf8(buffer)
            .map_err(|_| anyhow::anyhow!("error: invalid-input: stdin is not valid UTF-8"))?;
        let outcome = run_operation(TextCommandContext::stdin(TextPath::new("<stdin>"), &input))?;
        if outcome.partial_failure {
            partial_traversal_failure = true;
        }
        aggregate_text_command_outcome(
            operation,
            outcome,
            &mut records,
            &mut counters,
            &mut matched_files,
            &mut changed_files,
            &mut no_op_files,
        );
    } else {
        for file in &files.files {
            match file.classification {
                TextFileClassification::Text => {
                    counters.files += 1;
                    let decoded = file
                        .decoded
                        .as_ref()
                        .expect("text-classified files should have decoded text");
                    let outcome = run_operation(TextCommandContext::file(
                        TextPath::new(&file.display_path),
                        &decoded.text,
                        file,
                    ))?;
                    if outcome.partial_failure {
                        partial_traversal_failure = true;
                    }
                    aggregate_text_command_outcome(
                        operation,
                        outcome,
                        &mut records,
                        &mut counters,
                        &mut matched_files,
                        &mut changed_files,
                        &mut no_op_files,
                    );
                }
                TextFileClassification::Binary
                | TextFileClassification::InvalidEncoding
                | TextFileClassification::UnsupportedEncoding
                | TextFileClassification::Skipped
                | TextFileClassification::Errored => {
                    push_file_diagnostic(&mut records, &mut counters, file);
                    if matches!(file.classification, TextFileClassification::Errored) {
                        partial_traversal_failure = true;
                    }
                }
            }
        }
    }

    for diagnostic in &files.diagnostics {
        records.push(TextRecord::Warning {
            label: diagnostic.label.into(),
            path: diagnostic.path.as_deref().map(TextPath::new),
            reason: diagnostic.reason.clone(),
        });
        counters.warnings += 1;
        partial_traversal_failure = true;
    }

    counters.matched = matched_files;
    counters.changed = changed_files;

    Ok(TextCommandTraversalOutcome {
        operation,
        records,
        counters,
        matched_files,
        changed_files,
        no_op_files,
        partial_traversal_failure,
    })
}

fn aggregate_text_command_outcome(
    operation: TextOperationKind,
    outcome: TextCommandOutcome,
    records: &mut Vec<TextRecord>,
    counters: &mut TextSummaryCounters,
    matched_files: &mut usize,
    changed_files: &mut usize,
    no_op_files: &mut usize,
) {
    debug_assert_eq!(operation, outcome.operation);
    if outcome.matched {
        *matched_files += 1;
    }
    if outcome.changed {
        *changed_files += 1;
    }
    if outcome.no_op {
        *no_op_files += 1;
    }
    counters.replacements += outcome.replacements;
    counters.warnings += outcome.warnings;
    counters.errors += outcome.errors;
    records.extend(outcome.records);
}

fn finalize_text_command_output(
    mut outcome: TextCommandTraversalOutcome,
    options: TextCommandFinalizeOptions,
) -> TextCommandResult {
    if should_insert_text_summary(&outcome.counters, &options.summary_mode) {
        outcome.counters.truncated =
            options.skip + options.limit < outcome.records.len().saturating_add(1);
        outcome.records.push(TextRecord::Summary {
            counters: outcome.counters,
        });
    }

    let exit_code = classify_text_exit_code(&text_exit_input(&outcome, &options)).code();

    if options.null_paths {
        let path_records: Vec<_> = outcome
            .records
            .iter()
            .filter(|record| {
                matches!(
                    record,
                    TextRecord::PathMatch { .. } | TextRecord::PathNoMatch { .. }
                )
            })
            .cloned()
            .collect();
        return TextCommandResult {
            stdout: String::new(),
            stdout_bytes: Some(render_null_path_records(&path_records)),
            stderr: String::new(),
            exit_code,
        };
    }

    TextCommandResult {
        stdout: render_text_records(
            &outcome.records,
            TextRenderOptions::resume(options.skip, options.limit),
        ),
        stdout_bytes: None,
        stderr: String::new(),
        exit_code,
    }
}

fn should_insert_text_summary(counters: &TextSummaryCounters, mode: &TextSummaryMode) -> bool {
    match mode {
        TextSummaryMode::Always => true,
        TextSummaryMode::Never => false,
        TextSummaryMode::Diagnostics => {
            counters.skipped > 0 || counters.warnings > 0 || counters.errors > 0
        }
    }
}

fn text_exit_input(
    outcome: &TextCommandTraversalOutcome,
    options: &TextCommandFinalizeOptions,
) -> TextExitClassificationInput {
    let success = match options.success_criteria {
        TextExitSuccessCriteria::GrepMatchedFiles => outcome.matched_files > 0,
        TextExitSuccessCriteria::GrepPathNoMatchRecords => outcome
            .records
            .iter()
            .any(|record| matches!(record, TextRecord::PathNoMatch { .. })),
        TextExitSuccessCriteria::ChangedFiles => outcome.changed_files > 0,
    };

    let mut exit_input = match outcome.operation {
        TextOperationKind::Grep => TextExitClassificationInput::grep(success),
        TextOperationKind::SedPreview => TextExitClassificationInput::sed_preview(success),
        TextOperationKind::SedWrite => TextExitClassificationInput::sed_write(success),
    };
    exit_input.warnings = outcome.counters.warnings;
    exit_input.partial_traversal_failure = outcome.partial_traversal_failure;
    if outcome.counters.errors > 0 {
        match outcome.operation {
            TextOperationKind::SedWrite => exit_input.write_failure = true,
            TextOperationKind::Grep | TextOperationKind::SedPreview => {
                exit_input.fatal_error = true;
            }
        }
    }
    exit_input
}

fn validate_grep_output_modes(args: &GrepArgs) -> Result<Option<TextCommandResult>> {
    let path_mode_count = args.files_with_matches as usize
        + args.files_without_match as usize
        + args.paths_only as usize;
    if path_mode_count > 1 {
        return Ok(Some(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "path-family modes are mutually exclusive",
            TextExitClassificationInput::invalid_input(TextOperationKind::Grep),
        )));
    }
    let path_family = path_mode_count == 1;
    let output_modes = args.count_only as usize + path_family as usize;
    if output_modes > 1 {
        return Ok(Some(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "count and path-family modes are mutually exclusive",
            TextExitClassificationInput::invalid_input(TextOperationKind::Grep),
        )));
    }
    if args.null && !path_family {
        return Ok(Some(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "--null requires --paths-only, --files-with-matches, or --files-without-match",
            TextExitClassificationInput::invalid_input(TextOperationKind::Grep),
        )));
    }
    if args.limit == 0 {
        return Ok(Some(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "--limit must be greater than zero",
            TextExitClassificationInput::invalid_input(TextOperationKind::Grep),
        )));
    }
    Ok(None)
}

fn render_text_error_result(
    label: TextErrorLabel,
    reason: &str,
    exit_input: TextExitClassificationInput,
) -> TextCommandResult {
    TextCommandResult {
        stdout: String::new(),
        stdout_bytes: None,
        stderr: render_text_records(
            &[TextRecord::Error {
                label,
                path: None,
                reason: reason.to_string(),
            }],
            TextRenderOptions::unbounded(),
        ),
        exit_code: classify_text_exit_code(&exit_input).code(),
    }
}

fn text_error_from_message(
    operation: TextOperationKind,
    message: &str,
) -> Result<TextCommandResult> {
    if message.starts_with("error: invalid-path: ") {
        return Ok(render_text_error_result(
            TextErrorLabel::InvalidPath,
            strip_error_label(message, "error: invalid-path: "),
            TextExitClassificationInput::invalid_path(operation),
        ));
    }
    if message.starts_with("error: invalid-input: ") {
        return Ok(render_text_error_result(
            TextErrorLabel::InvalidInput,
            strip_error_label(message, "error: invalid-input: "),
            TextExitClassificationInput::invalid_input(operation),
        ));
    }
    Ok(render_text_error_result(
        TextErrorLabel::InvalidInput,
        message,
        TextExitClassificationInput::invalid_input(operation),
    ))
}

fn strip_error_label<'a>(message: &'a str, prefix: &str) -> &'a str {
    message.strip_prefix(prefix).unwrap_or(message)
}

fn grep_line_matches(text: &str, matcher: &Regex) -> Vec<GrepMatchRecord> {
    let mut out = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        for found in matcher.find_iter(line) {
            out.push(GrepMatchRecord {
                line: line_index + 1,
                byte: found.start() + 1,
                text: line.to_string(),
            });
        }
    }
    out
}

fn push_grep_records_for_matches(
    records: &mut Vec<TextRecord>,
    path: TextPath,
    text: &str,
    matches: &[GrepMatchRecord],
    before: usize,
    after: usize,
    args: &GrepArgs,
) {
    if args.count_only {
        records.push(TextRecord::GrepCount {
            path,
            count: matches.len(),
        });
        return;
    }

    if args.files_with_matches || args.paths_only {
        if !matches.is_empty() {
            records.push(TextRecord::PathMatch { path });
        }
        return;
    }

    if args.files_without_match {
        if matches.is_empty() {
            records.push(TextRecord::PathNoMatch { path });
        }
        return;
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut last_context_key: Option<(GrepContextKind, usize)> = None;
    for found in matches {
        let start = found.line.saturating_sub(before + 1);
        for line_index in start..found.line.saturating_sub(1) {
            if last_context_key == Some((GrepContextKind::Before, line_index + 1)) {
                continue;
            }
            records.push(TextRecord::GrepContext {
                kind: GrepContextKind::Before,
                path: path.clone(),
                line: line_index + 1,
                text: lines
                    .get(line_index)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
            });
            last_context_key = Some((GrepContextKind::Before, line_index + 1));
        }
        records.push(TextRecord::GrepMatch {
            path: path.clone(),
            line: found.line,
            byte: found.byte,
            text: found.text.clone(),
        });
        let end = (found.line + after).min(lines.len());
        for line_index in found.line..end {
            if last_context_key == Some((GrepContextKind::After, line_index + 1)) {
                continue;
            }
            records.push(TextRecord::GrepContext {
                kind: GrepContextKind::After,
                path: path.clone(),
                line: line_index + 1,
                text: lines
                    .get(line_index)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
            });
            last_context_key = Some((GrepContextKind::After, line_index + 1));
        }
    }
}

// =============================================================================
// sed (preview) — shares the same TextCommandContext/Outcome boundary as grep
// =============================================================================

pub(crate) struct SedArgs {
    pub(crate) expression: Option<String>,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) regex: Option<String>,
    pub(crate) replace: Option<String>,
    pub(crate) fixed: Vec<String>,
    pub(crate) pattern_file: Option<PathBuf>,
    pub(crate) replacement_file: Option<PathBuf>,
    pub(crate) ignore_case: bool,
    pub(crate) global: bool,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) glob_globs: Vec<String>,
    pub(crate) line: Option<String>,
    /// Explicit preview-mode marker. Preview is the v1 default, so this flag
    /// only exists to document intent in scripts; clap's `conflicts_with`
    /// already rejects `--preview --write` at parse time so we never observe
    /// both set here.
    #[allow(dead_code)]
    pub(crate) preview: bool,
    pub(crate) write: bool,
    pub(crate) limit: usize,
    pub(crate) skip: usize,
    pub(crate) pattern_stdin: bool,
    pub(crate) replacement_stdin: bool,
    pub(crate) quiet: bool,
}

/// Parsed substitution payload after combining argv, pattern/replacement files,
/// and `s<delim>...<delim>...<delim>flags` expressions.
struct SedSubstitution {
    pattern: String,
    replacement: String,
    fixed: bool,
    ignore_case: bool,
    global: bool,
}

struct SedReadRequest {
    lines: String,
    paths: Vec<PathBuf>,
}

/// Inclusive 1-based line range, both endpoints optional.
#[derive(Default, Clone, Copy)]
struct LineRange {
    start: Option<usize>,
    end: Option<usize>,
}

impl LineRange {
    fn contains(&self, line: usize) -> bool {
        if let Some(start) = self.start {
            if line < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if line > end {
                return false;
            }
        }
        true
    }
}

pub(crate) fn cmd_sed(args: SedArgs) -> Result<()> {
    let operation = resolve_sed_operation(&args);
    run_text_command(operation, || run_sed(args, operation))
}

fn resolve_sed_operation(args: &SedArgs) -> TextOperationKind {
    if args.write {
        TextOperationKind::SedWrite
    } else {
        TextOperationKind::SedPreview
    }
}

fn run_sed(args: SedArgs, operation: TextOperationKind) -> Result<TextCommandResult> {
    match resolve_sed_read_request(&args, operation) {
        Ok(Some(read_request)) => return run_sed_read(read_request, operation),
        Ok(None) => {}
        Err(result) => return Ok(result),
    }

    // The contract reserves a specific diagnostic for `--write -`, distinct
    // from the generic invalid-path stdin marker. Resolve it before path
    // collection so the message is byte-identical to SS-A008.
    if args.write
        && args
            .paths
            .iter()
            .any(|path| path == std::path::Path::new(agent_fs::text_ops::STDIN_MARKER))
    {
        return Ok(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "--write cannot target stdin",
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    if args.pattern_stdin || args.replacement_stdin {
        return Ok(render_text_error_result(
            TextErrorLabel::Unsupported,
            "stdin payload modes are deferred",
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    let substitution = match resolve_sed_substitution(&args, operation) {
        Ok(sub) => sub,
        Err(result) => return Ok(result),
    };

    let line_range = match parse_line_range(args.line.as_deref(), operation) {
        Ok(range) => range,
        Err(result) => return Ok(result),
    };

    if args.limit == 0 {
        return Ok(render_text_error_result(
            TextErrorLabel::InvalidInput,
            "--limit must be greater than zero",
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    let matcher = match build_text_matcher(TextMatcherConfig {
        pattern: substitution.pattern.clone(),
        fixed: substitution.fixed,
        ignore_case: substitution.ignore_case,
    }) {
        Ok(matcher) => matcher,
        Err(err) => {
            return Ok(render_text_error_result(
                TextErrorLabel::InvalidExpression,
                strip_error_label(&err.to_string(), "error: invalid-expression: "),
                TextExitClassificationInput::invalid_expression(operation),
            ));
        }
    };

    if !substitution.fixed {
        if let Err(err) = validate_regex_replacement(&matcher, &substitution.replacement) {
            return Ok(render_text_error_result(
                TextErrorLabel::InvalidExpression,
                strip_error_label(&err, "error: invalid-expression: "),
                TextExitClassificationInput::invalid_expression(operation),
            ));
        }
    }

    let cwd = std::env::current_dir()?;
    // sed preview accepts default `.` traversal and explicit paths; stdin
    // input mode is reserved for a future streaming contract per
    // docs/grep-sed-contract.md, so we don't allow `-` operands here.
    let options = text_target_options(
        args.include_globs.clone(),
        args.exclude_globs.clone(),
        args.glob_globs.clone(),
        false,
    );
    let files = match collect_text_files(&cwd, &args.paths, &options) {
        Ok(files) => files,
        Err(err) => {
            return text_error_from_message(operation, &err.to_string());
        }
    };

    let collected = collect_text_command_outcomes(operation, &files, |context| {
        let path = context.path().clone();
        let (records, replacements) =
            sed_preview_records(&path, context.text(), &matcher, &substitution, line_range);
        if operation == TextOperationKind::SedWrite {
            // The write path consumes the same preview records to derive a
            // canonical replacement record id for the first replacement, then
            // applies the substitution to the decoded text and atomically
            // rewrites the file on disk. Drift / IO failures surface as
            // recoverable per-file outcomes; subsequent files are still
            // processed per the partial-failure contract.
            Ok(sed_write_outcome(SedWriteInput {
                path: &path,
                text: context.text(),
                matcher: &matcher,
                substitution: &substitution,
                range: line_range,
                replacements,
                preview_records: records,
                text_file: context.text_file(),
            }))
        } else {
            Ok(TextCommandOutcome::sed_preview(records, replacements))
        }
    })?;

    Ok(finalize_text_command_output(
        collected,
        TextCommandFinalizeOptions {
            summary_mode: TextSummaryMode::Always,
            success_criteria: TextExitSuccessCriteria::ChangedFiles,
            null_paths: false,
            skip: args.skip,
            limit: args.limit,
        },
    ))
}

fn resolve_sed_read_request(
    args: &SedArgs,
    operation: TextOperationKind,
) -> std::result::Result<Option<SedReadRequest>, TextCommandResult> {
    if args.quiet || args.expression.as_deref() == Some("-n") {
        return resolve_sed_quiet_read_request(args, operation).map(Some);
    }

    if args.line.is_none()
        || args.write
        || args.regex.is_some()
        || !args.fixed.is_empty()
        || args.pattern_file.is_some()
        || args.pattern_stdin
    {
        return Ok(None);
    }

    let Some(lines) = args.line.clone() else {
        return Ok(None);
    };
    let mut paths = args.paths.clone();
    match args.expression.as_ref() {
        Some(expression) if should_treat_sed_expression_as_read_path(expression) => {
            paths.insert(0, PathBuf::from(expression));
        }
        Some(_) => return Ok(None),
        None => {}
    }

    if paths.is_empty() {
        return Err(render_text_error_result(
            TextErrorLabel::Unsupported,
            &sed_read_fallback_hint(None, Some(&lines)),
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    Ok(Some(SedReadRequest { lines, paths }))
}

fn resolve_sed_quiet_read_request(
    args: &SedArgs,
    operation: TextOperationKind,
) -> std::result::Result<SedReadRequest, TextCommandResult> {
    let (script, paths) = if args.quiet {
        (args.expression.as_deref(), args.paths.as_slice())
    } else {
        (
            args.paths.first().and_then(|path| path.to_str()),
            args.paths.get(1..).unwrap_or(&[]),
        )
    };

    let Some(script) = script else {
        return Err(render_text_error_result(
            TextErrorLabel::Unsupported,
            &sed_read_fallback_hint(None, None),
            TextExitClassificationInput::invalid_input(operation),
        ));
    };
    let Some(lines) = sed_print_script_to_read_lines(script) else {
        return Err(render_text_error_result(
            TextErrorLabel::Unsupported,
            &sed_read_fallback_hint(paths.first(), None),
            TextExitClassificationInput::invalid_input(operation),
        ));
    };
    if paths.is_empty() {
        return Err(render_text_error_result(
            TextErrorLabel::Unsupported,
            &sed_read_fallback_hint(None, Some(&lines)),
            TextExitClassificationInput::invalid_input(operation),
        ));
    }

    Ok(SedReadRequest {
        lines,
        paths: paths.to_vec(),
    })
}

fn run_sed_read(
    request: SedReadRequest,
    operation: TextOperationKind,
) -> Result<TextCommandResult> {
    let mut stdout = String::new();
    for path in &request.paths {
        match crate::cmd_read::read_lines_to_string(path, Some(&request.lines)) {
            Ok(text) => stdout.push_str(&text),
            Err(err) => {
                return Ok(render_text_error_result(
                    TextErrorLabel::InvalidPath,
                    &err.to_string(),
                    TextExitClassificationInput::invalid_path(operation),
                ));
            }
        }
    }

    Ok(TextCommandResult {
        stdout,
        stdout_bytes: None,
        stderr: String::new(),
        exit_code: 0,
    })
}

fn sed_read_fallback_hint(path: Option<&PathBuf>, lines: Option<&str>) -> String {
    let read_hint = match (path, lines) {
        (Some(path), Some(lines)) => {
            format!("`agent-tools read {} --lines {lines}`", path.display())
        }
        (Some(path), None) => format!("`agent-tools read {} --lines START:END`", path.display()),
        (None, Some(lines)) => format!("`agent-tools read <path> --lines {lines}`"),
        (None, None) => "`agent-tools read <path> --lines START:END`".to_string(),
    };

    format!(
        "we do not have this implemented; you may use similar functionality with {read_hint} for line ranges or `agent-tools grep <pattern> <path>` for matching lines"
    )
}

fn should_treat_sed_expression_as_read_path(expression: &str) -> bool {
    if std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(expression).exists())
        .unwrap_or(false)
    {
        return true;
    }

    !looks_like_sed_substitution_expression(expression)
}

fn looks_like_sed_substitution_expression(expression: &str) -> bool {
    let mut chars = expression.chars();
    if chars.next() != Some('s') {
        return false;
    }
    let Some(delim) = chars.next() else {
        return false;
    };
    !(delim.is_alphanumeric() || delim == '\\' || delim.is_whitespace())
}

fn sed_print_script_to_read_lines(script: &str) -> Option<String> {
    let body = script.trim().strip_suffix('p')?.trim();
    if body.is_empty() {
        return None;
    }

    if let Some((start, end)) = body.split_once(',') {
        let start = start.trim();
        let end = end.trim();
        if !is_one_based_line_address(start) || !is_supported_end_line_address(end) {
            return None;
        }
        let end = if end == "$" { "" } else { end };
        return Some(format!("{start}:{end}"));
    }

    is_one_based_line_address(body).then(|| body.to_string())
}

fn is_one_based_line_address(value: &str) -> bool {
    value.parse::<usize>().is_ok_and(|line| line > 0)
}

fn is_supported_end_line_address(value: &str) -> bool {
    value == "$" || is_one_based_line_address(value)
}

/// Apply the validated sed substitution to one file's decoded text and produce
/// the shared `TextCommandOutcome` for the write traversal pass. Behavior:
///
/// - Files with zero replacements emit no write/skip record and leave disk
///   untouched (the no-op-write contract).
/// - Files where the rewritten bytes are byte-identical to the original
///   (e.g. the regex matched but the replacement string equals the match)
///   are NOT rewritten and a `warning: write-unchanged` is recorded.
/// - Files whose on-disk snapshot drifted between preview classification and
///   write time emit `warning: write-drift` and are not mutated. Drift flips
///   partial-failure so the command exits 3 per the contract.
/// - Atomic write or rename failures emit `error: write-failed` and also flip
///   partial-failure. Traversal continues so later files still get a chance.
struct SedWriteInput<'a> {
    path: &'a TextPath,
    text: &'a str,
    matcher: &'a Regex,
    substitution: &'a SedSubstitution,
    range: LineRange,
    replacements: usize,
    preview_records: Vec<TextRecord>,
    text_file: Option<&'a TextFile>,
}

fn sed_write_outcome(input: SedWriteInput<'_>) -> TextCommandOutcome {
    let SedWriteInput {
        path,
        text,
        matcher,
        substitution,
        range,
        replacements,
        preview_records,
        text_file,
    } = input;

    // Zero replacements: the file was scanned and has nothing to rewrite. The
    // write path emits neither a write record nor a skip; the shared summary
    // counts it as unchanged.
    if replacements == 0 {
        return TextCommandOutcome::sed_write(Vec::new(), 0, false, 0, 0, false);
    }

    let snapshot = match text_file.and_then(|tf| tf.snapshot.as_ref()) {
        Some(snapshot) => snapshot,
        None => {
            // Stdin or otherwise non-file context. Sed write rejects stdin
            // earlier, so reaching this branch implies a programmer error.
            return TextCommandOutcome::sed_write(
                vec![TextRecord::Error {
                    label: TextErrorLabel::WriteFailed,
                    path: Some(path.clone()),
                    reason: "no preview snapshot available for write preflight".to_string(),
                }],
                0,
                false,
                0,
                1,
                true,
            );
        }
    };

    let new_text = apply_sed_substitution(text, matcher, substitution, range);
    let new_bytes = encode_text_with_bom(&new_text, snapshot);
    let old_size = snapshot.len;

    // Byte-identical rewrite. Per the contract this must NOT touch disk and
    // produces a `warning: write-unchanged` so users can audit which files
    // matched but produced no net change (e.g. replacing `foo` with `foo`).
    if new_bytes.len() as u64 == old_size {
        // Cheap path-equality short circuit before re-reading the original
        // bytes: if sizes match AND the encoded body equals the snapshot's
        // content hash of the original, we know they're identical.
        let same_hash = stable_content_hash(&new_bytes) == snapshot.content_hash;
        if same_hash {
            return TextCommandOutcome::sed_write(
                vec![TextRecord::Warning {
                    label: agent_core::TextWarningLabel::WriteUnchanged,
                    path: Some(path.clone()),
                    reason: "rewritten content is byte-identical to source".to_string(),
                }],
                0,
                false,
                1,
                0,
                false,
            );
        }
    }

    // Drift check: re-read the file at write time and compare to the preview
    // snapshot. Any mismatch in size, hash, or identity means the file
    // changed under us; we MUST NOT overwrite stale content.
    match recheck_file_drift(&snapshot.opened_path, snapshot) {
        DriftCheck::Unchanged => {}
        DriftCheck::Drifted { reason } | DriftCheck::Missing { reason } => {
            return TextCommandOutcome::sed_write(
                vec![TextRecord::Warning {
                    label: agent_core::TextWarningLabel::WriteDrift,
                    path: Some(path.clone()),
                    reason,
                }],
                0,
                false,
                1,
                0,
                true,
            );
        }
    }

    // Derive a stable write record id from the first preview record so
    // preview and write outputs cross-reference cleanly.
    let record_id = preview_records
        .iter()
        .find_map(|record| match record {
            TextRecord::SedPreview { record_id, .. } => Some(record_id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| ReplacementRecordId::new(format!("r:{}:write", path.as_str())));

    match atomic_write_bytes(&snapshot.opened_path, &new_bytes, snapshot) {
        Ok(new_size) => TextCommandOutcome::sed_write(
            vec![TextRecord::SedWrite {
                record_id,
                path: path.clone(),
                replacements,
                old_size,
                new_size,
            }],
            replacements,
            true,
            0,
            0,
            false,
        ),
        Err(err) => TextCommandOutcome::sed_write(
            vec![TextRecord::Error {
                label: TextErrorLabel::WriteFailed,
                path: Some(path.clone()),
                reason: err.to_string(),
            }],
            0,
            false,
            0,
            1,
            true,
        ),
    }
}

/// Apply the validated substitution to decoded line text and return the new
/// decoded body. Line endings inside `text` are preserved character-by-
/// character because we walk lines via `split_inclusive('\n')`, which keeps
/// the per-line terminator (LF or CRLF) attached. The trailing-newline
/// presence is preserved by the same iteration: if the source body had no
/// terminator after the last line, neither does the output.
fn apply_sed_substitution(
    text: &str,
    matcher: &Regex,
    substitution: &SedSubstitution,
    range: LineRange,
) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line_with_term) in text.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        if !range.contains(line_number) {
            out.push_str(line_with_term);
            continue;
        }
        // Separate the line text from its terminator so the matcher operates
        // on decoded content without seeing `\n` or `\r\n` sequences.
        let (line_text, term) = split_line_terminator(line_with_term);

        let mut rewritten = String::with_capacity(line_text.len());
        let mut last_end = 0usize;
        for found in matcher.find_iter(line_text) {
            rewritten.push_str(&line_text[last_end..found.start()]);
            if substitution.fixed {
                rewritten.push_str(&substitution.replacement);
            } else {
                let caps = matcher
                    .captures(&line_text[found.start()..])
                    .expect("captures must match a slice that already matched");
                caps.expand(&substitution.replacement, &mut rewritten);
            }
            last_end = found.end();
            if !substitution.global {
                break;
            }
        }
        rewritten.push_str(&line_text[last_end..]);
        out.push_str(&rewritten);
        out.push_str(term);
    }
    out
}

fn split_line_terminator(line: &str) -> (&str, &str) {
    if let Some(stripped) = line.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = line.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (line, "")
    }
}

/// Compose pattern/replacement/flags from the supported channels:
///   * `--regex P --replace R` argv form
///   * `--fixed OLD NEW` argv form
///   * sed-like positional expression `s<delim>PAT<delim>REPL<delim>FLAGS`
///   * `--pattern-file` / `--replacement-file` UTF-8 payload files
///
/// `--ignore-case` and `--global` argv flags compose with the expression flags
/// so e.g. `--ignore-case` plus `s/foo/bar/g` is equivalent to `s/foo/bar/gi`.
fn resolve_sed_substitution(
    args: &SedArgs,
    operation: TextOperationKind,
) -> std::result::Result<SedSubstitution, TextCommandResult> {
    let mut chosen: Option<SedSubstitution> = None;

    if let Some(regex) = args.regex.as_ref() {
        let replacement = match (args.replace.as_ref(), args.replacement_file.as_ref()) {
            (Some(replace), None) => replace.clone(),
            (None, Some(path)) => read_payload_file(path, "replacement-file", operation)?,
            (None, None) => {
                return Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    "--regex requires --replace or --replacement-file",
                    TextExitClassificationInput::invalid_input(operation),
                ));
            }
            (Some(_), Some(_)) => {
                return Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    "--replace and --replacement-file conflict",
                    TextExitClassificationInput::invalid_input(operation),
                ));
            }
        };
        chosen = Some(SedSubstitution {
            pattern: regex.clone(),
            replacement,
            fixed: false,
            ignore_case: false,
            global: false,
        });
    }

    if !args.fixed.is_empty() {
        if chosen.is_some() {
            return Err(render_text_error_result(
                TextErrorLabel::InvalidInput,
                "--fixed conflicts with other payload channels",
                TextExitClassificationInput::invalid_input(operation),
            ));
        }
        if args.fixed.len() != 2 {
            return Err(render_text_error_result(
                TextErrorLabel::InvalidInput,
                "--fixed requires exactly OLD and NEW",
                TextExitClassificationInput::invalid_input(operation),
            ));
        }
        chosen = Some(SedSubstitution {
            pattern: args.fixed[0].clone(),
            replacement: args.fixed[1].clone(),
            fixed: true,
            ignore_case: false,
            global: false,
        });
    }

    if let Some(path) = args.pattern_file.as_ref() {
        if chosen.is_some() {
            return Err(render_text_error_result(
                TextErrorLabel::InvalidInput,
                "--pattern-file conflicts with other payload channels",
                TextExitClassificationInput::invalid_input(operation),
            ));
        }
        let pattern = read_payload_file(path, "pattern-file", operation)?;
        let replacement = match (args.replace.as_ref(), args.replacement_file.as_ref()) {
            (Some(replace), None) => replace.clone(),
            (None, Some(path)) => read_payload_file(path, "replacement-file", operation)?,
            (None, None) => {
                return Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    "--pattern-file requires --replace or --replacement-file",
                    TextExitClassificationInput::invalid_input(operation),
                ));
            }
            (Some(_), Some(_)) => {
                return Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    "--replace and --replacement-file conflict",
                    TextExitClassificationInput::invalid_input(operation),
                ));
            }
        };
        chosen = Some(SedSubstitution {
            pattern,
            replacement,
            fixed: false,
            ignore_case: false,
            global: false,
        });
    }

    if let Some(expression) = args.expression.as_ref() {
        if chosen.is_some() {
            return Err(render_text_error_result(
                TextErrorLabel::InvalidInput,
                "sed expression conflicts with explicit payload flags",
                TextExitClassificationInput::invalid_input(operation),
            ));
        }
        match parse_sed_expression(expression) {
            Ok(sub) => {
                chosen = Some(sub);
            }
            Err(reason) => {
                return Err(render_text_error_result(
                    TextErrorLabel::InvalidExpression,
                    &reason,
                    TextExitClassificationInput::invalid_expression(operation),
                ));
            }
        }
    }

    let mut sub = chosen.ok_or_else(|| {
        render_text_error_result(
            TextErrorLabel::InvalidExpression,
            "missing expression",
            TextExitClassificationInput::invalid_expression(operation),
        )
    })?;

    if args.ignore_case {
        sub.ignore_case = true;
    }
    if args.global {
        sub.global = true;
    }

    Ok(sub)
}

/// Read a `--pattern-file` / `--replacement-file` payload and wrap any I/O or
/// UTF-8 decode failure into an `invalid-input` [`TextCommandResult`] for the
/// given `operation`.
///
/// Centralizes the read+wrap path shared by `resolve_text_pattern()` (grep
/// `--pattern-file`) and sed's `--pattern-file` / `--replacement-file`
/// branches so future drift-check / preflight policy (T007) can extend a
/// single location rather than forking per call site.
///
/// `field` is the user-facing flag stem (e.g. `"pattern-file"` /
/// `"replacement-file"`) embedded in the error message; `operation` drives
/// the exit-code classification. Error messages are byte-identical to the
/// pre-refactor grep / sed payload-file failure paths.
fn resolve_text_payload_file(
    path: &std::path::Path,
    field: &str,
    operation: TextOperationKind,
) -> std::result::Result<String, TextCommandResult> {
    std::fs::read_to_string(path).map_err(|err| {
        // Grep historically rendered `<field> is not valid UTF-8: <path>` via
        // `anyhow::Context::with_context` + `strip_error_label`, without the
        // io::Error suffix. Sed rendered `<field> is not valid UTF-8: <path>
        // (<err>)`. We preserve both by branching on `operation`; both forms
        // are exercised only off the conformance happy-path, but keeping them
        // byte-identical avoids surprising downstream agents that may parse
        // the messages.
        let message = match operation {
            TextOperationKind::Grep => {
                format!("{field} is not valid UTF-8: {}", path.display())
            }
            TextOperationKind::SedPreview | TextOperationKind::SedWrite => {
                format!("{field} is not valid UTF-8: {} ({err})", path.display())
            }
        };
        render_text_error_result(
            TextErrorLabel::InvalidInput,
            &message,
            TextExitClassificationInput::invalid_input(operation),
        )
    })
}

fn read_payload_file(
    path: &std::path::Path,
    field: &str,
    operation: TextOperationKind,
) -> std::result::Result<String, TextCommandResult> {
    resolve_text_payload_file(path, field, operation)
}

/// Parse `s<delim>pattern<delim>replacement<delim>flags`.
///
/// The delimiter is the first character after `s`. Inside `pattern` and
/// `replacement`, the delimiter may be escaped as `\<delim>`; other backslash
/// escapes are passed through unchanged so the regex/literal engines below
/// see the original payload.
fn parse_sed_expression(expression: &str) -> std::result::Result<SedSubstitution, String> {
    let mut chars = expression.chars();
    let leading = chars.next().ok_or_else(|| "empty expression".to_string())?;
    if leading != 's' {
        return Err(format!(
            "only substitution commands are supported (got `{leading}`)"
        ));
    }
    let delim = chars
        .next()
        .ok_or_else(|| "expression missing delimiter".to_string())?;
    if delim.is_alphanumeric() || delim == '\\' || delim.is_whitespace() {
        return Err(format!(
            "delimiter `{delim}` must be a non-alphanumeric, non-backslash, non-whitespace character"
        ));
    }

    let rest: String = chars.collect();
    let mut sections: Vec<String> = Vec::with_capacity(3);
    let mut current = String::new();
    let mut escape = false;
    for ch in rest.chars() {
        if escape {
            if ch != delim {
                current.push('\\');
            }
            current.push(ch);
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == delim {
            sections.push(std::mem::take(&mut current));
            if sections.len() == 3 {
                break;
            }
        } else {
            current.push(ch);
        }
    }
    if escape {
        current.push('\\');
    }
    if sections.len() == 2 {
        sections.push(std::mem::take(&mut current));
    } else if sections.len() == 3 && !current.is_empty() {
        return Err("trailing text after sed expression".to_string());
    } else if sections.len() < 2 {
        return Err(
            "expression must use the form s<delim>pattern<delim>replacement<delim>flags"
                .to_string(),
        );
    }

    let pattern = sections[0].clone();
    let replacement = sections[1].clone();
    let flags = sections.get(2).cloned().unwrap_or_default();

    let mut ignore_case = false;
    let mut global = false;
    let mut seen = std::collections::HashSet::new();
    for flag in flags.chars() {
        if !seen.insert(flag) {
            return Err(format!("repeated flag `{flag}`"));
        }
        match flag {
            'g' => global = true,
            'i' => ignore_case = true,
            other => return Err(format!("unsupported flag `{other}`")),
        }
    }

    if pattern.is_empty() {
        return Err("empty pattern".to_string());
    }

    Ok(SedSubstitution {
        pattern,
        replacement,
        fixed: false,
        ignore_case,
        global,
    })
}

fn parse_line_range(
    raw: Option<&str>,
    operation: TextOperationKind,
) -> std::result::Result<LineRange, TextCommandResult> {
    let Some(raw) = raw else {
        return Ok(LineRange::default());
    };
    let (start_raw, end_raw) = raw.split_once(':').ok_or_else(|| {
        render_text_error_result(
            TextErrorLabel::InvalidInput,
            "--line must be START:END (either endpoint may be empty)",
            TextExitClassificationInput::invalid_input(operation),
        )
    })?;
    let parse_endpoint =
        |value: &str, label: &str| -> std::result::Result<Option<usize>, TextCommandResult> {
            if value.is_empty() {
                return Ok(None);
            }
            match value.parse::<usize>() {
                Ok(0) => Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    &format!("--line {label} must be one-based"),
                    TextExitClassificationInput::invalid_input(operation),
                )),
                Ok(value) => Ok(Some(value)),
                Err(_) => Err(render_text_error_result(
                    TextErrorLabel::InvalidInput,
                    &format!("--line {label} must be a non-negative integer"),
                    TextExitClassificationInput::invalid_input(operation),
                )),
            }
        };
    let start = parse_endpoint(start_raw, "start")?;
    let end = parse_endpoint(end_raw, "end")?;
    if let (Some(start), Some(end)) = (start, end) {
        if end < start {
            return Err(render_text_error_result(
                TextErrorLabel::InvalidInput,
                "--line end is before start",
                TextExitClassificationInput::invalid_input(operation),
            ));
        }
    }
    Ok(LineRange { start, end })
}

/// Validate that a regex replacement template references only captures the
/// compiled regex actually exposes. The renderer doesn't surface
/// `regex::Replacer` errors at run time, so we shake them out up front so the
/// CLI fails with `error: invalid-expression:` rather than silently producing
/// empty expansions.
fn validate_regex_replacement(regex: &Regex, replacement: &str) -> std::result::Result<(), String> {
    let bytes = replacement.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'$' && index + 1 < bytes.len() {
            let next = bytes[index + 1];
            if next == b'$' {
                index += 2;
                continue;
            }
            if next == b'{' {
                let close = bytes[index + 2..]
                    .iter()
                    .position(|b| *b == b'}')
                    .ok_or_else(|| "unterminated `${...}` capture reference".to_string())?;
                let name = std::str::from_utf8(&bytes[index + 2..index + 2 + close])
                    .map_err(|_| "invalid capture name encoding".to_string())?;
                check_capture_name(regex, name)?;
                index += 2 + close + 1;
                continue;
            }
            if next.is_ascii_digit() {
                let mut end = index + 2;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                let name = std::str::from_utf8(&bytes[index + 1..end])
                    .map_err(|_| "invalid capture index encoding".to_string())?;
                check_capture_name(regex, name)?;
                index = end;
                continue;
            }
        }
        index += 1;
    }
    Ok(())
}

fn check_capture_name(regex: &Regex, name: &str) -> std::result::Result<(), String> {
    if let Ok(index) = name.parse::<usize>() {
        if index >= regex.captures_len() {
            return Err(format!("unknown capture `{name}`"));
        }
        return Ok(());
    }
    if regex.capture_names().flatten().any(|cap| cap == name) {
        return Ok(());
    }
    Err(format!("unknown capture `{name}`"))
}

/// Produce sed preview records for one decoded text body. Returns the records
/// plus the total replacement count (not file count) for summary aggregation.
fn sed_preview_records(
    path: &TextPath,
    text: &str,
    matcher: &Regex,
    substitution: &SedSubstitution,
    range: LineRange,
) -> (Vec<TextRecord>, usize) {
    let path_hash = relative_path_hash(path.as_str());
    let mut records = Vec::new();
    let mut total_replacements = 0usize;

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        if !range.contains(line_number) {
            continue;
        }
        let mut match_index = 0usize;
        for found in matcher.find_iter(line) {
            match_index += 1;
            let old_text = found.as_str().to_string();
            let new_text = if substitution.fixed {
                substitution.replacement.clone()
            } else {
                // Re-run captures on the matched slice so $1/$name expand. We
                // already validated the template up front, so any expansion
                // failures here are programmer errors, not user errors.
                let captures = matcher
                    .captures(&line[found.start()..])
                    .expect("captures must match a slice that already matched");
                let mut buf = String::new();
                captures.expand(&substitution.replacement, &mut buf);
                buf
            };
            records.push(TextRecord::SedPreview {
                record_id: ReplacementRecordId::new(format!(
                    "r:{}:{}:{}:{}",
                    path_hash,
                    line_number,
                    found.start() + 1,
                    match_index,
                )),
                path: path.clone(),
                line: line_number,
                byte: found.start() + 1,
                old_text,
                new_text,
            });
            total_replacements += 1;
            if !substitution.global {
                break;
            }
        }
    }

    (records, total_replacements)
}

fn push_file_diagnostic(
    records: &mut Vec<TextRecord>,
    counters: &mut TextSummaryCounters,
    file: &TextFile,
) {
    if let Some(diagnostic) = &file.diagnostic {
        records.push(TextRecord::Skip {
            label: diagnostic.label.into(),
            path: diagnostic.path.as_deref().map(TextPath::new),
            reason: diagnostic.reason.clone(),
        });
        counters.skipped += 1;
        counters.warnings += 1;
    }
}
