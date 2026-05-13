# Grep/Sed Conformance Matrix

This matrix is the canonical v1 conformance index for `agent-tools grep` and
`agent-tools sed`. Implementation tasks must consume these row IDs or add new
rows here before adding behavior.

Source contract: `docs/grep-sed-contract.md`.

## Status And Comparison Modes

| Value | Meaning |
| --- | --- |
| `automated` | Covered by `cargo test -p agent-cli grep_sed` once the implementing task enables the row. T002 only audits row metadata and fixtures. |
| `platform-manual` | Requires a checked-in platform validation report at `docs/grep-sed-platform-validation.md` until CI runs the named OS job. |
| `deferred` | Explicit v1 non-goal or future channel with a stable diagnostic. |
| `byte-exact` | Output and fixture bytes must match exactly. |
| `normalized` | Path separators and platform-specific diagnostic wording may be normalized before comparison; labels, field order, exit status, and counts remain exact. |

## Automated Matrix Rows

Every automated row below has a stable row ID, exact argv, fixture inputs,
process status, stdout/stderr record expectation, warning labels, and comparison
mode. Later implementation tasks should add executable assertions for the same
IDs without renaming them.

| Row ID | Command argv | Fixtures | Expected status | Stdout expectation | Stderr expectation | Warnings | Compare | Contract coverage |
| --- | --- | --- | ---: | --- | --- | --- | --- | --- |
| `GS-A001` | `["grep", "needle"]` | `basic/alpha.txt`, `basic/beta.txt`, `ignored/.gitignore`, `ignored/ignored.txt`, `.hidden_dir/secret.txt` | 0 | `match:` records ordered by normalized relative path, line, byte; no-path input defaults to `.` | empty | none | byte-exact | grep no-path current-directory traversal |
| `GS-A002` | `["grep", "absent", "basic/alpha.txt"]` | `basic/alpha.txt` | 1 | empty | empty | none | byte-exact | explicit file no-match exit class |
| `GS-A003` | `["grep", "--fixed", "a+b.$", "payloads/literals.txt"]` | `payloads/literals.txt` | 0 | one `match:` record for literal metacharacter text | empty | none | byte-exact | grep fixed dialect |
| `GS-A004` | `["grep", "--regex", "price=\\$[0-9]+", "payloads/literals.txt"]` | `payloads/literals.txt` | 0 | `match:` record with one-based byte offset | empty | none | byte-exact | grep regex dialect and dollar escaping |
| `GS-A005` | `["grep", "--", "-leading-dash", "payloads/literals.txt"]` | `payloads/literals.txt` | 0 | one `match:` record | empty | none | byte-exact | accepted `--` payload channel |
| `GS-A006` | `["grep", "--pattern-file", "payloads/pattern-with-newline.txt", "payloads/multiline.txt"]` | `payloads/pattern-with-newline.txt`, `payloads/multiline.txt` | 1 | empty | empty | none | byte-exact | accepted pattern-file channel; multiline matching remains line-oriented |
| `GS-A007` | `["grep", "needle", "platform/crlf.txt", "platform/utf8-bom.txt"]` | `platform/crlf.txt`, `platform/utf8-bom.txt` | 0 | `match:` records preserve original text model and omit BOM from match text | empty | none | byte-exact | CRLF and UTF-8 BOM text model |
| `GS-A008` | `["grep", "needle", "platform/invalid-utf8.bin", "platform/binary-nul.bin"]` | `platform/invalid-utf8.bin`, `platform/binary-nul.bin` | 1 | `skip:` records for both files and summary skipped count | empty | `warning: invalid-utf8`, `warning: binary-skipped` | byte-exact | warning-only skips plus no grep matches exit 1 |
| `GS-A009` | `["grep", "needle", "ignored"]` | `ignored/.gitignore`, `ignored/kept.txt`, `ignored/ignored.txt` | 0 | match only from `ignored/kept.txt` | empty | none | normalized | ignore handling and deterministic path order |
| `GS-A010` | `["grep", "--paths-only", "needle", "basic"]` | `basic/alpha.txt`, `basic/beta.txt` | 0 | `path-match:` family, one path per line | empty | none | normalized | path-family output |
| `GS-A011` | `["grep", "needle", "-", "basic/alpha.txt"]` | stdin text plus `basic/alpha.txt` | 2 | empty | `error: invalid-input: stdin marker cannot be combined with paths` | none | byte-exact | invalid stdin/path combination |
| `SS-A001` | `["sed", "--fixed", "needle", "thread", "basic/alpha.txt"]` | `basic/alpha.txt` | 0 | `preview:` record plus `summary:`; no write | empty | none | byte-exact | sed default preview mode |
| `SS-A002` | `["sed", "--fixed", "missing", "thread", "basic/alpha.txt", "--preview"]` | `basic/alpha.txt` | 0 | no `preview:` records; `summary:` changed=0 replacements=0 | empty | none | byte-exact | sed preview no-op exit class |
| `SS-A003` | `["sed", "--regex", "C:\\\\\\\\temp\\\\\\\\([^ ]+)", "--replace", "D:\\\\temp\\\\$1", "payloads/literals.txt", "--preview"]` | `payloads/literals.txt` | 0 | `preview:` record expands capture | empty | none | byte-exact | sed regex replacement expansion and backslashes |
| `SS-A004` | `["sed", "--fixed", "old", "", "payloads/literals.txt", "--preview"]` | `payloads/literals.txt` | 0 | `preview:` record with empty replacement | empty | none | byte-exact | accepted empty replacement argv |
| `SS-A005` | `["sed", "--pattern-file", "payloads/pattern-with-newline.txt", "--replacement-file", "payloads/replacement-with-newline.txt", "payloads/multiline.txt", "--preview"]` | `payloads/pattern-with-newline.txt`, `payloads/replacement-with-newline.txt`, `payloads/multiline.txt` | 0 | preview/summary shape for file payloads; no writes | empty | none | byte-exact | accepted pattern/replacement file channels preserving newlines |
| `SS-A006` | `["sed", "s/needle/thread/g", "basic/alpha.txt", "--preview"]` | `basic/alpha.txt` | 0 | `preview:` records for all non-overlapping line matches | empty | none | byte-exact | sed-like expression and `g` flag |
| `SS-A007` | `["sed", "--fixed", "needle", "thread", "platform/crlf.txt", "platform/utf8-bom.txt", "--write"]` | `platform/crlf.txt`, `platform/utf8-bom.txt` copied into a temp workspace | 0 | `write:` records and `summary:`; files changed atomically | empty | none | byte-exact | write mode, CRLF/BOM preservation |
| `SS-A008` | `["sed", "--fixed", "needle", "thread", "-", "--write"]` | stdin text | 2 | empty | `error: invalid-input: --write cannot target stdin` | none | byte-exact | invalid write/stdin combination |
| `SS-A009` | `["sed", "--fixed", "needle", "thread", "platform/invalid-utf8.bin", "platform/binary-nul.bin", "--preview"]` | `platform/invalid-utf8.bin`, `platform/binary-nul.bin` | 0 | `skip:` records plus summary skipped count | empty | `warning: invalid-utf8`, `warning: binary-skipped` | byte-exact | sed warning-only skips are nonfatal |
| `SS-A010` | `["sed", "--pattern-stdin", "--replace", "x", "basic/alpha.txt", "--preview"]` | `basic/alpha.txt` | 2 | empty | `error: unsupported: stdin payload modes are deferred` | none | byte-exact | deferred stdin payload channel diagnostic |

