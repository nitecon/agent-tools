# T011 - Unify grep/sed warning label model across traversal diagnostics and renderer records

**Team:** backend
**Phase:** 1 DRY follow-up
**Depends on:** T003, T004
**Status:** todo

## Scope

**In:** Consolidate the warning-label vocabulary shared by T003 traversal diagnostics and T004 renderer records before grep/sed command wiring starts.

**Out:** Grep matching, sed preview, sed writes, and broader traversal-profile refactors.

## Source references

- `docs/grep-sed-contract.md` defines stable skip and warning labels for traversal and rendering.
- T003 introduced traversal diagnostics in `crates/agent-fs/src/text_ops.rs`.
- T004 introduced renderer warning records in `crates/agent-core/src/output.rs`.
- DRY Pass 1 and Pass 2 both found duplicated label vocabularies across those modules.

## Deliverables

1. A single contract-owned warning label type, or a deliberately narrow traversal warning wrapper with exhaustive conversion, that covers:
   - `binary-skipped`
   - `invalid-utf8`
   - `unsupported-encoding`
   - `path-skipped`
   - `traversal-error`
2. `agent-fs` traversal diagnostics no longer maintain a separate warning-label string table that can drift from renderer labels.
3. Conformance coverage proving a T003 traversal diagnostic can be rendered through the T004 skip/warning record path without command-layer label retyping.
4. T005/T006 implementation notes or task comments point command wiring at the unified diagnostic-to-renderer path.

## Implementation notes

- Prefer keeping the shared label contract in `agent-core` if that remains acyclic for `agent-fs`.
- If `agent-fs` needs a narrower domain type, make the conversion exhaustive and test it mechanically.
- Do not add user-facing grep or sed commands in this task.

## Acceptance criteria

- [ ] A single contract-owned warning label type, or a deliberately narrow wrapper with exhaustive conversion, defines binary-skipped, invalid-utf8, unsupported-encoding, path-skipped, and traversal-error.
- [ ] `agent-fs` traversal diagnostics no longer own a duplicate label string table that can drift from `agent-core` renderer labels.
- [ ] Conformance coverage proves a traversal diagnostic can be converted or rendered as the contracted skip/warning output without command-specific label mapping.
- [ ] T005 and T006 implementation notes or task comments point command wiring at the unified label path.

## Validation plan

- Run `cargo test -p agent-fs text_ops`.
- Run `cargo test -p agent-core grep_sed`.
- Run `cargo test -p agent-cli grep_sed`.

## Provides to downstream tasks

- **T005:** A single path from traversal diagnostics to grep skip/warning records.
- **T006:** A single path from traversal diagnostics to sed preview skip/warning records.
