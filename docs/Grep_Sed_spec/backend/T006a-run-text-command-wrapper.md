# T006a - Consolidate cmd_grep/cmd_sed error-handling into shared run_text_command wrapper

**Team:** backend
**Phase:** 2 (DRY follow-up)
**Depends on:** T006
**Status:** todo

## Scope

**In:** Extract the success/error dispatch path duplicated in `cmd_grep` and `cmd_sed` (crates/agent-cli/src/cmd_text.rs) into a single generic helper `run_text_command<F>(operation: TextOperationKind, f: F) -> !`. Remove the thin `text_error_result()` wrapper; `render_text_error_result` becomes the single error renderer. Both `cmd_grep` and `cmd_sed` shrink to a single-line invocation bound to their `TextOperationKind` variant.

**Out:** Behavioral changes. This is a zero-diff refactor at the conformance level — every existing grep/sed conformance row must produce byte-identical stdout/stderr and exit codes.

## Rationale

DRY peer review of wave T006 (refactor-proposals + semantic-duplication, two passes, both lenses) converged on this as the highest-worth consolidation (worth_it 4-5). The two error handlers are byte-for-byte identical except for the `TextOperationKind` variant. Without this, T007 (sed --write) would spawn a third copy.

Consolidates findings: DRY-RP-001, DRY-SD-001, DRY-RPSD-001 (cross-lens emergent).

## Deliverables

1. `run_text_command<F>(operation: TextOperationKind, f: F) -> !` helper in `crates/agent-cli/src/cmd_text.rs` where:
   - `F: FnOnce() -> Result<TextCommandResult, TextCommandError>` (or the existing error type — match what's in the file).
   - On `Ok(TextCommandResult)`: write stdout/stderr (bytes or string per the result), then `std::process::exit` with the classified exit code.
   - On `Err`: render via `render_text_error_result`, then exit with `classify_text_exit_code(TextExitClassificationInput::invalid_input(operation))`.
2. Remove `text_error_result()` (thin wrapper) — call sites use `render_text_error_result` directly via the helper.
3. `cmd_grep` reduces to: `run_text_command(TextOperationKind::Grep, || /* existing body sans error/exit */)`.
4. `cmd_sed` reduces analogously for `TextOperationKind::SedPreview` (and any other preview variant).

## Validation

- `cargo build -p agent-cli` clean.
- `cargo test -p agent-cli` — all `grep_sed_conformance.rs` rows pass byte-identical (stdout, stderr, exit code) for every automated row including invalid-input and warning-only paths.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.

## Touch surface

- `crates/agent-cli/src/cmd_text.rs`

## Notes

Must land before T007 so sed --write inherits the unified funnel rather than cloning a third copy. Cross-host recommendation: run conformance tests on a second host to catch subtle stdout/stderr regressions in error paths.
