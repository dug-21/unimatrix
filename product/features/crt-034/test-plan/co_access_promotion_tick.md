# Test Plan: co_access_promotion_tick

## Component

**File created:** `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`

**Public surface:**
```rust
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
) // -> ()
```

**Module-private constant:**
```rust
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;
```

**Risks covered:** R-01, R-02, R-03, R-04, R-06, R-09, R-10, R-11, R-13
**Edge cases:** E-01, E-02, E-03, E-04, E-05, E-06

---

## Test Fixture Pattern

All tests use an in-process SQLite database with the full schema (same pattern as
`nli_detection_tick.rs` tests). Each test constructs a minimal `Store` from an in-memory or
temp-file SQLite, seeds `co_access` rows, calls `run_co_access_promotion_tick`, then queries
`graph_edges` directly.

```rust
// Helper: insert a co_access pair
fn seed_co_access(pool: &SqlitePool, a: i64, b: i64, count: i64) { ... }

// Helper: insert a graph_edges row (for "already promoted" scenarios)
fn seed_graph_edge(pool: &SqlitePool, source_id: i64, target_id: i64, weight: f64) { ... }

// Helper: query graph_edges for CoAccess edges
fn count_co_access_edges(pool: &SqlitePool) -> i64 { ... }
fn fetch_co_access_edge(pool: &SqlitePool, a: i64, b: i64) -> Option<GraphEdgeRow> { ... }
```

---

## Unit Test Expectations

### Group A: Basic Promotion (R-11, R-13, R-10)

#### `test_basic_promotion_new_qualifying_pair`

**Covers:** AC-01, R-13, R-10

**Arrange:**
- Empty `graph_edges`
- `co_access`: `(entry_id_a=1, entry_id_b=2, count=5)`
- Config: `max_co_access_promotion_per_tick = 200`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `graph_edges` has exactly 1 row
- `source_id = 1`, `target_id = 2`, `relation_type = "CoAccess"`
- `bootstrap_only = 0`, `source = "co_access"`, `created_by = "tick"` (R-13)
- `weight = 1.0` (only pair, count/max_count = 5/5)
- No reverse row: `(source_id=2, target_id=1, relation_type="CoAccess")` does not exist (R-10)

---

#### `test_inserted_edge_metadata_all_four_fields`

**Covers:** AC-12, R-13

**Arrange:**
- `co_access`: `(1, 2, count=3)`, `(1, 3, count=6)` → max_count=6
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- For the row `(source_id=1, target_id=2)`:
  - `bootstrap_only == 0`
  - `source == "co_access"` (equals `EDGE_SOURCE_CO_ACCESS`)
  - `created_by == "tick"`
  - `relation_type == "CoAccess"`
  - `weight == 0.5` (3.0/6.0)

**Note:** All four metadata fields must be asserted in a single test.

---

#### `test_inserted_edge_is_one_directional`

**Covers:** AC-12 (directionality), R-10

**Arrange:**
- `co_access`: `(entry_id_a=5, entry_id_b=10, count=4)`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Query `graph_edges WHERE relation_type = 'CoAccess'` → exactly 1 row
- `source_id = 5`, `target_id = 10`
- No row with `source_id = 10, target_id = 5`

---

### Group B: Cap and Ordering (R-11)

#### `test_cap_selects_highest_count_pairs`

**Covers:** AC-04, R-11

**Arrange:**
- `co_access`: 10 pairs with counts `[3, 3, 3, 3, 3, 10, 20, 50, 80, 100]`
  (entry_id_a = 1..10, entry_id_b = 11..20)
- Config: `max_co_access_promotion_per_tick = 3`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `graph_edges` has exactly 3 rows
- The 3 rows correspond to `count=100`, `count=80`, `count=50` (the three highest)
- Pairs with `count=3` are NOT present
- Weights are normalized: `1.0`, `0.8`, `0.5` (relative to global max=100)

**Critical:** Test must assert WHICH pairs are selected (not just that 3 exist). This is the
primary R-11 guard.

---

### Group C: Weight Refresh (R-04)

#### `test_existing_edge_stale_weight_updated`

**Covers:** AC-02, R-04

**Arrange:**
- `co_access`: `(1, 2, count=10)` → new normalized weight = 1.0
- `graph_edges`: pre-inserted CoAccess row `(source_id=1, target_id=2, weight=0.5)` (delta = 0.5 > 0.1)
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `graph_edges` still has exactly 1 row (no duplicate)
- `weight == 1.0` (updated from 0.5)

