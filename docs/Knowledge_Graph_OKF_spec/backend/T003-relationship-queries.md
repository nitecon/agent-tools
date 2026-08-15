# T003 - Tree-sitter relationship query coverage

Port or author relationship queries for calls, imports, inheritance, and
implements across every language currently supported by `agent-symbols`.

## Requirements

- Record per-language coverage and unavoidable ambiguity.
- Emit relation kind, original target spelling, and precise source spans.
- Keep extraction separate from cross-file resolution.
- Attribute reusable Graphify query material under its license.
- Pin behavior to the repository's actual grammar versions.

## Validation

Fixture tests cover C/C++, Rust, Python, TypeScript/JavaScript, C#, and Go and
assert extracted, unsupported, and ambiguous constructs deterministically.
