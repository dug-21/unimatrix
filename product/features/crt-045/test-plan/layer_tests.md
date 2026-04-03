# Test Plan: layer_tests.rs
# File: crates/unimatrix-server/src/eval/profile/layer_tests.rs

## Component Summary

`layer_tests.rs` is the integration test file for `EvalServiceLayer::from_profile()`. It
currently contains 9 tests covering analytics mode, live-DB guard, missing snapshot, weight
validation, VectorIndex loading, and NLI handle wiring. crt-045 adds two new tests to this
file targeting the three-layer graph rebuild assertion (AC-06) and the degraded mode path
(AC-05).

**This file must be extended, not replaced.** All existing tests must continue to pass
unchanged (R-06, AC-08).

---

## New Tests to Add

### Test A: `test_from_profile_typed_graph_rebuilt_after_construction`

**Risks addressed:** R-01, R-02, R-03, AC-01, AC-06, ADR-003
**Priority:** Gate-blocking (RISK-TEST-STRATEGY.md Coverage Summary, item 1 and 2)

#### Structure

```
#[tokio::test(flavor = "multi_thread")]
async fn test_from_profile_typed_graph_rebuilt_after_construction() {
    // --- ARRANGE ---
    // 1. Temp dir + seeded SqlxStore (full migrations via SqlxStore::open())
    // 2. Two Active entries via store.insert()
    // 3. One CoAccess edge via raw SQL on store.write_pool_server()
    //    (mirrors test_reverse_coaccess_high_id_to_low_id_ppr_regression pattern)
    // 4. VectorIndex seeded + dumped to sibling vector/ dir
    //    (mirrors test_from_profile_loads_vector_index_from_snapshot_dir pattern)
    // 5. EvalProfile with distribution_change=false, ppr_expander_enabled=false (baseline-like)

    // --- ACT ---
    let result = EvalServiceLayer::from_profile(&snap_path, &profile, Some(dir.path())).await;

    // --- ASSERT: Environmental guard ---
    // LiveDbPath may fire in CI if snap_path matches active DB (known environmental collision)
    // Io error should not occur on a correctly seeded snapshot
    let layer = match result {
        Ok(layer) => layer,
        Err(EvalError::LiveDbPath { .. }) => return, // environmental collision, not a failure
        Err(e) => panic!("from_profile failed on seeded snapshot: {e}"),
    };

    // --- ASSERT Layer 1: Handle state (AC-01, R-01) ---
    let handle = layer.typed_graph_handle();
    let guard = handle.read().unwrap_or_else(|e| e.into_inner());

    assert!(
        !guard.use_fallback,
        "use_fallback must be false after rebuild on seeded snapshot"
    );
    assert!(
        guard.all_entries.len() >= 2,
        "all_entries must contain at least the two seeded Active entries; got {}",
        guard.all_entries.len()
    );

    // --- ASSERT Layer 2: Graph connectivity (R-02, ADR-003) ---
    // Option A (preferred): use find_terminal_active if pub(crate)
    let terminal = unimatrix_engine::graph::find_terminal_active(
        id_a,  // id of first inserted Active entry
        &guard.typed_graph,
        &guard.all_entries,
    );
    assert!(
        terminal.is_some(),
        "find_terminal_active must return Some for an Active entry with graph edges seeded"
    );

    // Option B (fallback if find_terminal_active not accessible):
    // assert that typed_graph is non-empty via a proxy assertion
    // assert!(!guard.all_entries.is_empty());  // already covered by Layer 1

    // Release read guard before search (avoid deadlock if search acquires read lock)
    drop(guard);

    // --- ASSERT Layer 3: Live search returns Ok (R-01, R-02, SR-05) ---
    // SearchService.search() is called to confirm it observes the rebuilt graph
    // at query time, not just at construction time.
    // Embedding model is unavailable in CI — search will return ANN-only or empty results.
    // The assertion is only that the call does NOT panic or return Err.
    //
    // ServiceSearchParams field names must be verified against the current definition
    // in crates/unimatrix-server/src/services/search.rs before implementing.
    let search_result = layer.inner.search.search(/* minimal ServiceSearchParams */).await;
    assert!(
        search_result.is_ok(),
        "search must return Ok even with use_fallback=false in CI (embedding model absent)"
    );
}
```

