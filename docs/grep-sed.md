# Portable Grep And Sed

`agent-tools grep` and `agent-tools sed` provide deterministic, cross-platform
text search and replacement for agents. They use one traversal, output, and
exit-code contract on Linux, macOS, and Windows.

Detailed references:

- Contract: [grep-sed-contract.md](grep-sed-contract.md)
- Conformance matrix: [grep-sed-conformance.md](grep-sed-conformance.md)
- Bulk workflow decision: [grep-sed-t008-bulk-workflows-decision.md](grep-sed-t008-bulk-workflows-decision.md)

## Quick Start

Prefer argv-native `sed` forms. They avoid shell-specific escaping and make
payloads such as `/`, `$`, `\`, and leading dashes unambiguous.

```bash
# Preview a literal replacement in one file.
agent-tools sed --fixed needle thread basic/alpha.txt --preview

# Preview a regex replacement. $1 expands the first capture.
agent-tools sed --regex 'C:\\\\temp\\\\([^ ]+)' --replace 'D:\\temp\\$1' payloads/literals.txt --preview

# Apply a reviewed change. --write is the only mode that edits files.
agent-tools sed --fixed needle thread src --write

# Search with Rust regex syntax.
agent-tools grep 'needle|thread' src

# Search for literal text containing regex metacharacters.
agent-tools grep --fixed 'a+b.$' src
```

Sed-like substitution expressions are accepted for familiar one-liners:

```bash
agent-tools sed 's/needle/thread/g' basic/alpha.txt --preview
```

Use argv-native forms first in scripts and examples. Sed-like expressions use
the grammar documented in [Sed Dialect](#sed-dialect).

## Search Workflows

```bash
# Search the current directory recursively.
agent-tools grep needle

# Search explicit files or directories.
agent-tools grep needle README.md docs

# Search stdin. The stdin marker cannot be combined with paths.
printf 'needle\n' | agent-tools grep needle -

# Show context records.
agent-tools grep needle src -B 2 -A 2

# Count matches per file.
agent-tools grep needle src --count

# Emit path-family records.
agent-tools grep needle src --paths-only
agent-tools grep needle src --files-with-matches
agent-tools grep needle src --files-without-match

# Emit NUL-delimited raw paths for path-family modes.
agent-tools grep needle src --paths-only --null
```

## Replacement Workflows

`sed` defaults to preview mode for files and directories. `--preview` is
optional but recommended in examples because it makes intent visible.

```bash
# Preview literal replacement.
agent-tools sed --fixed old new src --preview

# Preview all regex matches per line, then write after review.
agent-tools sed --regex 'old_([a-z]+)' --replace 'new_$1' src --global --preview
agent-tools sed --regex 'old_([a-z]+)' --replace 'new_$1' src --global --write

# Restrict replacements to an inclusive line range per file.
agent-tools sed --fixed old new src/lib.rs --line 20:60 --preview