---

#### `test_existing_edge_current_weight_no_update`

**Covers:** AC-03, R-04

**Arrange:**
- `co_access`: `(1, 2, count=5)`, `(1, 3, count=10)` → normalized weight for pair (1,2) = 0.5
- `graph_edges`: pre-inserted CoAccess row `(source_id=1, target_id=2, weight=0.5)` (delta = 0.0 <= 0.1)
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `weight` for `(1, 2)` is still `0.5` (not updated)
- No additional rows inserted for `(1, 2)` (INSERT OR IGNORE no-op)
- `(1, 3)` is inserted as new edge with `weight = 1.0`

---

#### `test_weight_delta_exactly_at_boundary_no_update`

**Covers:** E-05 — delta = 0.1 exactly is NOT updated (strictly greater than, not >=)

**Arrange:**
- `co_access`: `(1, 2, count=6)`, `(1, 3, count=10)` → normalized weight for (1,2) = 0.6
- `graph_edges`: pre-inserted `(source_id=1, target_id=2, weight=0.5)` → delta = |0.6 - 0.5| = 0.1 exactly
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `weight` for `(1, 2)` remains `0.5` (NOT updated — delta == 0.1 is not > 0.1)

**This is a boundary precision test.** The condition is `|new - existing| > CO_ACCESS_WEIGHT_UPDATE_DELTA`
(strictly greater). At exactly 0.1, no update should occur.

**f64 precision note (ADR-003):** Both `weight` fetched from SQLite and `CO_ACCESS_WEIGHT_UPDATE_DELTA`
are `f64`. The comparison `(new_weight - stored_weight).abs() > CO_ACCESS_WEIGHT_UPDATE_DELTA`
avoids f32 cast precision noise. The test data is constructed with counts that produce exact
`f64` representations to avoid floating-point ambiguity at the boundary.

---

### Group D: Idempotency (R-09, R-04)

#### `test_double_tick_idempotent`

**Covers:** AC-14, R-09

**Arrange:**
- `co_access`: `(1, 2, count=5)`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`
- `run_co_access_promotion_tick(store, config, 11).await`

**Assert:**
- `graph_edges` has exactly 1 row (no duplicate)
- `weight` is unchanged after second tick

---

#### `test_sub_threshold_pair_not_gcd`

**Covers:** AC-15, R-09

**Arrange:**
- `co_access`: `(1, 2, count=5)` initially
- First tick: pair is promoted
- Update `co_access` pair count to `count=1` (below threshold=3)

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await` (second run with sub-threshold count)

**Assert:**
- `graph_edges` row `(source_id=1, target_id=2)` still exists
- Row is NOT deleted by the promotion tick

---

### Group E: Empty and Sub-threshold Table (R-02, R-06)

#### `test_empty_co_access_table_noop_late_tick`

**Covers:** AC-09(a), R-02, R-06

**Arrange:**
- `co_access` table is empty
- `current_tick = 10` (>= PROMOTION_EARLY_RUN_WARN_TICKS=5)

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- No panic
- No `warn!` emitted
- `graph_edges` remains empty
- `info!` log shows "0 inserted, 0 updated"

**Note on warn! assertion:** This requires a tracing test subscriber that captures log output.
Use the `tracing_test` crate or the pattern established by existing background tick tests in
`nli_detection_tick.rs`. If no such pattern exists, assert via `graph_edges` count and
absence of side effects.

---

#### `test_all_below_threshold_noop_late_tick`

**Covers:** AC-09(c), R-02

**Arrange:**
- `co_access`: multiple pairs all with `count=1` and `count=2` (all < 3)
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- No panic
- No `warn!` emitted
- `graph_edges` has 0 rows

---

#### `test_early_tick_warn_when_qualifying_count_zero`

**Covers:** AC-09(b), R-06 (quadrant: qualifying_count=0, tick < 5)

**Arrange:**
- `co_access` table is empty
- `current_tick = 0` (< PROMOTION_EARLY_RUN_WARN_TICKS=5)

**Act:**
- `run_co_access_promotion_tick(store, config, 0).await`

**Assert:**
- `warn!` IS emitted (SR-05 early-tick signal-loss detection)
- No panic

---

#### `test_late_tick_no_warn_empty_table`

**Covers:** R-06 (quadrant: qualifying_count=0, tick >= 5)

