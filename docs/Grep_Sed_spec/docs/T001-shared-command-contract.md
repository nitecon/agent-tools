# T001 - Write the shared grep/sed command contract

**Team:** docs
**Phase:** 0
**Depends on:** (none)
**Status:** todo

## Scope

**In:** Write the canonical shared contract for `agent-tools grep` and `agent-tools sed` before implementation begins.

**Out:** Implementing command behavior. The implementation work starts in T003 and later tasks.

## Source references

- `docs/Grep_Sed.md` lines 62-81 for command-state defaults and argv payload requirements.
- `docs/Grep_Sed.md` lines 89-105 for grep traversal and mode requirements.
- `docs/Grep_Sed.md` lines 127-155 for sed replacement, preview, write, and preservation requirements.
- `docs/Grep_Sed.md` lines 195-205 for text, match, byte, and encoding model requirements.
- `docs/Grep_Sed.md` lines 230-238 for output grammar and exact exit-code requirements.
- Prior iterate-needed memories for `docs/Grep_Sed.md` call out exit codes, output grammar, argv payloads, regex dialect, and default grep pattern mode as implementation blockers.

## Deliverables

1. **`docs/grep-sed-contract.md`** containing:
   - Command-state matrix for grep and sed with no paths, explicit files, directories, explicit stdin markers, sed preview, sed write, and invalid combinations.
   - Named traversal profile with ignore precedence, hidden-file behavior, symlink/reparse behavior, binary prefiltering, deterministic ordering, override precedence, and the inspected source of truth for current tree/search/symbol behavior.
   - Text, match, byte, encoding, line-ending, BOM, binary, and invalid-encoding model, including whether fixed sed mode operates on decoded UTF-8 text, bytes within accepted UTF-8 files, or raw bytes after classification.
   - Grep dialect: regex default, fixed-string flag, escaping examples, unsupported regex diagnostics, and an explicit v1 decision for null-delimited or other machine-safe path output.
   - Sed dialect: sed-like substitution grammar if retained, argv-native `--regex`/`--replace`, `--fixed`, replacement expansion, line ranges, global-per-line semantics, and unsupported constructs.
   - Agent-facing plain-text output grammar for each record family and stable warning/skip labels.
   - Exact exit-code table for grep and sed modes.
   - Write-safety contract covering atomicity, partial failure, file identity checks, content hash/preflight drift checks, metadata preservation, and optional manifest behavior.
   - Explicit deferred/non-goal note for syntax-aware tree-sitter extensions outside the portable textual core.

## Implementation notes

- Prefer argv-native examples as canonical, because the source doc identifies shell quoting as a portability risk.
- The grep default should be explicit. The design doc leans toward regex-by-default for grep/ripgrep familiarity with fixed-string available through a flag.
- Do not hide unresolved choices in prose. If a feature is deferred, record the deferral and its v1 behavior.
- Keep output natural-language oriented, but define stable labels and record shapes that agents can parse.

## Acceptance criteria

- [ ] A shared contract document exists and covers command-state defaults for no path, explicit paths, directories, explicit stdin markers, sed preview, and sed write.
- [ ] The contract names the default traversal profile and records any intentional difference from current tree/search/symbol traversal behavior.
- [ ] The contract defines regex default mode, fixed-string mode, replacement grammar, text model, encoding model, binary handling, output record families, warning labels, truncation/resume hints, mutation behavior, and exit-code classes.
- [ ] The contract includes argv payload cases for leading dashes, delimiter-like text, dollars, backslashes, empty replacements, and newlines.
- [ ] The contract contains a "Payload channels" decision table that explicitly accepts or defers `--` end-of-options handling, pattern-file input, replacement-file input, and stdin payload modes; accepted channels define v1 behavior and deferred channels define user-facing diagnostics or non-goal text.
- [ ] The contract explicitly distinguishes any v1 machine-safe path output requirement from optional null-delimited bulk workflow modes, or records that both are deferred.
- [ ] The contract defines exact grep and sed exit-code semantics for success with records, success with no records, invalid input, warnings/skips, and partial traversal failures.

## Validation plan

- **Coverage check:** Review `docs/grep-sed-contract.md` against every source-reference range above.
- **Exit-code check:** Confirm the document contains a table with grep match, grep no-match, grep error, sed preview changed, sed preview no-op, sed write changed, sed write no-op, invalid expression, invalid path, warning-only skips, and partial traversal failure.
- **Payload check:** Confirm examples include leading `-`, delimiter-like text, `$`, backslashes, empty replacement, and newline payloads.
- **Traversal check:** Compare the documented profile against existing `agent-tools tree`, `search`, and `symbols` behavior; any mismatch must be called out by name.

## Dependencies

(none)

## Provides to downstream tasks

- **T002:** Supplies the conformance matrix semantics.
- **T003:** Supplies traversal and classification semantics.
- **T004:** Supplies output grammar and exit codes.
- **T005-T008:** Supplies command behavior, write safety, and user-facing semantics.
