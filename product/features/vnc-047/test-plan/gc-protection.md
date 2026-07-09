# C10 — GC protection by omission + regression test

> File: `crates/unimatrix-store/src/retention.rs` — NO code change to DELETE paths (protection by
> OMISSION, ADR-005). Extend `test_gc_protected_tables_regression` (:521).
> Risks: **R-07 (High)**. ACs: AC-04. GC surfaces: `gc_cycle_activity` (:116), `gc_unattributed_
> activity` (:202).

## Reuse (extend, do not fork)
Extend the EXISTING `test_gc_protected_tables_regression` (`retention.rs:521-643`). It already:
seeds retained + purgeable cycles via `insert_cycle_review`; seeds protected rows (`entries`,
`cycle_events`, `observation_metrics`/`observation_phase_metrics`); snapshots via
`count_table(&store, "…")`; adds purgeable `sessions`+`observations`; runs `list_purgeable_cycles(1,
100)` then BOTH `gc_cycle_activity(cycle)` and `gc_unattributed_activity()` (+ `gc_audit_log`); and
asserts each protected table count is unchanged. Currently asserts on: `entries`, `cycle_events`,
`cycle_review_index`, `observation_phase_metrics`.

## AC-04 — extension expectations
- **Seed `cycle_tags`** for the purgeable cycle(s) (via `insert_cycle_start_with_tags` or raw insert
  on `write_pool_server()`), and snapshot `count_table(&store, "cycle_tags")` alongside the existing
  protected-table snapshots.
- Run the full GC pass across BOTH surfaces (unchanged in the test) and assert
  `count_table("cycle_tags")` is UNCHANGED after `gc_cycle_activity` AND `gc_unattributed_activity`
  (cycle_tags is absent from every DELETE enumeration).
- **Positive control (MANDATORY — anti-vacuous):** assert `sessions` rows for the SAME purgeable
  cycle ARE purged in the same pass (count drops to 0). This proves the GC actually ran and the
  cycle_tags survival is not a no-op false-pass (R-07/SR-09). The test already seeds purgeable
  `sessions` — assert their deletion explicitly.

## Coverage requirement
`cycle_tags` proven surviving BOTH GC DELETE surfaces WITH the `sessions` positive control in the
same test run. A future DELETE that starts touching `cycle_tags` must break this test.

## Note
Protection is by OMISSION — there is no protected-set data structure in `retention.rs`. Do NOT add
`cycle_tags` to any DELETE; do NOT "register" it. The test is the only guard against a future
accidental DELETE.
