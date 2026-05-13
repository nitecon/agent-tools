# Portable Grep And Sed For Agent Tools

## Vision

`agent-tools` should provide a portable, predictable text search and rewrite
surface that works the same way on Linux, macOS, and Windows.

The existing symbol search and tree-sitter tools are strong for code-aware
navigation. They answer questions like "where is this function defined?" and
"show me the implementation of this type." The missing layer is a reliable
cross-platform equivalent for the lower-level operations agents still use every
day: find matching text, inspect surrounding context, preview replacements, and
apply targeted rewrites.

Today those workflows usually fall back to `grep`, `rg`, `sed`, `awk`, `perl`,
or shell-specific snippets. That works until it does not:

- Windows does not consistently provide Unix text tools.
- macOS ships BSD tools whose flags and behavior differ from GNU tools.
- GNU `sed` supports useful modes, such as `-z`, that BSD `sed` does not.
- Shell quoting differs across platforms and agent runtimes.
- Multi-file rewrites are easy to make destructive without a preview.
- Agents waste time re-learning which local tool behavior is available.

A first-class `agent-tools grep` and `agent-tools sed` equivalent would make
text operations as dependable as the current tree, symbol, and task tools.

## Why This Matters

Agents need boring, repeatable primitives. Text search and replacement are too
fundamental to depend on whichever shell and platform happen to be available.

The practical win is not just convenience. It is safer automation:

- A Windows agent can run the same command as a Linux agent.
- A macOS agent does not need GNU-specific workarounds.
- A replacement can be previewed before files are touched.
- Output can be concise and structured for agent consumption.
- Exit codes can be stable enough for scripts and CI.
- Binary files, encodings, and null-separated workflows can have explicit
  behavior instead of platform-specific surprises.

This also strengthens `agent-tools` as the agent-native utility belt. Symbol
search answers semantic questions; portable grep/sed covers exact textual
questions and mechanical edits.

## Product Shape

The proposed surface is two related command families:

```bash
agent-tools grep <pattern> [path ...]
agent-tools sed <expression> [path ...]
agent-tools sed --regex <pattern> --replace <replacement> [path ...]
agent-tools sed --fixed <old> <new> [path ...]
```

Names are intentionally familiar. The goal is not to reproduce every historical
corner of GNU/BSD grep and sed. The goal is to provide the subset agents use
constantly, with consistent semantics and safe defaults.

The sed-like expression form is familiar, but the portable contract should not
depend on shell-specific quoting. The argv-native forms are the canonical shape
for scripts, agents, and cross-shell examples, and examples should prefer them
for v1. Milestone 0 should define a command-state and payload matrix covering
grep and sed with no paths, explicit files, directories, explicit stdin markers,
sed with neither `--preview` nor `--write`, sed preview, and sed write. Defaults
should be familiar to grep and sed users while remaining repository-scoped for
agents: when no path is supplied, operate within the current working directory
rather than escaping to broader filesystem state. Each row should specify the
input source, output destination, traversal behavior, mutation behavior, and
exit-code class.

Milestone 0 should also decide the quote-resistant payload surface for patterns
and replacements, such as `--`, `--pattern-file`, `--replacement-file`, and
stdin payload modes, or explicitly defer those channels. Acceptance examples
should include patterns and replacements that begin with `-`, delimiter-like
text, dollars, backslashes, empty replacements, and newlines. If the sed-like
form is kept, its pattern, expression, and replacement language must be
documented as an `agent-tools` grammar, not inherited accidentally from
whichever shell or platform sed a user knows.

## Grep Goals

`agent-tools grep` should search text consistently across platforms.

Core behavior:

- Search files recursively when given directories.
- Use a named, documented shared traversal profile by default, including
  `.ignore`, `.rgignore`, `.gitignore`, `.git/info/exclude`, global git
  excludes, parent ignore lookup, hidden-file defaults, symlink and reparse
  point behavior, binary prefiltering, deterministic path ordering, and override
  precedence. Grep and sed should align with the existing agent-tools
  tree/search/symbol traversal behavior unless Milestone 0 documents a narrow
  and intentional exception.
- Support file globs and include/exclude filters.
- Support case-sensitive and case-insensitive matching.
- Use regex matching by default for grep/ripgrep familiarity, with fixed-string
  mode available through an explicit flag and documented escaping examples.
- Show file path, line number, and matching line by default.
- Support before/after context.
- Support count-only and files-with-matches modes.
- Handle binary files explicitly and safely.
- Offer null-delimited output for machine-safe pipelines.

Example shape:

