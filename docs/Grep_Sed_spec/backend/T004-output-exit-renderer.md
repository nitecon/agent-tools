# T004 - Implement shared grep/sed output and exit-code renderer

**Team:** backend
**Phase:** 1
**Depends on:** T001, T002
**Status:** todo

## Scope

**In:** Implement shared deterministic rendering and exit-code classification for grep and sed.

**Out:** Matching, replacement, traversal, and write mutation behavior. Those are separate tasks.

## Source references

- `docs/Grep_Sed.md` lines 116-118 require compact deterministic plain text by default.
- `docs/Grep_Sed.md` lines 211-218 require bounded preview records, summaries, truncation, and resume hints.
- `docs/Grep_Sed.md` lines 230-238 require stable output grammar and exact exit codes.
- `docs/Grep_Sed.md` lines 252-253 require natural-language output with stable labels where agents parse warnings, skips, truncation, or resume hints.

## Deliverables

1. Typed intermediate result model for grep/sed rendering and status classification, carrying record family, path identity, line and byte position where applicable, warning/skip/error label, replacement record ID, truncation cursor or resume token, summary counters, and exit-classification inputs. The model location must be acyclic with T003: either keep any `agent-core` data primitive and independent of `agent-fs`, or place grep/sed result and renderer types in `agent-fs` or another deliberate shared text layer.
2. Shared renderer for:
   - Grep match, context, count, and files-with-matches records.
   - Sed preview records.
   - Sed write summary records.
   - Warning-only skip, error, truncation, and resume hint records.
3. Exit-code classifier matching the T001 table.
4. Exact-output tests for representative records and result classes.

## Implementation notes

- Keep the renderer reusable across CLI commands so grep and sed do not drift.
- Stable labels should be easy for agents to recognize without making JSON the primary interface.
- If null-delimited variants are in v1, define their boundary behavior here and test exact bytes.

## Acceptance criteria

- [ ] A typed intermediate result model exists before rendering and carries stable fields for grep matches, grep context, count mode, files-with-matches mode, sed previews, write summaries, warnings, skips, truncation, resume hints, and exit-code classification.
- [ ] A shared renderer emits compact deterministic plain-text records for grep matches, grep context, count mode, files-with-matches mode, sed previews, write summaries, warnings, skips, truncation, and resume hints.
- [ ] Exit codes match the T001 contract for grep match/no-match/error and sed preview changed/no-op/write changed/write no-op/invalid expression/invalid path/warning-only/partial traversal failure cases.
- [ ] Bounded output truncates at deterministic record boundaries and prints stable resume hints.
- [ ] Tests assert exact output grammar and exit codes for representative success, no-result, warning-only, invalid input, and partial traversal cases.

## Validation plan

- **Renderer tests:** Run exact-output tests in `agent-core`, `agent-fs`, or `agent-cli`, depending on the acyclic model/renderer location chosen above.
- **Exit-code tests:** Run CLI integration tests that assert process status for each result class.
- **Conformance alignment check:** Renderer golden cases and CLI process-status cases reference the same T002 result-class row IDs so renderer output and CLI status behavior cannot drift.
- **Bounded output test:** Use a fixture with more records than the limit and assert stable truncation/resume output.

## Dependencies

- **T001:** Defines output grammar and exit-code classes.
- **T002:** Provides result-class scenarios.

## Provides to downstream tasks

- **T005:** Grep output and status behavior.
- **T006:** Sed preview output and status behavior.
- **T007:** Sed write summary and status behavior.
- **T008/T009:** Stable user-facing behavior for docs and bulk workflows.
