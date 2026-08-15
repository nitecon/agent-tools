# Unified Knowledge Graph And OKF Conformance Matrix

This is the canonical executable scenario inventory for the unified project
knowledge graph and Google Open Knowledge Format integration. Implementations
must consume these row IDs or amend this matrix before adding behavior.

Source contract: `docs/knowledge-graph-okf.md`.

All KG-A001 through KG-A011 scenarios are automated by the targets below.
KG-A012 is automated by the checked-in `knowledge-graph-linux`,
`knowledge-graph-macos`, and `knowledge-graph-windows` CI matrix jobs.

## Status And Comparison Modes

| Value | Meaning |
| --- | --- |
| `scaffold` | Metadata and fixtures are audited; behavioral assertions land in the owning task. |
| `automated` | The complete behavior is asserted by the named workspace test. |
| `platform` | The behavior runs in the Linux, macOS, and Windows CI matrix. |
| `byte-exact` | Output and serialized bytes must match exactly. |
| `graph-exact` | Canonical resources, versions, segments, edges, and metadata must match independent of internal row IDs. |
| `normalized` | Host separators, timestamps, ports, and internal IDs may be normalized; identities, ordering, labels, and statuses remain exact. |

## Required Scenario Matrix

Every row specifies an operation, checked-in fixture root, process/result status,
database or graph expectation, visible output/response contract, comparison
mode, and platform closure. `scaffold` rows are promoted to `automated` by their
owning implementation task; all rows must be automated or platform-closed for
T011 and the specification definition of done.

| Row ID | Owner | Operation | Fixture | Expected status | Database / graph expectation | Output / response expectation | Compare | Platform |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| `KG-A001` | T002, T004, T007 | Fresh `agent-tools index` | `knowledge_graph/code/`, `knowledge_graph/markdown/`, `knowledge_graph/okf/v02/` | 0 | One canonical resource per file/concept/symbol; immutable current versions, shared segments, and typed producer-owned edges | Stable counts for resources, versions, segments, edges, resolved, unresolved, and diagnostics | graph-exact | all |
| `KG-A002` | T002, T004, T007 | Index, edit one source, re-index, then delete it | `knowledge_graph/code/rust/lib.rs`, `knowledge_graph/markdown/architecture.md` | 0 | Only changed producer output is replaced; unrelated edges remain; no duplicate versions/segments; deletion removes derived ownership safely | First, no-op, changed, and deleted summaries distinguish reused and replaced records | graph-exact | all |
| `KG-A003` | T003, T004, T005 | Extract and resolve code relationships | `knowledge_graph/code/` | 0 | Calls/imports/inherits/implements resolve where deterministic and retain unresolved/ambiguous spellings and source spans | Callers, callees, imports, implementors, and bounded graph traversal are deterministically ordered | graph-exact | all |
| `KG-A004` | T006, T007 | Validate and import OKF 0.2 bundle | `knowledge_graph/okf/v02/` | 0 | Path IDs, arbitrary type/fields, body, links, sources, verification, generation, lifecycle, hierarchy, and broken links normalize losslessly | Broken links and missing optional fields are warnings, not fatal errors | graph-exact | all |
| `KG-A005` | T006 | Validate and import OKF 0.1 compatibility concept | `knowledge_graph/okf/v01/legacy.md` | 0 | Legacy timestamp and Citations normalize to generation/provenance without losing raw input | Stable compatibility warnings identify both fallbacks | graph-exact | all |
| `KG-A006` | T006 | Import, deterministic export, and re-import | `knowledge_graph/okf/v02/` | 0 | Re-imported canonical graph equals the imported graph and preserves unknown metadata, provenance, trust, lifecycle, links, and body | Second export is byte-identical after normalized generation fields | byte-exact | all |
| `KG-A007` | T002, T004, T008 | Migrate previous indexes and run existing file/symbol commands | `knowledge_graph/migration/`, `knowledge_graph/code/` | 0 | Migrated/rebuilt resources are unique and old file/symbol results are equivalent | Existing file search, symbol search, `symbols`, and `symbol` output remains compatible | normalized | all |
| `KG-A008` | T008 | Unified lexical search plus bounded graph expansion | `knowledge_graph/code/`, `knowledge_graph/markdown/`, `knowledge_graph/okf/v02/` | 0 | One query returns ranked code and knowledge resources with typed neighbors and filters | Results include canonical URI, kind, origin, authority, version/hash, lifecycle/trust, and full-read command | normalized | all |
| `KG-A009` | T009 | Query local, default, additional, and failing gateway sources | `knowledge_graph/gateways/` | 0 | Exact identities deduplicate; equal hashes group distinct authorities; unauthorized entries are absent; successful sources survive one failure | Results label every origin and include a stable partial-failure diagnostic | normalized | all |
| `KG-A010` | T010 | Submit a prompt matching local and federated knowledge | `knowledge_graph/gateways/`, `knowledge_graph/okf/v02/services/runbook.md` | 0 | Hook selection is relevance-first with bounded graph context and no data mutation | Output is deterministic, token-bounded, source-labelled, lifecycle/trust-aware, and includes deeper-read commands | byte-exact | all |
| `KG-A011` | T006, T007, T009, T010, T011 | Parse/index/query hostile knowledge inputs | `knowledge_graph/hostile/` | 2 | No escaped path, URL fetch, HTML/script execution, SQL injection, YAML expansion, or computation execution occurs; limits are enforced | Stable diagnostics separate fatal invalid input from bounded warnings; hooks fail open | normalized | all |
| `KG-A012` | T011 | Run full conformance suite on Linux, macOS, and Windows | `knowledge_graph/` | 0 | Canonical URIs and graph state are equivalent across hosts | Deterministic output after documented path/timestamp/port normalization | normalized | platform |

