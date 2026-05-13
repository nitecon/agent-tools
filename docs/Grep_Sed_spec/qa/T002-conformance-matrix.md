# T002 - Create conformance matrix and fixture plan

**Team:** qa
**Phase:** 0
**Depends on:** T001
**Status:** todo

## Scope

**In:** Convert the shared contract into a concrete conformance matrix and fixture plan.

**Out:** Full implementation of grep or sed. This task may create test scaffolding, but command implementation lands in backend tasks.

## Source references

- `docs/Grep_Sed.md` lines 268-286 require a milestone 0 conformance scenario matrix.
- `docs/Grep_Sed.md` lines 256-258 call for Linux/macOS/Windows path, traversal, newline/BOM, and deterministic output tests.
- `docs/Grep_Sed.md` lines 74-80 require argv payload edge cases before implementation.

## Deliverables

1. **`docs/grep-sed-conformance.md`** containing the matrix rows, stable row IDs, expected result classes, exact process status, stdout/stderr record shape or golden output, warning labels, and byte-exact versus normalized comparison mode.
2. **`crates/agent-cli/tests/grep_sed_conformance.rs`** test scaffold or initial tests keyed to the matrix row IDs.
3. **`crates/agent-cli/tests/fixtures/grep_sed/`** fixture plan and seed fixtures.
4. A platform validation target or artifact plan that names Linux, macOS, and Windows automated versus manual rows and the exact commands expected to close them.

## Implementation notes

- The matrix should be useful before code exists. Mark rows as automated, platform-manual, or deferred.
- Treat the matrix as the canonical conformance index for later implementation tasks; later tasks should consume existing row IDs or add rows before adding behavior.
- Cover warning-only skips separately from errors so exit-code semantics are not conflated.
- Include no-path current-directory behavior because the source doc explicitly scopes no-path defaults to the current working directory.
- Name the platform validation closure path: CI matrix, checked-in validation report, or documented manual artifact with exact commands and expected outcomes.

## Acceptance criteria

- [ ] A conformance matrix lists representative grep and sed rows across command mode, input source, traversal profile, text model, dialect, output family, warning labels, mutation behavior, and exit-code class.
- [ ] Every automated row has a stable row ID, command argv, fixture inputs, exact process status, stdout/stderr record shape or golden output, warning labels, and byte-exact or normalized comparison mode.
- [ ] Fixtures cover Linux, macOS, and Windows path handling concerns including CRLF, BOM, invalid UTF-8, binary prefiltering, hidden files, ignored files, symlinks or reparse points, and deterministic path ordering.
- [ ] The matrix includes quote-resistant argv payload examples for patterns and replacements that begin with dashes or contain delimiter-like text, dollars, backslashes, empty replacements, and newlines, and maps them to the T001 accepted/deferred payload-channel decisions.
- [ ] The matrix maps every row to either an automated test target or a documented manual/platform validation target and names the platform closure artifact, report path, runbook path, or CI job that will close Linux, macOS, and Windows rows.

## Validation plan

- **Matrix completeness check:** Each major section in T001 has at least one matrix row or a named deferral.
- **Fixture check:** Fixture paths and contents are listed with expected classifications.
- **Automation check:** `cargo test -p agent-cli grep_sed` runs the available scaffold/tests without requiring unimplemented commands to pass prematurely.
- **Row audit check:** Mechanically audit every automated row for stable row ID, command argv, fixture inputs, exact process status, stdout/stderr expectation or golden output, warning labels, and byte-exact or normalized comparison mode.
- **Platform check:** Linux, macOS, and Windows rows name either cross-platform automated expectations or manual validation instructions, plus the concrete CI job name or checked-in validation report/runbook path and exact commands/results that close the row.

## Dependencies

- **T001:** Provides the contract being transformed into test scenarios.

## Provides to downstream tasks

- **T003-T008:** Supplies fixtures and expected behavior for implementation.
