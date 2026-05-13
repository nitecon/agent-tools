# T014a - Consolidate duplicated legacy validation-warning conformance assertions

**Team:** qa/refactor
**Phase:** 3
**Depends on:** T014
**Status:** todo
**Origin:** DRY peer-review consolidation after T014 wave (refactor-proposals + semantic-duplication lenses converged on a low-severity test drift risk).

## Scope

**In:** In `crates/agent-cli/tests/grep_sed_conformance.rs`, reduce overlap between the T014 shared `TextCliCase` / `ExpectedText` cross-command tables and older command-local tests. Move or keep shared invalid-input, invalid-path, invalid-expression, zero-limit, warning-summary, stdout/stderr, and status expectations in the shared harness or a direct successor helper.

Trim `grep_cli_renders_warning_and_invalid_input_classes` and `sed_preview_renders_warning_and_invalid_input_classes` so they retain only command-specific row-golden, payload-channel, leading-dash, replacement/capture, and mutation-safety coverage that is not already asserted by the cross-command table.

**Out:** Command behavior changes, renderer changes, new grep/sed features, or changes outside the conformance test file.

## Acceptance Criteria

- Shared validation and warning rendering expectations have one primary assertion location.
- Command-local grep/sed tests retain only unique command or row semantics.
- Existing grep golden rows remain byte-identical.
- `cargo test -p agent-cli grep_sed` passes.
- `cargo test -p agent-cli grep` passes.

## Touch Surface

- `crates/agent-cli/tests/grep_sed_conformance.rs`

## Rationale

T014 correctly added shared cross-command coverage. DRY Pass 2 peers agreed that older command-local warning and invalid-input assertions now duplicate some of the shared `TextCliCase` contract. This is not a correctness regression, but it creates a low-severity future drift surface if output labels or summary rendering change.
