# Test Plan: background-tick (unimatrix-server/src/background.rs)

Covers: GRAPH_EDGES orphaned-edge compaction (new step); tick sequence ordering
(compaction before rebuild, never concurrent); `TypedGraphState::rebuild` call from tick;
graph accessible after tick completes; write pool usage for compaction.

Risks addressed: R-04, R-11, AC-13, AC-14

---

## Tick Sequence Verification (R-04, AC-14, FR-24)

The mandatory tick sequence after crt-021 is:
1. `maintenance_tick()` (existing)
2. GRAPH_EDGES orphaned-edge compaction (NEW — direct `write_pool`)
3. VECTOR_MAP compaction (existing)
4. `TypedGraphState::rebuild()` (upgraded)
5. Contradiction scan (existing)

Steps 2, 3, 4 are strictly sequential. The tests below enforce ordering.

### `inspect_background_rs_no_concurrent_dispatch`
- Code review gate: confirm `background.rs` does NOT use `tokio::spawn` or `join!` or
  `futures::join!` across steps 2, 3, 4 of the tick
- Assert: each step `await`s sequentially — the compaction `.execute(...).await?` and
  `TypedGraphState::rebuild(...).await` calls are in sequential `await` chain, not spawned

---

## Integration Tests: Orphaned Edge Compaction (AC-14, R-11)

All integration tests use `#[tokio::test]`. Use `open_test_store` or equivalent to create a
`SqlxStore`. Seeded data is inserted via direct sqlx queries on the store's write_pool.

### `test_background_tick_compacts_orphaned_graph_edges`
- Arrange: open fresh store; insert entry with id=10; insert graph_edges row with
  `source_id=10, target_id=999` (999 does not exist in entries)
- Arrange: insert another graph_edges row `(source_id=10, target_id=10)` (valid — both exist)
- Act: trigger the background tick (or call the compaction step directly if exposed)
- Act: query `SELECT COUNT(*) FROM graph_edges WHERE target_id=999`
- Assert: count = 0 — orphaned row was deleted
- Act: query `SELECT COUNT(*) FROM graph_edges WHERE target_id=10`
- Assert: count = 1 — valid row is preserved

### `test_background_tick_compaction_handles_empty_graph_edges`
- Arrange: fresh store with no `graph_edges` rows
- Act: trigger compaction step
- Assert: completes without error; graph_edges is still empty (no panic on empty table)

### `test_background_tick_compaction_uses_write_pool`
- Structural assertion: compaction SQL is executed against `store.write_pool`
  (not via analytics queue)
- Code review: confirm the compaction DELETE in `background.rs` calls
  `sqlx::query(...).execute(&store.write_pool).await` (or equivalent direct write path)
- This ensures compaction is a bounded maintenance write, not shed-able

---

## Integration Tests: Compaction before Rebuild Ordering (R-04, AC-14)

### `test_background_tick_orphaned_edge_absent_from_rebuilt_graph`
- Arrange: open store; insert entry id=1; insert a `graph_edges` row
  `(source_id=1, target_id=999, relation_type='Supersedes', bootstrap_only=0)`
  where 999 is not in entries
- Act: trigger full tick (compaction + rebuild sequence)
- Act: acquire read lock on `TypedGraphStateHandle`
- Assert: `state.typed_graph` does NOT contain an edge with `target_id=999`
  (orphaned edge was compacted before rebuild — the graph was never built with the orphan)

### `test_background_tick_valid_edges_present_in_rebuilt_graph`
- Arrange: insert entries id=1 and id=2; insert graph_edges row
  `(source_id=1, target_id=2, relation_type='Supersedes', bootstrap_only=0)`
- Act: trigger full tick
- Assert: `state.typed_graph` has edge 1→2 of type Supersedes
- Assert: `state.use_fallback == false`

---

## Integration Tests: TypedGraphState Updated After Tick (AC-13)

### `test_background_tick_updates_typed_graph_state_handle`
- Arrange: create `TypedGraphStateHandle` via `TypedGraphState::new_handle()` (use_fallback=true)
- Arrange: open store; seed entries and graph_edges
- Act: run background tick with the handle and store
- Act: acquire read lock on handle
- Assert: `state.use_fallback == false`
- Assert: graph contains seeded edges

### `test_background_tick_cycle_detected_sets_fallback`
- Arrange: seed entries that form a Supersedes cycle (e.g., A.supersedes=B, B.supersedes=A)
- Act: run background tick
- Act: acquire read lock on handle
- Assert: `state.use_fallback == true` — cycle detection triggered fallback

---

## Tick Duration Regression (R-11, NF-09)

### `test_background_tick_compaction_completes_within_budget`
- Arrange: seed `graph_edges` with 1000 rows where 500 are orphaned (source_id not in entries)
- Act: measure time for the compaction DELETE query
- Assert: compaction completes in under 1 second for 1000 rows
  (baseline: DELETE with indexes on source_id/target_id should be fast for this scale)
- Note: this is a performance regression guard, not a correctness test.
  If the assertion is too tight for CI hardware, document the measured baseline and
  set the threshold at 10x baseline. Entry #1777 precedent: tick inflation must be caught
  before merging to main.

---

## Code Review Gates

The following assertions are enforced via code review / grep, not runtime tests:

1. **No `AnalyticsWrite` calls in compaction code**: `grep` for `AnalyticsWrite` in
   `background.rs` tick compaction step — must be zero (compaction is direct `write_pool`)

2. **Sequential await chain**: compaction `.await` must appear before `TypedGraphState::rebuild(...).await`
   in the same sequential flow — no `tokio::spawn` or `join!` wrapping either step

3. **Poison recovery on write lock**: the write lock acquisition for state swap must use
   `.unwrap_or_else(|e| e.into_inner())` — existing convention preserved through rename

---

## Test Module Location

Tests live in `crates/unimatrix-server/src/background.rs` `#[cfg(test)]` module,
or an integration test that wires together a `SqlxStore` + `TypedGraphStateHandle` +
tick trigger. Use `#[tokio::test]` for all async tests.

If the background tick function is not directly testable from a unit test module,
expose a test-only `run_tick_once(store: &Store, handle: &TypedGraphStateHandle)` helper
under `#[cfg(test)]` — do not add this to the public API.
