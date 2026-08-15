# T006 - Lossless OKF codec

Implement OKF 0.2 parsing, validation, normalization, and deterministic export
as a codec over the shared resource model.

## Requirements

- Preserve arbitrary types, unknown fields, raw frontmatter, body, path ID,
  links, sources, generated/verified data, status, and staleness.
- Treat missing `index.md` and broken links as nonfatal.
- Normalize bare verification mappings and support documented OKF 0.1 fallbacks.
- Resolve bundle-root links safely and reject traversal/symlink escapes.
- Retain but never execute Attested Computation metadata.
- Emit richer typed relationships under a namespaced extension when exporting.

## Validation

Use vendored, attributed upstream-compatible fixtures plus hostile and round-trip
fixtures. Tests run without network access and assert graph equivalence.
