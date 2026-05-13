# Grep/Sed T008 Bulk Workflow Decision

## Decision

T008 does not add a new optional bulk workflow flag for v1. The current
T001/T002 contract and conformance matrix keep the optional bulk surfaces
deferred unless a later contract row promotes them:

- `--files0-from` remains a deferred null-delimited input-list channel with the
  stable diagnostic `error: unsupported: null-delimited input lists are deferred`.
- Null-delimited full match records remain deferred; only path-family
  `--null` output is accepted by the v1 contract.
- Optional preview/write manifests remain deferred. Normal `sed --write`
  remains protected by stable replacement record IDs, content hashes, and
  preflight drift checks, without requiring a reviewed manifest.
- Backup/recovery affordances remain out of the v1 default path. Backup files
  are not created by default.

## Default Behavior

No command implementation changes accompany this decision. Default
`agent-tools grep`, `agent-tools sed` preview, and `agent-tools sed --write`
behavior remains governed by `docs/grep-sed-contract.md` and
`docs/grep-sed-conformance.md`.

The proof for this task is the unchanged grep/sed conformance validation:

- `cargo test -p agent-cli grep_sed`
- `cargo test -p agent-cli grep`

Because this decision is documentation-only, a full workspace build is not part
of the required validation path.

## T009 Documentation Guidance

T009 should document that v1 has no additional optional bulk workflow surface
beyond the accepted core grep/sed flags already present in the contract. It
should mention the deferred diagnostics for null-delimited input lists and stdin
payload modes, and it should avoid publishing manifest, replay, backup, or
recovery workflows as user-facing features until those modes receive contract
rows and tests.
