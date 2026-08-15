# T005 - Cross-file resolution and code graph queries

Resolve extracted code relationships and expose deterministic graph operations.

## Requirements

- Use language-aware modules/imports and normalized symbol candidates.
- Never discard the original unresolved target.
- Represent multiple candidates as ambiguous instead of selecting silently.
- Provide callers, callees, imports, implementors, inbound/outbound neighbors,
  and bounded traversal through reusable APIs.
- Make graph depth and result limits explicit.

## Validation

Cover same-file and cross-file resolution, aliases, modules, duplicate names,
missing targets, cycles, ambiguity, deterministic ordering, and bounded walks.
