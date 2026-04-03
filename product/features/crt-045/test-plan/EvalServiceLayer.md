# Test Plan: EvalServiceLayer (eval/profile/layer.rs)

## Component Summary

`EvalServiceLayer::from_profile()` is the sole construction entry point for eval service
layers. crt-045 adds:
1. A `TypedGraphState::rebuild()` call after `SqlxStore` open (Step 5b)
2. A post-construction write-back into the shared `Arc<RwLock<TypedGraphState>>` handle (Step 13b)
3. A `pub(crate) typed_graph_handle()` accessor delegating to `self.inner.typed_graph_handle()`

The primary risk is the **wired-but-unused anti-pattern** (entry #1495): the handle can
hold a rebuilt state that `SearchService` never observes.

---

## Unit Test Expectations

Unit tests live in `#[cfg(test)]` mod inside `typed_graph.rs` (existing — no new file).
The `EvalServiceLayer` itself has no pure-unit-testable logic because `from_profile()` is
async and opens a real store. All `EvalServiceLayer` tests are in-process integration tests
in `layer_tests.rs`.

**Existing unit tests that already cover component primitives** (must not regress):

| Test | Location | What It Covers |
|------|----------|---------------|
| `test_arc_clone_shares_state` | `typed_graph.rs::tests` | Write through one Arc clone is visible from another — proves R-01 mechanism |
| `test_typed_graph_state_handle_write_lock_swap` | `typed_graph.rs::tests` | `*guard = new_state` swap pattern used in Step 13b |
| `test_new_handle_write_then_read` | `typed_graph.rs::tests` | Write lock acquisition idiom |
| `test_typed_graph_state_handle_poison_recovery` | `typed_graph.rs::tests` | `.unwrap_or_else(|e| e.into_inner())` pattern |
| `test_typed_graph_state_new_handle_sets_use_fallback_true` | `typed_graph.rs::tests` | Cold-start state before Step 13b runs |

**New unit test expectation — accessor visibility (R-08, ADR-004):**

No runtime test. The Rust compiler enforces `pub(crate)` visibility. The delivery agent
must confirm the accessor declaration is `pub(crate)`, not `pub`. The PR reviewer must
verify this before approving the merge.

Assert (code review, not runtime):
```rust
// MUST compile from within unimatrix-server crate
pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle {
    self.inner.typed_graph_handle()
}

// MUST NOT compile from outside unimatrix-server
// (Rust compile error enforces this — no runtime test needed)
```

---

## Integration Test Expectations

All integration tests for `EvalServiceLayer` live in
`crates/unimatrix-server/src/eval/profile/layer_tests.rs`.

### Test 1: `test_from_profile_typed_graph_rebuilt_after_construction` (NEW — AC-06, R-01, R-02, R-03)

**Purpose:** Three-layer assertion proving the rebuilt `TypedGraphState` is (1) present in
the handle, (2) structurally valid (graph connectivity), and (3) observed by `SearchService`
at query time. This guards against the wired-but-unused anti-pattern.

**Arrange:**
- Open a `SqlxStore` in a temp directory via `SqlxStore::open()` (full migrations).
- Insert exactly two `Active` (not Quarantined, not Deprecated) entries via `store.insert()`.
  - Entry A: category `"decision"`, topic `"test"`, status `Active`
  - Entry B: category `"decision"`, topic `"test"`, status `Active`
- Insert one CoAccess or Supports (S1/S2/S8) graph edge between them via raw SQL:
  ```sql
  INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
  VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)
  ```
  Use `store.write_pool_server()` as the executor (pattern from `test_reverse_coaccess_high_id_to_low_id_ppr_regression`).
- Dump a `VectorIndex` into the sibling `vector/` directory (pattern from `test_from_profile_loads_vector_index_from_snapshot_dir`).
- Build an `EvalProfile` with `ppr_expander_enabled: false` (baseline-like) and `distribution_change: false`.

**Act:**
```rust
let result = EvalServiceLayer::from_profile(&snap_path, &profile, Some(temp_dir.path())).await;
```

**Assert — Layer 1: Handle state (AC-01, AC-06)**
```rust
let layer = result.expect("from_profile must succeed on seeded snapshot");
let handle = layer.typed_graph_handle();
let guard = handle.read().unwrap_or_else(|e| e.into_inner());
assert!(!guard.use_fallback, "use_fallback must be false after rebuild");
assert!(guard.all_entries.len() >= 2, "all_entries must contain at least the two seeded Active entries");
```

**Assert — Layer 2: Graph connectivity (R-02, ADR-003)**
```rust
// Verify the graph has the seeded entries as nodes (not just use_fallback flipped)
// If find_terminal_active is pub(crate) accessible:
let terminal = unimatrix_engine::graph::find_terminal_active(
    id_a, &guard.typed_graph, &guard.all_entries
);
assert_eq!(terminal, Some(id_a), "Active entry must be reachable via find_terminal_active");
// Fallback if find_terminal_active is not accessible: assert node count directly
// assert!(guard.typed_graph is non-empty via node count or edge count)
```

**Assert — Layer 3: Live search returns Ok (R-01, R-02, SR-05, ADR-003)**
```rust
// Drop the read guard before the search call to avoid deadlock
drop(guard);

use crate::services::search::ServiceSearchParams;
let params = ServiceSearchParams {
    query_embedding: vec![0.0f32; /* dim */],
    k: 3,
    // ... other fields at their default/minimal values
};
let search_result = layer.inner.search.search(params).await;
assert!(
    search_result.is_ok(),
    "search must return Ok(_) when use_fallback=false; embedding model unavailable in CI is acceptable"
);
```

**Existing test pattern to mirror:** `test_from_profile_loads_vector_index_from_snapshot_dir`
for store setup and VectorIndex dump. `test_reverse_coaccess_high_id_to_low_id_ppr_regression`
for raw SQL edge insertion.

**IR-04 note:** If `find_terminal_active` is not `pub(crate)` in `typed_graph.rs`, use
direct graph node/edge count assertions instead. Do NOT add a visibility change to
`typed_graph.rs` without a scope variance flag.

---

### Test 2: `test_from_profile_returns_ok_on_cycle_error` (NEW — AC-05, R-04)

**Purpose:** Prove degraded mode — rebuild failure on cycle-producing edge set returns
`Ok(layer)` with `use_fallback == true`, never aborts `from_profile()`.

**Arrange:**
- Open a `SqlxStore` with two `Active` entries (A and B).
- Insert a Supersedes cycle via raw SQL:
  ```sql
  -- A supersedes B and B supersedes A: cycle
  INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, ...)
  VALUES (id_a, id_b, 'Supersedes', 1.0, ...)
  
  INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, ...)
  VALUES (id_b, id_a, 'Supersedes', 1.0, ...)
  ```
- Dump a `VectorIndex` (same as above — required by `from_profile()`).
- Build a baseline `EvalProfile`.

**Act:**
```rust
let result = EvalServiceLayer::from_profile(&snap_path, &profile, Some(temp_dir.path())).await;
```

**Assert:**
```rust
// from_profile() must not abort on cycle — degraded mode (AC-05, FR-03)
let layer = result.expect("from_profile must return Ok even on rebuild cycle error");
let handle = layer.typed_graph_handle();
let guard = handle.read().unwrap_or_else(|e| e.into_inner());
assert!(
    guard.use_fallback,
    "use_fallback must remain true when rebuild fails due to cycle"
);
```

**ADR-002 note:** Success log at `info!` on rebuild; cycle failure log at `warn!`. Tracing
subscriber not required in test — the `Ok(layer)` assertion is the non-negotiable check.
If a test tracing subscriber is available via `tracing_test` or similar, optionally assert
that a `warn` event was emitted.

---

### Test 3: Regression guard — existing tests pass unchanged (R-06, AC-08)

**No new tests needed.** The following pre-existing tests in `layer_tests.rs` must all
continue to pass after the crt-045 change:

| Test | What It Covers |
|------|---------------|
| `test_from_profile_analytics_mode_is_suppressed` | Analytics mode unaffected by graph rebuild |
| `test_from_profile_returns_live_db_path_error_for_same_path` | Live DB guard unaffected |
| `test_from_profile_snapshot_does_not_exist_returns_io_error` | Missing snapshot still returns Io error |
| `test_from_profile_invalid_weights_returns_config_invariant` | Confidence weight validation unaffected |
| `test_from_profile_loads_vector_index_from_snapshot_dir` | VectorIndex loading unaffected |
| `test_from_profile_nli_disabled_no_nli_handle` | NLI wiring unaffected |
| `test_from_profile_nli_enabled_has_nli_handle` | NLI wiring unaffected |
| `test_from_profile_invalid_nli_model_name_returns_config_invariant` | NLI model validation unaffected |
| `test_from_profile_valid_weights_passes_validation` | Valid weights still accepted |

**Regression signal:** If any of these tests begin returning `EvalError::ConfigInvariant`
or a new error variant after the crt-045 change, the fix has unintentionally altered
`from_profile()` beyond the intended scope.

---

## Edge Cases to Assert

| Edge Case | Expected Behavior | Test Coverage |
|-----------|------------------|---------------|
| EC-01: Empty snapshot (zero entries, zero edges) | `from_profile()` returns `Ok(layer)` with `use_fallback=false` but `typed_graph` is empty | Covered by existing tests that use `make_snapshot_db()` (empty store) |
| EC-02: Snapshot with entries but no edges | `use_fallback=false`, `typed_graph` has nodes but zero edges; search returns `Ok(_)` | Incidentally covered by existing tests |
| EC-04: Pre-crt-021 snapshot missing `GRAPH_EDGES` table | `StoreError::*` from `query_graph_edges()` → degraded mode `use_fallback=true` | Difficult to test without schema rollback; accepted as residual risk (IR-02) |
| C-09: Snapshot with only Quarantined entries | `use_fallback=false` but `typed_graph` is empty; `non-empty` assertion would fail (correct behavior — vacuous pass guard) | Covered by C-09 fixture requirement in new test |

---

## Specific Assertions Summary

| Assertion | Method | Guards |
|-----------|--------|--------|
| `!guard.use_fallback` | `assert!(!guard.use_fallback)` | R-01, AC-01, AC-06 |
| `guard.all_entries.len() >= 2` | `assert!(guard.all_entries.len() >= 2)` | R-03, C-09 |
| `find_terminal_active(id_a, ...) == Some(id_a)` | `assert_eq!(terminal, Some(id_a))` | R-02, ADR-003 layer 2 |
| `search(params).await.is_ok()` | `assert!(search_result.is_ok())` | R-01, R-02, SR-05 |
| `result.is_ok()` on cycle error | `result.expect(...)` | R-04, AC-05 |
| `guard.use_fallback == true` after cycle | `assert!(guard.use_fallback)` | R-04, AC-05 |
| `pub(crate)` visibility of `typed_graph_handle()` | Code review | R-08, ADR-004 |

---

## Constraints Checklist (delivery verification)

- [ ] `rebuild()` called with `.await` directly from `from_profile()` body — no `spawn_blocking` (C-01)
- [ ] Rebuild error path: `tracing::warn!` + `rebuilt_state = None` + `Ok(layer)` returned (C-02)
- [ ] `with_rate_config()` signature unchanged (C-03)
- [ ] `typed_graph_handle()` accessor declared `pub(crate)` on `EvalServiceLayer` (C-04)
- [ ] `typed_graph_handle()` delegates to `self.inner.typed_graph_handle()` — no new state (C-08)
- [ ] `#[cfg(test)]` guard NOT applied to `typed_graph_handle()` — also available to `runner.rs` (C-10, ADR-004)
