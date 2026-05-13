# T013 - Extract shared grep/sed text command engine before sed preview/write

**Team:** backend/refactor
**Phase:** 2
**Depends on:** T005
**Status:** todo

## Scope

**In:** Extract the command-local grep execution semantics added by T005 into a narrow reusable text-command layer before sed preview/write implement equivalent traversal, matching, diagnostics, counters, and exit assembly.

**Out:** Sed preview/write behavior itself. Those still land in T006 and T007.

## Source references

- T005 implementation in `crates/agent-cli/src/main.rs` now owns grep pattern validation, matcher setup, target option construction, per-file matching, diagnostic promotion, summary accounting, null path rendering selection, and exit-code assembly.
- DRY pass 2 tasks `019e1eef-8076-70c2-85b7-86573a83f281` and `019e1eef-a023-76b1-aeb9-2db6f5affaf4` both recommend a narrow CLI-local shared text operation layer.

## Deliverables

1. A shared text command module, preferably `crates/agent-cli/src/cmd_text.rs` or equivalent, that owns grep/sed command planning and matcher helpers.
2. `crates/agent-cli/src/main.rs` reduced to parsing and dispatch for grep-related shared behavior.
3. Existing grep output, exit codes, and conformance rows unchanged.
4. Clear implementation notes or code shape that T006/T007 can consume instead of copying `run_grep`-style logic.

## Boundary guidance

- Keep `agent-fs` focused on target discovery, file text classification, and CLI-independent text primitives.
- Keep `agent-core` focused on records, rendering, and exit classification.
- Keep regex as a direct `agent-cli` dependency if matcher orchestration remains in an `agent-cli` module. Move it to `agent-fs` only if matcher compilation becomes an `agent-fs` API. Do not move regex into `agent-core`.

## Acceptance criteria

- [ ] Grep keeps existing T005 conformance output and exit codes.
- [ ] CLI main keeps parsing/dispatch only for grep/sed shared text behavior.
- [ ] Shared code owns pattern-source validation hooks for inline, pattern-file, and stdin-deferred modes.
- [ ] Shared code builds regex/default/fixed/ignore-case matchers.
- [ ] Shared code constructs `TextTargetOptions` from include/exclude/glob/path operands.
- [ ] Shared code handles per-file match iteration, `TextFile` diagnostic to `TextRecord`/counter promotion, summary insertion, null path result selection, and `TextExitClassificationInput` assembly.
- [ ] T006 and T007 can consume the shared engine without copying `run_grep`-style matcher or diagnostic logic.

## Validation plan

- Run `cargo test -p agent-cli grep`.
- Inspect T006/T007 specs and ensure their implementation notes point at the shared engine.

## Dependencies

- **T005:** Supplies the first command implementation to extract from.

## Provides to downstream tasks

- **T006:** Shared pattern, target, matcher, diagnostic, counter, and exit assembly helpers.
- **T007:** Shared command planning and diagnostics for write mode.
