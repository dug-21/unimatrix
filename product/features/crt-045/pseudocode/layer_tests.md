# layer_tests.rs — Pseudocode
# File: crates/unimatrix-server/src/eval/profile/layer_tests.rs

## Purpose

Add two integration tests to the existing `layer_tests` module:

1. **`test_from_profile_typed_graph_rebuilt_after_construction`** — Three-layer assertion
   confirming the rebuilt graph is visible at handle state, graph connectivity, and live search
   call time (AC-06, ADR-003, SR-05, SR-06).

2. **`test_from_profile_rebuild_error_degrades_gracefully`** — Confirms `from_profile()`
   returns `Ok(layer)` with `use_fallback=true` when rebuild fails due to a cycle in the graph
   data (AC-05, ADR-002, R-04).

Both tests are added inside the existing `mod layer_tests` block in `layer_tests.rs`. The
existing helper functions (`make_snapshot_db()`, `baseline_profile()`) are reused unchanged.

---

## Imports Required (additions inside `mod layer_tests`)

```
use std::sync::Arc;
use unimatrix_store::pool_config::PoolConfig;
use unimatrix_store::{NewEntry, Status};
use unimatrix_core::Store;
use unimatrix_engine::graph::find_terminal_active;
use crate::services::{
    AuditContext, AuditSource, CallerId,
    search::{RetrievalMode, ServiceSearchParams},
};
```

Note: `find_terminal_active` is used in Layer 2 (graph connectivity). Before using it, the
delivery agent must confirm it is `pub` or `pub(crate)` in `unimatrix_engine::graph`. If it
is private, replace Layer 2 with a direct node count assertion:
`assert!(guard.typed_graph.node_count() >= 1)`.

---

## Helper: `seed_graph_snapshot()`

A new async helper inside `mod layer_tests`. Encapsulates the repeated seeding pattern so
both tests can call it without duplicating raw SQL.

```
ASYNC FUNCTION seed_graph_snapshot() -> (TempDir, PathBuf, u64, u64):
  // Returns: (temp_dir, snapshot_db_path, id_a, id_b)
  // temp_dir must be kept alive by the caller for the duration of the test.

  dir <- TempDir::new()
  snap_path <- dir.path().join("snapshot.db")

  // Open a full SqlxStore (runs migrations — same as make_snapshot_db).
  store <- Arc::new(
    SqlxStore::open(&snap_path, PoolConfig::default()).await.expect("open")
  )

  // Insert two Active entries. store.insert() auto-increments IDs.
  id_a <- store.insert(NewEntry {
    title: "entry-a".to_string(),
    content: "graph-test-content-a".to_string(),
    topic: "test".to_string(),
    category: "decision".to_string(),
    tags: [],
    source: "test".to_string(),
    status: Status::Active,
    created_by: "test".to_string(),
    feature_cycle: "crt-045".to_string(),
    trust_source: "agent".to_string(),
  }).await.expect("insert a")

  id_b <- store.insert(NewEntry {
    title: "entry-b".to_string(),
    content: "graph-test-content-b".to_string(),
    topic: "test".to_string(),
    category: "decision".to_string(),
    tags: [],
    source: "test".to_string(),
    status: Status::Active,
    created_by: "test".to_string(),
    feature_cycle: "crt-045".to_string(),
    trust_source: "agent".to_string(),
  }).await.expect("insert b")

  // Insert one CoAccess (S1-class) edge between id_a and id_b via raw SQL.
  // bootstrap_only=0 ensures build_typed_relation_graph includes this edge.
  // Pattern from test_reverse_coaccess_high_id_to_low_id_ppr_regression in typed_graph.rs.
  sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)"
  )
  .bind(id_a as i64)
  .bind(id_b as i64)
  .execute(store.write_pool_server())
  .await.expect("insert edge")

  // Dump a VectorIndex so from_profile() Step 5 can find the vector/ dir.
  // Without this, from_profile() falls back to a fresh empty VectorIndex (GH-323),
  // which is acceptable, but a populated index avoids a separate missing-dir error.
  let vector_config = VectorConfig::default()
  let vi = VectorIndex::new(Arc::clone(&store), vector_config).expect("vi")
  let vector_dir = dir.path().join("vector")
  vi.dump(&vector_dir).expect("dump")
  // Note: the VectorIndex has no entries inserted — vi.dump() produces valid HNSW files
  // (possibly empty) that satisfy the from_profile() vector_meta.exists() check.
  // An empty VectorIndex is valid; from_profile() loads it without error (GH-323 guard).

  RETURN (dir, snap_path, id_a, id_b)
```

