# T006 - Add agent-tools sed preview command

**Team:** backend
**Phase:** 2
**Depends on:** T003, T004, T005
**Status:** todo

## Scope

**In:** Add `agent-tools sed` read-only preview behavior, including substitution parsing, matching, replacement expansion, line ranges, and preview records.

**Out:** File mutation. Explicit writes land in T007.

## Source references

- `docs/Grep_Sed.md` lines 51-56 show the proposed sed command forms.
- `docs/Grep_Sed.md` lines 62-81 require argv-native forms and payload edge cases.
- `docs/Grep_Sed.md` lines 127-155 list sed behavior and preview/write safety goals.
- `docs/Grep_Sed.md` lines 295-300 define the sed preview milestone.

## Deliverables

1. CLI parser support for:
   - `agent-tools sed <expression> [path ...]`
   - `agent-tools sed --regex <pattern> --replace <replacement> [path ...]`
   - `agent-tools sed --fixed <old> <new> [path ...]`
2. Substitution parser and replacement engine matching the T001 grammar.
3. Preview mode integration with T003 traversal/classification and T004 renderer/exit-code classifier.
4. Conformance tests for named sed preview row IDs from T002, including exact exit code plus golden or record-shape output for changed, no-op, warning-only, invalid input, payload-channel, and partial-failure classes.
5. Any required dependency additions or feature choices recorded in the relevant Cargo.toml touch surfaces before implementation, including regex support and replacement-engine behavior required by the T001 contract.

## Implementation notes

- The argv-native forms are canonical for scripts and cross-shell examples.
- If sed-like expression syntax is kept, implement only the contracted subset and diagnose unsupported syntax clearly.
- Preview should produce stable replacement record IDs that can also be referenced by write summaries or optional manifests.
- Reuse the T013/T015 shared text command layer in `crates/agent-cli/src/cmd_text.rs` for pattern-source validation, target option construction, matcher setup, traversal diagnostics, summary counters, and exit assembly instead of copying the T005 `run_grep` command loop.
- Implement preview by adding a sed operation callback over the T015 `TextCommandContext`/`TextCommandOutcome` boundary: use the context's `TextPath`, decoded text, and optional `TextFile` metadata to build stable preview record IDs, return changed/replacement/no-op counts in the outcome, and let shared finalization insert summaries, select render/null-path output, propagate traversal diagnostics, and construct `TextExitClassificationInput::sed_preview`.
- Do not add a second `collect_text_files` loop for sed preview; extend the shared `collect_text_command_outcomes` path so grep and sed preview share traversal, skip/warning promotion, truncation, and exit classification.

## Acceptance criteria

- [ ] `agent-tools sed` supports the contracted substitution grammar plus canonical argv-native `--regex <pattern> --replace <replacement>` and `--fixed <old> <new>` forms.
- [ ] Preview mode is the default for multi-file edits and can be requested explicitly with `--preview`; no files are mutated by this task.
- [ ] Regex replacement uses the documented Rust replacement expansion contract, fixed replacements are byte-literal within the supported text model, and unsupported constructs produce stable diagnostics.
- [ ] Preview records include file, line, old text, new text, replacement count, and stable replacement record IDs with deterministic ordering and truncation/resume behavior.
- [ ] Tests cover line ranges, global-per-line replacement, case-insensitive matching, unchanged files, binary/invalid-encoding skips, include/exclude filters, and argv payload edge cases.

## Validation plan

- **Unit and integration tests:** Run `cargo test -p agent-cli sed_preview`.
- **Mutation guard:** Confirm file hashes are unchanged after preview tests.
- **Payload cases:** Test leading `-`, delimiter-like strings, `$`, backslashes, empty replacement, and newline payloads through argv-native forms.

## Dependencies

- **T003:** Supplies traversal and file text records.
- **T004:** Supplies preview renderer and exit codes.
- **T005:** Establishes the shared CLI/conformance plumbing shape before sed preview extends the same command/test surfaces. If the team adds a separate shared CLI/parser/conformance scaffold task instead, depend on that task rather than T005.

## Provides to downstream tasks

- **T007:** Supplies replacement records and matching semantics for writes.
- **T009:** Supplies user-facing sed preview examples.
