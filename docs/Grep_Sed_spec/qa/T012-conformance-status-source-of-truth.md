# T012 - Parse grep/sed expected statuses from the conformance matrix

**Team:** qa
**Phase:** 1 DRY follow-up
**Depends on:** T004, T010
**Status:** todo

## Scope

**In:** Keep `docs/grep-sed-conformance.md` as the source of truth for expected status values used by grep/sed classifier tests.

**Out:** Runtime grep/sed command behavior, warning-label model changes, and platform-only row execution.

## Source references

- T010 made the conformance markdown the source of truth for row IDs and fixture inventory.
- T004 added classifier tests that still duplicate row-id to expected-status literals in Rust.
- DRY Pass 2 confirmed this should gate T005 so new command rows do not add more hardcoded status pairs.

## Deliverables

1. Extend the conformance markdown parser in `crates/agent-cli/tests/grep_sed_conformance.rs` so automated rows expose parsed `Expected status` values.
2. Update `grep_sed_exit_status_rows_share_t002_row_ids`, or its replacement, so Rust maps row IDs to `TextExitClassificationInput` scenarios and compares classifier output against the parsed markdown status.
3. Preserve existing T010 audits for row IDs, fixture inventory, and fixture mappings.
4. Treat unsupported, deferred, and platform-only rows explicitly: skip them with a reason or audit them separately without weakening automated status checks.

## Acceptance criteria

- [ ] The conformance parser extracts `Expected status` values from automated rows and exposes them to executable status assertions.
- [ ] `grep_sed_exit_status_rows_share_t002_row_ids` no longer duplicates row-id to status literals that are already present in `docs/grep-sed-conformance.md`.
- [ ] Unsupported, deferred, or platform-only rows are either skipped with an explicit reason or audited separately without weakening automated status checks.
- [ ] Changing a status value in `docs/grep-sed-conformance.md` without updating the executable scenario mapping produces a targeted test failure.

## Validation plan

- Run `cargo test -p agent-cli grep_sed`.
- If parser helpers are factored, run the narrow conformance parser tests by name.

## Provides to downstream tasks

- **T005:** Grep command status assertions compare against markdown-owned expected statuses.
- **T006/T007:** Sed preview and write rows inherit the same source-of-truth status path.
