# T005 - Add agent-tools grep command

**Team:** backend
**Phase:** 2
**Depends on:** T003, T004
**Status:** todo

## Scope

**In:** Add the user-facing `agent-tools grep` command backed by the shared traversal, text, output, and exit-code layers.

**Out:** Sed preview/write behavior. Those land in T006 and T007.

## Source references

- `docs/Grep_Sed.md` lines 51-56 show the proposed command family.
- `docs/Grep_Sed.md` lines 89-105 list grep goals.
- `docs/Grep_Sed.md` lines 107-118 show example command shape and output expectations.
- `docs/Grep_Sed.md` lines 288-293 define the grep milestone.

## Deliverables

1. CLI parser support for `agent-tools grep <pattern> [path ...]`.
2. Flags per the T001 contract, including regex default, `--fixed`, `--ignore-case`, include/exclude globs, context, count-only, files-with-matches, and v1 machine-safe output mode.
3. Integration with T003 traversal/classification and T004 renderer/exit-code classifier.
4. Conformance tests for named grep row IDs from T002, including exact exit code plus golden or record-shape output for match, no-result, warning-only, invalid input, and partial-failure classes.
5. Any required dependency additions or feature choices recorded in the relevant Cargo.toml touch surfaces before implementation, including regex support and any hash/write-safety dependencies consumed by shared text operations.

## Implementation notes

- Regex should be the default unless T001 records a contrary final decision.
- No-path behavior is current-directory scoped, not broader filesystem state.
- Do not add JSON as the primary workflow; keep plain text compact and deterministic.

## Acceptance criteria

- [ ] `agent-tools grep <pattern> [path ...]` supports regex-by-default search, `--fixed`, `--ignore-case`, recursive directory search, include/exclude globs, before/after context, count-only, files-with-matches, and machine-safe path output semantics per the contract.
- [ ] Default output includes path, line number, and matching line in deterministic order, with bounded output and resume hints for large result sets.
- [ ] Binary, invalid-encoding, hidden, ignored, symlink/reparse, and traversal-error cases produce the stable labels and exit codes defined by T001.
- [ ] Tests cover explicit files, directories, no-path current-directory scope, regex/fixed matching, context, count/files modes, include/exclude filters, and null-delimited or machine-safe output variants included in v1.

## Validation plan

- **Unit and integration tests:** Run `cargo test -p agent-cli grep`.
- **Conformance subset:** Run T002 grep rows across supported local fixtures.
- **Manual smoke:** Use `agent-tools grep "BrokerActor" .` and `agent-tools grep --fixed "literal.name" . --files-with-matches` on the repo and inspect deterministic output.

## Dependencies

- **T003:** Supplies traversal and file text records.
- **T004:** Supplies renderer and exit codes.

## Provides to downstream tasks

- **T009:** User-facing documentation examples and behavior.
- **T008:** Shared include/exclude and machine-safe output behavior for bulk workflows.
