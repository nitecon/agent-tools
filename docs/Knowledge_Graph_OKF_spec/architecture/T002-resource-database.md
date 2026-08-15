# T002 - Versioned unified project resource database

Implement the common resource, immutable version, content segment, edge, tag,
provenance, verification, alias, file, and symbol schema from the source spec.

## Requirements

- Add explicit schema versions and transactional, restartable migrations.
- Normalize canonical URIs, authority, origin, and producer ownership centrally.
- Reuse the full-text/content pipeline; do not create an OKF database or corpus.
- Preserve specialized file and symbol query APIs during migration.
- Support one controlled writer with WAL readers and bounded busy timeouts.
- Store raw extension metadata losslessly while promoting query-critical fields.

## Validation

Test empty creation, migration from current `files.db` and `symbols.db`, restart
after an interrupted migration, uniqueness, immutable-version reuse, edge-owner
replacement, rollback, concurrent readers, and deterministic rebuilds.