## Platform Rows

These rows close OS-specific path, symlink/reparse, and traversal behavior.
Until a CI matrix exists, each run must append results to
`docs/grep-sed-platform-validation.md` with the OS, shell, filesystem type,
command, exit status, and observed record labels.

| Row ID | Target | Command argv | Fixtures | Expected status | Expected result | Closure command |
| --- | --- | --- | --- | ---: | --- | --- |
| `GS-P001` | Linux/macOS automated, Windows manual until symlink privilege is known | `["grep", "needle", "platform/symlink-file.txt"]` | `platform/symlink-file.txt` pointing at `basic/alpha.txt` | 0 | explicit file symlink is read through link | `cargo test -p agent-cli grep_sed -- --ignored platform_symlink` |
| `GS-P002` | Linux/macOS automated, Windows manual | `["grep", "needle", "platform/symlink-dir"]` | directory symlink to `basic/` | 1 | recursive traversal does not follow symlinked directories | `cargo test -p agent-cli grep_sed -- --ignored platform_symlink` |
| `GS-P003` | Windows manual | `["grep", "needle", "platform/reparse-dir"]` | junction/reparse point to `basic/` | 1 | recursive traversal does not follow reparse points | `cargo test -p agent-cli grep_sed -- --ignored windows_reparse` |
| `SS-P001` | Linux/macOS/Windows manual drift harness | `["sed", "--fixed", "needle", "thread", "platform/drift.txt", "--write"]` | temp file mutated between preflight and write | 3 | `warning: write-drift`; file is not modified by stale preflight | `cargo test -p agent-cli grep_sed -- --ignored write_drift` |
| `GS-P004` | Linux/macOS/Windows automated once CI matrix exists | `["grep", "needle", "platform/path-order"]` | mixed-case and separator-sensitive paths listed in fixture plan | 0 | deterministic byte-wise normalized relative path ordering | CI jobs `grep-sed-linux`, `grep-sed-macos`, `grep-sed-windows` run `cargo test -p agent-cli grep_sed` |

## Deferred Rows

