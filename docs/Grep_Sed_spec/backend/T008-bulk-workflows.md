# T008 - Add optional safer bulk workflow affordances

**Team:** backend
**Phase:** 4
**Depends on:** T007
**Status:** todo

## Scope

**In:** Add explicitly scoped bulk workflow affordances after the core grep/sed/preview/write behavior is stable.

**Out:** Syntax-aware tree-sitter extensions. The source doc names those as a later exploration outside the portable textual core.

## Source references

- `docs/Grep_Sed.md` lines 308-312 define the safer bulk workflow milestone.
- `docs/Grep_Sed.md` lines 211-225 describe preview, summaries, bounded output, deterministic ordering, explicit writes, dry-run friendliness, and backup policy.
- `docs/Grep_Sed.md` lines 261-264 defer syntax-aware filters until later milestones.

## Deliverables

1. Optional features from the T001/T002 contract that were deliberately deferred from the core implementation, such as richer include/exclude workflows, null-delimited modes, optional audit manifests, or recovery/audit affordances.
2. Tests that prove optional modes do not alter default behavior.
3. Documentation updates for each opt-in workflow.
4. If T001/T002 defer every optional bulk workflow for v1, a checked-in decision record or task note documenting the deferral, confirming no default behavior changed, and stating whether T009 has optional workflow docs to publish.

## Implementation notes

- This task should stay narrow. Do not add every imaginable grep/sed compatibility flag.
- Optional audit manifests should use stable record IDs and content hashes so they are useful for agents and scripts.
- Hidden backups should remain opt-in if backup support is added.

## Acceptance criteria

- [ ] Richer include/exclude workflows, null-delimited modes, or optional audit manifests are implemented only if T001/T002 mark them in scope for this milestone.
- [ ] Any audit manifest or replay support uses stable record IDs, content hashes, and explicit failure labels from the contract.
- [ ] New modes remain opt-in and do not change default grep, sed preview, or sed write behavior.
- [ ] Tests cover each added workflow and prove existing output and exit-code contracts do not regress.
- [ ] If no optional workflow is implemented, the task records the scoped deferral and proves default grep, sed preview, and sed write behavior remain unchanged.

## Validation plan

- **Regression tests:** Re-run grep, sed preview, and sed write conformance suites.
- **Optional mode tests:** Run exact-output tests for each added mode.
- **Docs check:** Confirm every new flag has CLI help and user-facing docs.

## Dependencies

- **T007:** Supplies stable write behavior.

## Provides to downstream tasks

- **T009:** Supplies optional workflow docs.
