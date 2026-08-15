# T007 - Markdown and OKF knowledge indexing

Make Markdown and OKF producers populate shared resources, versions, segments,
metadata, provenance, verification, and edges.

## Requirements

- Reuse existing full-text storage and segmentation rather than copying bodies.
- Segment by fence-aware headings with stable source spans and hashes.
- Extract internal links, citations, and external resource identities without fetching.
- Make repository authority and derived index state explicit.
- Replace only producer-owned data on incremental updates.
- Bound file size, frontmatter, segments, links, and diagnostics.

## Validation

Cover fresh, unchanged, edited, moved, removed, malformed, oversized, fenced,
broken-link, citation, provenance, and lifecycle inputs.