**Arrange:**
- `co_access` table is empty
- `current_tick = 5` (exactly at boundary — no longer in early window)

**Act:**
- `run_co_access_promotion_tick(store, config, 5).await`

**Assert:**
- No `warn!` emitted
- No panic

---

#### `test_fully_promoted_table_no_warn`

**Covers:** R-06 (quadrant: qualifying_count > 0, tick < 5)

**Arrange:**
- `co_access`: 3 qualifying pairs (count >= 3), all already promoted in `graph_edges`
- `current_tick = 0`

**Act:**
- `run_co_access_promotion_tick(store, config, 0).await`

**Assert:**
- No `warn!` emitted (qualifying_count > 0, so SR-05 condition is not met)
- `graph_edges` count unchanged
- The warn fires ONLY when `qualifying_count == 0 AND current_tick < 5`

---

### Group F: Write Failure Handling (R-01) — Critical Priority

#### `test_write_failure_mid_batch_warn_and_continue`

**Covers:** AC-11, R-01

**Arrange:**
- `co_access`: 3 qualifying pairs: `(1,2,count=10)`, `(1,3,count=8)`, `(1,4,count=6)`
- Inject a write failure on the INSERT for pair `(1,2)` only (use a mock or a
  pre-seeded constraint conflict — e.g., insert a non-CoAccess edge with same
  `(source_id=1, target_id=2)` but different `relation_type` to cause a UNIQUE violation
  that INSERT OR IGNORE would silence — this approach may not work. Alternative: use a
  mock Store that returns error for the first write call)
- `current_tick = 10`

**Implementation note:** If the Store interface does not support injection, simulate write
failure by pre-populating `graph_edges` with a row that would cause a non-IGNORE constraint
error. The exact injection mechanism is left to the implementation agent — the test plan
requires that the failure path is exercised.

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Function returns `()` (no panic)
- `warn!` is emitted (at least one warn log)
- Remaining pairs `(1,3)` and `(1,4)` are attempted (presence in `graph_edges`)

---

#### `test_write_failure_info_log_always_fires`

**Covers:** R-01 (info! log fires even when all writes fail)

**Arrange:**
- Set up a scenario where all write attempts fail (or return error)
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `info!` log with inserted/updated counts is emitted
- The counts reflect the actual success count (which may be 0)

**Note:** The `info!` log at the end of the batch must fire unconditionally, even if all
individual writes failed. This is the observability contract for the infallible tick.

---

### Group G: Normalization (R-03)

#### `test_global_max_normalization_subquery_shape`

**Covers:** AC-13, R-03

**Arrange:**
- `co_access`: 10 pairs with counts `[1,2,3,4,5,6,7,8,9,10]`
  (only counts >= 3 qualify: [3,4,5,6,7,8,9,10], max=10)
- Config: `max_co_access_promotion_per_tick = 3` (selects counts [10,9,8])
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- The pair with `count=10` has `weight = 1.0` (10/10)
- The pair with `count=9` has `weight = 0.9` (9/10)
- The pair with `count=8` has `weight = 0.8` (8/10)
- Normalization anchor is 10 (global max over all qualifying pairs), not 10 (batch max
  of selected 3 — same here, both equal)

**Note on AC-13 future-proofing:** Because `ORDER BY count DESC` selects the top-count
pairs, global max and batch max are equal under the current SQL. This test does not
distinguish between them by output value alone. The real guard is:

---

#### `test_global_max_outside_capped_batch`

**Covers:** R-03 (scenario 2 — max is outside capped batch is impossible under ORDER BY DESC)

**Arrange:**
- `co_access`: 5 pairs with counts `[3, 4, 5, 80, 100]`
- Config: `max_co_access_promotion_per_tick = 3` → selects [100, 80, 5]
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Pair with `count=100` has `weight = 1.0` (100/100)
- Pair with `count=80` has `weight = 0.8` (80/100)
- Pair with `count=5` has `weight = 0.05` (5/100)
- Global max = 100 is correctly used (both as the highest-count pair in the batch AND as
  the normalization anchor)

**Note:** Under `ORDER BY count DESC`, the global max is ALWAYS in the selected batch (it
has the highest count and is always selected first). The subquery embedded in the SQL must
compute the max over ALL qualifying pairs regardless of LIMIT. This test confirms the
normalization anchor is computed globally even though in practice the result equals the
batch max. See AC-13 framing.

---