```bash
agent-tools grep "BrokerActor" app/src
agent-tools grep "timeout" crates --glob "*.rs" --context 2
agent-tools grep "foo\\s+bar" . --ignore-case
agent-tools grep --fixed "literal.name" . --files-with-matches
```

The output should stay agent-facing: compact, deterministic, and plain text by
default. It should avoid JSON flags in the primary workflow, consistent with
the current task/comms/patterns guidance.

## Sed Goals

`agent-tools sed` should perform predictable stream-style and file rewrite
operations without depending on platform sed behavior.

Core behavior:

- Support common substitution syntax: `s/pattern/replacement/flags`.
- Define the regex-mode replacement grammar explicitly. For v1, prefer Rust
  `regex::Captures::expand` semantics for capture references, named captures,
  literal `$`, unsupported flags, and invalid capture behavior.
- Support fixed-string replacements as a safer alternative to regex; fixed mode
  treats replacement text byte-literally, with no metacharacter expansion.
- Support global replacement per line using documented non-overlapping,
  left-to-right replacement semantics.
- Support case-insensitive matching.
- Support line-range constrained replacements, with v1 either restricted to a
  single file or explicitly named as per-file semantics for recursive runs.
- Support file filters and recursive directory traversal through the same
  internal traversal adapter as `agent-tools grep`, while explicitly defining
  any different defaults for grep, sed preview, and sed write.
- Default to preview mode for multi-file edits.
- Require an explicit write flag before modifying files.
- Define `--write` behavior before implementation: per-file atomicity,
  concurrent-modification handling, partial-failure policy, symlink and
  reparse-point handling, metadata preservation, stable replacement record IDs,
  file identity checks before mutation, content hash/preflight drift checks,
  drift/skip labels, optional manifest output for auditability, and the write
  preflight or override policy. V1 should not require a reviewed preview
  manifest as the normal write path unless implementation discovery shows that
  stable record IDs plus hash/preflight checks are insufficient.
- Print a concise summary of changed files, replacement counts, and stable
  warning/skip labels with counts.
- Preserve line endings, trailing-newline presence, and BOM handling according
  to the documented text model.
- Avoid touching files whose content does not change.

Example shape:

```bash
agent-tools sed 's/oldName/newName/g' crates --preview
agent-tools sed 's/timeout_ms/request_timeout_ms/g' crates --write
agent-tools sed 's/foo/bar/' README.md --line 20:60 --write
agent-tools sed --fixed "old literal" "new literal" app/src --write
```

The important design choice is safety. GNU/BSD `sed -i` semantics differ, and
agents can easily apply a broad rewrite accidentally. `agent-tools sed` should
make broad edits visible before they are written.

## Compatibility Philosophy

The commands should be familiar, not clone-perfect.

Good compatibility targets:

- Common grep flags that agents regularly use.
- Common sed substitution behavior.
- Predictable regex syntax based on Rust's regex ecosystem, including documented
  diagnostics for unsupported pattern constructs.
- A shared replacement expansion contract for regex mode and literal replacement
  behavior for fixed-string mode.
- Cross-platform path handling.
- Stable output and exit codes.

Non-goals:

- Implementing every GNU grep flag.
- Implementing the full sed scripting language.
- Preserving platform-specific quirks.
- Replacing semantic refactoring tools.

When a historical flag has incompatible behavior across platforms, `agent-tools`
should prefer one documented behavior over emulation by operating system.

Before implementation, the design should define the text and match model used
by both commands. That contract should state whether v1 is line-oriented,
whole-file, or record-oriented; how `^`, `$`, `.`, context, line ranges, CRLF,
and future multiline matching behave; and whether NUL-delimited records are in
scope or deferred.

The same contract should define the byte and encoding model. It should state
whether v1 is UTF-8 only, UTF-8 plus BOM-based transcoding, or byte-oriented;
how invalid encodings and binary files are detected; whether explicit file
paths differ from recursive traversal; and whether `sed --write` refuses files
that do not match the supported text model.

## Safety And Agent Ergonomics

The rewrite command should be designed around agent mistakes being cheap:

- Preview first: show file, line, old text, and new text for a bounded number of
  matches, using a stable replacement record identity that can also appear in
  write summaries or manifests.
- Summarize always: print changed files, total replacements, skipped files by
  stable warning label, and errors.
- Bound output: large result sets should truncate with clear next steps,
  including deterministic resume hints such as skip/limit over the stable
  ordering.
- Keep ordering deterministic by default, using byte-wise path ordering and
  file-order matches so bounded output truncates at stable points; if a faster
  unsorted mode exists, make it explicit.
