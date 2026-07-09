# C11 — Deferred mutation seam (comment-only)

> File: `crates/unimatrix-server/src/mcp/tools.rs` near the `context_tag` handler (~:1542-1614).
> Comment-only reservation (ADR-006). NOT a new `context_cycle_tag` tool, NOT built now.
> Risks: R-06 (interface stability). ACs: AC-06 (`context_tag`/`context_correct` unchanged).

## Test expectations (minimal — comment-only component)
- `test_context_tag_handler_unchanged` — the entry-targeting `context_tag` handler behavior is
  unchanged: existing `context_tag` unit/integration tests remain green (no behavioral delta from the
  comment-only seam). Diff review confirms only a comment was added — no code, no new tool, no new
  param wired to persist.
- `test_context_correct_unchanged` — `context_correct` untouched (diff-clean).

## Coverage requirement
No behavioral test is needed for the seam itself (it is inert). The obligation is a diff-review
sign-off (recorded in RISK-COVERAGE-REPORT.md) that:
1. no `context_cycle_tag` tool was added,
2. no cycle-tag mutation/add/remove/replace path exists,
3. `context_tag` / `context_correct` handlers are byte-unchanged except for the reservation comment.

If a future reviewer adds prefix querying at this seam, `like_escape` becomes mandatory (flag per
ADR-006 / security note) — out of scope now.