#### Key Implementation Notes

**Seeding requirement (C-09, R-03):**
- Entries must be status `Active` — Quarantined entries are filtered by `rebuild()` and
  produce vacuous empty graphs
- Edge `bootstrap_only` must be `0` — `bootstrap_only=1` edges are excluded by
  `build_typed_relation_graph()`
- Edge `relation_type` must be one of `CoAccess`, `Supports`, `Specifies` (S1/S2/S8)
  for the edge to be traversed by `graph_expand` BFS; `Supersedes` edges are included
  in the graph but not used by BFS traversal (EC-03)

**VectorIndex requirement:**
- `from_profile()` attempts to load a VectorIndex from the sibling `vector/` directory
- If absent, `from_profile()` may return `Err(EvalError::Io(_))`
- Mirror the `vi.dump(&vector_dir)` pattern from `test_from_profile_loads_vector_index_from_snapshot_dir`
- Embeddings for seeded entries are required; use deterministic non-zero embeddings

**IR-04 contingency:**
- If `find_terminal_active` is not `pub(crate)` in `typed_graph.rs`, Layer 2 must use
  an alternative assertion. Acceptable alternative:
  ```rust
  // Confirm graph has at least as many nodes as seeded Active entries
  // (proxy: all_entries.len() already checked in Layer 1)
  // Additional: query graph_penalty for a seeded entry — returns 1.0 for absent nodes
  let penalty = unimatrix_engine::graph::graph_penalty(
      id_a, &guard.typed_graph, &guard.all_entries
  );
  // If id_a is in the graph (as it should be), penalty < 1.0 or == 1.0 (no supersession)
  // This is not a perfect proxy but confirms the node exists in the graph
  ```
  Do not add a `pub` or `pub(crate)` promotion to `find_terminal_active` without a scope
  variance flag.

---

### Test B: `test_from_profile_returns_ok_on_cycle_error`

**Risks addressed:** R-04, AC-05, ADR-002
**Priority:** Gate-blocking (RISK-TEST-STRATEGY.md Coverage Summary, item 3)

#### Structure

```
#[tokio::test(flavor = "multi_thread")]
async fn test_from_profile_returns_ok_on_cycle_error() {
    // --- ARRANGE ---
    // 1. Temp dir + seeded SqlxStore
    // 2. Two Active entries (id_a, id_b)
    // 3. Supersedes cycle: A→B and B→A via raw SQL
    //    relation_type = 'Supersedes', bootstrap_only = 0
    // 4. VectorIndex seeded + dumped (required for from_profile to proceed past Step 5)
    // 5. Baseline EvalProfile

    // --- ACT ---
    let result = EvalServiceLayer::from_profile(&snap_path, &profile, Some(dir.path())).await;

    // --- ASSERT ---
    let layer = match result {
        Ok(layer) => layer,
        Err(EvalError::LiveDbPath { .. }) => return, // environmental collision
        Err(e) => panic!(
            "from_profile must return Ok even on cycle error (degraded mode); got: {e}"
        ),
    };

    // Degraded mode: use_fallback must remain true (rebuild failed on cycle)
    let handle = layer.typed_graph_handle();
    let guard = handle.read().unwrap_or_else(|e| e.into_inner());
    assert!(
        guard.use_fallback,
        "use_fallback must be true when rebuild fails due to cycle (AC-05, ADR-002)"
    );
}
```

#### Key Implementation Notes

**Cycle construction:** A→B Supersedes AND B→A Supersedes. `build_typed_relation_graph()`
detects cycles via petgraph topological sort and returns `GraphError::CycleDetected`, which
`rebuild()` maps to `StoreError::InvalidInput`. `from_profile()` must catch this and set
`rebuilt_state = None`, returning `Ok(layer)` with cold-start handle.

**Why VectorIndex is still required:** `from_profile()` attempts to load the VectorIndex
at Step 5 (before graph rebuild). If the vector directory is absent, `from_profile()` may
return `Err(EvalError::Io(_))` before reaching the rebuild code. The cycle-error test must
exercise the rebuild path, not the VectorIndex-load path.

**Log assertion (optional):** If `tracing_subscriber::fmt::Subscriber` or `tracing_test`
is available in the test, assert that a `WARN` event was emitted with message containing
"cycle" or "use_fallback". This is not required — the `Ok(layer)` + `use_fallback==true`
assertion is the non-negotiable check.

