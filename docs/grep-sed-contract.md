# Grep/Sed Shared Command Contract

This is the v1 contract for `agent-tools grep` and `agent-tools sed`. It gates
implementation and conformance tests. Later implementation tasks may add flags,
but must not change the defaults below without updating this contract and the
conformance matrix.

## Command State Matrix

Paths are resolved relative to the current working directory. When no path is
supplied, both commands operate on `.` and stay within the shared traversal
profile.

| Command state | Input source | Output destination | Traversal | Mutation | Exit class |
| --- | --- | --- | --- | --- | --- |
| `grep <pattern>` with no path | Files under `.` | stdout records plus stderr diagnostics | recursive `agent-text-walk-v1` | none | match/no-match/error |
| `grep <pattern> <file...>` | Explicit files | stdout records plus stderr diagnostics | no recursive walk for file operands | none | match/no-match/error |
| `grep <pattern> <dir...>` | Files under each directory | stdout records plus stderr diagnostics | recursive `agent-text-walk-v1` per directory | none | match/no-match/error |
| `grep <pattern> -` | stdin text stream | stdout records plus stderr diagnostics | no filesystem traversal | none | match/no-match/error |
| `grep <pattern> - <path...>` | invalid | stderr `error: invalid-input: stdin marker cannot be combined with paths` | none | none | invalid input |
| `sed <expr>` with no path | Files under `.` | preview records plus summary | recursive `agent-text-walk-v1` | none | changed/no-op/error |
| `sed <expr> <file...>` | Explicit files | preview records plus summary | no recursive walk for file operands | none unless `--write` | changed/no-op/error |
| `sed <expr> <dir...>` | Files under each directory | preview records plus summary | recursive `agent-text-walk-v1` per directory | none unless `--write` | changed/no-op/error |
| `sed <expr> -` | stdin text stream | transformed stream records to stdout | no filesystem traversal | none | changed/no-op/error |
| `sed <expr> --preview [path...]` | path/default/stdin inputs | preview records plus summary | as above | none | changed/no-op/error |
| `sed <expr> --write [path...]` | path/default inputs only | write records plus summary | as above | explicit file writes | changed/no-op/error |
| `sed <expr> --write -` | invalid | stderr `error: invalid-input: --write cannot target stdin` | none | none | invalid input |
| `sed <expr>` with neither `--preview` nor `--write` | path/default inputs | preview records plus summary | as above | none | changed/no-op/error |
| any command with no pattern/expression | invalid | stderr `error: invalid-expression: missing pattern` or `missing expression` | none | none | invalid expression |
| any command with unknown flag or misplaced payload | invalid | stderr `error: invalid-input: <reason>` | none | none | invalid input |

Sed defaults to preview mode. `--preview` and `--write` are mutually exclusive.
`--write` is the only mode that may modify files.

## Traversal Profile: `agent-text-walk-v1`

`agent-text-walk-v1` is the default filesystem traversal profile for recursive
grep and sed over directories or implicit `.`.

Current source of truth inspected for existing behavior:

- `crates/agent-fs/src/tree.rs`: `tree()` uses `ignore::WalkBuilder` with
  `hidden(true)`, `git_ignore(true)`, `git_global(false)`, `git_exclude(true)`,
  then renders directories first and names alphabetically inside each parent.
- `crates/agent-search/src/indexer.rs`: `FileIndexer::build()` uses the same
  `WalkBuilder` settings and indexes files only.
- `crates/agent-symbols/src/index.rs`: `SymbolIndex::build()` uses the same
  `WalkBuilder` settings, indexes files only, and then filters to supported
  parser languages.

V1 grep/sed traversal:

