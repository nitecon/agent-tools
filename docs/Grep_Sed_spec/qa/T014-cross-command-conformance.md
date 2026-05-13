# T014 - Add cross-command conformance for shared text validation and warning paths

**Team:** qa/refactor
**Phase:** 3
**Depends on:** T013, T006, T007
**Status:** todo

## Scope

**In:** After grep and sed preview/write exist, add shared conformance tests or helpers proving invalid input, diagnostics, output-mode validation, warning summaries, and partial traversal classification stay aligned across commands and reuse the shared text command engine.

**Out:** Implementing missing command behavior. This task validates and consolidates test assertions after T006/T007 land.

## Source references

- DRY pass 2 tasks `019e1eef-8076-70c2-85b7-86573a83f281` and `019e1eef-a023-76b1-aeb9-2db6f5affaf4` both recommend this as a post-sed QA consolidation, not as a gate before T006.
- Existing conformance tests live in `crates/agent-cli/tests/grep_sed_conformance.rs`.

## Deliverables

1. Shared test helpers or table rows for cross-command invalid input and diagnostic paths.
2. Assertions that grep, sed preview, and sed write use equivalent warning-summary and exit classification behavior where applicable.
3. Existing grep golden rows remain unchanged.

## Acceptance criteria

- [ ] Shared tests or table rows assert equivalent invalid-expression, invalid-input, invalid-path, mutually exclusive output mode, zero limit, warning-summary, and partial traversal classification behavior across grep, sed preview, and sed write where the command supports the mode.
- [ ] Tests demonstrate reuse of the shared engine rather than independent command-local string matching.
- [ ] Existing grep golden rows remain unchanged.
- [ ] Sed preview/write rows use shared helpers rather than parallel command-specific assertions.
- [ ] Coverage explicitly exercises invalid-input, invalid-path, write-failed, partial-traversal-failure, stdout/stderr rendering, and classification alignment so the T006c/T007 prefix-label parsing path cannot drift silently.

## Validation plan

- Run `cargo test -p agent-cli grep_sed`.
- Run `cargo test -p agent-cli grep`.

## Dependencies

- **T013:** Provides the shared command engine to validate.
- **T006:** Supplies sed preview behavior.
- **T007:** Supplies sed write behavior.

## Provides to downstream tasks

- **T008:** Confidence that optional bulk workflows do not drift from shared command semantics.
- **T009:** Stable cross-command behavior for user-facing docs.
