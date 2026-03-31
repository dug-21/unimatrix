# Test Plan: co_access_promotion_tick_tests.rs (crt-035)

**File:** `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`

---

## Scope

This plan covers:
1. Eight mandatory test updates (T-BLR-01 through T-BLR-08) — existing tests broken by the
   bidirectional tick change.
2. Three new Group I tests (T-NEW-01, T-NEW-02, T-NEW-03) — positive assertions for the
   new bidirectional behavior.
3. One coverage gap note (R-06).

---

## Pre-Condition: Test Helper Inventory

The following helpers already exist in the test file and must be used unchanged:

- `seed_co_access(store, a, b, count)` — inserts `(a < b)` pair into CO_ACCESS.
- `seed_graph_edge(store, source_id, target_id, weight)` — inserts a CoAccess row into
  GRAPH_EDGES with `created_by='test'`, `source='co_access'`, `bootstrap_only=0`.
- `count_co_access_edges(store)` — returns `COUNT(*) FROM graph_edges WHERE relation_type='CoAccess'`.
- `fetch_co_access_edge(store, a, b)` — returns `Option<GraphEdgeRow>` for the edge
  `(source_id=a, target_id=b, relation_type='CoAccess')`.

**Invariant after crt-035:** `count_co_access_edges` returns `2 * N` for N qualifying pairs.
Every `assert_eq!` on its return value must be even. An odd value is a delivery defect.

---

## T-BLR-01: `test_basic_promotion_new_qualifying_pair` (update)

**Current assertion (must be removed):**
```rust
assert!(fetch_co_access_edge(&store, 2, 1).await.is_none(), "no reverse edge must exist");
```

**Required assertions after update:**
```rust
assert_eq!(count_co_access_edges(&store).await, 2, "both directions must be inserted");
let reverse = fetch_co_access_edge(&store, 2, 1)
    .await
    .expect("reverse edge must exist after bidirectional tick");
assert_eq!(reverse.source_id, 2);
assert_eq!(reverse.target_id, 1);
assert!((reverse.weight - 1.0).abs() < 1e-9, "reverse edge weight must equal forward");
assert_eq!(reverse.created_by, "tick");
assert_eq!(reverse.source, "co_access");
assert_eq!(reverse.bootstrap_only, 0);
```

**Risks covered:** R-02 (stale "no reverse" assertion removed), R-08 (even count 2).
**AC covered:** AC-01, AC-02.

---

## T-BLR-02: `test_inserted_edge_is_one_directional` (rename + full replacement)

**Action:** Rename function to `test_inserted_edge_is_bidirectional`. Replace all
three existing assertions.

**Current assertions (all must be removed):**
```rust
assert_eq!(count_co_access_edges(&store).await, 1);
assert!(fetch_co_access_edge(&store, 5, 10).await.is_some());
assert!(fetch_co_access_edge(&store, 10, 5).await.is_none(), "reverse edge must not be created");
```

**Required assertions after replacement:**
```rust
assert_eq!(count_co_access_edges(&store).await, 2, "bidirectional: 2 edges for 1 pair");
let fwd = fetch_co_access_edge(&store, 5, 10)
    .await
    .expect("forward edge (5→10) must exist");
let rev = fetch_co_access_edge(&store, 10, 5)
    .await
    .expect("reverse edge (10→5) must exist after crt-035");
assert!((fwd.weight - rev.weight).abs() < 1e-9, "both directions must have equal weight");
```

**Risks covered:** R-02 (test name, comment, and all assertions updated), R-08 (even count 2).
**AC covered:** AC-01, AC-02.

---

## T-BLR-03: `test_double_tick_idempotent` (update)

**Current assertion (must be updated):**
```rust
assert_eq!(count_co_access_edges(&store).await, 1, "exactly 1 row after 2 ticks");
```

