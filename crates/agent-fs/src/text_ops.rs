use agent_core::TraversalWarningLabel;
use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;
use serde::Serialize;
use std::cmp::Ordering;
use std::fs::{self, File, Metadata};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const STDIN_MARKER: &str = "-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTraversalProfile {
    pub name: &'static str,
    pub use_ignore_files: bool,
    pub use_git_ignore: bool,
    pub use_git_global: bool,
    pub use_git_exclude: bool,
    pub skip_hidden: bool,
    pub follow_links: bool,
}

impl TextTraversalProfile {
    /// Shared recursive traversal profile for grep/sed.
    ///
    /// This intentionally aligns with the documented `agent-text-walk-v1`
    /// contract. It starts from the existing tree/search/symbol `ignore`
    /// walker shape, then enables global git excludes for grep/sed and keeps
    /// symlink/reparse traversal disabled by default.
    pub fn agent_text_walk_v1() -> Self {
        Self {
            name: "agent-text-walk-v1",
            use_ignore_files: true,
            use_git_ignore: true,
            use_git_global: true,
            use_git_exclude: true,
            skip_hidden: true,
            follow_links: false,
        }
    }
}

impl Default for TextTraversalProfile {
    fn default() -> Self {
        Self::agent_text_walk_v1()
    }
}

#[derive(Debug, Clone)]
pub struct TextTargetOptions {
    pub profile: TextTraversalProfile,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub allow_stdin: bool,
    pub include_hidden: bool,
}

