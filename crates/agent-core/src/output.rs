use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Compact text for CLI usage (minimal tokens)
    Text,
    /// JSON for MCP server / programmatic usage
    Json,
}

pub struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn text() -> Self {
        Self::new(OutputFormat::Text)
    }

    pub fn json() -> Self {
        Self::new(OutputFormat::Json)
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// Format a serializable value according to the output format.
    /// For Text format, uses the provided text representation.
    /// For Json format, serializes to JSON.
    pub fn output<T: Serialize>(&self, text: &str, json_value: &T) -> String {
        match self.format {
            OutputFormat::Text => text.to_string(),
            OutputFormat::Json => {
                serde_json::to_string(json_value).unwrap_or_else(|_| text.to_string())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOperationResult {
    pub records: Vec<TextRecord>,
    pub exit: TextExitClassificationInput,
}

impl TextOperationResult {
    pub fn new(records: Vec<TextRecord>, exit: TextExitClassificationInput) -> Self {
        Self { records, exit }
    }

    pub fn exit_code(&self) -> TextExitCode {
        classify_text_exit_code(&self.exit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRecord {
    GrepMatch {
        path: TextPath,
        line: usize,
        byte: usize,
        text: String,
    },
    GrepContext {
        kind: GrepContextKind,
        path: TextPath,
        line: usize,
        text: String,
    },
    GrepCount {
        path: TextPath,
        count: usize,
    },
    PathMatch {
        path: TextPath,
    },
    PathNoMatch {
        path: TextPath,
    },
    SedPreview {
        record_id: ReplacementRecordId,
        path: TextPath,
        line: usize,
        byte: usize,
        old_text: String,
        new_text: String,
    },
    SedWrite {
        record_id: ReplacementRecordId,
        path: TextPath,
        replacements: usize,
        old_size: u64,
        new_size: u64,
    },
    Skip {
        label: TextWarningLabel,
        path: Option<TextPath>,
        reason: String,
    },
    Warning {
        label: TextWarningLabel,
        path: Option<TextPath>,
        reason: String,
    },
    Error {
        label: TextErrorLabel,
        path: Option<TextPath>,
        reason: String,
    },
    Summary {
        counters: TextSummaryCounters,
    },
    Truncated {
        shown: usize,
        remaining: usize,
    },
    Resume {
        token: TextResumeToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextPath {
    display: String,
}

impl TextPath {
    pub fn new(display: impl Into<String>) -> Self {
        Self {
            display: display.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.display
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplacementRecordId {
    value: String,
}

impl ReplacementRecordId {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepContextKind {
    Before,
    After,
}

/// Contract-owned subset of warning labels emitted by shared traversal/decoding
/// (`agent-fs::text_ops`). These five variants are the labels a traversal layer
/// is allowed to produce per `docs/grep-sed-contract.md`; write-time labels
/// (`write-drift`, `write-unchanged`) are not reachable from traversal and live
/// only on [`TextWarningLabel`].
///
/// `agent-fs` re-exports this type as its diagnostic label so the traversal
/// layer cannot maintain a drifting string table. Conversion to the broader
/// [`TextWarningLabel`] is exhaustive and lossless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalWarningLabel {
    BinarySkipped,
    InvalidUtf8,
    UnsupportedEncoding,
    PathSkipped,
    TraversalError,
}

impl Serialize for TraversalWarningLabel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Use the contract-stable kebab-case name so traversal diagnostics
        // serialize identically to the renderer's `TextWarningLabel`.
        serializer.serialize_str(self.as_name())
    }
}

impl TraversalWarningLabel {
    /// Stable label name from the shared contract, e.g. `"binary-skipped"`.
    pub fn as_name(&self) -> &'static str {
        TextWarningLabel::from(*self).as_name()
    }

    /// Full record label as it appears in renderer output, e.g.
    /// `"warning: binary-skipped"`.
    pub fn as_label(&self) -> String {
        TextWarningLabel::from(*self).as_label()
    }
}

impl From<TraversalWarningLabel> for TextWarningLabel {
    fn from(value: TraversalWarningLabel) -> Self {
        match value {
            TraversalWarningLabel::BinarySkipped => Self::BinarySkipped,
            TraversalWarningLabel::InvalidUtf8 => Self::InvalidUtf8,
            TraversalWarningLabel::UnsupportedEncoding => Self::UnsupportedEncoding,
            TraversalWarningLabel::PathSkipped => Self::PathSkipped,
            TraversalWarningLabel::TraversalError => Self::TraversalError,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWarningLabel {
    BinarySkipped,
    InvalidUtf8,
    UnsupportedEncoding,
    PathSkipped,
    TraversalError,
    WriteDrift,
    WriteUnchanged,
}

impl TextWarningLabel {
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::BinarySkipped => "binary-skipped",
            Self::InvalidUtf8 => "invalid-utf8",
            Self::UnsupportedEncoding => "unsupported-encoding",
            Self::PathSkipped => "path-skipped",
            Self::TraversalError => "traversal-error",
            Self::WriteDrift => "write-drift",
            Self::WriteUnchanged => "write-unchanged",
        }
    }

    pub fn as_label(&self) -> String {
        format!("warning: {}", self.as_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextErrorLabel {
    InvalidExpression,
    InvalidInput,
    InvalidPath,
    Unsupported,
    PartialTraversalFailure,
    WriteFailed,
}

impl TextErrorLabel {
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::InvalidExpression => "invalid-expression",
            Self::InvalidInput => "invalid-input",
            Self::InvalidPath => "invalid-path",
            Self::Unsupported => "unsupported",
            Self::PartialTraversalFailure => "partial-traversal-failure",
            Self::WriteFailed => "write-failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextSummaryCounters {
    pub files: usize,
    pub matched: usize,
    pub changed: usize,
    pub replacements: usize,
    pub skipped: usize,
    pub warnings: usize,
    pub errors: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextResumeToken {
    pub skip: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRenderOptions {
    pub skip: usize,
    pub limit: Option<usize>,
}

impl TextRenderOptions {
    pub fn unbounded() -> Self {
        Self {
            skip: 0,
            limit: None,
        }
    }

    pub fn bounded(limit: usize) -> Self {
        Self {
            skip: 0,
            limit: Some(limit),
        }
    }

    pub fn resume(skip: usize, limit: usize) -> Self {
        Self {
            skip,
            limit: Some(limit),
        }
    }
}

pub fn render_text_records(records: &[TextRecord], options: TextRenderOptions) -> String {
    let start = options.skip.min(records.len());
    let remaining_records = &records[start..];
    let limit = options.limit.unwrap_or(remaining_records.len());
    let shown = remaining_records.len().min(limit);

    let mut out = String::new();
    for record in &remaining_records[..shown] {
        push_text_record(&mut out, record);
    }

    if shown < remaining_records.len() {
        let remaining = remaining_records.len() - shown;
        push_text_record(&mut out, &TextRecord::Truncated { shown, remaining });
        push_text_record(
            &mut out,
            &TextRecord::Resume {
                token: TextResumeToken {
                    skip: start + shown,
                    limit,
                },
            },
        );
    }

    out
}

pub fn render_text_result(result: &TextOperationResult, options: TextRenderOptions) -> String {
    render_text_records(&result.records, options)
}

pub fn render_null_path_records(records: &[TextRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    for record in records {
        match record {
            TextRecord::PathMatch { path } | TextRecord::PathNoMatch { path } => {
                out.extend_from_slice(path.as_str().as_bytes());
                out.push(0);
            }
            _ => {}
        }
    }
    out
}

fn push_text_record(out: &mut String, record: &TextRecord) {
    match record {
        TextRecord::GrepMatch {
            path,
            line,
            byte,
            text,
        } => push_line(out, format!("match: {}:{}:{}: {}", path.as_str(), line, byte, text)),
        TextRecord::GrepContext {
            kind,
            path,
            line,
            text,
        } => {
            let family = match kind {
                GrepContextKind::Before => "context-before",
                GrepContextKind::After => "context-after",
            };
            push_line(out, format!("{family}: {}:{}: {}", path.as_str(), line, text));
        }
        TextRecord::GrepCount { path, count } => {
            push_line(out, format!("count: {}: {}", path.as_str(), count));
        }
        TextRecord::PathMatch { path } => {
            push_line(out, format!("path-match: {}", path.as_str()));
        }
        TextRecord::PathNoMatch { path } => {
            push_line(out, format!("path-no-match: {}", path.as_str()));
        }
        TextRecord::SedPreview {
            record_id,
            path,
            line,
            byte,
            old_text,
            new_text,
        } => push_line(
            out,
            format!(
                "preview: {} {}:{}:{} {} => {}",
                record_id.as_str(),
                path.as_str(),
                line,
                byte,
                old_text,
                new_text
            ),
        ),
        TextRecord::SedWrite {
            record_id,
            path,
            replacements,
            old_size,
            new_size,
        } => push_line(
            out,
            format!(
                "write: {} {}: replacements={} bytes={}->{}",
                record_id.as_str(),
                path.as_str(),
                replacements,
                old_size,
                new_size
            ),
        ),
        TextRecord::Skip {
            label,
            path,
            reason,
        } => push_labeled_path_line(out, "skip", &label.as_label(), path.as_ref(), reason),
        TextRecord::Warning {
            label,
            path,
            reason,
        } => push_labeled_path_line(out, "warning", label.as_name(), path.as_ref(), reason),
        TextRecord::Error {
            label,
            path,
            reason,
        } => push_labeled_path_line(out, "error", label.as_name(), path.as_ref(), reason),
        TextRecord::Summary { counters } => push_line(
            out,
            format!(
                "summary: files={} matched={} changed={} replacements={} skipped={} warnings={} errors={} truncated={}",
                counters.files,
                counters.matched,
                counters.changed,
                counters.replacements,
                counters.skipped,
                counters.warnings,
                counters.errors,
                counters.truncated
            ),
        ),
        TextRecord::Truncated { shown, remaining } => {
            push_line(
                out,
                format!("truncated: output-limit shown={shown} remaining={remaining}"),
            );
        }
        TextRecord::Resume { token } => {
            push_line(
                out,
                format!("resume: --skip {} --limit {}", token.skip, token.limit),
            );
        }
    }
}

fn push_labeled_path_line(
    out: &mut String,
    family: &str,
    label: &str,
    path: Option<&TextPath>,
    reason: &str,
) {
    match path {
        Some(path) => push_line(
            out,
            format!("{family}: {label} {}: {reason}", path.as_str()),
        ),
        None => push_line(out, format!("{family}: {label}: {reason}")),
    }
}

fn push_line(out: &mut String, line: String) {
    out.push_str(&line);
    out.push('\n');
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOperationKind {
    Grep,
    SedPreview,
    SedWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextExitClassificationInput {
    pub operation: TextOperationKind,
    pub matched: bool,
    pub changed: bool,
    pub invalid_expression: bool,
    pub invalid_input: bool,
    pub invalid_path: bool,
    pub fatal_error: bool,
    pub partial_traversal_failure: bool,
    pub write_failure: bool,
    pub warnings: usize,
}

impl TextExitClassificationInput {
    pub fn grep(matched: bool) -> Self {
        Self {
            operation: TextOperationKind::Grep,
            matched,
            changed: false,
            invalid_expression: false,
            invalid_input: false,
            invalid_path: false,
            fatal_error: false,
            partial_traversal_failure: false,
            write_failure: false,
            warnings: 0,
        }
    }

    pub fn sed_preview(changed: bool) -> Self {
        Self {
            operation: TextOperationKind::SedPreview,
            matched: changed,
            changed,
            invalid_expression: false,
            invalid_input: false,
            invalid_path: false,
            fatal_error: false,
            partial_traversal_failure: false,
            write_failure: false,
            warnings: 0,
        }
    }

    pub fn sed_write(changed: bool) -> Self {
        Self {
            operation: TextOperationKind::SedWrite,
            matched: changed,
            changed,
            invalid_expression: false,
            invalid_input: false,
            invalid_path: false,
            fatal_error: false,
            partial_traversal_failure: false,
            write_failure: false,
            warnings: 0,
        }
    }

    pub fn invalid_expression(operation: TextOperationKind) -> Self {
        Self {
            operation,
            invalid_expression: true,
            ..Self::default_for(operation)
        }
    }

    pub fn invalid_input(operation: TextOperationKind) -> Self {
        Self {
            operation,
            invalid_input: true,
            ..Self::default_for(operation)
        }
    }

    pub fn invalid_path(operation: TextOperationKind) -> Self {
        Self {
            operation,
            invalid_path: true,
            ..Self::default_for(operation)
        }
    }

    pub fn partial_traversal_failure(operation: TextOperationKind) -> Self {
        Self {
            operation,
            partial_traversal_failure: true,
            ..Self::default_for(operation)
        }
    }

    fn default_for(operation: TextOperationKind) -> Self {
        Self {
            operation,
            matched: false,
            changed: false,
            invalid_expression: false,
            invalid_input: false,
            invalid_path: false,
            fatal_error: false,
            partial_traversal_failure: false,
            write_failure: false,
            warnings: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextExitCode {
    Success = 0,
    GrepNoMatch = 1,
    InvalidOrFatal = 2,
    PartialFailure = 3,
}

impl TextExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}

pub fn classify_text_exit_code(input: &TextExitClassificationInput) -> TextExitCode {
    if input.invalid_expression || input.invalid_input || input.invalid_path || input.fatal_error {
        return TextExitCode::InvalidOrFatal;
    }

    if input.partial_traversal_failure || input.write_failure {
        return TextExitCode::PartialFailure;
    }

    match input.operation {
        TextOperationKind::Grep if !input.matched => TextExitCode::GrepNoMatch,
        TextOperationKind::Grep | TextOperationKind::SedPreview | TextOperationKind::SedWrite => {
            TextExitCode::Success
        }
    }
}

#[cfg(test)]
mod grep_sed_output_tests {
    use super::*;

    #[test]
    fn grep_sed_renderer_emits_exact_plain_text_records() {
        let output = render_text_records(
            &[
                TextRecord::GrepMatch {
                    path: TextPath::new("src/lib.rs"),
                    line: 12,
                    byte: 4,
                    text: "let needle = true;".to_string(),
                },
                TextRecord::GrepContext {
                    kind: GrepContextKind::Before,
                    path: TextPath::new("src/lib.rs"),
                    line: 11,
                    text: "fn demo() {".to_string(),
                },
                TextRecord::GrepContext {
                    kind: GrepContextKind::After,
                    path: TextPath::new("src/lib.rs"),
                    line: 13,
                    text: "}".to_string(),
                },
                TextRecord::GrepCount {
                    path: TextPath::new("src/lib.rs"),
                    count: 1,
                },
                TextRecord::PathMatch {
                    path: TextPath::new("src/lib.rs"),
                },
                TextRecord::PathNoMatch {
                    path: TextPath::new("src/main.rs"),
                },
                TextRecord::SedPreview {
                    record_id: ReplacementRecordId::new("r:abc:12:4:1"),
                    path: TextPath::new("src/lib.rs"),
                    line: 12,
                    byte: 4,
                    old_text: "oldName".to_string(),
                    new_text: "newName".to_string(),
                },
                TextRecord::SedWrite {
                    record_id: ReplacementRecordId::new("r:abc:12:4:1"),
                    path: TextPath::new("src/lib.rs"),
                    replacements: 2,
                    old_size: 100,
                    new_size: 104,
                },
                TextRecord::Skip {
                    label: TextWarningLabel::BinarySkipped,
                    path: Some(TextPath::new("target/blob.bin")),
                    reason: "contains NUL byte".to_string(),
                },
                TextRecord::Warning {
                    label: TextWarningLabel::TraversalError,
                    path: Some(TextPath::new("missing")),
                    reason: "permission denied".to_string(),
                },
                TextRecord::Error {
                    label: TextErrorLabel::InvalidPath,
                    path: Some(TextPath::new("missing")),
                    reason: "not found".to_string(),
                },
                TextRecord::Summary {
                    counters: TextSummaryCounters {
                        files: 3,
                        matched: 1,
                        changed: 1,
                        replacements: 2,
                        skipped: 1,
                        warnings: 1,
                        errors: 1,
                        truncated: false,
                    },
                },
            ],
            TextRenderOptions::unbounded(),
        );

        assert_eq!(
            output,
            concat!(
                "match: src/lib.rs:12:4: let needle = true;\n",
                "context-before: src/lib.rs:11: fn demo() {\n",
                "context-after: src/lib.rs:13: }\n",
                "count: src/lib.rs: 1\n",
                "path-match: src/lib.rs\n",
                "path-no-match: src/main.rs\n",
                "preview: r:abc:12:4:1 src/lib.rs:12:4 oldName => newName\n",
                "write: r:abc:12:4:1 src/lib.rs: replacements=2 bytes=100->104\n",
                "skip: warning: binary-skipped target/blob.bin: contains NUL byte\n",
                "warning: traversal-error missing: permission denied\n",
                "error: invalid-path missing: not found\n",
                "summary: files=3 matched=1 changed=1 replacements=2 skipped=1 warnings=1 errors=1 truncated=false\n",
            )
        );
    }

    #[test]
    fn grep_sed_renderer_truncates_at_record_boundaries_with_resume_hint() {
        let records = vec![
            TextRecord::PathMatch {
                path: TextPath::new("a.txt"),
            },
            TextRecord::PathMatch {
                path: TextPath::new("b.txt"),
            },
            TextRecord::PathMatch {
                path: TextPath::new("c.txt"),
            },
        ];

        assert_eq!(
            render_text_records(&records, TextRenderOptions::bounded(2)),
            concat!(
                "path-match: a.txt\n",
                "path-match: b.txt\n",
                "truncated: output-limit shown=2 remaining=1\n",
                "resume: --skip 2 --limit 2\n",
            )
        );

        assert_eq!(
            render_text_records(&records, TextRenderOptions::resume(2, 2)),
            "path-match: c.txt\n"
        );
    }

    #[test]
    fn grep_sed_null_path_renderer_emits_exact_nul_delimited_bytes() {
        assert_eq!(
            render_null_path_records(&[
                TextRecord::PathMatch {
                    path: TextPath::new("src/lib.rs"),
                },
                TextRecord::PathNoMatch {
                    path: TextPath::new("src/main.rs"),
                },
                TextRecord::GrepCount {
                    path: TextPath::new("ignored"),
                    count: 0,
                },
            ]),
            b"src/lib.rs\0src/main.rs\0".to_vec()
        );
    }

    #[test]
    fn grep_sed_traversal_warning_label_matches_contract_strings() {
        // The five traversal labels must serialize to the exact strings named in
        // docs/grep-sed-contract.md and must round-trip losslessly into the
        // broader renderer label. This proves the traversal layer cannot drift.
        let cases = [
            (
                TraversalWarningLabel::BinarySkipped,
                "binary-skipped",
                "warning: binary-skipped",
                TextWarningLabel::BinarySkipped,
            ),
            (
                TraversalWarningLabel::InvalidUtf8,
                "invalid-utf8",
                "warning: invalid-utf8",
                TextWarningLabel::InvalidUtf8,
            ),
            (
                TraversalWarningLabel::UnsupportedEncoding,
                "unsupported-encoding",
                "warning: unsupported-encoding",
                TextWarningLabel::UnsupportedEncoding,
            ),
            (
                TraversalWarningLabel::PathSkipped,
                "path-skipped",
                "warning: path-skipped",
                TextWarningLabel::PathSkipped,
            ),
            (
                TraversalWarningLabel::TraversalError,
                "traversal-error",
                "warning: traversal-error",
                TextWarningLabel::TraversalError,
            ),
        ];

        for (traversal, expected_name, expected_label, expected_text) in cases {
            assert_eq!(traversal.as_name(), expected_name);
            assert_eq!(traversal.as_label(), expected_label);
            let promoted: TextWarningLabel = traversal.into();
            assert_eq!(promoted, expected_text);
            assert_eq!(promoted.as_name(), expected_name);
            assert_eq!(promoted.as_label(), expected_label);
        }
    }

    #[test]
    fn grep_sed_exit_classifier_matches_contract_table() {
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::grep(true)).code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::grep(false)).code(),
            1
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::sed_preview(true)).code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::sed_preview(false)).code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::sed_write(true)).code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::sed_write(false)).code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::invalid_expression(
                TextOperationKind::Grep
            ))
            .code(),
            2
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::invalid_input(
                TextOperationKind::SedWrite
            ))
            .code(),
            2
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::invalid_path(
                TextOperationKind::SedPreview
            ))
            .code(),
            2
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput {
                warnings: 2,
                ..TextExitClassificationInput::grep(true)
            })
            .code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput {
                warnings: 2,
                ..TextExitClassificationInput::grep(false)
            })
            .code(),
            1
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput {
                warnings: 2,
                ..TextExitClassificationInput::sed_preview(false)
            })
            .code(),
            0
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput::partial_traversal_failure(
                TextOperationKind::Grep
            ))
            .code(),
            3
        );
        assert_eq!(
            classify_text_exit_code(&TextExitClassificationInput {
                write_failure: true,
                ..TextExitClassificationInput::sed_write(true)
            })
            .code(),
            3
        );
    }
}
