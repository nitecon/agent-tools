# T009 - Gateway projection and federation

Project OKF concepts into gateway Documentation and merge authorized knowledge
from local, default, and repository-configured additional gateways.

## Requirements

- Publishing is one-way, hash-based, idempotent, provenance-preserving, and has
  a side-effect-free dry run.
- Preserve OKF identity in `source_ref` and structured lossless content.
- Keep gateway IDs, scope, authority, artifact versions, and origin visible.
- Deduplicate canonical identities; group equal hashes without erasing authority.
- Return labelled partial results when one gateway fails or times out.
- Do not implement two-way synchronization in this milestone.

## Validation

Mocked gateway tests cover create/reuse/update, dry-run, default plus alternates,
authorization filtering, duplicates, equal content under distinct authority,
timeouts, and partial failures.