- Make writes explicit: use `--write`, not implicit in-place mutation.
- Support dry-run in scripts: preview behavior should be stable and exit-code
  friendly.
- Avoid hidden backups by default: if backup support is added, make it explicit.

This is especially important because agents commonly run commands over whole
repositories. The tool should make "what will change?" the default answer.

Stable output and exit codes should be part of the contract rather than left to
implementation. Define a plain-text record grammar for grep match records,
context records, count and files-with-matches modes, sed preview records,
write summaries, warning-only skips, truncation records, resume hints, partial
traversal failures, and null-delimited variants where supported. At minimum,
assign concrete exit codes for grep match, grep no-match, grep error, sed
preview with changes, sed preview with no changes, sed write changed, sed
write no-op, invalid expression, invalid path, warning-only skips, and partial
traversal failure behavior.

## Implementation Direction

The implementation can build on existing crates and conventions in this repo:

- Use the Rust `regex` crate for portable regex behavior, and document
  unsupported constructs such as backreferences and lookaround with stable
  diagnostics.
- Factor a shared text-operation layer only after the grep/sed command-state,
  traversal, file classification, text decoding, deterministic ordering,
  skip-label, and write-target selection contracts are chosen; do not
  accidentally drift from the current tree/search/symbol traversal behavior
  unless Milestone 0 records the reason for a grep/sed-specific exception.
- Keep command output brief and natural-language oriented, with stable labels
  where agents need to parse warnings, skips, truncation, or resume hints.
- Keep all filesystem writes explicit, narrow, and covered by the documented
  atomicity and preservation contract.
- Add conformance tests for Linux/macOS/Windows path handling, traversal,
  symlink/reparse behavior, newline/BOM preservation, and deterministic output
  where practical.
- Keep binary-file behavior explicit rather than best-effort.

A minimal implementation does not need tree-sitter. The first milestone should
be dependable textual search and replacement. Later milestones can layer in
syntax-aware filters, such as "only replace inside Rust identifiers" or "only
search comments/strings," using the existing parser foundation.

## Proposed Milestones

0. Shared Text, Traversal, Regex, Output, Write-Safety, And Exit-Code Contract

   Before implementation, define the common contract that both grep and sed
   must share. Cover command-state defaults for paths, stdin, preview, and
   write, including grep/sed-familiar no-path behavior scoped to the current
   working directory; traversal and ignore precedence aligned with existing
   tree/search/symbol lookup behavior; deterministic ordering; text and
   match units; CRLF and future multiline behavior; byte/encoding and
   binary-file handling; regex pattern dialect; replacement expansion; argv
   payload edge cases; bounded output and resume; warning/error reporting;
   write atomicity and preservation; optional audit manifests; stable record
   IDs plus content hash/preflight drift checks for writes; and stable exit
   codes.

   Capture these as a conformance scenario matrix before implementation starts.
   Representative rows should combine command mode, input source, traversal
   profile, text model, match/replacement dialect, output record family,
   warning labels, truncation/resume behavior, mutation behavior, and exit-code
   class, and should run across Linux, macOS, and Windows.

1. `agent-tools grep`

   Add recursive, ignore-aware text search with path, line number, context,
   fixed-string mode, regex mode, case-insensitive mode, include/exclude
   filters, binary and invalid-encoding handling, machine-safe path output
   semantics, and bounded output.

2. `agent-tools sed --preview`

   Add substitution preview over files and directories. No writes yet. Focus on
   output clarity, matching semantics, line ranges, replacement counts,
   include/exclude filters, binary and invalid-encoding handling, and stable
   preview record identities.

3. `agent-tools sed --write`

   Add explicit file mutation with unchanged-file detection, line-ending
   preservation, summary output, stable replacement record IDs, content
   hash/preflight drift detection, and focused tests.

4. Safer Bulk Workflows

   Add richer include/exclude workflows, richer null-delimited modes where
   useful, optional manifest output for audit/replay if it proves valuable, and
   stronger recovery/audit affordances for broad rewrites.

5. Syntax-Aware Extensions

   Explore optional tree-sitter-backed modes once the portable textual core is
   stable.

## Success Criteria

The feature is successful when an agent can use one command surface for common
text search and replacement across supported platforms without asking:

- Is GNU sed installed?
- Is this BSD sed?
- Does this shell quote the replacement correctly?
- Will this command mutate files immediately?
- Will Windows behave differently?

The user experience should be: search confidently, preview changes, write only
when explicit, and get the same result everywhere.
