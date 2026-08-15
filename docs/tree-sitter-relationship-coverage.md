# Tree-sitter Relationship Query Coverage

This document records the T003 extraction boundary for the unified project
knowledge graph. It is pinned to the Tree-sitter 0.24 Rust runtime and the 0.23
language grammar crates declared by `agent-symbols`.

## Graphify assessment

Graphify revision `7281f27eac568f77f50910f59f84543458f5dfd1` was inspected on
2026-08-15. The task's original assumption that Graphify stored portable
relationship `.scm` files is no longer true. Current Graphify uses Python AST
walkers, `LanguageConfig` node/field sets, and language-specific resolution
passes in `graphify/extract.py` and `graphify/extractors/`.

The reusable design observations are:

- calls are collected from language-specific call node types and attributed to
  the nearest function boundary;
- imports require language-specific normalization;
- inheritance/implementation frequently needs a second resolution pass;
- extracted spelling must survive resolution;
- uncertain classification is represented rather than silently guessed.

Agent-tools implements native compiled Tree-sitter queries against its pinned
grammars. No Python code is copied. Graphify's MIT-licensed repository informed
the node/field coverage and confidence model; retain this attribution when the
query implementation moves.

## Coverage

| Language | Calls | Imports | Inherits | Implements | Notes |
| --- | --- | --- | --- | --- | --- |
| C/C++ | yes | `#include` | base classes | no distinct syntax | Qualified and template bases retain written spelling. |
| Rust | calls | `use` | trait supertraits | `impl Trait for Type` | Generic target normalization is deferred to T005. |
| Python | calls | `import` / `from` | class arguments | no distinct syntax | Metaclass/keyword class arguments require later ambiguity filtering. |
| TypeScript | calls and constructors | ES imports | `extends` and interface `extends` | `implements` | Generic and nested type spellings are retained. |
| JavaScript | calls and constructors | ES imports | `extends` | no distinct syntax | The current project intentionally reuses the TypeScript grammar. |
| C# | calls and constructors | `using` | base list | base list | The grammar does not encode class-vs-interface target kind; extraction uses the `I` naming convention as ambiguous until T005 resolves target kinds. |
| Go | calls | import specs | not explicit | implicit only; not emitted | Interface satisfaction must never be fabricated by syntax extraction. |

## Confidence And Handoff

Direct syntax is `extracted`. C# base-list classification is `ambiguous` until
the target resolves to a known class or interface. T004 persists both target
spelling and confidence. T005 performs cross-file resolution and may promote
the relationship to `resolved` without discarding its original spelling.
