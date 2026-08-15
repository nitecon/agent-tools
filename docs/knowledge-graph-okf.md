# Unified Project Knowledge Graph And OKF Integration

## Status

- Specification version: 1
- Target: `agent-tools` and its gateway-backed knowledge workflows
- Baseline revision: `7201491`
- OKF compatibility target: Google Open Knowledge Format 0.2

## Summary

Agent-tools will evolve its separate file, symbol, content, documentation, and
planned relationship indexes into one logical project knowledge graph. Google
Open Knowledge Format (OKF) is the portable authoring, interchange, and
federation contract for knowledge concepts. It is not a second database and it
does not constrain the internal graph to OKF's untyped Markdown links.

The internal model is a semantic superset of OKF:

- files, symbols, documents, OKF concepts, tasks, patterns, artifacts, actors,
  and external resources have stable resource identities;
- mutable or generated content is represented by immutable resource versions;
- searchable text is segmented once and reused by full-text and optional
  semantic retrieval;
- typed, provenance-bearing edges connect every resource kind;
- Tree-sitter, Markdown, gateway records, and OKF bundles are producers for the
  same graph;
- OKF import and export preserve path identities, raw frontmatter, unknown
  fields, Markdown bodies, links, trust, lifecycle, and provenance;
- local, default-gateway, and additional-upstream-gateway results are merged
  without losing origin or authority.

## Goals

1. Eliminate duplicated file identity and parallel relationship stores.
2. Reuse the existing database content, full-text, hashing, and incremental
   indexing capabilities rather than creating an OKF-specific database.
3. Make Tree-sitter code relationships and Markdown/OKF relationships
   queryable through one typed graph.
4. Provide lossless OKF 0.2 import, validation, indexing, and export.
5. Allow repository knowledge to coexist with default and additional upstream
   gateways, with deterministic federation and source attribution.
6. Expose the graph through CLI, MCP, and bounded prompt-hook context.
7. Preserve current command behavior and provide a rebuildable migration path.
8. Prove the complete workflow with deterministic, cross-platform end-to-end
   fixtures and tests.

## Non-goals

- Executing OKF Attested Computation programs, executors, or attesters.
- Treating trust metadata as authorization or access control.
- Replacing gateway scope, governance, artifact versioning, or acceptance.
- Requiring every symbol or derived resource to be materialized as a Markdown
  file in the repository.
- Flattening all domain-specific properties into one weakly typed table.
- Adding a graph server, graph database, or mandatory external runtime.
- Fetching external `resource` or source URLs during parse or index operations.

## Design Principles

### One logical graph, typed extensions

All graph participants share resource identity, versions, content segments,
edges, provenance, tags, and verification. Domain tables retain properties
that do not belong in the common model. A symbol remains a typed symbol record;
it also owns a graph resource identity.

### Authority is explicit

The database contains both authored and derived state. Each resource records
its origin and authority:

- repository-authored OKF or Markdown is authoritative in Git;
- source files are authoritative for Tree-sitter-derived symbols and edges;
- gateway records are authoritative for gateway-owned documentation, tasks,
  patterns, and artifacts;
- index rows, chunks, resolved edges, and ranking features are rebuildable.

No synchronization path may silently overwrite an authoritative source.

### Rich internally, lossless at the boundary

Agent-tools retains typed edges and specialized metadata that OKF does not
standardize. OKF export represents portable relationships with Markdown links
and places additional lossless data under a namespaced `x-agent-tools` mapping.
Unknown OKF fields are retained verbatim and emitted again.

### Versioned and migratable

The project index uses explicit schema migrations. Rebuildable local indexes
may be regenerated, but migration tests must also cover preserving existing
records because gateway and future locally-authored state may not be
reconstructible from source files alone.

## Logical Data Model

Names are normative; exact SQL types may vary between SQLite and the gateway
database so long as behavior and uniqueness constraints remain equivalent.

### `resources`

```text
id                    internal stable identifier
project_id            project ownership/scope
namespace             file | symbol | okf | docs | task | pattern | artifact | actor | external
external_id           identity within namespace and origin
canonical_uri         globally comparable normalized identity
kind                  domain type, including arbitrary OKF type values
title                 display/search title
description           optional summary
origin_kind           repository | local-derived | gateway | external
origin_id             bundle, gateway, repository, or producer identity
authority             repository | gateway | derived
status                draft | stable | deprecated; stable when absent for OKF
stale_after            optional UTC date
current_version_id    current immutable version
created_at
updated_at
metadata_json         normalized extensions not promoted to columns
```

