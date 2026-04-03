# Agent Report: crt-044-agent-4-tick

**Component:** `graph_enrichment_tick_s1_s2_s8`
**Feature:** crt-044 — Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment
**Agent:** crt-044-agent-4-tick (claude-sonnet-4-6)
**Date:** 2026-04-03

---

## Files Modified

- `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
- `crates/unimatrix-server/src/services/graph_enrichment_tick_tests.rs`
- `crates/unimatrix-server/src/server.rs`

---

## Summary of Changes

### `graph_enrichment_tick.rs`

Added a second `write_graph_edge` call per pair in all three tick functions, with `source_id` and `target_id` swapped. All other arguments are identical to the first call. Conforms to ADR-002 (two-call pattern) and C-09 (false return is expected — no warn, no error counter).

- `run_s1_tick`: second call passes `row.target_id as u64, row.source_id as u64` with `EDGE_SOURCE_S1` and `"Informs"`. `edges_written` increments only on `true` return.
- `run_s2_tick`: identical pattern with `EDGE_SOURCE_S2`.
- `run_s8_tick`: second call passes `*b, *a` with `EDGE_SOURCE_S8` and `"CoAccess"`. `pairs_written` now counts per-edge (C-06): new pair increments by 2; steady-state post-migration increments by 0 or 1.

File is 436 lines (within 500-line limit).

### `graph_enrichment_tick_tests.rs`

**16 existing tests updated** — all exact `count_edges_by_source` assertions updated to reflect doubled counts from bidirectional writes:

| Test | Old assertion | New assertion |
|------|--------------|--------------|
| `test_s1_idempotent` | count == 1 | count == 2 |
| `test_s1_having_threshold_exactly_3` | count == 1 | count == 2 |
| `test_s1_source_value_is_s1_not_nli` | count == 1 | count == 2 |
| `test_s1_cap_respected` | count == 3 | count == 6 |
| `test_s2_idempotent` | count == 1 | count == 2 |
| `test_s2_cap_respected` | count == 2 | count == 4 |
| `test_s2_source_value_is_s2` | count == 1 | count == 2 |
| `test_s2_threshold_exactly_2_terms` | count == 1 | count == 2 |
| `test_s8_idempotent` | count == 1 | count == 2 |
| `test_s8_watermark_advances_past_malformed_json_row` | count == 2 (second run check) | count == 4 |
| `test_s8_pair_cap_not_row_cap` | count == 5 | count == 10 |
| `test_s8_partial_row_watermark_semantics` | count == 3 | count == 6 |
| `test_s8_gated_by_tick_interval` | written == 1, count == 1 | written == 2, count == 2 |
| `test_enrichment_tick_calls_s1_and_s2_always` | count("S8") == 1 | count("S8") == 2 |
| `test_enrichment_tick_s8_runs_on_batch_tick` | count("S8") == 1 | count("S8") == 2 |

**5 new tests added** per test plan:

| Test ID | Function | Coverage |
|---------|----------|---------|
| TICK-S1-U-10 | `test_s1_both_directions_written` | AC-03, AC-10, R-03 |
| TICK-S2-U-10 | `test_s2_both_directions_written` | AC-04, AC-10, R-03 |
| TICK-S8-U-10 | `test_s8_both_directions_written` | AC-05, AC-10, R-03 |
| TICK-S8-U-11 | `test_s8_pairs_written_counter_per_edge_new_pair` | AC-05, AC-12, R-05 |
| TICK-S8-U-12 | `test_s8_false_return_on_existing_reverse_no_warn_no_increment` | AC-13, R-04 |

### `server.rs`

Updated `test_migration_v7_to_v8_backfill` — two assertions changed from `version == 19` to `version == 20`. This test validates that the DB migrates to the current schema version when opened from a v7 state. The migration agent bumped `CURRENT_SCHEMA_VERSION` to 20; these assertions were stale.

---

## Test Results

All 41 `graph_enrichment_tick` tests pass. Full workspace suite (2686+ tests) passes with `--features test-support`. Zero new failures introduced.

---

## Constraints Verified

| Constraint | Status |
|-----------|--------|
| C-06: `pairs_written` counts per-edge | Satisfied — each `write_graph_edge` call incremented independently |
| C-09: false return not treated as error | Satisfied — no warn/error log, no counter increment on false |
| ADR-002: two `write_graph_edge` calls per pair with swapped args | Satisfied |
| SQL query shapes unchanged | Satisfied — only loop body extended |
| File under 500 lines | Satisfied — 436 lines |

---

## Issues / Blockers

None. The migration agent had already bumped `CURRENT_SCHEMA_VERSION` to 20 before this agent ran. The stale `version == 19` assertion in `test_migration_v7_to_v8_backfill` was the only integration point requiring attention beyond the task scope.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #4060, #3884, #4054, #4080, #4041, #4078 relevant to bidirectional write patterns and write_graph_edge return contract
- Stored: entry #4083 "Adding bidirectional writes to graph_enrichment_tick breaks existing count_edges_by_source assertions — update all exact counts" via `/uni-store-pattern`