| Concern | Contract |
| --- | --- |
| Ignore precedence | Explicit include/exclude flags override all ignore files. Then nearest `.ignore` and `.rgignore`, nearest `.gitignore`, parent ignore lookup, `.git/info/exclude`, and global git excludes. Later, more specific ignore files override earlier parent rules according to `ignore` crate semantics. |
| Difference from current tree/search/symbol | Current tree/search/symbol enable `.gitignore` and `.git/info/exclude`, filter hidden files, and disable global git excludes. Grep/sed intentionally add `.rgignore`, global git excludes, explicit include/exclude overrides, binary prefiltering, and stable resume ordering. |
| Hidden files | Hidden path components are skipped by default. An accepted future `--hidden` flag may include them; absent that flag, hidden files remain skipped even when explicitly recursing through a parent directory. Explicit file operands that are hidden are accepted unless excluded by an explicit exclude flag. |
| Symlinks and Windows reparse points | Recursive traversal does not follow symlinked directories, file symlinks, junctions, or other reparse points by default. Explicit file operands that are symlinks are read through the link, but `sed --write` records the link target identity during preflight and refuses if it changes before write. |
| Binary prefilter | Candidate files are classified before matching. Binary files are skipped with `warning: binary-skipped`. Explicit binary file operands are also skipped in v1, not searched as bytes. |
| Ordering | Results use deterministic byte-wise normalized relative path ordering, then ascending line number, then ascending byte offset within the decoded line. This differs from current `tree` display, which groups directories first inside each parent. |
| Overrides | `--include`, `--exclude`, `--glob`, and future type filters apply after path normalization and before text classification. Exclude wins over include on equal specificity. Explicit file operands bypass recursive ignore discovery except explicit include/exclude flags and binary/text validation. |
| Traversal errors | Permission, disappearing-file, invalid-path, and decode/classification failures are emitted as stable warning/error labels and counted in summaries. Partial traversal failure uses exit code 3. |

## Text, Match, Byte, And Encoding Model

V1 is line-oriented decoded-text processing.

- Accepted text is UTF-8, optionally with a UTF-8 BOM. UTF-16 and other
  encodings are deferred and skipped with `warning: unsupported-encoding`.
- A UTF-8 BOM is not part of line text for matching. `sed --write` preserves an
  existing UTF-8 BOM and does not add one to files without a BOM.
- Invalid UTF-8 is skipped with `warning: invalid-utf8`. `sed --write` refuses
  invalid text rather than rewriting lossy text.
- Binary detection runs before UTF-8 decoding. Any NUL byte in the inspected
  prefix marks the file binary for v1. Binary files are skipped with
  `warning: binary-skipped`.
- Line endings are preserved per line. A CRLF line remains CRLF after rewrite,
  an LF line remains LF, and the final trailing-newline presence is preserved.
- Matching operates on decoded line text without the line terminator. `^` and
  `$` anchor to the start and end of one decoded line. `.` does not match a
  newline in v1. Multiline matching is deferred.
- Offsets in output are one-based byte offsets in the original UTF-8 line, not
  Unicode scalar or display-column offsets.
- Fixed sed mode operates on decoded UTF-8 text after classification, not raw
  bytes. Its replacement is literal text and has no capture expansion.
- Stdin mode accepts UTF-8 text only. Invalid UTF-8 on stdin is an error, not a
  warning-only skip.

## Payload Channels

Argv-native forms are canonical. Examples in docs and tests should prefer them
over shell-sensitive sed-like expressions.

| Channel | V1 decision | Behavior or diagnostic |
| --- | --- | --- |
| `--` end-of-options | accepted | Everything after `--` is positional. This supports payloads beginning with `-`: `agent-tools grep -- -pattern .`; `agent-tools sed --fixed -- -old new .`. |
| Pattern as argv | accepted | Primary channel for grep patterns and sed `--regex <pattern>`. Payload is exactly the argv string. |
| Replacement as argv | accepted | Primary channel for sed `--replace <replacement>` and fixed new text. Empty replacement is accepted: `agent-tools sed --fixed old "" . --preview`. |
| Sed-like expression argv | accepted | `s<delim>pattern<delim>replacement<delim>flags` where delimiter is one non-alphanumeric, non-backslash, non-whitespace Unicode scalar. |
| `--pattern-file <file>` | accepted | Reads the entire file as one pattern, preserving newlines. Invalid UTF-8 in the payload file is `error: invalid-input: pattern-file is not valid UTF-8`. |
| `--replacement-file <file>` | accepted | Reads the entire file as one replacement, preserving newlines. Empty files are valid empty replacements. Invalid UTF-8 is `error: invalid-input: replacement-file is not valid UTF-8`. |
| Stdin payload modes | deferred | `--pattern-stdin` and `--replacement-stdin` are non-goals for v1 and emit `error: unsupported: stdin payload modes are deferred`; stdin remains reserved for input text via `-`. |
| Null-delimited input lists | deferred | `--files0-from` is deferred to bulk workflow work and emits `error: unsupported: null-delimited input lists are deferred`. |

