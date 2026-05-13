# T010 - Consolidate conformance row and fixture audits

**Team:** qa
**Phase:** 0
**Depends on:** T002
**Status:** todo

## Scope

**In:** Remove the drift-prone duplication introduced by the initial
conformance scaffold by making `docs/grep-sed-conformance.md` the source of
truth for row IDs and fixture inventory checks.

**Out:** Full grep/sed command implementation, golden stdout/stderr assertions,
or changes to the shared command contract beyond clarifying fixture membership.

## Source references

- `docs/grep-sed-conformance.md` declares itself the canonical v1 conformance
  index.
- `crates/agent-cli/tests/grep_sed_conformance.rs` currently keeps row and
  fixture inventories as separate Rust constants.
- DRY review tasks `019e1de0-6b19-7812-843f-1f2816a61938`,
  `019e1de0-869a-7721-82ec-fd94ac5d30bf`,
  `019e1de7-0162-7c72-a0f9-5bec5974de1f`, and
  `019e1de7-1dd1-7400-8783-8280474f9533` converged that this duplication can
  let future rows or fixtures drift from the scaffold.

## Deliverables

1. Update `crates/agent-cli/tests/grep_sed_conformance.rs` so row IDs are
   parsed from the Automated, Platform, and Deferred tables in
   `docs/grep-sed-conformance.md`, with section-level assertions instead of a
   second complete row inventory.
2. Parse the Fixture Inventory table and verify every non-generated and
   non-planned fixture entry exists on disk. Generated or planned fixtures must
   be explicitly exempted by classification or wording in the inventory.
3. Cross-check row fixture cells against inventory row mappings so a fixture
   named by a row and a fixture inventory entry disagreeing about row ownership
   fails the scaffold.
4. Resolve the immediate GS-A001 ambiguity by either adding `ignored/kept.txt`
   to GS-A001 where appropriate or documenting/testing that GS-A001 runs in an
   isolated workspace that excludes it.
5. Resolve GS-P004 by either marking `platform/path-order/` as generated or
   planned consistently, or by adding exact seed filenames and auditing them.

## Acceptance criteria

- [ ] The row audit treats `docs/grep-sed-conformance.md` as the source of
      truth and cannot pass when a markdown row is added without being parsed
      into its section.
- [ ] The fixture audit derives required fixture paths from the Fixture
      Inventory table instead of a manually duplicated Rust list.
- [ ] Generated and planned fixtures are deliberately exempted and cannot be
      confused with missing seed fixtures.
- [ ] GS-A001 fixture scope is unambiguous for future golden-output assertions.
- [ ] GS-P004 has either exact audited seed filenames or an explicit
      generated/planned classification.

## Validation plan

- Run `cargo test -p agent-cli grep_sed`.
- Inspect the scaffold to confirm the markdown parser checks both directions:
  canonical markdown rows are audited, and fixture inventory entries are either
  present on disk or explicitly exempted.
- If practical, perform a local negative check while developing: add a temporary
  markdown row or fixture inventory entry and confirm the audit fails before
  reverting that temporary change.

## Dependencies

- **T002:** Provides the initial conformance matrix, fixture inventory, seed
  fixtures, and scaffold tests.

## Provides to downstream tasks

- **T003-T008:** A less drift-prone conformance index and fixture scaffold for
  implementation tasks to extend.
