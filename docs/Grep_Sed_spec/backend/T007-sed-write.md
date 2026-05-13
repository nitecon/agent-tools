# T007 - Add agent-tools sed write mode

**Team:** backend
**Phase:** 3
**Depends on:** T006
**Status:** todo

## Scope

**In:** Add explicit `--write` mutation for sed after preview semantics are implemented.

**Out:** Optional audit/replay or richer bulk workflows not required by the core write contract. Those land in T008 if kept in scope.

## Source references

- `docs/Grep_Sed.md` lines 141-155 require preview defaults, explicit write, write-safety policy, summaries, preservation, and unchanged-file avoidance.
- `docs/Grep_Sed.md` lines 211-225 describe safe preview/write ergonomics.
- `docs/Grep_Sed.md` lines 302-306 define the sed write milestone.

## Deliverables

1. `--write` support for `agent-tools sed`.
2. Per-file atomic write implementation matching the T001 contract.
3. File identity and content hash/preflight drift checks.
4. Preservation behavior for line endings, trailing newline, BOM, permissions, and metadata defined by T001.
5. Write summary output and tests for named T002 sed write row IDs covering changed, no-op, skipped, warning, drift, atomic failure, and partial failure cases with exact exit code plus golden or record-shape output.
6. Documented implementation choices for same-directory temporary files, rename/replace behavior on Windows, metadata preservation limits, and whether fsync is required or explicitly deferred.

## Implementation notes

- Writes must remain explicit. Do not make a broad rewrite implicit from expression syntax.
- Avoid touching files with no content change.
- Stable replacement record IDs should connect preview and write results where possible.
- If normal write does not require a reviewed preview manifest, still retain hash/preflight checks sufficient to catch drift.
- Reuse the T013/T015 shared text command layer in `crates/agent-cli/src/cmd_text.rs` for command planning, pattern/matcher setup, target construction, traversal diagnostics, summary counters, and exit assembly. Keep write-only preflight and atomic mutation logic separate from the shared read/diagnostic loop.
- Use the T015 `TextCommandContext` to carry `TextPath`, decoded text, and `TextFile` metadata into write preflight so snapshot, content hash, file identity, preservation metadata, and stable write record IDs come from the traversal pass rather than a recomputed file scan.
- Return write results through the shared `TextCommandOutcome` shape, including changed files, replacement counts, no-op state, warnings/errors, and write records, then let shared finalization build `TextExitClassificationInput::sed_write` and handle summaries, truncation, traversal partial-failure state, and render selection.
- T006c now resolves the sed `TextOperationKind` before invoking the shared wrapper. When adding `--write`, keep preview/write behavior cohesive through a small execution-mode shape if needed instead of scattering `SedPreview`/`SedWrite` literals across validation, mutation, and finalization.
- If write-mode work touches fallback, traversal, `write-failed`, or partial-failure rendering, centralize prefix-to-label parsing for text errors in the same change while preserving current byte output and exit classification semantics.

## Acceptance criteria

- [ ] `agent-tools sed ... --write` mutates only supported text files with actual changes and refuses or skips files according to the T001 write-safety contract.
- [ ] Writes preserve line endings, trailing-newline presence, BOM behavior, permissions, and relevant metadata as defined by the contract.
- [ ] The implementation performs per-file atomic writes, file identity checks, content hash or preflight drift checks, and stable drift/skip reporting.
- [ ] Write summaries include changed files, replacement counts, skipped files by stable label, warnings, errors, and stable replacement record IDs or manifest references where supported.
- [ ] Tests cover changed files, no-op writes, drift detection, partial failures, unchanged-file avoidance, metadata preservation where practical, and cross-platform newline/BOM behavior.

## Validation plan

- **Integration tests:** Run `cargo test -p agent-cli sed_write`.
- **Mutation tests:** Use temp fixtures to assert exact content and metadata outcomes after write.
- **Drift tests:** Simulate content changes between preflight and write and assert stable drift labels.
- **Atomic failure tests:** Simulate permission, write, or rename failure where portable, or mark as platform-manual, and assert original content remains intact, stable failure labels are emitted, and status follows the T001 partial-failure class.
- **No-op tests:** Assert unchanged files are not rewritten.

## Dependencies

- **T006:** Supplies sed parser, matching, replacement expansion, and preview records.

## Provides to downstream tasks

- **T008:** Supplies core write behavior for optional bulk features.
- **T009:** Supplies user-facing write docs and safety examples.