Required uniqueness is `(project_id, canonical_uri)`. Federation may return
multiple resources with equivalent content but different authorities; those
remain distinct and are grouped by aliases or hashes rather than overwritten.

Canonical URI families:

```text
repo://<project>/<normalized-relative-path>
symbol://<project>/<path>#<stable-symbol-key>
okf://<origin>/<bundle>/<concept-id>
gateway://<gateway-id>/<artifact-or-record-id>
task://<gateway-id>/<task-id>
pattern://<gateway-id>/<pattern-id>
artifact://<gateway-id>/<artifact-id>
actor:<producer-or-human-id>
```

Paths use forward slashes. Repository and OKF path normalization rejects host
absolute paths, `..` escapes, NULs, and symlink escapes. An OKF `/path` link is
bundle-root-relative, never host-root-relative.

### `resource_versions`

```text
id
resource_id
revision              monotonic per resource or immutable upstream version
source_format         source | markdown | okf/0.1 | okf/0.2 | gateway-json | generated
media_type
body                   full normalized textual body when applicable
raw_metadata           lossless serialized frontmatter/upstream metadata
content_hash           BLAKE3 over canonical version inputs
generated_by
generated_at
indexed_at
```

`UNIQUE(resource_id, content_hash)` prevents duplicate versions. Updating a
resource creates or selects an immutable version and atomically updates
`current_version_id`.

### `content_segments`

```text
id
resource_version_id
ordinal
heading_path
text
start_line / end_line
start_byte / end_byte
token_count
content_hash
metadata_json
```

The existing full-text/chunk pipeline is adapted to own segments through
resource versions. SQLite FTS indexes segment title, heading path, text, tags,
and resource description. Embeddings, when configured, attach to segment IDs
without changing identity.

### `edges`

```text
id
src_resource_id
dst_resource_id        nullable until resolved
dst_ref                original unresolved reference
relation               typed relation
confidence             extracted | resolved | inferred | ambiguous
extractor               producer and version
source_resource_id      file/document that asserted the edge
source_version_id
start_line / end_line
start_byte / end_byte
content_hash
metadata_json
```

Core relations initially include:

```text
contains, defines, calls, imports, inherits, implements,
links_to, derived_from, cites, documents, applies_to,
generated_by, verified_by, supersedes, aliases
```

Relation values are extensible. Incremental re-indexing replaces edges owned
by one producer/source-version transactionally and never deletes edges owned
by other producers.

### Specialized tables

- `files(resource_id, path, language, size, mtime, content_hash, ...)`
- `symbols(resource_id, file_resource_id, name, symbol_kind, parent_resource_id,
  stable_key, source spans, ...)`
- `tags(resource_id, tag)`
- `provenance(resource_version_id, source_resource_id, source_ref, author,
  usage_count, last_modified, metadata_json)`
- `verifications(resource_version_id, actor_resource_id, verified_at,
  verification_kind, metadata_json)`
- `resource_aliases(resource_id, alias_uri, reason)`

The current file and symbol schemas migrate into these tables. Compatibility
query methods remain available while callers move to graph queries.

## Producers And Extraction

### File traversal

The existing ignore-aware project traversal discovers files once. File
metadata and hashes feed the shared resource table. A producer registry selects
additional extraction based on language, extension, configured bundle roots,
and gateway origin.

### Tree-sitter

Tree-sitter produces file and symbol resources plus `contains`, `defines`,
`calls`, `imports`, `inherits`, and `implements` edges. Relationship queries are
versioned per language. Cross-file resolution runs as a second pass and retains
the unresolved spelling even when a target resolves.

Resolution must be deterministic and report ambiguous candidates rather than
choosing silently. Supported languages remain C/C++, Rust, Python,
TypeScript/JavaScript, C#, and Go unless separately extended.

### Markdown

Markdown extraction produces content segments from headings and source spans,
plus `links_to` and `cites` edges. It is fence-aware and does not interpret links
inside code fences as graph edges. External links become external resources but
are not fetched.

### Gateway records

Documentation, tasks, patterns, and artifacts are normalized into resources,
versions, segments, and typed edges while retaining gateway IDs, scope,
artifact versions, wiki paths, ranks, and source gateway. Gateway access
controls remain enforced at retrieval time and are never inferred from OKF
trust metadata.

