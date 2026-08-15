# T008 - Unified CLI and MCP retrieval

Expose full-text retrieval and typed graph traversal over all resource kinds.

## Requirements

- Extend search with knowledge/all modes and resource filters while preserving
  current file/symbol behavior.
- Add get, graph, refs/backlink, and bounded expansion operations.
- Return canonical URI, origin, authority, current version/hash, lifecycle,
  trust, spans, and unresolved relationships.
- Rank text first and graph context second with deterministic tie-breaking.
- Provide equivalent read-only MCP tools and schemas.

## Validation

CLI and MCP fixture tests assert equivalent resources, filters, bounds,
ordering, lifecycle defaults, graph cycles, and error behavior.