**Required assertions after update:**
```rust
// After first tick: both directions inserted.
assert_eq!(count_co_access_edges(&store).await, 2, "exactly 2 rows after first tick");
let weight_after_first_fwd = fetch_co_access_edge(&store, 1, 2)
    .await
    .expect("forward edge after first tick")
    .weight;
let weight_after_first_rev = fetch_co_access_edge(&store, 2, 1)
    .await
    .expect("reverse edge after first tick")
    .weight;

// Run second tick.
run_co_access_promotion_tick(&store, &make_config(200), 11).await;

// After second tick: same 2 rows, weights unchanged.
assert_eq!(count_co_access_edges(&store).await, 2, "exactly 2 rows after second tick (idempotent)");
assert!(
    (fetch_co_access_edge(&store, 1, 2).await.unwrap().weight - weight_after_first_fwd).abs() < 1e-9,
    "forward weight unchanged after second tick"
);
assert!(
    (fetch_co_access_edge(&store, 2, 1).await.unwrap().weight - weight_after_first_rev).abs() < 1e-9,
    "reverse weight unchanged after second tick"
);
```

**Risks covered:** R-08 (even count 2 after both ticks).
**AC covered:** AC-04 (idempotency via INSERT OR IGNORE).

---

## T-BLR-04: `test_cap_selects_highest_count_pairs` (update)

**Current assertion (must be updated):**
```rust
assert_eq!(count_co_access_edges(&store).await, 3, "cap must be respected");
```

**Required assertion after update:**
```rust
assert_eq!(
    count_co_access_edges(&store).await,
    6,
    "cap=3 pairs × 2 directions = 6 edges"
);
```

Also add reverse-direction presence assertions for the three selected pairs:
```rust
assert!(fetch_co_access_edge(&store, 20, 10).await.is_some(), "reverse of count=100 pair");
assert!(fetch_co_access_edge(&store, 19, 9).await.is_some(), "reverse of count=80 pair");
assert!(fetch_co_access_edge(&store, 18, 8).await.is_some(), "reverse of count=50 pair");
```

**Risks covered:** R-08 (even count 6 = 3 × 2).
**AC covered:** AC-01.

---

## T-BLR-05: `test_tied_counts_secondary_sort_stable` (update)

**Current assertion (must be updated):**
```rust
assert_eq!(count_co_access_edges(&store).await, 3, "cap=3 respected");
```

**Required assertion after update:**
```rust
assert_eq!(
    count_co_access_edges(&store).await,
    6,
    "cap=3 pairs × 2 directions = 6 edges"
);
```

**Risks covered:** R-08 (even count 6).
**AC covered:** AC-01.

---

## T-BLR-06: `test_cap_equals_qualifying_count` (update)

**Current assertion (must be updated):**
```rust
assert_eq!(count_co_access_edges(&store).await, 5, "all 5 pairs promoted");
```

**Required assertion after update:**
```rust
assert_eq!(
    count_co_access_edges(&store).await,
    10,
    "5 pairs × 2 directions = 10 edges; all pairs promoted"
);
```

**Risks covered:** R-08 (even count 10).
**AC covered:** AC-01.

---

## T-BLR-07: `test_cap_one_selects_highest_count` (update)

**Current assertion (must be updated):**
```rust
assert_eq!(count_co_access_edges(&store).await, 1);
```

**Required assertions after update:**
```rust
assert_eq!(count_co_access_edges(&store).await, 2, "1 pair × 2 directions = 2 edges");
assert!(fetch_co_access_edge(&store, 1, 2).await.is_some(), "forward edge present");
assert!(fetch_co_access_edge(&store, 2, 1).await.is_some(), "reverse edge present");
```

**Risks covered:** R-08 (even count 2).
**AC covered:** AC-01.

---

## T-BLR-08: `test_existing_edge_stale_weight_updated` (update — Critical)

This is the highest-priority blast-radius update. The "no duplicate" comment is the
trigger for GATE-3B-01 and encodes the stale one-directional contract.

