# T010 - Graph-aware prompt-hook retrieval

Add bounded knowledge context to `user-prompt-submit` after existing task and
pattern context.

## Requirements

- Query local and authorized gateway sources using the unified retriever.
- Inject compact segments with identity, origin, trust/lifecycle warnings, and
  commands for full reads rather than whole documents.
- Bound time, tokens, results, segment size, and graph expansion.
- Fail open when indexes or gateways are unavailable.
- Never mutate, publish, fetch arbitrary URLs, or execute OKF computations.

## Validation

Golden tests cover ranking, source labels, lifecycle/trust handling, token
budgets, deterministic output, unavailable stores, gateway timeouts, and hostile data.
