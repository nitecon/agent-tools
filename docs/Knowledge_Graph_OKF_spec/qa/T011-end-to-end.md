# T011 - End-to-end closure

Execute and close the T001 matrix across migration, indexing, graph extraction,
OKF, retrieval, gateway federation, prompt hooks, security, and platforms.

## Requirements

- Every required source-spec scenario has an automated assertion or exact
  checked-in platform closure evidence.
- Security fixtures demonstrate bounded resource use and absence of unsafe
  path access, URL fetch, HTML execution, SQL injection, or computation execution.
- Migration tests start from real previous-schema fixtures.
- No network is required except tests explicitly using a local mock server.

## Validation

Run formatting, clippy with warnings denied, all workspace tests, workspace
build, the conformance audit, and the supported CI platform matrix. Record exact
commands and outcomes in the conformance document.
