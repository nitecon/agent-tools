# T006b - Extract shared resolve_text_payload_file helper for pattern/replacement file I/O

**Team:** backend
**Phase:** 2 (DRY follow-up)
**Depends on:** T006, T006a
**Status:** todo

## Scope

**In:** Centralize `std::fs::read_to_string` + error wrapping for all text-command payload file reads into a single helper `resolve_text_payload_file(path: &str, field: &str, operation: TextOperationKind) -> Result<String>` in `crates/agent-cli/src/cmd_text.rs`. Delegate `resolve_text_pattern()`'s `--pattern-file` branch and `read_payload_file()`'s `--pattern-file` / `--replacement-file` reads to it.

**Out:** Drift-check or preflight semantics (those belong to T007). This task only consolidates the read+wrap path so T007 can extend a single location.

## Rationale

DRY peer review (Pass 2 cross-pollination, both lenses) endorsed this as explicit T007 prep (worth_it 3). `resolve_text_pattern()` (grep, cmd_text.rs ~300-339) and `read_payload_file()` (sed, ~1097-1108) duplicate file I/O + error wrapping. Centralizing now lets T007 add `--replacement-file` drift-check policy in a single place rather than forking sed's payload reader.

Consolidates findings: DRY-SD-003, DRY-CONV-003.

## Deliverables

1. `resolve_text_payload_file(path: &str, field: &str, operation: TextOperationKind) -> Result<String>` in `crates/agent-cli/src/cmd_text.rs`. Returns the file contents on success; on I/O error, returns an error that produces a `TextExitClassificationInput::invalid_input(operation)` classification identical to current behavior.
2. `resolve_text_pattern()` delegates its `--pattern-file` branch to the helper with `field="pattern"` and `operation=TextOperationKind::Grep`.
3. `read_payload_file()` delegates its `--pattern-file` / `--replacement-file` reads to the helper with `operation=TextOperationKind::SedPreview` and the appropriate `field` name (`"pattern"` or `"replacement"`).
4. Error messages are byte-identical to current behavior for both grep and sed payload-file failure conformance rows.

## Validation

- `cargo build -p agent-cli` clean.
- `cargo test -p agent-cli` — all `grep_sed_conformance.rs` rows pass byte-identical, including the pattern-file and replacement-file failure rows.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.

## Touch surface

- `crates/agent-cli/src/cmd_text.rs`

## Notes

Must land before T007. T007 will add `--replacement-file` drift/preflight policy *inside* this helper rather than cloning sed's payload reader.
