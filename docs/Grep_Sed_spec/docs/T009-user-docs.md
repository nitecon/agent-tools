# T009 - Publish user-facing grep/sed documentation

**Team:** docs
**Phase:** 4
**Depends on:** T005, T006, T007, T008
**Status:** todo

## Scope

**In:** Publish user-facing documentation and CLI help updates for the grep/sed feature.

**Out:** Changing command semantics. Documentation follows the implemented contract and conformance results.

## Source references

- `docs/Grep_Sed.md` lines 107-118 show grep examples and plain-text output expectations.
- `docs/Grep_Sed.md` lines 157-168 show sed examples and preview/write safety expectations.
- `docs/Grep_Sed.md` lines 319-331 define success from the user's perspective.

## Deliverables

1. **`docs/grep-sed.md`** user workflow page that introduces `agent-tools grep` and `agent-tools sed`.
2. README updates that point users to `docs/grep-sed.md`, `docs/grep-sed-contract.md`, and `docs/grep-sed-conformance.md` without duplicating detailed semantics.
3. CLI help text that points to detailed contract documentation where appropriate.
4. Examples for common search, preview, and write workflows across supported platforms.
5. Stable exit-code, output record, warning/skip label, traversal, regex, replacement, encoding, and write-safety documentation.

## Implementation notes

- Put argv-native sed examples first.
- Do not duplicate the whole contract in README; link to detailed docs to avoid drift.
- Include examples that are safe to run and demonstrate preview before write.

## Acceptance criteria

- [ ] User-facing docs describe grep and sed commands with argv-native examples first and sed-like expression examples only where their grammar is explicitly documented.
- [ ] Docs include stable exit-code tables, output record examples, skip/warning labels, traversal rules, regex and replacement dialects, binary/encoding behavior, and write-safety behavior.
- [ ] Examples cover Windows/macOS/Linux-safe commands and payloads containing leading dashes, delimiter-like text, dollars, backslashes, empty replacements, and newlines.
- [ ] README or CLI help points users to the detailed contract without duplicating stale semantics.

## Validation plan

- **Docs build/read check:** Review rendered Markdown for broken headings, stale links, and example formatting.
- **Example smoke tests:** Run safe grep and sed preview examples against fixtures.
- **Help check:** Run `agent-tools grep --help` and `agent-tools sed --help` once implemented and confirm key flags are discoverable.
- **Contract drift check:** Reconcile every exit-code table, output record example, warning/skip label, payload-channel claim, platform caveat, and write-safety statement against `docs/grep-sed-contract.md` and `docs/grep-sed-conformance.md`.

## Dependencies

- **T005:** Supplies grep behavior.
- **T006:** Supplies sed preview behavior.
- **T007:** Supplies sed write behavior.
- **T008:** Supplies optional workflow behavior or a documented no-op/deferral decision that determines whether optional workflow docs are published.

## Provides to downstream tasks

- User-facing release readiness for the portable grep/sed feature.