## OKF Compatibility

### Bundle discovery

The conventional repository bundle root is `.agents/knowledge/`. Commands also
accept explicit bundle paths. Optional future configuration may declare
multiple roots; absence of `index.md` cannot make a valid bundle undiscoverable
when its path is explicitly configured.

### Import mapping

| OKF element | Internal model |
| --- | --- |
| bundle-relative path without `.md` | concept `external_id` and canonical URI |
| `type` | resource `kind` |
| `title` | resource title |
| `description` | resource description |
| `tags` | tags |
| Markdown body | resource version body and segments |
| Markdown links | `links_to` edges |
| `sources` and footnote joins | provenance and `cites`/`derived_from` edges |
| `generated` | version generation fields and `generated_by` edge |
| `verified` | verification rows and `verified_by` edges |
| `status` | resource status |
| `stale_after` | resource staleness date |
| unknown fields | lossless raw metadata plus normalized extension metadata |
| `index.md` | hierarchy/contains hints and optional declared OKF version |
| `log.md` | optional history resource; never required for validity |

OKF 0.2 conformance is permissive: unknown types, extra fields, missing
optional fields, broken links, and missing index files are nonfatal. Missing or
empty `type`, invalid required field shapes, unsafe paths, or invalid UTF-8 are
errors. Broken and ambiguous links generate diagnostics and unresolved edges.

The importer normalizes a bare `verified` mapping to a list. It recognizes OKF
0.1 `timestamp` and body `# Citations` as compatibility fallbacks with warnings.

### Export mapping

Export creates deterministic UTF-8 Markdown and YAML frontmatter. A previously
imported concept that has not semantically changed must round-trip without
losing unknown metadata. Generated resources include `generated.by` and
`generated.at`. Typed edges beyond portable Markdown links are serialized under
`x-agent-tools.edges`; consumers that ignore extensions still receive a valid
OKF graph.

File moves change native OKF path IDs. Agent-tools may record an `aliases` or
`supersedes` relation, but must not pretend the old and new OKF IDs are equal.

### Attested Computation

Agent-tools validates the declared contract and retains computation metadata.
It never executes the computation, executor, or attester in this milestone.

## Retrieval And Ranking

Unified retrieval runs in two stages:

1. lexical/full-text retrieval over content segments and resource metadata;
2. bounded graph expansion over relevant typed edges.

Filters include namespace, kind, relation, origin, gateway, status, trust tier,
path, and language. Ranking starts with textual relevance and may then adjust
for graph distance, accepted gateway versions, trust, lifecycle, and freshness.

- deprecated resources are excluded by default unless explicitly requested;
- draft, stale, or unverified resources remain usable but are labelled and
  down-ranked;
- trust is advisory and never bypasses gateway authorization;
- every result carries canonical URI, origin, authority, version/hash, and a
  retrieval command that can fetch the full record.

## CLI And MCP Contract

The exact argument grammar is finalized by implementation tasks, but the
required capabilities are:

```bash
agent-tools index [PATH] [--rebuild]
agent-tools search QUERY --type file|symbol|knowledge|all
agent-tools graph <RESOURCE> [--relation RELATION] [--direction in|out|both]
agent-tools refs <SYMBOL>
agent-tools knowledge get <RESOURCE>
agent-tools okf validate [BUNDLE]
agent-tools okf import [BUNDLE]
agent-tools okf export [TARGET] [--dry-run]
agent-tools okf publish [BUNDLE] [--gateway NAME] [--dry-run]
```

MCP exposes equivalent read operations for search, get, graph traversal,
backlinks/references, OKF validation, and origin metadata. Mutating publish or
export operations require explicit CLI/user action and are not performed by
prompt hooks.

Existing `search --type symbol|file`, `symbols`, `symbol`, and `index` behavior
remains compatible unless a separately documented versioned change is accepted.

## Gateway Projection And Federation

An OKF concept projects to gateway Documentation without replacing its OKF ID:

```text
type                -> kind
title               -> title
description         -> summary
tags                -> labels
concept ID/path     -> source_ref
OKF version         -> source_format (okf/0.2)
body/frontmatter    -> structured content.okf
resolved links      -> linked artifact/resource identities
```

Publishing is initially one-way, hash-based, idempotent, and supports
`--dry-run`. Two-way synchronization is deferred until rename, deletion,
tombstone, and conflict semantics are specified.