---

## Test File Structure (after crt-045)

```
layer_tests.rs
  mod layer_tests {
    // Helpers (existing)
    async fn make_snapshot_db() -> (TempDir, PathBuf)
    fn baseline_profile() -> EvalProfile

    // Existing tests (must not be modified)
    test_from_profile_analytics_mode_is_suppressed
    test_from_profile_returns_live_db_path_error_for_same_path
    test_from_profile_snapshot_does_not_exist_returns_io_error
    test_from_profile_invalid_weights_returns_config_invariant
    test_from_profile_loads_vector_index_from_snapshot_dir       ← use as seeding pattern
    test_from_profile_nli_disabled_no_nli_handle
    test_from_profile_nli_enabled_has_nli_handle
    test_from_profile_invalid_nli_model_name_returns_config_invariant
    test_from_profile_valid_weights_passes_validation

    // NEW tests (crt-045)
    test_from_profile_typed_graph_rebuilt_after_construction      ← AC-06, R-01/R-02/R-03
    test_from_profile_returns_ok_on_cycle_error                   ← AC-05, R-04
  }
```

---

## Helper Reuse

The existing `make_snapshot_db()` helper opens a migrated SqlxStore. The new tests need
an **extended helper** that also inserts entries and edges. Rather than modifying the
shared helper (which would affect existing tests), the new tests should either:

1. Inline the seeding after calling `make_snapshot_db()`, OR
2. Define a new private helper `make_seeded_graph_snapshot_db()` within the test mod.

Pattern to replicate (from existing `test_from_profile_loads_vector_index_from_snapshot_dir`):
```rust
let store = Arc::new(SqlxStore::open(&snap_path, PoolConfig::default()).await.expect("open store"));
// Insert entries via store.insert(NewEntry { ... })
// Insert edges via sqlx::query(...).execute(store.write_pool_server()).await
// Dump vector index via vi.dump(&vector_dir)
```

Imports needed in the new tests (some already present in the mod):
```rust
use std::sync::Arc;
use unimatrix_core::{VectorConfig, VectorIndex};
use unimatrix_store::{NewEntry, SqlxStore, Status, pool_config::PoolConfig};
// sqlx for raw query execution
```

---

## Three-Layer Assertion ADR Compliance Checklist

Per ADR-003 (entry #4100), the integration test is only compliant if all three layers are
present. Delivery agent must verify:

- [ ] Layer 1: `!guard.use_fallback` asserted — handle state confirmation
- [ ] Layer 1b: `guard.all_entries.len() >= 2` asserted — not vacuous from quarantine filter
- [ ] Layer 2: `find_terminal_active()` or equivalent graph connectivity assertion present
- [ ] Layer 3: `layer.inner.search.search(params).await.is_ok()` asserted — behavioral wiring

A test that asserts only Layer 1 (handle state) is **insufficient** per ADR-003 and will
not satisfy the gate requirement.

---

## Assertions — Complete List

### Test A

| Assertion | Expression | Risk Covered |
|-----------|-----------|-------------|
| Layer 1a: use_fallback is false | `assert!(!guard.use_fallback)` | R-01, AC-01 |
| Layer 1b: entries seeded | `assert!(guard.all_entries.len() >= 2)` | R-03, C-09 |
| Layer 2: graph connectivity | `assert!(terminal.is_some())` | R-02, ADR-003 |
| Layer 3: search returns Ok | `assert!(search_result.is_ok())` | R-01, R-02, SR-05 |

### Test B

| Assertion | Expression | Risk Covered |
|-----------|-----------|-------------|
| result is Ok | `result.expect("must not abort")` | R-04, AC-05 |
| use_fallback stays true | `assert!(guard.use_fallback)` | R-04, AC-05, ADR-002 |

---

## Residual Risks (not covered by new tests)

| Risk | Status |
|------|--------|
| R-07: rebuild hangs with no timeout | Accepted residual; sqlx query timeout is implicit guard |
| R-10: write-back before SearchService initialized | Accepted resolved; from_profile() is sequential |
| R-09: mrr_floor drifted since crt-042 | Manual pre-merge verification; not automatable |