Required payload examples:

```bash
agent-tools grep -- -leading-dash .
agent-tools grep --fixed 's/foo/bar/g' .
agent-tools grep --regex 'price=\$[0-9]+' .
agent-tools sed --regex 'C:\\\\temp\\\\([^ ]+)' --replace 'D:\\temp\\$1' . --preview
agent-tools sed --fixed old "" . --preview
agent-tools sed --pattern-file /tmp/pattern-with-newline.txt --replacement-file /tmp/replacement-with-newline.txt . --preview
```

## Grep Dialect

- Regex is the default grep mode and uses the Rust `regex` crate dialect.
- `--fixed` treats the pattern as a literal decoded-text string.
- Unsupported regex constructs are invalid expressions with exit code 2 and a
  stable diagnostic prefixed by `error: invalid-expression:`.
- Backreferences, lookaround, conditional expressions, and backtracking-only
  constructs are unsupported in regex patterns.
- Escaping examples:
  - literal dot in regex mode: `agent-tools grep 'literal\.name' .`
  - whitespace class: `agent-tools grep 'foo\s+bar' .`
  - fixed literal with metacharacters: `agent-tools grep --fixed 'a+b.$' .`

Machine-safe path output is accepted without making null-delimited output the
default. `--paths-only` emits one path per line and escapes embedded newlines as
`\n` in v1. `--null` is accepted only with path-family modes
`--paths-only`/`--files-with-matches`/`--files-without-match` and emits raw path
bytes followed by NUL. Null-delimited match records are deferred because match
lines and context records require a richer record envelope.

## Sed Dialect

Canonical argv-native forms:

```bash
agent-tools sed --regex <pattern> --replace <replacement> [path...] [--preview|--write]
agent-tools sed --fixed <old> <new> [path...] [--preview|--write]
```

Sed-like substitution form is retained for familiarity:

```text
s<delim><pattern><delim><replacement><delim><flags>
```

Rules:

- The delimiter is one non-alphanumeric, non-backslash, non-whitespace Unicode
  scalar and can be `/`, `|`, `#`, or similar. Delimiter-like text inside the
  pattern or replacement must be escaped in sed-like form, so argv-native forms
  are preferred.
- Supported flags are `g` for all non-overlapping matches per line and `i` for
  case-insensitive matching. Repeated flags are invalid.
- Without `g`, sed replaces the first non-overlapping match per line. With `g`,
  it replaces all non-overlapping matches per line, left to right.
- Regex replacement expansion uses `regex::Captures::expand` semantics:
  `$1`, `${1}`, and `${name}` expand captures. Unknown captures are invalid
  expressions. Use `$$` for a literal dollar in regex replacement mode.
- Fixed replacement mode is literal text. Dollars, backslashes, delimiters, and
  capture-like text are not expanded.
- Line ranges are per file. `--line 20:60` includes both endpoints. Open ranges
  `--line 20:` and `--line :60` are accepted. Line ranges are invalid for stdin
  until a future streaming range contract exists.
- Unsupported sed constructs include addresses in the expression, commands
  other than substitution, hold space, branching, labels, shell execution, and
  GNU/BSD-specific extensions. They emit `error: unsupported: <construct>`.

## Output Grammar

Output is plain text with stable record families. Human text can change only
outside labels and field order.

Common labels:

- `warning: binary-skipped`
- `warning: invalid-utf8`
- `warning: unsupported-encoding`
- `warning: path-skipped`
- `warning: traversal-error`
- `warning: write-drift`
- `warning: write-unchanged`
- `truncated: output-limit`
- `resume: --skip <n> --limit <n>`
- `error: invalid-expression`
- `error: invalid-input`
- `error: invalid-path`
- `error: unsupported`
- `error: partial-traversal-failure`

