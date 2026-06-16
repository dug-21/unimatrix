# Test Plan — Review pipeline ordering (tools.rs context_cycle_review)

**Component**: `unimatrix-server/src/mcp/tools.rs` `context_cycle_review` handler — the integration spine ordering auto_close → read-before-purge → aggregate → reload → presence → single persist
**Risks**: R-01 (second writer / empty-clobber — CRITICAL), R-02 (stale-version no-flush — CRITICAL), R-03 (ordering), R-14 (auto_close ordering), R-11 (leak gate)
**ACs**: AC-17 (single writer / no-clobber), AC-18 (guarded recompute), AC-16 (#206-4 knowledge-that-helped), AC-19 (leak gate), AC-01 (composed presence)

> This component validates the COMPOSITION and ORDERING that no unit test can reproduce: the six-step pipeline order, the four success returns, the #758 guarded-recompute coexistence. Lesson #5022: the serve/recompute decision must live in the HANDLER (typed `Staleness` enum reading `attributed`), not in `check_stored_review` (which holds only `&CycleReviewRecord`).

## Integration tests (handler-level, in-process + MCP harness)

### The four success returns / no-clobber — the three #5022 assertions (R-01, AC-17)
- `test_full_pipeline_return_writes_columns` (AC-17) — full-pipeline return → the single `store_cycle_review()` lands all v5 columns.
- `test_memo_hit_return_does_not_write` (AC-17) — memo-hit → serve stored, NO recompute, NO write.
- `test_purged_retain_return_byte_identical_no_write` (AC-17b, #5022-b) — source purged + stale → retain stored row byte-identical, NO write; advisory = "source purged, cannot recompute" (NEVER "use force=true").
- `test_force_purged_no_clobber` (AC-17c, #5022-c) — `force=true` on a purged row → `computed_at` does NOT advance, columns NOT clobbered with zeros (the `:2089`-class interceptor path).

### Guarded-recompute coexistence with #758 (R-02, AC-18)
- `test_stale_present_recomputes_fresh_nonzero` (AC-18, #5022-a) — pre-v5 stale row, source PRESENT → recompute via clear-memo-and-fall-through (the single writer) → fresh NON-ZERO columns at `schema_version == 5`. A version-equality test alone is insufficient — assert the cache actually FLUSHES.
- `test_stale_purged_retains_no_recompute` (AC-18) — pre-v5 stale row, source PURGED → retain stored columns, no recompute, no "use force=true" advisory.
- `test_staleness_decision_in_handler_not_check_stored_review` (R-02, #5022) — the serve/recompute decision reads `attributed` in the handler (typed `Staleness` enum), not inside `check_stored_review`.
- `test_recompute_routes_through_single_writer` (R-01/R-02) — recompute is clear-memo-and-fall-through, NEVER a second `store_cycle_review` near the memo site (empty-clobber structurally impossible).

### Pipeline ordering (R-03, R-14) — composition of component units
- `test_pipeline_order_auto_close_then_purge_read_then_persist` — assert the handler executes in order: (1) auto_close, (2) read-before-purge, (3) aggregate, (4) reload, (5) presence flags, (6) single persist. Ordering risks R-03 (read-before-purge) and R-14 (auto_close-before-rank1) are composed here; the unit-level inversion tests live in activity_fold_landing.md and auto_close.md.

### Knowledge-that-helped #206-4 (AC-16)
- `test_retrospective_surfaces_contributing_entries` (AC-16) — the retrospective output includes the cycle's contributing entries (response-time enrichment).
- `test_no_durable_column_for_knowledge_that_helped` (AC-16, ADR-009) — assert NO new column was added for #206-4 (it is response-time only, not persisted).

### Leak gate composition (R-11, AC-19)
- `test_full_pipeline_persists_no_content` (AC-19) — after a full review, `test_candidates_structurally_absent_from_memoized_report` holds; no content field reaches the persisted record.

## Harness integration tests (extend test_lifecycle.py)
- `test_cycle_review_stale_present_recomputes` / `test_cycle_review_stale_purged_retains` (AC-17/18) — end-to-end #5022 a/b/c through the compiled binary with real memo state + real SQLite.

## Edge cases (three distinct recompute outcomes — RISK-TEST-STRATEGY §Edge Cases)
- Pre-v5 stale + purged → retain byte-identical.
- Pre-v5 stale + present → recompute fresh non-zero at v5.
- Fresh v5 → served from memo, no recompute.
- Force=true + purged → serve stored, no clobber, `computed_at` unchanged.

## Expected behaviors / assertions summary
- Exactly one writer at the full-pipeline return; the other three returns never write the new columns.
- Stale+present recomputes (cache flushes) to fresh non-zero at v5; stale+purged retains byte-identical.
- Pipeline order: auto_close → read-before-purge → aggregate → reload → presence → single persist.
- #206-4 surfaced response-time only, no durable column; leak gate holds post-pipeline.
