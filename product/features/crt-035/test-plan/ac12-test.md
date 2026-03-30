# Test Plan: AC-12 SqlxStore PPR Regression Test (crt-035)

**File:** `crates/unimatrix-server/src/services/typed_graph.rs`
**Block:** Extend existing `#[cfg(test)] mod tests` — do NOT create a new test file.

---

## Scope

One new `#[tokio::test]` that verifies the full
`GRAPH_EDGES → TypedGraphState::rebuild() → PPR` pipeline with a reverse CoAccess edge.

This test addresses:
- AC-12 (PPR seeding high-ID entry surfaces low-ID entry via reverse CoAccess edge).
- R-07 (AC-12 must use real SqlxStore, not a synthetic in-memory TypedRelationGraph).
- D3 (placement in typed_graph.rs test block, not graph_ppr_tests.rs).

---

## Why This Test Exists

crt-034 (ADR-006) wrote only forward CoAccess edges `(a→b)`. PPR uses
`Direction::Outgoing`. Seeding the higher-ID entry B found no outgoing path to A through
CoAccess — half of co-access signals were invisible to PPR when entry B was the query
anchor. crt-035 fixes this by writing the reverse edge `(b→a)`. This test confirms the
fix end-to-end: the reverse edge written by the tick (or back-fill) is readable by
`TypedGraphState::rebuild()` and produces a non-zero PPR score for A when seeded at B.

---

## Test Name and Placement

- **Function name:** `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry`
- **Placement:** Inside the existing `#[cfg(test)] mod tests` block in `typed_graph.rs`.
  The established pattern is `test_rebuild_excludes_quarantined_entries` — uses
  `SqlxStore` + `tempfile::TempDir`. Follow this pattern exactly.
- **Decorator:** `#[tokio::test]`

---

## Test Structure (Arrange / Act / Assert)

### Arrange

1. Open a real `SqlxStore` using `tempfile::TempDir`:
   ```rust
   let tmp = tempfile::TempDir::new().unwrap();
   let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
   ```

2. Insert two entries (A < B) using the store's write API or a direct SQL insert
   that satisfies all NOT NULL constraints:
   - Entry A: `id=1`, `title='entry-a'`, `status=Active`.
   - Entry B: `id=2`, `title='entry-b'`, `status=Active`.

   Use the existing `store_entry` or SQL insert pattern already present in the
   typed_graph.rs test block. Check the neighboring test (`test_rebuild_excludes_quarantined_entries`)
   for the exact insert helper call.

3. Insert a reverse CoAccess edge `(B→A)` directly into GRAPH_EDGES:
   ```rust
   sqlx::query(
       "INSERT INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
        VALUES (2, 1, 'CoAccess', 1.0, 0, 'tick', 'co_access', 0)"
   )
   .execute(store.write_pool_server())
   .await
   .unwrap();
   ```
   This directly simulates what `run_co_access_promotion_tick` writes as the reverse
   direction, bypassing the tick to isolate the graph/PPR layer.

### Act

4. Call `TypedGraphState::rebuild(&store)`:
   ```rust
   let graph_state = TypedGraphState::rebuild(&store).await.unwrap();
   ```

5. Build the seed map with entry B (id=2) as the seed:
   ```rust
   let mut seeds = std::collections::HashMap::new();
   seeds.insert(2u64, 1.0f64);
   ```

6. Call `personalized_pagerank` on the graph state:
   ```rust
   let scores = graph_state.run_ppr(&seeds, 0.85, 20);
   ```
   (Use the exact call signature present in other tests in this file.)

### Assert

7. Entry A (id=1) must have a non-zero PPR score:
   ```rust
   let score_a = scores.get(&1u64).copied().unwrap_or(0.0);
   assert!(
       score_a > 0.0,
       "entry A (id=1) must have non-zero PPR score when seeded at B (id=2) via reverse CoAccess edge"
   );
   ```

8. Entry B may have a self-score (depends on damping) — this is incidental and does
   not need to be asserted.

---

## Critical Design Constraint (GATE-3B-04)

The test must open a `SqlxStore` (via `open_test_store` or `SqlxStore::open`) and insert
the edge via SQL, then call `TypedGraphState::rebuild`. It must NOT build a
`TypedRelationGraph` directly in memory and bypass the store read path.

**Rationale:** The risk (R-07) is that an in-memory shortcut skips
`build_typed_relation_graph`, which reads `GRAPH_EDGES` using the `bootstrap_only` filter
and the `source_id/target_id` column mapping. Using a real store confirms the full
GRAPH_EDGES → graph read path works with the reverse edge layout.

**GATE-3B-04 grep (delivery validation):**
```bash
grep -n 'SqlxStore\|open_test_store' \
  crates/unimatrix-server/src/services/typed_graph.rs | grep -A2 test_ppr_reverse
```
Must show the test function uses a real store fixture.

---

## Assumptions / Pre-conditions

- The `TypedGraphState` module must expose a way to run PPR from the test. The existing
  pattern in the test block (used by other tests) is the authoritative reference.
- `bootstrap_only = 0` on the inserted edge ensures `build_typed_relation_graph` includes
  it (the reader filters out `bootstrap_only = 1` rows).
- `relation_type = 'CoAccess'` is included in `build_typed_relation_graph` traversal
  (cycle detection uses Supersedes-only; CoAccess is not excluded from the PPR graph).
- Entry IDs used in the test (1, 2) must not conflict with any auto-seeded entries in
  `open_test_store`. If the store starts with a non-zero `next_entry_id`, the test should
  use the store's context_store API to get real IDs rather than hardcoding 1 and 2.
  Check the neighboring tests in the block to confirm the safe pattern.

---

## Acceptance Criteria Covered

| AC-ID | Coverage |
|-------|---------|
| AC-12 | Full — SqlxStore + TypedGraphState::rebuild + PPR end-to-end |
| R-07 | Full — real store (GATE-3B-04), not synthetic fixture |
| D3 | Full — placed in typed_graph.rs test block, not graph_ppr_tests.rs |