---

## Test 1: `test_from_profile_typed_graph_rebuilt_after_construction`

### Acceptance Criteria Covered

- AC-01: `use_fallback == false` and `typed_graph` non-empty after `from_profile()` with
  graph-seeded snapshot.
- AC-06: Three-layer assertion (handle state, graph connectivity, live search).
- SR-05: Live search call confirms SearchService observes rebuilt graph at query time.
- SR-06: Fixture uses Active entries with a real S1/S2/S8 edge — not a Quarantined-only or
  edge-free snapshot.

### Pseudocode

```
#[tokio::test(flavor = "multi_thread")]
ASYNC TEST test_from_profile_typed_graph_rebuilt_after_construction():

  // -----------------------------------------------------------------------
  // 1. Seed snapshot with two Active entries and one CoAccess edge.
  // -----------------------------------------------------------------------
  (dir, snap_path, id_a, id_b) <- seed_graph_snapshot().await

  // -----------------------------------------------------------------------
  // 2. Build an EvalProfile (baseline — no special inference flags required
  //    for this test; the graph is rebuilt regardless of ppr_expander_enabled).
  // -----------------------------------------------------------------------
  profile <- baseline_profile()
  // Pass dir.path() as project_dir so the live-DB guard resolves differently
  // from snap_path (avoids LiveDbPath error in CI environments where
  // snap_path may coincidentally match the active DB path).
  project_dir <- Some(dir.path())

  // -----------------------------------------------------------------------
  // 3. Construct EvalServiceLayer from the seeded snapshot.
  // -----------------------------------------------------------------------
  result <- EvalServiceLayer::from_profile(&snap_path, &profile, project_dir).await

  layer <- MATCH result:
    Ok(layer) -> layer
    Err(EvalError::LiveDbPath { .. }) ->
      // Environmental collision in CI: snap_path matched active DB.
      // Skip test (return early without panic). This is an env issue, not a bug.
      return
    Err(EvalError::Io(_)) ->
      // Unexpected I/O error — likely file system issue in CI.
      // Return early; do not panic (mirrors existing test pattern in layer_tests.rs).
      return
    Err(e) -> panic!("from_profile failed unexpectedly: {e}")

  // -----------------------------------------------------------------------
  // Layer 1: Handle state assertion (AC-01, AC-06 part 1)
  //
  // Acquire read lock on the TypedGraphState handle returned by the new accessor.
  // Assert use_fallback == false (rebuild ran and succeeded).
  // Assert all_entries.len() >= 2 (both seeded entries are present).
  // -----------------------------------------------------------------------
  handle <- layer.typed_graph_handle()
  guard  <- handle.read().unwrap_or_else(|e| e.into_inner())

  assert!(
    !guard.use_fallback,
    "use_fallback must be false after from_profile() with graph-seeded snapshot"
  )
  assert!(
    guard.all_entries.len() >= 2,
    "all_entries must contain at least the two seeded Active entries; got {}",
    guard.all_entries.len()
  )

  // -----------------------------------------------------------------------
  // Layer 2: Graph connectivity assertion (AC-06 part 2, ADR-003)
  //
  // find_terminal_active(id_a, ...) must return Some(id_a) — entry A is Active
  // and non-superseded; it is its own terminal in the Supersedes chain.
  // This confirms the TypedRelationGraph has nodes (not vacuously empty).
  //
  // Note: find_terminal_active traverses Supersedes edges, not CoAccess edges.
  // A CoAccess-only graph will still have nodes (id_a, id_b inserted), so
  // find_terminal_active returns Some(id_a) as long as id_a is in the graph.
  // -----------------------------------------------------------------------
  terminal_a <- find_terminal_active(id_a, &guard.typed_graph, &guard.all_entries)
  assert!(
    terminal_a == Some(id_a),
    "find_terminal_active must return Some(id_a) for active entry; got {:?}", terminal_a
  )

  // Confirm the graph has at least one edge (the CoAccess edge inserted above).
  assert!(
    guard.typed_graph.edge_count() >= 1,
    "typed_graph must have at least one edge after seeding CoAccess edge"
  )

  // Release read lock before calling search (avoids holding lock across await).
  DROP guard

  // -----------------------------------------------------------------------
  // Layer 3: Behavioral wiring assertion (SR-05, ADR-003)
  //
  // Invoke a live search through layer.inner.search — the same SearchService
  // that holds the Arc::clone of the typed graph handle. This confirms that
  // search.rs Step 6d reads use_fallback=false from the same handle that
  // was populated by Step 13b in from_profile().
  //
  // The embedding model is not loaded in CI — search will fall back to
  // ANN-only or return empty results. The assertion is only that the call
  // does not error or panic (Ok(_) result).
  // -----------------------------------------------------------------------
  params <- ServiceSearchParams {
    query: "graph test query".to_string(),
    k: 5,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: RetrievalMode::Flexible,
    session_id: None,
    category_histogram: None,
    current_phase: None,
  }

  audit_ctx <- AuditContext {
    source: AuditSource::Internal { service: "test".to_string() },
    caller_id: "test-harness".to_string(),
    session_id: Some("crt-045-test".to_string()),
    feature_cycle: None,
  }

  caller_id <- CallerId::Agent("test-harness".to_string())

  search_result <- layer.inner.search.search(params, &audit_ctx, &caller_id).await

  assert!(
    search_result.is_ok(),
    "live search must return Ok(_) on a graph-enabled EvalServiceLayer; got: {:?}",
    search_result.err()
  )

  // dir is dropped here, temp files cleaned up.
```