impl Default for TextTargetOptions {
    fn default() -> Self {
        Self {
            profile: TextTraversalProfile::default(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            allow_stdin: true,
            include_hidden: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInput {
    Stdin,
    ExplicitFile(PathBuf),
    ExplicitDirectory(PathBuf),
    DefaultCurrentDirectory(PathBuf),
}

#[derive(Debug, Clone)]
pub struct TextFileSet {
    pub inputs: Vec<TextInput>,
    pub files: Vec<TextFile>,
    pub diagnostics: Vec<TextDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextTargetSource {
    ExplicitFile,
    RecursiveDirectory,
    DefaultCurrentDirectory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextFile {
    pub source: TextTargetSource,
    pub opened_path: PathBuf,
    pub display_path: String,
    pub classification: TextFileClassification,
    pub snapshot: Option<FileSnapshot>,
    pub decoded: Option<DecodedText>,
    pub diagnostic: Option<TextDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextFileClassification {
    Text,
    Binary,
    InvalidEncoding,
    UnsupportedEncoding,
    Skipped,
    Errored,
}

/// Re-export of the contract-owned traversal warning label.
///
/// `agent-fs` does not own a parallel string table for traversal/decoding
/// labels; the canonical names live on [`agent_core::TraversalWarningLabel`]
/// and promote losslessly into the renderer's [`agent_core::TextWarningLabel`].
/// This alias preserves the historical `TextDiagnosticLabel` name at the
/// traversal-layer API boundary while keeping the label vocabulary single-sourced.
pub type TextDiagnosticLabel = TraversalWarningLabel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextDiagnostic {
    pub label: TextDiagnosticLabel,
    pub path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecodedText {
    pub text: String,
    pub has_utf8_bom: bool,
    pub line_endings: LineEndingStyle,
    pub trailing_newline: TrailingNewline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileSnapshot {
    pub opened_path: PathBuf,
    pub display_path: String,
    pub canonical_path: Option<PathBuf>,
    pub len: u64,
    pub modified: Option<SystemTimeSnapshot>,
    pub permissions_readonly: bool,
    pub file_kind: FileKindSnapshot,
    pub symlink_target: Option<PathBuf>,
    pub content_hash: String,
    pub line_endings: LineEndingStyle,
    pub has_utf8_bom: bool,
    pub trailing_newline: TrailingNewline,
    pub identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileIdentity {
    pub unix: Option<UnixFileIdentity>,
    pub windows: Option<WindowsFileIdentity>,
    /// Cross-platform fallback used when the OS does not expose stable file IDs.
    ///
    /// Reserved for stable OS-specific IDs; all platforms keep this fallback so
    /// sed write preflight can still compare canonical path, length, modified
    /// time, and content hash before mutation.
    pub fallback: FileIdentityFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileIdentityFallback {
    pub canonical_path: Option<PathBuf>,
    pub len: u64,
    pub modified: Option<SystemTimeSnapshot>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnixFileIdentity {
    pub dev: u64,
    pub ino: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WindowsFileIdentity {
    pub volume_serial_number: Option<u32>,
    pub file_index: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SystemTimeSnapshot {
    pub secs: i64,
    pub nanos: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKindSnapshot {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineEndingStyle {
    None,
    Lf,
    Crlf,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrailingNewline {
    None,
    Lf,
    Crlf,
}

pub fn collect_text_files(
    cwd: &Path,
    operands: &[PathBuf],
    options: &TextTargetOptions,
) -> Result<TextFileSet> {
    let cwd = absolutize(cwd)?;
    let filters = CompiledFilters::new(&options.include_globs, &options.exclude_globs)?;
    let inputs = resolve_inputs(&cwd, operands, options)?;
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();

    for input in &inputs {
        match input {
            TextInput::Stdin => {}
            TextInput::ExplicitFile(path) => {
                if filters.is_skipped(path, &display_path(&cwd, path)) {
                    files.push(skipped_file(
                        &cwd,
                        path.clone(),
                        TextTargetSource::ExplicitFile,
                        "excluded by explicit filter",
                    ));
                    continue;
                }
                files.push(classify_file(&cwd, path, TextTargetSource::ExplicitFile));
            }
            TextInput::ExplicitDirectory(path) => walk_directory(
                &cwd,
                path,
                TextTargetSource::RecursiveDirectory,
                options,
                &filters,
                &mut files,
                &mut diagnostics,
            ),
            TextInput::DefaultCurrentDirectory(path) => walk_directory(
                &cwd,
                path,
                TextTargetSource::DefaultCurrentDirectory,
                options,
                &filters,
                &mut files,
                &mut diagnostics,
            ),
        }
    }

    files.sort_by(compare_text_files);
    diagnostics.sort_by(|a, b| a.path.cmp(&b.path).then(a.reason.cmp(&b.reason)));

    Ok(TextFileSet {
        inputs,
        files,
        diagnostics,
    })
}

pub fn decode_text(
    bytes: &[u8],
) -> (
    TextFileClassification,
    Option<DecodedText>,
    Option<TextDiagnosticLabel>,
) {
    if bytes.contains(&0) {
        return (
            TextFileClassification::Binary,
            None,
            Some(TextDiagnosticLabel::BinarySkipped),
        );
    }

    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return (
            TextFileClassification::UnsupportedEncoding,
            None,
            Some(TextDiagnosticLabel::UnsupportedEncoding),
        );
    }

    let has_utf8_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let text_bytes = if has_utf8_bom { &bytes[3..] } else { bytes };
    let line_endings = classify_line_endings(text_bytes);
    let trailing_newline = classify_trailing_newline(text_bytes);

    match std::str::from_utf8(text_bytes) {
        Ok(text) => (
            TextFileClassification::Text,
            Some(DecodedText {
                text: text.to_string(),
                has_utf8_bom,
                line_endings,
                trailing_newline,
            }),
            None,
        ),
        Err(_) => (
            TextFileClassification::InvalidEncoding,
            None,
            Some(TextDiagnosticLabel::InvalidUtf8),
        ),
    }
}

fn resolve_inputs(
    cwd: &Path,
    operands: &[PathBuf],
    options: &TextTargetOptions,
) -> Result<Vec<TextInput>> {
    if operands.is_empty() {
        return Ok(vec![TextInput::DefaultCurrentDirectory(cwd.to_path_buf())]);
    }

    let has_stdin = operands
        .iter()
        .any(|operand| operand == Path::new(STDIN_MARKER));
    if has_stdin {
        if !options.allow_stdin {
            bail!("error: invalid-input: stdin marker is not accepted in this mode");
        }
        if operands.len() > 1 {
            bail!("error: invalid-input: stdin marker cannot be combined with paths");
        }
        return Ok(vec![TextInput::Stdin]);
    }

    let mut inputs = Vec::new();
    for operand in operands {
        let path = absolutize_from(cwd, operand);
        let metadata = fs::metadata(&path)
            .with_context(|| format!("error: invalid-path: {}", display_path(cwd, &path)))?;
        if metadata.is_dir() {
            inputs.push(TextInput::ExplicitDirectory(path));
        } else if metadata.is_file() {
            inputs.push(TextInput::ExplicitFile(path));
        } else {
            bail!(
                "error: invalid-path: unsupported file type: {}",
                display_path(cwd, &path)
            );
        }
    }
    Ok(inputs)
}

fn walk_directory(
    cwd: &Path,
    root: &Path,
    source: TextTargetSource,
    options: &TextTargetOptions,
    filters: &CompiledFilters,
    files: &mut Vec<TextFile>,
    diagnostics: &mut Vec<TextDiagnostic>,
) {
    let profile = &options.profile;
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(profile.skip_hidden && !options.include_hidden)
        .ignore(profile.use_ignore_files)
        .git_ignore(profile.use_git_ignore)
        .require_git(false)
        .git_global(profile.use_git_global)
        .git_exclude(profile.use_git_exclude)
        .follow_links(profile.follow_links);

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                diagnostics.push(TextDiagnostic {
                    label: TextDiagnosticLabel::TraversalError,
                    path: None,
                    reason: err.to_string(),
                });
                continue;
            }
        };

        let path = entry.path();
        if path == root {
            continue;
        }
        if !entry.file_type().map(|ty| ty.is_file()).unwrap_or(false) {
            continue;
        }

        let display = display_path(cwd, path);
        if filters.is_skipped(path, &display) {
            files.push(skipped_file(
                cwd,
                path.to_path_buf(),
                source,
                "excluded by explicit filter",
            ));
            continue;
        }

        files.push(classify_file(cwd, path, source));
    }
}

fn classify_file(cwd: &Path, path: &Path, source: TextTargetSource) -> TextFile {
    let opened_path = path.to_path_buf();
    let display = display_path(cwd, path);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return TextFile {
                source,
                opened_path,
                display_path: display.clone(),
                classification: TextFileClassification::Errored,
                snapshot: None,
                decoded: None,
                diagnostic: Some(TextDiagnostic {
                    label: TextDiagnosticLabel::TraversalError,
                    path: Some(display),
                    reason: err.to_string(),
                }),
            };
        }
    };

    let (classification, decoded, label) = decode_text(&bytes);
    let snapshot = snapshot_file(path, display.clone(), &bytes).ok();
    let diagnostic = label.map(|label| TextDiagnostic {
        label,
        path: Some(display.clone()),
        reason: match label {
            TextDiagnosticLabel::BinarySkipped => "file contains a NUL byte".to_string(),
            TextDiagnosticLabel::InvalidUtf8 => "file is not valid UTF-8".to_string(),
            TextDiagnosticLabel::UnsupportedEncoding => {
                "file uses an unsupported encoding".to_string()
            }
            TextDiagnosticLabel::PathSkipped | TextDiagnosticLabel::TraversalError => {
                "file skipped".to_string()
            }
        },
    });

    TextFile {
        source,
        opened_path,
        display_path: display,
        classification,
        snapshot,
        decoded,
        diagnostic,
    }
}

fn skipped_file(cwd: &Path, path: PathBuf, source: TextTargetSource, reason: &str) -> TextFile {
    let display = display_path(cwd, &path);
    TextFile {
        source,
        opened_path: path,
        display_path: display.clone(),
        classification: TextFileClassification::Skipped,
        snapshot: None,
        decoded: None,
        diagnostic: Some(TextDiagnostic {
            label: TextDiagnosticLabel::PathSkipped,
            path: Some(display),
            reason: reason.to_string(),
        }),
    }
}

fn snapshot_file(path: &Path, display_path: String, bytes: &[u8]) -> Result<FileSnapshot> {
    let symlink_metadata = fs::symlink_metadata(path)?;
    let metadata = fs::metadata(path)?;
    let canonical_path = fs::canonicalize(path).ok();
    let symlink_target = if symlink_metadata.file_type().is_symlink() {
        fs::read_link(path).ok()
    } else {
        None
    };
    let content_hash = stable_content_hash(bytes);
    let modified = metadata.modified().ok().map(system_time_snapshot);
    let text_bytes = bytes_without_utf8_bom(bytes);
    let line_endings = classify_line_endings(text_bytes);
    let has_utf8_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let trailing_newline = classify_trailing_newline(text_bytes);
    let identity = file_identity(&metadata, canonical_path.clone(), modified, &content_hash);

    Ok(FileSnapshot {
        opened_path: path.to_path_buf(),
        display_path,
        canonical_path,
        len: metadata.len(),
        modified,
        permissions_readonly: metadata.permissions().readonly(),
        file_kind: file_kind(&symlink_metadata),
        symlink_target,
        content_hash,
        line_endings,
        has_utf8_bom,
        trailing_newline,
        identity,
    })
}

fn file_identity(
    metadata: &Metadata,
    canonical_path: Option<PathBuf>,
    modified: Option<SystemTimeSnapshot>,
    content_hash: &str,
) -> FileIdentity {
    FileIdentity {
        unix: unix_identity(metadata),
        windows: windows_identity(metadata),
        fallback: FileIdentityFallback {
            canonical_path,
            len: metadata.len(),
            modified,
            content_hash: content_hash.to_string(),
        },
    }
}

#[cfg(unix)]
fn unix_identity(metadata: &Metadata) -> Option<UnixFileIdentity> {
    use std::os::unix::fs::MetadataExt;

    Some(UnixFileIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn unix_identity(_metadata: &Metadata) -> Option<UnixFileIdentity> {
    None
}

#[cfg(windows)]
fn windows_identity(_metadata: &Metadata) -> Option<WindowsFileIdentity> {
    None
}

#[cfg(not(windows))]
fn windows_identity(_metadata: &Metadata) -> Option<WindowsFileIdentity> {
    None
}

fn file_kind(metadata: &Metadata) -> FileKindSnapshot {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        FileKindSnapshot::Symlink
    } else if file_type.is_dir() {
        FileKindSnapshot::Directory
    } else if file_type.is_file() {
        FileKindSnapshot::File
    } else {
        FileKindSnapshot::Other
    }
}

fn bytes_without_utf8_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    }
}

fn classify_line_endings(bytes: &[u8]) -> LineEndingStyle {
    let mut lf = 0usize;
    let mut crlf = 0usize;

    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && bytes[index - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }

    match (lf, crlf) {
        (0, 0) => LineEndingStyle::None,
        (_, 0) => LineEndingStyle::Lf,
        (0, _) => LineEndingStyle::Crlf,
        _ => LineEndingStyle::Mixed,
    }
}

fn classify_trailing_newline(bytes: &[u8]) -> TrailingNewline {
    if bytes.ends_with(b"\r\n") {
        TrailingNewline::Crlf
    } else if bytes.ends_with(b"\n") {
        TrailingNewline::Lf
    } else {
        TrailingNewline::None
    }
}

fn system_time_snapshot(time: SystemTime) -> SystemTimeSnapshot {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => SystemTimeSnapshot {
            secs: duration.as_secs() as i64,
            nanos: duration.subsec_nanos(),
        },
        Err(err) => {
            let duration = err.duration();
            SystemTimeSnapshot {
                secs: -(duration.as_secs() as i64),
                nanos: duration.subsec_nanos(),
            }
        }
    }
}

struct CompiledFilters {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl CompiledFilters {
    fn new(include_globs: &[String], exclude_globs: &[String]) -> Result<Self> {
        validate_globs(include_globs)?;
        validate_globs(exclude_globs)?;
        Ok(Self {
            include: include_globs.to_vec(),
            exclude: exclude_globs.to_vec(),
        })
    }

    fn is_skipped(&self, path: &Path, display_path: &str) -> bool {
        if self.matches(&self.exclude, path, display_path) {
            return true;
        }
        if !self.include.is_empty() && !self.matches(&self.include, path, display_path) {
            return true;
        }
        false
    }

    fn matches(&self, patterns: &[String], path: &Path, display_path: &str) -> bool {
        patterns.iter().any(|pattern| {
            wildcard_match(pattern, display_path)
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| wildcard_match(pattern, name))
                    .unwrap_or(false)
        })
    }
}

fn validate_globs(patterns: &[String]) -> Result<()> {
    for pattern in patterns {
        if pattern.is_empty() {
            bail!("invalid glob: empty pattern");
        }
    }
    Ok(())
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0usize, 0usize);
    let mut star_index = None;
    let mut star_value_index = 0usize;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

pub fn stable_content_hash(bytes: &[u8]) -> String {
    const OFFSETS: [u64; 4] = [
        0xcbf29ce484222325,
        0x9ae16a3b2f90404f,
        0xc949d7c7509e6557,
        0x100000001b3,
    ];
    const PRIMES: [u64; 4] = [0x100000001b3, 0x100000001c3, 0x100000001d3, 0x100000001f3];

    let mut hashes = OFFSETS;
    for byte in bytes {
        for (index, hash) in hashes.iter_mut().enumerate() {
            *hash ^= u64::from(*byte).wrapping_add(index as u64);
            *hash = hash.wrapping_mul(PRIMES[index]);
        }
    }

    format!(
        "{:016x}{:016x}{:016x}{:016x}",
        hashes[0], hashes[1], hashes[2], hashes[3]
    )
}

/// Stable, short hex hash of a normalized display path. This intentionally
/// keeps record IDs compact while matching the FNV-1a shape used by content
/// hashing.
pub fn relative_path_hash(path: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn absolutize_from(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn display_path(cwd: &Path, path: &Path) -> String {
    let display = path.strip_prefix(cwd).unwrap_or(path);
    normalize_path(display)
}

fn normalize_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        ".".to_string()
    } else {
        value
    }
}

fn compare_text_files(a: &TextFile, b: &TextFile) -> Ordering {
    a.display_path
        .as_bytes()
        .cmp(b.display_path.as_bytes())
        .then(
            a.classification
                .as_sort_key()
                .cmp(&b.classification.as_sort_key()),
        )
}

impl TextFileClassification {
    fn as_sort_key(self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Binary => 1,
            Self::InvalidEncoding => 2,
            Self::UnsupportedEncoding => 3,
            Self::Skipped => 4,
            Self::Errored => 5,
        }
    }
}

/// Outcome of re-snapshotting a file at write time and comparing it against a
/// preview-time [`FileSnapshot`]. Used by `sed --write` preflight to skip
/// files whose content drifted between preview classification and the
/// atomic-replace step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftCheck {
    /// File identity, size, and content hash all match the preview snapshot.
    Unchanged,
    /// At least one preflight field changed; the file must not be rewritten.
    Drifted { reason: String },
    /// Re-reading the file failed (permission, missing, etc.); treated as a
    /// drift class because we cannot prove the previewed content still exists.
    Missing { reason: String },
}

/// Re-read `path` and compare critical preflight fields (size, content hash,
/// file identity where available) against `preview`. The comparison is
/// intentionally conservative: any mismatch returns [`DriftCheck::Drifted`]
/// rather than rewriting stale content.
pub fn recheck_file_drift(path: &Path, preview: &FileSnapshot) -> DriftCheck {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return DriftCheck::Missing {
                reason: err.to_string(),
            };
        }
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) => {
            return DriftCheck::Missing {
                reason: err.to_string(),
            };
        }
    };
    if metadata.len() != preview.len {
        return DriftCheck::Drifted {
            reason: format!(
                "file size changed since preview ({} -> {})",
                preview.len,
                metadata.len()
            ),
        };
    }
    let current_hash = stable_content_hash(&bytes);
    if current_hash != preview.content_hash {
        return DriftCheck::Drifted {
            reason: "file content hash changed since preview".to_string(),
        };
    }
    // Identity comparison is best-effort: a missing identity field on either
    // side is treated as match (Windows std exposure varies by toolchain), but
    // when both sides have the field, mismatch is drift.
    if let (Some(now), Some(prev)) = (unix_identity(&metadata), preview.identity.unix) {
        if now != prev {
            return DriftCheck::Drifted {
                reason: "file identity (dev/inode) changed since preview".to_string(),
            };
        }
    }
    DriftCheck::Unchanged
}

/// Atomically replace `path` with `new_bytes`. Writes to a same-directory
/// temporary file, attempts to mirror permissions, then `rename`s over the
/// destination. On POSIX `rename` is atomic within a filesystem; on Windows
/// `std::fs::rename` maps to `MoveFileEx` with replace-existing semantics.
///
/// fsync is intentionally deferred: the v1 contract does not require crash
/// durability for `sed --write`, only that partially-completed writes never
/// observe truncated content. Same-directory `rename` after a successful
/// `write_all` already satisfies that. If a future task adds a `--fsync` mode
/// the durability hook lives here.
pub fn atomic_write_bytes(path: &Path, new_bytes: &[u8], source: &FileSnapshot) -> Result<u64> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent directory: {}", path.display()))?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());

    // Same-directory temp name avoids cross-filesystem rename. Unique suffix
    // is process-id + monotonic nanos to keep collisions vanishingly unlikely
    // without pulling tempfile into the runtime path.
    let suffix = format!(
        "{}.{}.{}.agtmp",
        file_name,
        std::process::id(),
        unique_nanos()
    );
    let tmp_path = parent.join(suffix);

    {
        let mut tmp = File::create(&tmp_path)
            .with_context(|| format!("creating temp file {}", tmp_path.display()))?;
        tmp.write_all(new_bytes)
            .with_context(|| format!("writing temp file {}", tmp_path.display()))?;
        // Drop closes the handle. We intentionally do not fsync; see fn docs.
    }

    // Best-effort permission preservation. Restoring readonly state matters
    // most on Windows where `rename` over a readonly destination would fail
    // anyway; on Unix the rename succeeds regardless and we just want to keep
    // the file mode stable for downstream tooling.
    if let Err(err) = mirror_permissions(&tmp_path, source) {
        // Non-fatal: log via context attached to the eventual rename error if
        // any. Production guidance is that permission preservation is
        // best-effort per the contract.
        let _ = err;
    }

    fs::rename(&tmp_path, path).map_err(|err| {
        // Clean up the temp on failure so we don't leak stray files.
        let _ = fs::remove_file(&tmp_path);
        anyhow::anyhow!("atomic rename failed: {} ({})", tmp_path.display(), err)
    })?;

    Ok(new_bytes.len() as u64)
}

fn mirror_permissions(tmp_path: &Path, source: &FileSnapshot) -> Result<()> {
    // Preserve the original readonly bit. This is the portable subset of
    // metadata preservation supported by std on both Unix and Windows.
    let mut perms = fs::metadata(tmp_path)?.permissions();
    if perms.readonly() != source.permissions_readonly {
        perms.set_readonly(source.permissions_readonly);
        fs::set_permissions(tmp_path, perms)?;
    }
    Ok(())
}

fn unique_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Re-encode decoded sed output back to bytes using the preview snapshot's
/// BOM presence as the source of truth. Line endings are emitted by the caller
/// (per-line CRLF/LF preservation lives in the sed render loop); this helper
/// only handles the BOM prefix so all sed write paths agree on it.
pub fn encode_text_with_bom(new_text: &str, snapshot: &FileSnapshot) -> Vec<u8> {
    let mut out = Vec::with_capacity(new_text.len() + if snapshot.has_utf8_bom { 3 } else { 0 });
    if snapshot.has_utf8_bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    out.extend_from_slice(new_text.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn text_ops_no_path_uses_current_directory_profile() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("zeta.txt"), "needle\n").unwrap();
        fs::write(root.join("alpha.txt"), "needle\n").unwrap();
        fs::create_dir(root.join(".hidden_dir")).unwrap();
        fs::write(root.join(".hidden_dir/secret.txt"), "needle\n").unwrap();
        fs::create_dir(root.join("ignored")).unwrap();
        fs::write(root.join("ignored/.gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("ignored/ignored.txt"), "needle\n").unwrap();
        fs::write(root.join("ignored/kept.txt"), "needle\n").unwrap();

        let files = collect_text_files(root, &[], &TextTargetOptions::default()).unwrap();
        let paths: Vec<_> = files
            .files
            .iter()
            .map(|file| file.display_path.as_str())
            .collect();

        assert_eq!(paths, vec!["alpha.txt", "ignored/kept.txt", "zeta.txt"]);
        assert!(matches!(
            files.inputs.as_slice(),
            [TextInput::DefaultCurrentDirectory(_)]
        ));
    }

    #[test]
    fn text_ops_explicit_hidden_file_is_accepted() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join(".hidden.txt"), "visible when explicit\n").unwrap();

        let files = collect_text_files(
            root,
            &[PathBuf::from(".hidden.txt")],
            &TextTargetOptions::default(),
        )
        .unwrap();

        assert_eq!(files.files.len(), 1);
        assert_eq!(files.files[0].classification, TextFileClassification::Text);
        assert_eq!(files.files[0].display_path, ".hidden.txt");
    }

    #[test]
    fn text_ops_directory_results_are_byte_ordered() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/b.txt"), "b").unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("sub/a.txt"), "a").unwrap();

        let files = collect_text_files(
            root,
            &[PathBuf::from("sub"), PathBuf::from("a.txt")],
            &TextTargetOptions::default(),
        )
        .unwrap();
        let paths: Vec<_> = files
            .files
            .iter()
            .map(|file| file.display_path.as_str())
            .collect();

        assert_eq!(paths, vec!["a.txt", "sub/a.txt", "sub/b.txt"]);
    }

    #[test]
    fn text_ops_stdin_marker_is_resolved_only_by_itself() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let files = collect_text_files(
            root,
            &[PathBuf::from(STDIN_MARKER)],
            &TextTargetOptions::default(),
        )
        .unwrap();
        assert_eq!(files.inputs, vec![TextInput::Stdin]);
        assert!(files.files.is_empty());

        let err = collect_text_files(
            root,
            &[PathBuf::from(STDIN_MARKER), PathBuf::from("file.txt")],
            &TextTargetOptions::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("stdin marker cannot be combined"));
    }

