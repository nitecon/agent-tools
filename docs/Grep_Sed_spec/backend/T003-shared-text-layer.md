# T003 - Implement shared text traversal and classification layer

**Team:** backend
**Phase:** 1
**Depends on:** T001, T002
**Status:** todo

## Scope

**In:** Build a shared internal text-operation layer for target discovery, traversal, decoding, file classification, deterministic ordering, and write-preflight metadata capture.

**Out:** User-facing grep/sed command output and exit-code rendering. Those land in T004-T007.

## Source references

- `docs/Grep_Sed.md` lines 89-96 require recursive traversal aligned with existing tree/search/symbol behavior.
- `docs/Grep_Sed.md` lines 138-140 require sed to use the same traversal adapter as grep unless a difference is documented.
- `docs/Grep_Sed.md` lines 201-205 require an explicit byte and encoding model.
- `docs/Grep_Sed.md` lines 247-251 warn not to factor shared text operations until command-state, traversal, decoding, deterministic ordering, skip-label, and write-target contracts are chosen.

## Deliverables

1. **`crates/agent-fs/src/text_ops.rs`** or equivalent shared module for:
   - Target resolution for explicit files, directories, no-path current directory, and stdin marker where contracted.
   - Traversal profile, include/exclude filters, hidden/ignored behavior, symlink/reparse policy, and deterministic ordering.
   - Text decoding and classification for text, binary, invalid encoding, skipped, and errored files.
   - File identity/content metadata needed for sed write preflight checks, exposed as a concrete FileSnapshot/FileIdentity shape covering opened path and display path, canonical path where available, length, mtime granularity, content hash, permissions, file type or symlink/reparse disposition, line-ending/BOM/trailing-newline classification, Unix dev/inode where available, and a documented Windows fallback.
2. **`crates/agent-fs/src/lib.rs`** exports for the shared layer.
3. Unit tests and integration hooks for the T002 conformance fixtures.

## Implementation notes

- Prefer existing crate boundaries: filesystem concerns belong in `agent-fs`; CLI argument plumbing belongs in `agent-cli`.
- Extract or document a single traversal profile source of truth aligned with current tree/search/symbol behavior, including hidden-file, gitignore, git-global, git-exclude, symlink/reparse, and deterministic ordering choices. If grep/sed intentionally differ, name the delta in the contract and tests.
- Do not introduce user-facing command behavior that bypasses T004's renderer.
- Keep skip and warning labels stable and contract-derived.

## Acceptance criteria

- [ ] A shared internal layer resolves file targets using the documented traversal profile, deterministic ordering, include/exclude filters, hidden/ignored file rules, symlink/reparse behavior, and binary prefiltering.
- [ ] The layer exposes file classification for text, binary, invalid encoding, skipped, and errored files using stable warning labels from the contract.
- [ ] The layer preserves enough file metadata and content identity for later sed write preflight checks.
- [ ] Unit tests cover no-path current-directory scope, explicit file paths, directories, stdin marker behavior where supported, include/exclude precedence, invalid encoding, and binary detection.

## Validation plan

- **Unit tests:** Run `cargo test -p agent-fs text_ops`.
- **CLI fixture tests:** Run the T002 conformance scaffold rows that exercise target discovery and classification.
- **Determinism check:** Re-run traversal tests with unsorted fixture creation order and confirm output ordering remains stable.

## Dependencies

- **T001:** Defines traversal, text, encoding, and skip-label semantics.
- **T002:** Provides matrix and fixtures.

## Provides to downstream tasks

- **T005:** Target discovery and text records for grep.
- **T006:** Target discovery and text records for sed preview.
- **T007:** File identity and preflight metadata for sed write.