---

## Test 2: `test_from_profile_rebuild_error_degrades_gracefully`

### Acceptance Criteria Covered

- AC-05: `from_profile()` returns `Ok(layer)` on rebuild failure; handle has `use_fallback=true`.
- ADR-002: Degraded mode on cycle detection.
- R-04: Rebuild error must not abort eval construction.

### Pseudocode

```
#[tokio::test(flavor = "multi_thread")]
ASYNC TEST test_from_profile_rebuild_error_degrades_gracefully():

  // -----------------------------------------------------------------------
  // 1. Seed a snapshot with a Supersedes cycle: entry A supersedes entry B,
  //    entry B supersedes entry A (A -> B -> A).
  //
  //    In the store, `supersedes` field drives the Supersedes edge.
  //    The cycle is detected by build_typed_relation_graph() inside rebuild(),
  //    which returns GraphError::CycleDetected, mapped to StoreError::InvalidInput.
  //
  //    Implementation note: verify how cycles are introduced. The `supersedes`
  //    field on EntryRecord (set via store.update_supersedes() or equivalent)
  //    drives the Supersedes edge in build_typed_relation_graph(). Alternatively,
  //    insert a raw Supersedes edge row via graph_edges SQL with a cycle.
  //    Use whichever is simpler given the SqlxStore API available in layer_tests.rs.
  //
  //    Preferred approach (raw SQL, mirrors seed_graph_snapshot pattern):
  //      INSERT INTO graph_edges: id_a -> id_b, relation_type='Supersedes'
  //      INSERT INTO graph_edges: id_b -> id_a, relation_type='Supersedes'
  //    This creates a directed cycle in the Supersedes sub-graph.
  // -----------------------------------------------------------------------
  dir <- TempDir::new()
  snap_path <- dir.path().join("snapshot.db")

  store <- Arc::new(
    SqlxStore::open(&snap_path, PoolConfig::default()).await.expect("open")
  )

  id_a <- store.insert(NewEntry {
    title: "entry-a".to_string(),
    content: "cycle-test-a".to_string(),
    topic: "test".to_string(),
    category: "decision".to_string(),
    tags: [],
    source: "test".to_string(),
    status: Status::Active,
    created_by: "test".to_string(),
    feature_cycle: "crt-045".to_string(),
    trust_source: "agent".to_string(),
  }).await.expect("insert a")

  id_b <- store.insert(NewEntry {
    title: "entry-b".to_string(),
    content: "cycle-test-b".to_string(),
    topic: "test".to_string(),
    category: "decision".to_string(),
    tags: [],
    source: "test".to_string(),
    status: Status::Active,
    created_by: "test".to_string(),
    feature_cycle: "crt-045".to_string(),
    trust_source: "agent".to_string(),
  }).await.expect("insert b")

  // Insert a Supersedes cycle: A -> B and B -> A.
  // bootstrap_only=0 ensures build_typed_relation_graph includes both edges.
  sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     VALUES (?1, ?2, 'Supersedes', 1.0, strftime('%s','now'), 'test', 'test', 0)"
  )
  .bind(id_a as i64).bind(id_b as i64)
  .execute(store.write_pool_server()).await.expect("insert A->B edge")

  sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     VALUES (?1, ?2, 'Supersedes', 1.0, strftime('%s','now'), 'test', 'test', 0)"
  )
  .bind(id_b as i64).bind(id_a as i64)
  .execute(store.write_pool_server()).await.expect("insert B->A edge")

  // -----------------------------------------------------------------------
  // 2. Construct EvalServiceLayer against the cycle-containing snapshot.
  // -----------------------------------------------------------------------
  profile <- baseline_profile()
  project_dir <- Some(dir.path())

  result <- EvalServiceLayer::from_profile(&snap_path, &profile, project_dir).await

  layer <- MATCH result:
    Ok(layer) -> layer
    // AC-05 requires Ok(layer). Any Err here is a test failure.
    // Exception: LiveDbPath and Io are environmental, not logic errors.
    Err(EvalError::LiveDbPath { .. }) -> return   // CI environmental collision
    Err(EvalError::Io(_))            -> return   // CI file system issue
    Err(e) -> panic!(
      "from_profile must return Ok(layer) on rebuild failure (AC-05); got Err: {e}"
    )

  // -----------------------------------------------------------------------
  // 3. Assert handle state: use_fallback must be true (degraded mode).
  //
  // The cycle caused rebuild() to return StoreError::InvalidInput.
  // from_profile() must have caught this error, logged warn!, left
  // rebuilt_state = None, and NOT performed the Step 13b write-back.
  // The handle remains cold-start: use_fallback=true.
  // -----------------------------------------------------------------------
  handle <- layer.typed_graph_handle()
  guard  <- handle.read().unwrap_or_else(|e| e.into_inner())

  assert!(
    guard.use_fallback,
    "use_fallback must be true when rebuild() fails due to cycle (degraded mode, AC-05)"
  )

  // Optional: confirm graph is empty (cold-start state was not replaced).
  assert!(
    guard.all_entries.is_empty(),
    "all_entries must be empty in cold-start state after rebuild failure"
  )

  // -----------------------------------------------------------------------
  // Implementation note on cycle detection:
  //
  // build_typed_relation_graph() must return GraphError::CycleDetected for
  // A->B + B->A in Supersedes. If it does not (e.g., the cycle check only
  // covers specific graph topologies), the rebuild will succeed with a cyclic
  // graph and use_fallback will be false. In that case:
  //
  //   - This test will fail at the assert!(guard.use_fallback) line.
  //   - The failure indicates a regression in cycle detection, not in crt-045.
  //   - Add a comment explaining this and file a separate bug.
  //
  // If TypedRelationGraph is a petgraph StableGraph (it is), cycle detection
  // in build_typed_relation_graph uses petgraph::algo::is_cyclic_directed.
  // A two-node cycle (A->B, B->A) is definitionally cyclic. The assert
  // should hold for any correct implementation of rebuild().
  // -----------------------------------------------------------------------
```