    #[test]
    fn text_ops_include_exclude_filters_exclude_wins() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("keep.txt"), "keep").unwrap();
        fs::write(root.join("skip.txt"), "skip").unwrap();
        fs::write(root.join("ignore.md"), "ignore").unwrap();

        let files = collect_text_files(
            root,
            &[],
            &TextTargetOptions {
                include_globs: vec!["*.txt".to_string()],
                exclude_globs: vec!["skip.txt".to_string()],
                ..TextTargetOptions::default()
            },
        )
        .unwrap();

        let text_paths: Vec<_> = files
            .files
            .iter()
            .filter(|file| file.classification == TextFileClassification::Text)
            .map(|file| file.display_path.as_str())
            .collect();
        let skipped_paths: Vec<_> = files
            .files
            .iter()
            .filter(|file| file.classification == TextFileClassification::Skipped)
            .map(|file| file.display_path.as_str())
            .collect();

        assert_eq!(text_paths, vec!["keep.txt"]);
        assert_eq!(skipped_paths, vec!["ignore.md", "skip.txt"]);
    }

    #[test]
    fn text_ops_classifies_invalid_utf8_and_binary() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("invalid.bin"), b"valid\xffinvalid").unwrap();
        fs::write(root.join("binary.bin"), b"abc\0def").unwrap();

        let files = collect_text_files(
            root,
            &[PathBuf::from("invalid.bin"), PathBuf::from("binary.bin")],
            &TextTargetOptions::default(),
        )
        .unwrap();

        let binary = files
            .files
            .iter()
            .find(|file| file.display_path == "binary.bin")
            .unwrap();
        let invalid = files
            .files
            .iter()
            .find(|file| file.display_path == "invalid.bin")
            .unwrap();

        assert_eq!(binary.classification, TextFileClassification::Binary);
        assert_eq!(
            binary.diagnostic.as_ref().unwrap().label.as_label(),
            "warning: binary-skipped"
        );
        assert_eq!(
            invalid.classification,
            TextFileClassification::InvalidEncoding
        );
        assert_eq!(
            invalid.diagnostic.as_ref().unwrap().label.as_label(),
            "warning: invalid-utf8"
        );
    }

    #[test]
    fn text_ops_snapshot_preserves_write_preflight_metadata() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("bom-crlf.txt"), b"\xEF\xBB\xBFneedle\r\n").unwrap();

        let files = collect_text_files(
            root,
            &[PathBuf::from("bom-crlf.txt")],
            &TextTargetOptions::default(),
        )
        .unwrap();
        let snapshot = files.files[0].snapshot.as_ref().unwrap();

        assert_eq!(snapshot.len, 11);
        assert!(snapshot.has_utf8_bom);
        assert_eq!(snapshot.line_endings, LineEndingStyle::Crlf);
        assert_eq!(snapshot.trailing_newline, TrailingNewline::Crlf);
        assert_eq!(snapshot.content_hash.len(), 64);
        assert!(snapshot.identity.fallback.modified.is_some());
    }
}
