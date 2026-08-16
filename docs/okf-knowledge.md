# OKF And Unified Knowledge Workflows

Agent-tools stores files, symbols, Markdown documents, Open Knowledge Format
(OKF) concepts, and their relationships in one project knowledge graph. The
database is the canonical store; OKF is the portable rendering and interchange
format, not a second database.

## Knowledge synthesized from code

`agent-tools index` is the primary producer. After indexing symbols it
synthesizes concepts directly into `project.db`:

- one `CodeModule` concept per source file, and
- one `CodeSymbol` concept per *exported* symbol (visibility is read from the
  declaration per language; test scaffolding is excluded).

Identities are virtual bundle paths — `code/<path>.md` for a file and
`code/<path>/<symbol>-<digest>.md` for a symbol — so concepts are portable
without existing on disk. Containment links resolve inside each file's own
concept set; call and import relationships travel as portable edges under the
namespaced `x-agent-tools` frontmatter key.

```bash
agent-tools index
agent-tools search dispatch --type knowledge
agent-tools read okf://<project>/okf-synth/code/src/lib.rs.md
agent-tools doc outline okf://<project>/okf-synth/code/src/lib.rs.md
```

Synthesis is content-hash gated by its own producer (`okf-synth/1`), so an
existing index picks it up without a rebuild, unchanged files cost nothing, and
deleting a file prunes only that file's concepts. Concepts are recorded as
`local-derived` origin with `derived` authority: retrieval ranks them below
repository- and gateway-authored knowledge, and they never substitute for
reading the source.

Nothing is written to the working tree. To materialize the stored bundle as
files — for review, diffing, or sharing — export it explicitly:

```bash
agent-tools okf export --destination /tmp/synth --with-index
```

## Knowledge from tool use

Normal operation feeds the graph. Two grains, deliberately different:

**Access signals.** `read`, `symbol`, `symbols`, `grep`, `get`, and `graph`
record that they touched a resource. A read is evidence of interest, not
knowledge, so it produces no concept — one row per (resource, tool) keeps
`resource_access` bounded by the resource count however heavily the tools are
used. `grep` records only the files that actually matched, deduplicated, so a
repository-wide sweep does not flatten the signal. Counts are reported by
`agent-tools get`.

**Observations.** Completing a task *is* knowledge, so `agent-tools tasks done`
writes a draft `Observation` concept linked to the resources the session
touched (the twelve-hour window, top eight by use). Those links resolve into the
synthesized code concepts, which is what connects work to code:

```bash
agent-tools graph "okf://<project>/okf-observe/observations/<task>.md" --direction out
```

Observations are `draft` status and `derived` authority — an unreviewed trace of
what an agent did, never something the repository asserts — and are capped at the
200 most recent, pruned within their own origin so authored and synthesized
concepts are untouched.

All of this is best-effort bookkeeping alongside the command you actually ran: it
happens after that command produces its output and a failure is dropped rather
than surfaced. Set `AGENT_TOOLS_OBSERVE=off` to disable recording entirely.

## Author a repository bundle

The conventional bundle root is `.agents/knowledge/`. Each concept is a UTF-8
Markdown file with YAML frontmatter and a required `type`:

```markdown
---
okf_version: "0.2"
type: Runbook
title: Checkout Recovery
status: stable
stale_after: 2030-01-01
tags: [checkout, sre]
verified:
  human: reviewer@example.com
  at: 2026-08-15T01:00:00Z
---

# Checkout Recovery

See the [service](service.md).
```

Concept identity is its bundle-relative Markdown path. Links beginning with `/`
are bundle-root-relative, never host-root-relative. Unknown types and fields are
retained. Broken links are warnings and remain queryable as unresolved edges;
missing/empty `type`, invalid YAML, traversal paths, and bundle escapes are
fatal validation errors.

`index.md` and `log.md` are optional. A missing index does not invalidate an
explicitly selected bundle.

## Validate and index

```bash
# Read-only validation; invalid required structures exit with status 2
agent-tools okf validate .agents/knowledge

# Index files, symbols, code relationships, and the conventional OKF bundle
agent-tools index

# Import a different bundle inside the repository
agent-tools okf import docs/operations-knowledge
```

Indexing is incremental and content-hash based. Unchanged resources reuse their
immutable versions. Edits replace only the current producer-owned segments and
edges; removals prune that repository bundle's projection without touching
other origins.

## Search and traverse

Existing search modes remain compatible:

