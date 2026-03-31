# Agent Report: crt-035-agent-3-tick

**Component:** Bidirectional Promotion Tick (Component A)
**Feature:** crt-035
**Date:** 2026-03-30

---

## Files Modified

- `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`
- `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`

---

## Implementation Summary

### co_access_promotion_tick.rs

- Extracted module-private `promote_one_direction(store, source_id, target_id, new_weight) -> (bool, bool)` helper encapsulating the three-step INSERT-fetch-UPDATE sequence per directed edge.
- Updated `run_co_access_promotion_tick` Phase 3 loop to call `promote_one_direction` twice per qualifying pair: forward `(entry_id_a, entry_id_b)` then reverse `(entry_id_b, entry_id_a)`.
- Accumulates `inserted_count` and `updated_count` across both directions per pair.
- Updated Phase 4 `tracing::info!` fields from `inserted`/`updated`/`qualifying` to `promoted_pairs`/`edges_inserted`/`edges_updated` (D2).
- Updated module doc comment from "One-directional edges v1" to bidirectional description.
- All early-return paths (fetch error, degenerate max, zero qualifying) updated to emit the new `promoted_pairs`/`edges_inserted`/`edges_updated` field names.

### co_access_promotion_tick_tests.rs

Updated T-BLR-01 through T-BLR-08:
- T-BLR-01: removed `is_none()` reverse assertion; added `count == 2` + reverse `is_some()` + field checks.
- T-BLR-02: renamed `test_inserted_edge_is_one_directional` → `test_inserted_edge_is_bidirectional`; replaced all three old assertions with `count == 2` + both directions `is_some()` + equal weights.
- T-BLR-03: added first-tick `count == 2` assertion + reverse weight check; second-tick `count == 2` idempotency check.
- T-BLR-04: `count == 3` → `count == 6`; added reverse direction `is_some()` assertions for all three selected pairs.
- T-BLR-05: `count == 3` → `count == 6`.
- T-BLR-06: `count == 5` → `count == 10`.
- T-BLR-07: `count == 1` → `count == 2`; added forward + reverse `is_some()` assertions.
- T-BLR-08: removed `"no duplicate"` assertion message; replaced with forward-updated + reverse-inserted checks + `count == 2`.

Added T-NEW-01 through T-NEW-03 in new Group I section:
- T-NEW-01: `test_bidirectional_edges_inserted_same_weight` — both directions at equal weight.
- T-NEW-02: `test_bidirectional_both_directions_updated_when_drift_exceeds_delta` — both stale directions converge.
- T-NEW-03: `test_log_format_promoted_pairs_and_edges_inserted` — `promoted_pairs=2`, `edges_inserted=4`, `edges_updated=0`.

Also applied R-06 coverage gap recommendation: extended `test_existing_edge_current_weight_no_update` to assert reverse edge is inserted even when forward weight is unchanged.

---

## Gate Results

| Gate | Status | Detail |
|------|--------|--------|
| GATE-3B-01 | PASS | `grep '"no duplicate"' co_access_promotion_tick_tests.rs` → 0 matches |
| GATE-3B-02 | PASS | All `count_co_access_edges` assertion values: 0, 2, 2, 6, 2, 2, 2, 0, 6, 10, 2, 2, 2 — all even |
| GATE-3B-04 | PASS | `co_access_promotion_tick.rs` = 344 lines (< 500) |

---

## Test Results

`cargo test -p unimatrix-server --lib services::co_access_promotion_tick::tests`

**26 passed, 0 failed**

All 26 tests pass including the 8 updated T-BLR tests and 3 new T-NEW tests.

Full library test suite: **2518 passed, 0 failed** — no regressions.

---

## Issues / Blockers

None. Implementation followed pseudocode exactly. No deviations.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- surfaced Pattern #3822 (promotion tick idempotency oscillation) and ADR #3890 (eventual consistency, not atomic per pair). Both applied directly: INSERT OR IGNORE idempotency preserved; each direction independent per ADR-001.
- Stored: entry #3893 "Helper extraction pattern for infallible bidirectional tick writes" via /uni-store-pattern — captures the promote_one_direction extraction approach, why inline doubling is inadvisable (nesting + line limit), and the self-loop safety property via UNIQUE constraint.