**Current assertions (must be updated — all three lines):**
```rust
assert_eq!(count_co_access_edges(&store).await, 1, "no duplicate");
// (weight assertion follows)
```

**Required assertions after update:**
```rust
// Forward edge updated.
let fwd = fetch_co_access_edge(&store, 1, 2)
    .await
    .expect("forward edge must exist");
assert!(
    (fwd.weight - 1.0).abs() < 1e-9,
    "forward edge weight must be updated to 1.0"
);

// Reverse edge newly inserted by tick.
let rev = fetch_co_access_edge(&store, 2, 1)
    .await
    .expect("reverse edge must be inserted by tick");
assert!(
    (rev.weight - 1.0).abs() < 1e-9,
    "reverse edge weight must be 1.0 (newly inserted)"
);

// Exactly 2 rows: forward (updated) + reverse (new). No third row.
assert_eq!(
    count_co_access_edges(&store).await,
    2,
    "no duplicate: forward (updated) + reverse (new) = 2"
);
```

**GATE-3B-01 confirmation:** After this update, `grep '"no duplicate"'` must return zero
matches. The comment is removed; the assert message now reads "no duplicate: forward
(updated) + reverse (new) = 2".

**Risks covered:** R-02 (Critical — "no duplicate" removed), R-03 (OQ-01 count=2 confirmed),
R-08 (even count 2).
**AC covered:** AC-03, AC-04.

---

## T-NEW-01: `test_bidirectional_edges_inserted_same_weight` (new — Group I)

**Location:** Add in a new `// Group I: Bidirectional Assertions` section.
**AC covered:** AC-01, AC-02.

```rust
/// AC-01, AC-02: both directions inserted with equal weight on a fresh pair.
#[tokio::test]
async fn test_bidirectional_edges_inserted_same_weight() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Single qualifying pair: count=5, max_count=5, new_weight=1.0
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(count_co_access_edges(&store).await, 2, "both directions must be inserted");
    let fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge (1→2) must exist");
    let rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge (2→1) must exist");
    assert!(
        (fwd.weight - 1.0).abs() < 1e-9,
        "forward weight must be 1.0"
    );
    assert!(
        (rev.weight - 1.0).abs() < 1e-9,
        "reverse weight must be 1.0"
    );
    assert!(
        (fwd.weight - rev.weight).abs() < 1e-9,
        "both directions must carry equal weight"
    );
}
```

---

## T-NEW-02: `test_bidirectional_both_directions_updated_when_drift_exceeds_delta` (new — Group I)

**AC covered:** AC-03 (both edges updated when delta > 0.1), FR-12 (convergence), R-05.

```rust
/// AC-03, FR-12, R-05: pre-seeded stale forward and reverse edges both converge on tick.
#[tokio::test]
async fn test_bidirectional_both_directions_updated_when_drift_exceeds_delta() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Pre-seed asymmetric stale weights (simulates a partial prior tick failure).
    seed_graph_edge(&store, 1, 2, 0.5).await; // forward: delta = |1.0 - 0.5| = 0.5 > 0.1
    seed_graph_edge(&store, 2, 1, 0.2).await; // reverse: delta = |1.0 - 0.2| = 0.8 > 0.1
    // Single pair with count=10 → new_weight = 1.0 (max is itself).
    seed_co_access(&store, 1, 2, 10).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge must exist");
    let rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge must exist");
    assert!(
        (fwd.weight - 1.0).abs() < 1e-9,
        "forward weight must be updated from 0.5 to 1.0"
    );
    assert!(
        (rev.weight - 1.0).abs() < 1e-9,
        "reverse weight must be updated from 0.2 to 1.0"
    );
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "exactly 2 rows: both directions present"
    );
}
```

---

## T-NEW-03: `test_log_format_promoted_pairs_and_edges_inserted` (new — Group I)

