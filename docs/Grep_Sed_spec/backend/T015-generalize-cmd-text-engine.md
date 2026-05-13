# T015 - Generalize cmd_text operation context and finalization before sed preview

**Team:** backend/refactor
**Phase:** 2
**Depends on:** T013
**Status:** todo

## Scope

**In:** Generalize the shared `cmd_text` engine around per-file text command context and structured operation outcomes before sed preview/write are implemented.

**Out:** Sed preview/write behavior itself. This task should preserve grep behavior and prepare the shared engine boundary for T006/T007.

## Source references

- T013 moved grep execution from `crates/agent-cli/src/main.rs` into `crates/agent-cli/src/cmd_text.rs`.
- DRY pass 2 tasks `019e1f12-5a46-7001-886b-e078d82751c0` and `019e1f12-820a-7ab1-9be8-c327be622098` converged that the T013 boundary is still too grep-shaped for sed preview/write.

## Deliverables

1. A `TextCommandContext` or equivalent passed to operation callbacks with `TextPath` plus the relevant `TextFile` metadata for filesystem inputs and a clear stdin variant if applicable.
2. A structured `TextCommandOutcome` or equivalent carrying records, matched files, changed files, replacement counts, no-op state, diagnostics, and operation kind as applicable.
3. Shared finalization helpers for summary insertion, truncation flagging, render/null-path selection where applicable, warning/error propagation, partial traversal state, and `TextExitClassificationInput` construction.
4. T006/T007 implementation notes pointing at this context/outcome/finalization boundary.

## Acceptance criteria

- [ ] Grep conformance output and exit codes remain unchanged.
- [ ] Shared traversal invokes operation callbacks with a `TextCommandContext` that includes `TextPath` plus the `TextFile` metadata needed by preview IDs and write preflight, with a clear stdin variant if applicable.
- [ ] Operation callbacks return structured `TextCommandOutcome` values carrying records, matched files, changed files, replacement counts, no-op state, diagnostics, and operation kind as applicable.
- [ ] Shared finalization handles summary insertion, truncation flagging, render/null-path selection where applicable, warning/error propagation, partial traversal state, and `TextExitClassificationInput` construction for grep, sed preview, and sed write.
- [ ] T006 can implement sed preview without a second `collect_text_files` loop or duplicated `run_grep` finalization.
- [ ] T007 can reuse the same per-file context for snapshot/hash/identity/preservation preflight without recomputing traversal state.

## Validation plan

- Run `cargo test -p agent-cli grep`.
- Run `cargo test -p agent-cli grep_cli_matches_core_conformance_rows`.

## Dependencies

- **T013:** Provides the first shared `cmd_text` module to generalize.

## Provides to downstream tasks

- **T006:** Shared per-file context, operation outcome, and finalization for sed preview.
- **T007:** Shared per-file context for write-safety preflight and shared finalization for write summaries/exits.