Record families:

```text
match: <path>:<line>:<byte>: <line-text>
context-before: <path>:<line>: <line-text>
context-after: <path>:<line>: <line-text>
count: <path>: <count>
path-match: <path>
path-no-match: <path>
preview: <record-id> <path>:<line>:<byte> <old-text> => <new-text>
write: <record-id> <path>: replacements=<n> bytes=<old-size>-><new-size>
skip: <label> <path>: <reason>
warning: <label> <path>: <reason>
error: <label> <path>: <reason>
summary: files=<n> matched=<n> changed=<n> replacements=<n> skipped=<n> warnings=<n> errors=<n> truncated=<true|false>
truncated: output-limit shown=<n> remaining=<n>
resume: --skip <n> --limit <n>
```

`<record-id>` is stable for one preflight over unchanged content:
`r:<relative-path-hash>:<line>:<byte>:<match-index>`.

Large output is bounded by `--limit` or the command default. Truncation must
occur at record boundaries and include both `truncated:` and `resume:` records
using deterministic ordering.

## Exit Codes

Exit codes are exact:

| Case | Grep exit | Sed preview exit | Sed write exit |
| --- | ---: | ---: | ---: |
| grep match / records found | 0 | n/a | n/a |
| grep no-match / no records | 1 | n/a | n/a |
| grep invalid expression or other grep error before traversal | 2 | n/a | n/a |
| sed preview changed | n/a | 0 | n/a |
| sed preview no-op | n/a | 0 | n/a |
| sed write changed | n/a | n/a | 0 |
| sed write no-op | n/a | n/a | 0 |
| invalid expression | 2 | 2 | 2 |
| invalid path or invalid input combination | 2 | 2 | 2 |
| warning-only skips plus at least one grep match / sed completes | 0 | 0 | 0 |
| warning-only skips plus no grep matches | 1 | n/a | n/a |
| partial traversal failure after at least one candidate was processed | 3 | 3 | 3 |
| write drift or partial write failure | n/a | n/a | 3 |

For grep, warning-only skips do not mask match status: matches plus skips exit
0, no matches plus skips exit 1. For sed, warning-only skips with no fatal
errors exit 0 whether the operation changed content or was a no-op.

## Write Safety

`sed --write` must use the same match engine and text model as preview. It may
write only files that have replacements and pass preflight.

- Atomicity is per file. Write to a temporary file in the same directory,
  flush, preserve metadata, then atomically replace the original where the
  platform supports it.
- Partial failure is allowed across multiple files but must be reported. Files
  already replaced are not rolled back. The command exits 3 and prints
  `error: partial-traversal-failure` or `error: write-failed` with counts.
- File identity checks record device/inode where available, Windows file ID
  where available, canonical path, file type, symlink target for explicit file
  symlinks, size, modified time, and content hash before write.
- Content hash/preflight drift checks are mandatory. If identity, size, mtime,
  or hash changes between match preflight and write, skip the file with
  `warning: write-drift` and do not mutate it.
- Metadata preservation includes permissions and modified times where
  supported. Ownership, ACLs, extended attributes, and alternate data streams
  are best-effort v1 and must warn on known preservation failure.
- File encodings, line endings, UTF-8 BOM, and trailing newline presence are
  preserved according to the text model.
- Files whose rewritten content is byte-identical are not touched and emit
  `warning: write-unchanged` only in verbose modes; summaries count them as
  unchanged.
- Backup files are not created by default.
- Optional manifest behavior is deferred to the bulk workflow task. V1 may add
  `--manifest <file>` to record preview/write details, but normal `--write`
  does not require a reviewed manifest.

## Deferred And Non-Goals

- Tree-sitter and syntax-aware extensions are explicit non-goals for v1. The
  portable textual core must not depend on language parsers. Future syntax-aware
  filters may layer on top after this contract is implemented.
- Full GNU/BSD grep and sed compatibility is not a goal.
- Raw-byte search/rewrite, UTF-16 transcoding, multiline regex, null-delimited
  full match records, stdin payload modes, and null-delimited input file lists
  are deferred.