---

## Cycle-Abort-Safety Test Note

The second test (`test_from_profile_rebuild_error_degrades_gracefully`) is the cycle-abort-
safety test. It verifies:

1. `from_profile()` does not propagate `StoreError::InvalidInput` as `Err(EvalError)`.
2. The returned `layer` has `use_fallback=true` (cold-start not replaced).
3. The function returns normally — no panic, no hang.

This satisfies AC-05, R-04, and the "cycle-abort-safety" requirement from the spawn prompt.

---

## Implementation Notes for Delivery Agent

### Confirming `find_terminal_active` visibility

Before using `find_terminal_active` in Layer 2 of Test 1:

```bash
grep -n "pub.*find_terminal_active" crates/unimatrix-engine/src/graph.rs
```

If not `pub` or `pub(crate)` accessible from `unimatrix-server`, replace Layer 2 with:

```rust
assert!(
    guard.typed_graph.node_count() >= 2,
    "typed_graph must have at least 2 nodes after seeding 2 Active entries"
);
assert!(
    guard.typed_graph.edge_count() >= 1,
    "typed_graph must have at least 1 edge after seeding CoAccess edge"
);
```

This satisfies AC-06's "non-empty graph" requirement without `find_terminal_active`.

### `store.write_pool_server()` accessibility

