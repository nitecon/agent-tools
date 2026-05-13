# T007a - Consolidate hash + test-fixture helpers from T007 sed --write

**Team:** backend
**Phase:** 3
**Depends on:** T007
**Status:** todo
**Origin:** DRY peer-review consolidation after T007 wave (refactor-proposals + semantic-duplication lenses converged on three concrete items).

## Scope

**In:**
1. Make `agent_fs::text_ops::stable_content_hash` public (or re-export through `agent_fs::lib`) and delete `stable_content_hash_local` in `crates/agent-cli/src/cmd_text.rs` so sed write drift checks call the single canonical FNV-1a implementation.
2. Promote `relative_path_hash` from `crates/agent-cli/src/cmd_text.rs` (currently duplicated by `crates/agent-cli/tests/grep_sed_conformance.rs`) into a single source of truth — either `pub(crate)` and import via `use agent_cli::cmd_text::relative_path_hash`, or move both copies into a shared internal helper module. Tests must use the production implementation so drift between the two is impossible.
3. Extract a `sed_write_fixture()` helper in `crates/agent-cli/tests/grep_sed_conformance.rs` covering the common per-test setup that the new T007 sed write rows duplicate; refactor the new sed write tests to use it. Keep grep / sed preview fixtures as-is.

**Out:** Anything not above. Do not touch sed preview semantics, drift logic, atomic write, or main.rs.

## Acceptance criteria
- `stable_content_hash_local` is gone from cmd_text.rs; sed write drift check calls the agent-fs symbol.
- `relative_path_hash` exists in exactly one place; tests import it.
- New `sed_write_fixture()` helper exists in the conformance test file and is used by the T007 sed write rows.
- `cargo test -p agent-cli` passes with byte-identical conformance output for every existing automated row.
- `cargo build --workspace` is warning-free.

## Touch surface
- `crates/agent-cli/src/cmd_text.rs`
- `crates/agent-fs/src/text_ops.rs`
- `crates/agent-fs/src/lib.rs`
- `crates/agent-cli/tests/grep_sed_conformance.rs`