```bash
agent-tools search Worker --type symbol
agent-tools search service --type file
```

Knowledge and combined modes use the shared index:

```bash
agent-tools search checkout --type knowledge
agent-tools search checkout --type knowledge --namespace okf --kind Service
agent-tools search recovery --type knowledge --status draft --relation links_to
agent-tools search worker --type all --file src --language Rust
```

Use a canonical URI whenever a title or symbol name is ambiguous:

```bash
agent-tools get "Checkout Service"
agent-tools graph "okf://project/.agents/knowledge/services/service.md" \
  --relation links_to --direction both --depth 2 --limit 20
agent-tools refs process_checkout
agent-tools imports src/main.rs
agent-tools impls CheckoutService
```

`get` reports the current revision/hash, source format, origin, authority,
status, staleness, tags, provenance count, verification count, and direct
relationships. Graph traversal is breadth-first, cycle-safe, depth-bounded,
result-bounded, and deterministic. A `?` destination is an unresolved spelling,
not a guessed target.

Equivalent MCP reads are available as `search_knowledge`, `get_knowledge`,
`knowledge_graph`, and `knowledge_refs`.

## Export and publish

```bash
# Deterministic normalized OKF export; re-import is graph-equivalent
agent-tools okf export .agents/knowledge --destination /tmp/knowledge-export

# Inspect the one-way gateway projection without gateway access or side effects
agent-tools okf publish .agents/knowledge --dry-run

# Publish create/update operations to default-gateway Documentation
agent-tools okf publish .agents/knowledge
```

Publishing uses the OKF canonical identity as `source_ref` and the content hash
as `version`. An identical accepted projection is reused; a changed hash is an
update. The structured content retains raw frontmatter, normalized metadata,
Markdown body, links, path identity, and provenance. Publishing is one-way:
gateway edits are not synchronized back into repository files.

## Default and project upstream gateways

The default gateway remains the target for creates. Read operations also query
eligible repository-configured upstreams in `.agents/alternate-gateways.yml`:

```yaml
version: 1
gateways:
  - profile: prod-sre
    url: https://prod-gateway.example.com
    read: [tasks, patterns, docs]
```

Configure credentials outside Git:

```bash
agent-tools setup gateway --add-upstream prod-sre
agent-tools setup gateway --list
```

Federated reads preserve the gateway/origin and authority, deduplicate exact
canonical identities, and group equal hashes without erasing distinct
authoritative records. Unauthorized results are never reintroduced locally.
If an additional gateway times out, successful local/default results remain and
the failure is labelled; a default-gateway failure remains an error for
gateway-only commands.

## Authority, lifecycle, and prompt hooks

- Git files and repository OKF are authoritative at the repository.
- Gateway Documentation is authoritative at its source gateway.
- Tree-sitter symbols, segments, resolved edges, and the local SQLite database
  are derived and rebuildable.
- `draft`, `deprecated`, and `stale_after` are lifecycle signals, not access
  controls.
- Verification and provenance are trust evidence, not authorization.

Prompt hooks add only compact, bounded excerpts after relevant patterns and
tasks. Every excerpt labels source, authority, lifecycle, and trust and points
to `agent-tools get` or `agent-tools docs get` for a deeper read. Gateway work is
time-bounded and fail-open; local knowledge still works without a gateway. Set
`AGENT_TOOLS_HOOK=off` to disable all hook injection.

## Migration, rebuild, and recovery

Legacy `files.db` and `symbols.db` data migrates into the shared `project.db`
without modifying the source databases. Current versions are immutable and
producer replacement is transactionally scoped.

If derived state is damaged or stale, rebuild it from authoritative sources:

```bash
agent-tools index --rebuild
agent-tools okf import path/to/explicit-bundle
```

`--rebuild` removes only the project's derived local index directory before
re-indexing. It does not change repository files or gateway records. Keep
repository content and gateway backups as the recovery sources; do not treat
`project.db` as the only copy of authored knowledge.

## Security and deferred capabilities

Parsing and indexing never fetch external URLs, execute HTML/scripts, or run
OKF Attested Computation executors or attesters. Attested Computation metadata
and bodies are retained as inert knowledge. File/frontmatter/link/result/depth
limits and symlink/path checks bound hostile inputs.

Two-way gateway synchronization and Attested Computation execution are not
implemented. A future feature must define separate authorization, sandboxing,
attestation, and conflict-resolution contracts before enabling either.
