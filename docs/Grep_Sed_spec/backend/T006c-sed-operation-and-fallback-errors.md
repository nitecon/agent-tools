# T006c - Centralize sed operation selection and structured fallback error rendering

**Team:** backend
**Phase:** 2 (DRY follow-up)
**Depends on:** T006a, T006b
**Status:** todo

## Scope

**In:** Consolidate the wrapper/operation boundary exposed by T006a before sed write mode lands. `run_text_command` should use one structured fallback error-rendering path, and sed should derive its effective operation mode before invoking the wrapper so preview and write failures classify through the same operation used by inner validation and finalization.

**Out:** Implementing sed `--write`, atomic write behavior, drift checks, or payload-file helper work. T006b owns payload-file read consolidation; T007 owns mutation semantics.

## Rationale

The T006a DRY pass found two related risks at the same boundary:

- `run_text_command` centralizes terminal dispatch, but its fallback `Err` and stdout-write failure paths still manually format invalid-input output instead of using the shared structured renderer.
- `cmd_sed` currently binds the wrapper to `TextOperationKind::SedPreview`, while T007 will introduce real `SedWrite` paths. Any future wrapper-level write error must classify as write, not preview.

Both peer-review lenses converged on one prerequisite task rather than folding this into T007. Keeping this small and separate gives T007 a clean operation-selection contract for write mode.

## Deliverables

1. Add a narrow sed execution mode or operation resolver that derives the effective `TextOperationKind` from `SedArgs` before calling `run_text_command`.
2. Thread the resolved operation through sed validation, fallback error classification, collection, and finalization paths instead of adding new hard-coded `SedPreview` literals for write-related behavior.
3. Route `run_text_command` fallback `Err` paths and stdout-write failures through `render_text_error_result` or a single equivalent `TextCommandResult` helper.
4. Preserve existing byte output and exit codes for every automated grep/sed conformance row.
5. Leave T006b payload-file helper behavior intact; T006c should consume that helper after it lands rather than duplicating file payload handling.

## Acceptance Criteria

- [ ] `run_text_command` no longer manually formats fallback invalid-input errors or stdout-write failures with ad hoc `eprintln!` string assembly.
- [ ] Wrapper-level fallback errors are converted into `TextCommandResult` output through `render_text_error_result` or one equivalent helper, preserving existing byte output and exit codes.
- [ ] `cmd_sed` derives a sed execution mode or `TextOperationKind` from `SedArgs` before invoking `run_text_command`.
- [ ] Preview and write wrapper-level failures use the same operation kind as inner sed validation/finalization.
- [ ] Existing sed preview conformance rows remain byte-identical.
- [ ] T007 can add `--write` without introducing new `SedPreview` literals for write-mode validation, traversal, finalization, or fallback error classification.

## Validation Plan

- `cargo build -p agent-cli`
- `cargo test -p agent-cli --test grep_sed_conformance`
- `cargo test -p agent-cli`
- `cargo clippy --workspace --all-targets -- -D warnings`
- If practical, add or force one targeted wrapper-level fallback error check; otherwise document why normal conformance rows do not exercise that path.

## Touch Surface

- `crates/agent-cli/src/cmd_text.rs`
- `crates/agent-cli/tests/grep_sed_conformance.rs`

## Notes

Cross-host recommendation: after T006c and again after T007, run grep/sed conformance on a second host with attention to invalid input, replacement-file failure, stdout/stderr rendering, sed write classification, and partial write failure rows.
