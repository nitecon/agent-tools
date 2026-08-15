# T004 - Tree-sitter graph persistence

Adapt file and symbol indexing to the common resource database and persist
Tree-sitter relationships as generic typed edges.

## Requirements

- One canonical file identity is shared by traversal, symbols, content, and edges.
- Symbols have stable keys based on normalized path, semantic ancestry, kind,
  and source identity without pretending source lines are permanent identity.
- Edges retain unresolved references, producer/version, confidence, and spans.
- Re-indexing one changed file atomically replaces only producer-owned output.
- Existing `search --type file|symbol`, `symbols`, and `symbol` remain compatible.

## Validation

Test fresh build, no-op rebuild, update, deletion, rename, duplicate names,
rollback, stale-edge removal, and compatibility output.