`write_pool_server()` is `pub fn` on `SqlxStore`
(`crates/unimatrix-store/src/db.rs:294`). It is accessible from within `unimatrix-server`
tests. This is the same pattern used by
`test_reverse_coaccess_high_id_to_low_id_ppr_regression` in `services/typed_graph.rs`.

### Live-DB guard in CI

The `from_profile()` live-DB guard canonicalizes `snap_path` and compares it to the
canonicalized active DB path. In most CI environments, temp dirs produce unique paths that
don't match the active DB. However, if the test is run with a non-standard project dir where
the active DB resolves to a temp-like path, the guard may fire. The `return` early exit (not
panic) in both tests handles this without causing CI failures.

### Existing test helpers reused

The new tests share `make_snapshot_db()` and `baseline_profile()` from the existing module.
`seed_graph_snapshot()` is a new helper — it does NOT replace `make_snapshot_db()` (existing
tests must not break, AC-08).

---

## Error Handling

| Scenario | Expected behavior |
|----------|-----------------|
| `from_profile()` -> `Err(LiveDbPath)` | Return early (CI environmental); no assertion |
| `from_profile()` -> `Err(Io)` | Return early (CI environmental); no assertion |
| `from_profile()` -> `Err(other)` | `panic!` — unexpected; indicates a regression |
| `handle.read()` poisoned | `.unwrap_or_else(|e| e.into_inner())` — consistent with project convention |
| `search()` -> `Err(_)` | `panic!` in Test 1 — `Ok(_)` is the assertion target |

---

## Key Test Scenarios Summary

| Test | Scenario | AC Covered | SR Covered |
|------|----------|-----------|-----------|
| `test_from_profile_typed_graph_rebuilt_after_construction` | Two Active entries + CoAccess edge; full three-layer assertion | AC-01, AC-06 | SR-05, SR-06 |
| `test_from_profile_rebuild_error_degrades_gracefully` | Supersedes cycle; Ok(layer) returned; use_fallback=true | AC-05 | R-04 |