Federated reads query the local graph, configured default gateway, and
repository-configured additional upstream gateways. They preserve origin and
authorization boundaries. Exact canonical URI matches deduplicate; matching
content hashes group equivalent results but do not erase distinct authority.
Partial gateway failure returns labelled partial results and does not hide
successful local or gateway results.

## Prompt-Hook Integration

`user-prompt-submit` adds bounded knowledge retrieval after existing task and
pattern context. It queries local and authorized gateway sources, injects
compact segments rather than full documents, and includes origin, lifecycle,
trust, and commands for deeper reads. A failure or unavailable gateway must not
block the user prompt.

Ranking and token budgets are deterministic and covered by tests. Hooks never
publish, export, fetch arbitrary external URLs, or execute computation metadata.

## Migration And Compatibility

1. Introduce schema versioning and the common resource tables.
2. Migrate or rebuild the existing `files.db` and `symbols.db` content into a
   project index while maintaining old query APIs.
3. Dual-read or compatibility-test existing commands during transition.
4. Add generic edges before landing symbol-only relationship persistence.
5. Adapt existing full-text/chunk storage to resource versions rather than
   copying content into an OKF-specific index.
6. Make migration transactional and restartable. Preserve the old databases
   until the new index commits successfully; derived indexes can then be
   archived or rebuilt according to documented policy.

Database migration and rebuild output must be deterministic. Concurrent readers
remain supported under WAL; indexing uses a controlled writer transaction and
the existing bounded busy-timeout behavior.

## Security And Resource Limits

- Treat repository bundles, gateway data, YAML, Markdown, and extension metadata
  as untrusted input.
- Bound file size, YAML nesting/aliases, link count, segment count, graph
  expansion depth, query results, and hook token output.
- Reject traversal, NUL, unsafe archive, and symlink-escape paths.
- Never fetch resource URLs as a parsing side effect.
- Sanitize raw HTML in any rendered Markdown view.
- Parameterize all SQL and validate FTS query construction.
- Preserve gateway authorization and source identity throughout federation.
- Do not execute OKF computation or attestation code.

## Observability And Diagnostics

Index and import operations report files/resources seen, versions created or
reused, segments indexed, edges extracted/resolved/unresolved/ambiguous,
diagnostics, gateway partial failures, and elapsed time. Machine-readable output
uses stable fields. Human output remains compact and identifies repair commands.

## End-To-End Conformance Scenarios

The implementation is not complete until automated tests cover:

1. **Fresh project indexing:** code and Markdown become resources, versions,
   segments, symbols, and typed edges in one graph.
2. **Incremental update:** changing one file replaces only producer-owned rows,
   retains unrelated edges, and leaves no duplicate versions or segments.
3. **Cross-file code graph:** calls/imports/inheritance resolve for supported
   fixture languages; unresolved and ambiguous references remain diagnosable.
4. **OKF 0.2 import:** valid bundles, unknown types/fields, missing indexes, and
   broken links behave according to the spec.
5. **OKF compatibility:** 0.1 timestamp/citation fallbacks warn and normalize.
6. **Round trip:** import/export/re-import preserves identity, body, unknown
   metadata, provenance, lifecycle, verification, and graph equivalence.
7. **Existing CLI compatibility:** current file and symbol searches return
   equivalent results after migration.
8. **Unified retrieval:** one query returns ranked code, knowledge, and gateway
   resources with typed neighboring context.
9. **Federation:** default and additional gateways merge deterministically,
   preserve authority, deduplicate correctly, and tolerate one gateway failure.
10. **Prompt hook:** bounded context includes relevant local and remote knowledge
    without blocking on failure or exceeding its token budget.
11. **Security:** traversal, symlink escape, oversized inputs, hostile YAML,
    external URLs, and computation metadata are handled without execution or
    unsafe access.
12. **Cross-platform:** supported tests pass on Linux, macOS, and Windows with
    normalized path identities and deterministic output.

## Definition Of Done

- The accepted manifest tasks are complete and linked to their implementation.
- Schema migrations, compatibility behavior, and rebuild paths are documented.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, and `cargo build --workspace` pass.
- OKF conformance and round-trip fixtures pass without network access.
- End-to-end CLI, MCP, hook, and gateway-federation tests pass.
- User documentation describes authoring, indexing, querying, publishing,
  diagnostics, trust/lifecycle labels, and recovery.
- No separate OKF datastore or duplicate full-text corpus has been introduced.