## Fixture Inventory

All paths are relative to `crates/agent-cli/tests/fixtures/`.

| Fixture | Purpose | Rows |
| --- | --- | --- |
| `knowledge_graph/code/` | Seven-language definition and relationship corpus | KG-A001, KG-A003, KG-A007, KG-A008, KG-A012 |
| `knowledge_graph/code/rust/lib.rs` | Incremental source update seed | KG-A002 |
| `knowledge_graph/markdown/architecture.md` | Heading, code-fence, internal-link, external-link, and citation extraction | KG-A001, KG-A002, KG-A008 |
| `knowledge_graph/okf/v02/` | OKF 0.2 hierarchy, concepts, lifecycle, trust, sources, unknown fields, and broken links | KG-A001, KG-A004, KG-A006, KG-A008, KG-A010 |
| `knowledge_graph/okf/v01/legacy.md` | OKF 0.1 timestamp and Citations compatibility | KG-A005 |
| `knowledge_graph/migration/files-v0.sql` | Previous file-index schema/data seed | KG-A007 |
| `knowledge_graph/migration/symbols-v0.sql` | Previous symbol-index schema/data seed | KG-A007 |
| `knowledge_graph/gateways/default.json` | Default-gateway results and authority | KG-A009, KG-A010 |
| `knowledge_graph/gateways/additional.json` | Additional upstream duplicates and production-only knowledge | KG-A009, KG-A010 |
| `knowledge_graph/gateways/failure.json` | Deterministic simulated gateway failure | KG-A009, KG-A010 |
| `knowledge_graph/hostile/invalid-frontmatter.md` | Missing required type and unsafe references | KG-A011 |
| `knowledge_graph/hostile/attested-computation.md` | Retained but never executed computation contract | KG-A011 |
| `knowledge_graph/hostile/limits.md` | Limit-test seed expanded in a temporary test directory | KG-A011 |

## Executable Ownership

| Task | Required rows | Primary test target |
| --- | --- | --- |
| T001 | Matrix and fixture audit for KG-A001..KG-A012 | `cargo test -p agent-cli --test knowledge_graph_conformance` |
| T002 | KG-A001, KG-A002, KG-A007 | resource database unit and migration tests |
| T003-T005 | KG-A001, KG-A002, KG-A003, KG-A007 | `agent-symbols` unit/integration tests plus CLI graph cases |
| T006-T007 | KG-A001, KG-A002, KG-A004, KG-A005, KG-A006, KG-A011 | OKF/Markdown unit tests and conformance cases |
| T008 | KG-A007, KG-A008 | CLI and MCP retrieval tests |
| T009 | KG-A009 | mocked gateway integration tests |
| T010 | KG-A010 | prompt-hook golden tests |
| T011 | KG-A001..KG-A012 | full workspace and platform matrix |

## Platform Closure

CI targets are `knowledge-graph-linux`, `knowledge-graph-macos`, and
`knowledge-graph-windows`, each running:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Until all jobs exist, a platform row requires a checked-in validation entry
containing OS, architecture, filesystem, commit, commands, statuses, normalized
differences, and pass/fail result. Linux-only success does not close KG-A012.

## Validation Evidence

Local closure on 2026-08-15:

```text
OS: Linux 7.0.0-29-generic x86_64
Filesystem: repository working tree on a case-sensitive Linux filesystem
Baseline: 7201491 plus this specification's working-tree changes
Toolchain: rustc 1.96.1; cargo 1.96.1

cargo fmt --all --check                                      PASS
cargo clippy --workspace --all-targets -- -D warnings        PASS
cargo test --workspace                                       PASS
cargo build --workspace                                      PASS

Knowledge conformance integration tests: 9 passed
Agent CLI unit tests: 125 passed
Agent knowledge unit tests: 17 passed
Workspace failures: 0
Normalized differences: none observed
Result: PASS
```

Platform closure is executable rather than asserted from Linux evidence: the
CI matrix runs the same four commands on `ubuntu-latest`, `macos-latest`, and
`windows-2025-vs2026`. Canonical URI tests normalize separators to `/`; tests
use isolated state directories and do not depend on host timestamps or ports.
