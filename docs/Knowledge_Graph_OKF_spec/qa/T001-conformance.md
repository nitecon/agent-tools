# T001 - Unified graph and OKF conformance matrix

Define the executable scenario inventory before schema and command implementation.

## Deliverables

- `docs/knowledge-graph-okf-conformance.md` with stable scenario IDs.
- Multi-language, Markdown, OKF 0.2/0.1, hostile-input, migration, and simulated-gateway fixtures.
- A Rust integration-test scaffold that audits the matrix and fixture inventory.

## Contract

Each scenario records argv/API operation, fixture, expected database/graph state,
stdout/stderr or structured response, status, comparison mode, and platform
coverage. The source specification's twelve end-to-end scenarios must each map
to at least one row. Deferred rows require an explicit rationale and do not
count toward the final definition of done when they cover required behavior.

## Validation

Run `cargo test -p agent-cli knowledge_graph_conformance` and verify the scaffold
fails when a required scenario, fixture, or expected result is removed.
