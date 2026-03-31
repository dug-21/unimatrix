# crt-035 Pseudocode: typed_graph.rs AC-12 PPR Regression Test

## Purpose

Add one `#[tokio::test]` to the existing `#[cfg(test)] mod tests` block in
`crates/unimatrix-server/src/services/typed_graph.rs`. This test verifies the full
GRAPH_EDGES → TypedGraphState::rebuild() → personalized_pagerank pipeline using a real
SQLite-backed store — confirming that a reverse CoAccess edge `(B → A)` written to
GRAPH_EDGES is read by `rebuild()` and produces a non-zero PPR score for entry A when
PPR is seeded at B.

## Placement

**File:** `crates/unimatrix-server/src/services/typed_graph.rs`

**Location:** Append to the existing `#[cfg(test)] mod tests { ... }` block, after
`test_rebuild_retains_deprecated_entries`. Do NOT create a new file. Do NOT move
existing tests. (Cumulative infrastructure rule, D3.)

## Why This Test Must Use SqlxStore

Per spec AC-12 (authoritative over the architecture doc's earlier in-memory description):
- An in-memory `TypedRelationGraph::new()` + manual edge insertion would bypass the
  `build_typed_relation_graph` read path from GRAPH_EDGES entirely.
- The defect was that edges written to GRAPH_EDGES were not reachable via PPR seeding
  the high-ID entry. The fix must be verified through the full read path.
- A synthetic in-memory fixture would pass even if the read path from GRAPH_EDGES was
  broken.

[GATE-3B-05: delivery agent must grep `typed_graph.rs` for
`test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` and confirm the test calls
`SqlxStore::open`, not `TypedRelationGraph::new()`.]

---

## Test Name

```
test_ppr_reverse_coaccess_edge_seeds_lower_id_entry
```

(This matches the spec AC-12 verification method and the IMPLEMENTATION-BRIEF.md D3 decision.)

---

## Imports Required

The test uses the same imports as `test_rebuild_excludes_quarantined_entries`:
```rust
use unimatrix_core::Store;
use unimatrix_store::{NewEntry, SqlxStore, Status};
```

Plus, for PPR:
```rust
use unimatrix_engine::graph::personalized_pagerank;
```

(`personalized_pagerank` is already imported in the file's outer scope via
`use unimatrix_engine::graph::{ ..., personalized_pagerank, ... }` but that
import is at the non-test module level. Within `mod tests`, a local `use` is safer.)

Also need:
```rust
use std::collections::HashMap;
```

---

## Algorithm

```
#[tokio::test]
async fn test_ppr_reverse_coaccess_edge_seeds_lower_id_entry():

  // Step 1: Open a real SqlxStore (tempfile-backed).
  // Pattern: identical to test_rebuild_excludes_quarantined_entries.
  dir = tempfile::TempDir::new().expect("tempdir")
  store = Arc::new(
    SqlxStore::open(
      &dir.path().join("test.db"),
      unimatrix_store::pool_config::PoolConfig::default(),
    )
    .await
    .expect("open store")
  )

  // Step 2: Insert two Active entries (IDs will be assigned by the store).
  // The store auto-increments IDs starting at 1, so the first insert → id=1 (entry A),
  // second insert → id=2 (entry B).
  // We need A < B for the test to match the CoAccess pair invariant.
  id_a = store
    .insert(NewEntry {
      title: "entry-a".to_string(),
      content: "content-a".to_string(),
      topic: "test".to_string(),
      category: "decision".to_string(),
      tags: vec![],
      source: "test".to_string(),
      status: Status::Active,
      created_by: "test".to_string(),
      feature_cycle: "crt-035".to_string(),
      trust_source: "agent".to_string(),
    })
    .await
    .expect("insert entry A")
  // id_a is typically 1

  id_b = store
    .insert(NewEntry {
      title: "entry-b".to_string(),
      content: "content-b".to_string(),
      topic: "test".to_string(),
      category: "decision".to_string(),
      tags: vec![],
      source: "test".to_string(),
      status: Status::Active,
      created_by: "test".to_string(),
      feature_cycle: "crt-035".to_string(),
      trust_source: "agent".to_string(),
    })
    .await
    .expect("insert entry B")
  // id_b is typically 2; id_a < id_b guaranteed by insertion order

  // Defensive assertion: confirm ordering matches test intent.
  assert!(id_a < id_b, "id_a must be less than id_b (test invariant)")

  // Step 3: Insert a CoAccess edge (B → A) directly into GRAPH_EDGES.
  // This simulates what the tick writes as the reverse direction for pair (A, B).
  // Direct SQL insert — not through any store API — because no public API exists
  // for writing CoAccess edges (tick uses write_pool_server() directly, pattern #3883).
  //
  // Fields match the tick's INSERT:
  //   relation_type = 'CoAccess'
  //   created_by = 'tick'          (simulating the tick path)
  //   source = 'co_access'         (EDGE_SOURCE_CO_ACCESS)
  //   bootstrap_only = 0           (included in build_typed_relation_graph reads)
  //   weight = 1.0                 (maximum weight; ensures PPR boost is large)
  sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)"
  )
  .bind(id_b as i64)  // ?1: source_id = B (the high-ID entry)
  .bind(id_a as i64)  // ?2: target_id = A (the low-ID entry)
  .execute(store.write_pool_server())
  .await
  .expect("insert reverse CoAccess edge B→A")

  // Step 4: Call TypedGraphState::rebuild() on the store.
  // This reads ALL bootstrap_only=0 edges from GRAPH_EDGES, including the one just inserted.
  store_ref: &Store = &*store
  state = TypedGraphState::rebuild(store_ref)
    .await
    .expect("rebuild must succeed")

  // Step 5: Call personalized_pagerank seeded at B.
  // Seed map: { id_b: 1.0 } — 100% seed weight on B.
  // PPR parameters: alpha=0.85, iterations=10 (same defaults used in search.rs; see
  //   config.ppr_alpha and config.ppr_iterations. Use constants directly for the test.)
  //
  // PPR traverses Direction::Outgoing. The graph has edge B→A (outgoing from B).
  // Mass from B flows along the B→A edge to A.
  seed_scores: HashMap<u64, f64> = HashMap::new()
  seed_scores.insert(id_b, 1.0)

  ppr_scores = personalized_pagerank(
    &state.typed_graph,
    &seed_scores,
    0.85,  // alpha (damping factor)
    10,    // iterations
  )

  // Step 6: Assert that entry A has a non-zero PPR score.
  //
  // With one outgoing edge B→A and weight=1.0, PPR mass flows from B to A.
  // The exact value depends on alpha and iteration count; we only assert > 0.0.
  // This verifies the defect regression: before crt-035, seeding B returned A's
  // score as 0.0 or absent (no outgoing path from B to A).
  score_for_a = ppr_scores.get(&id_a).copied().unwrap_or(0.0)
  assert!(
    score_for_a > 0.0,
    "PPR seeded at B must produce a non-zero score for A via the reverse CoAccess edge (AC-12). \
     Got score_for_a={score_for_a}. This indicates the reverse edge B→A was not read by rebuild()."
  )

END TEST
```

---

## Integration Points Exercised

| Integration Point | Exercised By |
|------------------|-------------|
| `SqlxStore::open` | Step 1 |
| `Store::insert` (NewEntry) | Step 2 |
| `write_pool_server()` for direct SQL insert | Step 3 |
| `GRAPH_EDGES` UNIQUE constraint (INSERT OR IGNORE) | Step 3 |
| `TypedGraphState::rebuild()` reads GRAPH_EDGES | Step 4 |
| `build_typed_relation_graph` CoAccess edge inclusion | Step 4 (internal to rebuild) |
| `personalized_pagerank` Direction::Outgoing traversal | Step 5 |
| PPR score non-zero for A when B seeded | Step 6 (the defect regression assertion) |

---

## What This Test Does NOT Cover

- The tick writing the reverse edge (that is covered by T-NEW-01 through T-NEW-03 in
  `co_access_promotion_tick_tests.rs`).
- The migration back-filling the reverse edge (that is covered by MIG-U-03 and MIG-U-04
  in `migration_v18_to_v19.rs`).
- PPR score magnitude or ranking accuracy (out of scope for AC-12; only non-zero is required).
- Cycle detection — CoAccess edges are excluded from the Supersedes-only cycle subgraph;
  this test introduces no Supersedes edge, so cycle detection is not triggered.

---

## Error Handling

All `.await.expect("...")` calls are appropriate in tests — a failure means the test
infrastructure is broken, not the feature logic. The test is not infallible; it fails
fast if setup steps fail, which is the correct behavior for a unit test.

---

## Key Test Scenario Alignment

| Spec Reference | Test Step |
|---------------|-----------|
| AC-12 step 1 — real SqlxStore | Step 1 |
| AC-12 step 2 — two entries, A < B | Step 2 |
| AC-12 step 3 — insert reverse CoAccess B→A | Step 3 |
| AC-12 step 4 — TypedGraphState::rebuild() | Step 4 |
| AC-12 step 5 — personalized_pagerank seeded at B | Step 5 |
| AC-12 step 6 — A has non-zero score | Step 6 |
| R-07 (not in-memory fixture) | Confirmed: SqlxStore::open used, not TypedRelationGraph::new() |
| GATE-3B-05 (grep for SqlxStore) | Confirmed by Step 1 and the SqlxStore::open call |

---

## Knowledge Stewardship

- Pattern #3730 (unimatrix-engine): new graph traversal functions live in dedicated submodules.
  This test exercises the existing `personalized_pagerank` from `graph_ppr.rs` — no new
  submodule needed.
- Pattern `test_rebuild_excludes_quarantined_entries`: established SqlxStore + TempDir test
  fixture pattern in this file — followed exactly for Step 1/2.
- ADR-001 (crt-035, #3890): confirms the reverse edge is the data fix; PPR code is unchanged.
- Deviations from established patterns: none. Test structure is a direct copy of the
  quarantine exclusion test with a different setup (GRAPH_EDGES insert) and assertion (PPR score).