**AC covered:** AC-05, FR-05, D2 (log format with `promoted_pairs`/`edges_inserted`/`edges_updated`).

```rust
/// AC-05, FR-05: tracing summary emits promoted_pairs, edges_inserted, edges_updated.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_log_format_promoted_pairs_and_edges_inserted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Two qualifying pairs, all fresh: 2 pairs × 2 directions = 4 inserts, 0 updates.
    seed_co_access(&store, 1, 2, 5).await;
    seed_co_access(&store, 3, 4, 4).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // Structured key-value fields in the tracing::info! record.
    assert!(
        logs_contain("promoted_pairs=2") || logs_contain("promoted_pairs: 2"),
        "log must contain promoted_pairs=2"
    );
    assert!(
        logs_contain("edges_inserted=4") || logs_contain("edges_inserted: 4"),
        "log must contain edges_inserted=4 (2 pairs × 2 directions)"
    );
    assert!(
        logs_contain("edges_updated=0") || logs_contain("edges_updated: 0"),
        "log must contain edges_updated=0 (all fresh inserts)"
    );
}
```

---

## R-06 Coverage Gap: `test_existing_edge_current_weight_no_update`

This test currently checks that an existing forward edge at the correct weight is not
re-updated. After crt-035, the tick also inserts the reverse edge. The test does not assert
the reverse was inserted.

**Recommendation:** Extend this test with:
```rust
assert!(
    fetch_co_access_edge(&store, 2, 1).await.is_some(),
    "reverse edge for (1,2) must be inserted even when forward weight is unchanged"
);
```

This is a coverage gap (not a breaking assertion), so it is acceptable as a follow-up
if not included in the initial delivery. The tester must flag it if not addressed.

---

## Tests NOT Requiring Modification (confirmed)

The following tests have no breaking assertions under crt-035. They must pass without
changes (any failure indicates a regression in the implementation):

| Test | Why unchanged |
|------|---------------|
| `test_inserted_edge_metadata_all_four_fields` | Checks field values on one edge, no total count |
| `test_existing_edge_current_weight_no_update` | Forward weight check only; no total count |
| `test_weight_delta_exactly_at_boundary_no_update` | Delta logic unchanged per direction |
| `test_sub_threshold_pair_not_gc` | `is_some()` assertion on forward edge only |
| `test_empty_co_access_table_noop_late_tick` | count == 0, still correct |
| `test_all_below_threshold_noop_late_tick` | count == 0, still correct |
| `test_early_tick_warn_when_qualifying_count_zero` | Log assertion only |
| `test_late_tick_no_warn_empty_table` | Log assertion only |
| `test_fully_promoted_table_no_warn` | Log assertion only; no count |
| `test_write_failure_mid_batch_warn_and_continue` | `is_some()` on two pairs; no total count |
| `test_write_failure_info_log_always_fires` | Log assertion only |
| `test_global_max_normalization_subquery_shape` | Weight assertions only |
| `test_global_max_outside_capped_batch` | Weight assertions only |
| `test_single_qualifying_pair_weight_one` | Weight assertion only; no count |
| `test_self_loop_pair_no_panic` | No-panic guard; zero qualifying |

---

## Acceptance Criteria Covered by This Plan

| AC-ID | Test(s) |
|-------|---------|
| AC-01 | T-BLR-01, T-BLR-02, T-BLR-03, T-BLR-04, T-BLR-05, T-BLR-06, T-BLR-07, T-BLR-08, T-NEW-01 |
| AC-02 | T-BLR-01, T-BLR-02, T-NEW-01 |
| AC-03 | T-BLR-08, T-NEW-02 |
| AC-04 | T-BLR-08, T-BLR-03 (second tick idempotent) |
| AC-05 | T-NEW-03 |
| AC-11 | All T-BLR updates + GATE-3B-01 + GATE-3B-02 |
| AC-13 | Existing cycle detection tests pass unchanged |