### Group H: Edge Cases (E-01..E-06)

#### `test_single_qualifying_pair_weight_one`

**Covers:** E-01

**Arrange:**
- `co_access`: exactly one qualifying pair `(1, 2, count=7)`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- `weight = 1.0` (7/7 = 1.0)
- On second tick with no count change: weight remains 1.0, no UPDATE (delta=0 <= 0.1)

---

#### `test_tied_counts_secondary_sort_stable`

**Covers:** E-02

**Arrange:**
- `co_access`: 5 pairs all with `count=5`, cap=3
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Exactly 3 edges inserted (cap respected)
- All have `weight = 1.0` (5/5)
- Test does NOT assert which 3 pairs were selected (tie-breaking is arbitrary but
  deterministic within a single SQLite session)

**Open question (E-02):** If the spec requires deterministic selection under ties,
the SQL must add a secondary sort (`ORDER BY count DESC, entry_id_a ASC`). The test
plan notes this as an open question for the implementation agent. The test can
assert count=3 without specifying which 3.

---

#### `test_cap_equals_qualifying_count`

**Covers:** E-03

**Arrange:**
- `co_access`: exactly 5 qualifying pairs, cap=5
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Exactly 5 edges inserted (no off-by-one)
- All pairs processed

---

#### `test_cap_one_selects_highest_count`

**Covers:** E-04

**Arrange:**
- `co_access`: `(1,2,count=5)`, `(1,3,count=3)`, `(1,4,count=4)`
- Config: `max_co_access_promotion_per_tick = 1`
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- Exactly 1 edge inserted
- It is the pair with `count=5` (highest count)

---

#### `test_self_loop_pair_no_panic`

**Covers:** E-06

**Arrange:**
- `co_access`: `(entry_id_a=1, entry_id_b=1, count=5)` (self-loop, violates convention)
- `current_tick = 10`

**Act:**
- `run_co_access_promotion_tick(store, config, 10).await`

**Assert:**
- No panic
- If UNIQUE constraint allows the row, it is inserted (or ignored if the DB rejects it)
- The tick does not crash regardless

---

## f64 Precision Note (ADR-003)

`CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` (not f32). sqlx fetches SQLite REAL columns
as `f64`. Tests involving the delta boundary (E-05) should use count values that produce
exact f64 representations to avoid floating-point ambiguity. Example:

- count=5 of max=10 → weight=0.5 exactly representable as f64
- count=6 of max=10 → weight=0.6 (not exactly representable, but the delta test should
  use counts that produce a clean boundary)

The test `test_weight_delta_exactly_at_boundary_no_update` should use:
- Stored weight = `0.5` (counts: 5/10)
- New weight = `0.6` (counts: 6/10) — delta = 0.1
- Comparison: `0.6 - 0.5 = 0.1` which is NOT `> 0.1`, so NO update

Both values are chosen to be close to their f64 representations. If precision
drift causes issues, the test should document the actual f64 delta observed.

---

## Acceptance Criteria Mapped

| AC-ID | Test Function | Expected Result |
|-------|--------------|-----------------|
| AC-01 | `test_basic_promotion_new_qualifying_pair` | Row in graph_edges with CoAccess type |
| AC-02 | `test_existing_edge_stale_weight_updated` | Weight updated when delta > 0.1 |
| AC-03 | `test_existing_edge_current_weight_no_update` | No update when delta <= 0.1 |
| AC-04 | `test_cap_selects_highest_count_pairs` | Only top-3 by count promoted |
| AC-09(a) | `test_empty_co_access_table_noop_late_tick` | No panic, no warn, 0/0 log |
| AC-09(b) | `test_early_tick_warn_when_qualifying_count_zero` | warn! emitted |
| AC-09(c) | `test_all_below_threshold_noop_late_tick` | No warn, 0/0 |
| AC-11 | `test_write_failure_mid_batch_warn_and_continue` | warn!, continues, returns () |
| AC-12 | `test_inserted_edge_metadata_all_four_fields` | All 4 fields correct |
| AC-13 | `test_global_max_normalization_subquery_shape` | Global max used as normalization anchor |
| AC-14 | `test_double_tick_idempotent` | Exactly 1 row, weight unchanged |
| AC-15 | `test_sub_threshold_pair_not_gcd` | Row still present after count drops below threshold |
| E-05 | `test_weight_delta_exactly_at_boundary_no_update` | No update at exactly delta=0.1 |