| Row ID | Command argv | Expected status | Expected diagnostic | Reason |
| --- | --- | ---: | --- | --- |
| `GS-D001` | `["grep", "--files0-from", "files.lst", "needle"]` | 2 | `error: unsupported: null-delimited input lists are deferred` | bulk workflow input lists are deferred |
| `GS-D002` | `["grep", "--regex", "(?=needle)", "basic/alpha.txt"]` | 2 | `error: invalid-expression:` | lookaround unsupported by Rust regex dialect |
| `SS-D001` | `["sed", "1,3s/needle/thread/", "basic/alpha.txt", "--preview"]` | 2 | `error: unsupported:` | sed addresses and non-substitution command forms deferred |
| `SS-D002` | `["sed", "--regex", "needle", "--replace", "thread", "-", "--line", "1:2", "--preview"]` | 2 | `error: invalid-input:` | line ranges for stdin are deferred |
| `SS-D003` | `["sed", "--replacement-stdin", "--regex", "needle", "basic/alpha.txt", "--preview"]` | 2 | `error: unsupported: stdin payload modes are deferred` | stdin reserved for input text in v1 |

## Fixture Inventory And Plan

All paths are relative to `crates/agent-cli/tests/fixtures/grep_sed/`.

| Fixture | Classification | Required bytes/content | Rows |
| --- | --- | --- | --- |
| `basic/alpha.txt` | UTF-8 LF text | contains `needle`, repeated matches, and no trailing platform-specific bytes | `GS-A001`, `GS-A002`, `GS-A010`, `SS-A001`, `SS-A002`, `SS-A006`, `SS-A010` |
| `basic/beta.txt` | UTF-8 LF text | contains a later-sorting match to verify deterministic ordering | `GS-A001`, `GS-A010` |
| `.hidden_dir/secret.txt` | hidden UTF-8 text | contains `needle`; skipped during implicit recursive traversal | `GS-A001` |
| `ignored/.gitignore` | ignore file | ignores `ignored.txt` | `GS-A001`, `GS-A009` |
| `ignored/kept.txt` | UTF-8 text | contains `needle`; not ignored | `GS-A009` |
| `ignored/ignored.txt` | UTF-8 text | contains `needle`; ignored by `.gitignore` | `GS-A001`, `GS-A009` |
| `payloads/literals.txt` | UTF-8 LF text | includes `-leading-dash`, `s/foo/bar/g`, `a+b.$`, `price=$42`, `C:\\temp\\cache`, and `old` | `GS-A003`, `GS-A004`, `GS-A005`, `SS-A003`, `SS-A004` |
| `payloads/pattern-with-newline.txt` | UTF-8 payload file | contains `needle\nsecond` exactly | `GS-A006`, `SS-A005` |
| `payloads/replacement-with-newline.txt` | UTF-8 payload file | contains `thread\nsecond` exactly | `SS-A005` |
| `payloads/multiline.txt` | UTF-8 LF text | contains `needle` and `second` on adjacent lines | `GS-A006`, `SS-A005` |
| `platform/crlf.txt` | UTF-8 CRLF text | contains `needle` on CRLF lines | `GS-A007`, `SS-A007` |
| `platform/utf8-bom.txt` | UTF-8 BOM text | starts with bytes `EF BB BF` and contains `needle` | `GS-A007`, `SS-A007` |
| `platform/invalid-utf8.bin` | invalid UTF-8 text candidate | contains invalid byte sequence without NUL | `GS-A008`, `SS-A009` |
| `platform/binary-nul.bin` | binary prefilter candidate | contains NUL in inspected prefix | `GS-A008`, `SS-A009` |
| `platform/symlink-file.txt` | generated platform fixture | symlink to `../basic/alpha.txt`; created by ignored platform tests where supported | `GS-P001` |
| `platform/symlink-dir` | generated platform fixture | symlink to `../basic`; created by ignored platform tests where supported | `GS-P002` |
| `platform/reparse-dir` | generated Windows fixture | junction/reparse point to `../basic`; created manually from the validation runbook | `GS-P003` |
| `platform/drift.txt` | generated temp fixture | copied from `basic/alpha.txt` and mutated during write preflight harness | `SS-P001` |
| `platform/path-order/` | planned fixture set | mixed case, spaces, Unicode, and separator-sensitive names; add files when traversal implementation lands | `GS-P004` |

## Platform Validation Closure

The closure artifact is `docs/grep-sed-platform-validation.md`. It should be
created by T005/T006/T007 when the first executable platform rows are enabled.

Required manual entry format:

```text
## <YYYY-MM-DD> <os> <arch> <filesystem>
Command: cargo test -p agent-cli grep_sed -- --ignored <row-filter>
Rows: GS-P001, GS-P002
Expected: exact statuses and labels from this matrix
Observed: <status, stdout labels, stderr labels>
Result: pass|fail
Notes: <symlink privilege, reparse setup, filesystem normalization>
```

CI closure target:

```text
grep-sed-linux:   cargo test -p agent-cli grep_sed
grep-sed-macos:   cargo test -p agent-cli grep_sed
grep-sed-windows: cargo test -p agent-cli grep_sed
```