# Transform stdin without writing files.
printf 'old\n' | agent-tools sed --fixed old new -
```

`--preview` and `--write` are mutually exclusive. `--write -` is invalid
because stdin has no file identity to protect.

## Platform-Safe Payloads

The examples below are written so the payload reaches `agent-tools` as argv or
UTF-8 file content rather than being interpreted by `sed`, PowerShell, `cmd`,
or a POSIX shell.

| Payload need | Safe command |
| --- | --- |
| Leading dash pattern | `agent-tools grep -- -leading-dash payloads/literals.txt` |
| Leading dash fixed pattern | `agent-tools sed --fixed -leading-dash REPLACED payloads/literals.txt --preview` |
| Leading dash replacement | `agent-tools sed --fixed old -new payloads/literals.txt --preview` |
| Delimiter-like text | `agent-tools sed --fixed 's/foo/bar/g' 'literal/replacement' payloads/literals.txt --preview` |
| Literal dollar in fixed mode | `agent-tools sed --fixed 'price=$42' 'price=$43' payloads/literals.txt --preview` |
| Regex dollar capture | `agent-tools sed --regex 'price=\\$([0-9]+)' --replace 'cost=$1' payloads/literals.txt --preview` |
| Literal backslashes | `agent-tools sed --fixed 'C:\\temp\\cache' 'D:\\temp\\cache' payloads/literals.txt --preview` |
| Empty replacement | `agent-tools sed --fixed old "" payloads/literals.txt --preview` |
| Newline payloads | `agent-tools sed --pattern-file /tmp/pattern.txt --replacement-file /tmp/replacement.txt payloads/multiline.txt --preview` |

PowerShell passes single-quoted strings literally, except a single quote must be
written as `''`. POSIX shells also pass single-quoted strings literally, but a
single quote must be closed and reopened. On `cmd.exe`, prefer payload files for
text containing `%`, `^`, `&`, `|`, `<`, or `>`.

Payload files are UTF-8 text files. `--pattern-file` reads the entire file as
one pattern and `--replacement-file` reads the entire file as one replacement,
preserving newlines. Empty replacement files are valid.

## Traversal Rules

When a command receives no path, it operates on `.`. Directory operands recurse
with `agent-text-walk-v1`; explicit file operands are read directly.

The traversal profile:

- skips hidden path components by default during recursive traversal;
- respects explicit include/exclude/glob filters, then `.ignore`, `.rgignore`,
  `.gitignore`, parent ignores, `.git/info/exclude`, and global git excludes;
- does not follow symlinked directories, junctions, or other reparse points;
- reads explicit file symlinks through the link;
- classifies binary and unsupported text before matching;
- emits deterministic records ordered by normalized relative path, then line,
  then one-based byte offset.

Explicit file operands bypass recursive ignore discovery, but they still go
through explicit filters and binary/encoding validation.

## Regex And Replacement Dialects

### Grep Dialect

`grep` defaults to Rust `regex` syntax. `--fixed` treats the pattern as literal
decoded UTF-8 text.

Unsupported Rust-regex constructs such as lookaround, backreferences,
conditionals, and backtracking-only features are invalid expressions:

```bash
agent-tools grep 'literal\.name' .
agent-tools grep 'foo\s+bar' .
agent-tools grep --fixed 'a+b.$' .
```

### Sed Dialect

Canonical argv-native forms:

```bash
agent-tools sed --regex <pattern> --replace <replacement> [path...] [--preview|--write]
agent-tools sed --fixed <old> <new> [path...] [--preview|--write]
```

Sed-like substitution form:

```text
s<delim><pattern><delim><replacement><delim><flags>
```

The delimiter is one non-alphanumeric, non-backslash, non-whitespace Unicode
scalar. Supported flags are `g` for all non-overlapping matches per line and
`i` for case-insensitive matching. Without `g`, only the first non-overlapping
match per line is replaced.

Regex replacements use Rust `regex::Captures::expand`: `$1`, `${1}`, and
`${name}` expand captures, and `$$` produces a literal dollar. Fixed
replacements are literal text; dollars, backslashes, delimiters, and
capture-like text are not expanded.

Line ranges are per file. `--line 20:60`, `--line 20:`, and `--line :60` are
accepted for file and directory inputs. Line ranges for stdin are deferred.

## Binary And Encoding Behavior

Grep and sed are decoded-text commands.

| Input | Behavior |
| --- | --- |
| UTF-8 | accepted |
| UTF-8 with BOM | accepted; the BOM is not matched as line text |
| CRLF text | accepted; line endings are preserved by `sed --write` |
| Invalid UTF-8 file | skipped with `warning: invalid-utf8` |
| Stdin invalid UTF-8 | fatal `error: invalid-input` |
| NUL byte in inspected prefix | skipped with `warning: binary-skipped` |
| UTF-16 or other unsupported encoding | skipped with `warning: unsupported-encoding` |

Matching is line-oriented. `^` and `$` anchor to one decoded line, `.` does not
match a newline, and multiline matching is deferred.

## Output Records

Output is plain text with stable record families. Labels and field order are
part of the contract.

```text
match: basic/alpha.txt:1:7: first needle line
context-before: basic/alpha.txt:1: first needle line
context-after: basic/alpha.txt:3: final old value
count: basic/alpha.txt: 3
path-match: basic/alpha.txt
path-no-match: basic/beta.txt
preview: r:alpha:1:1:1 basic/alpha.txt:1:1 needle => thread
write: r:alpha:1:1:1 basic/alpha.txt: replacements=1 bytes=51->51
skip: warning: invalid-utf8 platform/invalid-utf8.bin: file is not valid UTF-8
warning: write-unchanged basic/alpha.txt: rewritten content is byte-identical to source
error: invalid-input: --write cannot target stdin
summary: files=1 matched=1 changed=1 replacements=1 skipped=0 warnings=0 errors=0 truncated=false
truncated: output-limit shown=1000 remaining=4
resume: --skip 1000 --limit 1000
```

Preview record IDs have the form
`r:<relative-path-hash>:<line>:<byte>:<match-index>` and are stable for one
preflight over unchanged content.

## Warning, Skip, And Error Labels

Stable labels:

| Label | Meaning |
| --- | --- |
| `warning: binary-skipped` | File was classified as binary before matching. |
| `warning: invalid-utf8` | File bytes were not valid UTF-8. |
| `warning: unsupported-encoding` | Text encoding is not supported in v1. |
| `warning: path-skipped` | Explicit include/exclude/glob filtering skipped a path. |
| `warning: traversal-error` | A path could not be read during traversal. |
| `warning: write-drift` | A file changed between preflight and write; it was not overwritten. |
| `warning: write-unchanged` | A matched write would be byte-identical; no rewrite occurred. |
| `truncated: output-limit` | Output stopped at a record boundary because of `--limit`. |
| `resume: --skip <n> --limit <n>` | Resume hint for deterministic record order. |
| `error: invalid-expression` | Pattern, expression, or replacement template is invalid. |
| `error: invalid-input` | Flag combination or input channel is invalid. |
| `error: invalid-path` | A path operand is invalid. |
| `error: unsupported` | A recognized but deferred feature was requested. |
| `error: partial-traversal-failure` | Traversal failed after some candidates were processed. |
| `error: write-failed` | A write operation failed after preflight. |

Warning-only skips do not make `sed` fail. For `grep`, warning-only skips keep
normal match status: exit 0 when at least one file matched and exit 1 when no
file matched.

## Exit Codes

Exit codes are stable.

| Case | Grep | Sed preview | Sed write |
| --- | ---: | ---: | ---: |
| Match or records found | 0 | n/a | n/a |
| No match or no records | 1 | n/a | n/a |
| Preview changed | n/a | 0 | n/a |
| Preview no-op | n/a | 0 | n/a |
| Write changed | n/a | n/a | 0 |
| Write no-op | n/a | n/a | 0 |
| Invalid expression | 2 | 2 | 2 |
| Invalid input or invalid path | 2 | 2 | 2 |
| Warning-only skips plus grep match or sed completion | 0 | 0 | 0 |
| Warning-only skips plus no grep matches | 1 | n/a | n/a |
| Partial traversal failure after at least one candidate | 3 | 3 | 3 |
| Write drift or partial write failure | n/a | n/a | 3 |

## Write Safety

`sed --write` uses the same matcher and text model as preview. It writes only
files that have replacements and pass preflight.

Write safety behavior:

- atomicity is per file;
- files are rewritten through a temporary file in the same directory where the
  platform supports atomic replace;
- metadata is preserved where supported;
- content hash and file identity are checked before write;
- files that drift between preview/preflight and write emit
  `warning: write-drift` and are not overwritten;
- byte-identical rewrites emit `warning: write-unchanged` and do not touch
  disk;
- partial failure across many files is reported and exits 3; files already
  replaced are not rolled back.

Always run a preview first for broad changes:

```bash
agent-tools sed --fixed old new src --preview
agent-tools sed --fixed old new src --write
```

## Deferred Bulk Workflow Surface

V1 has no additional optional bulk workflow surface beyond the accepted core
flags documented in the contract. These workflows are intentionally deferred:

- `--files0-from` null-delimited input lists;
- stdin payload modes such as `--pattern-stdin` and `--replacement-stdin`;
- null-delimited full match records;
- preview/write manifests, replay modes, and manifest-gated writes;
- backup/recovery modes and default backup file creation.

Deferred features use stable diagnostics, for example:

```text
error: unsupported: null-delimited input lists are deferred
error: unsupported: stdin payload modes are deferred
```

Do not build user workflows around manifest, replay, backup, recovery, or
null-input-list modes until the contract and conformance matrix promote them.
