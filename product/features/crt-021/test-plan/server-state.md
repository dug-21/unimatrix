# Test Plan: server-state (unimatrix-server/src/services/typed_graph.rs)

Covers: `TypedGraphState` struct; `TypedGraphStateHandle` type alias; `new_handle()`
cold-start state; `rebuild(store)` from GRAPH_EDGES; pre-built graph accessible under
read lock; poison recovery; search path reads pre-built graph (no per-query rebuild).

Risks addressed: R-05, R-14, AC-13, AC-15

---

## Rename Enforcement (R-14, NF-07)

### `compile_gate_no_supersession_state_symbols`
- Build gate: `cargo build --workspace` must succeed with zero type alias tricks
- Grep gate: `grep -r "SupersessionState\|SupersessionStateHandle" crates/` must return
  zero matches in `.rs` source files (excluding comments and doc strings)
- Enforced by the compiler — if any call site still uses the old name and no type alias
  exists, the build fails

This is the primary enforcement mechanism for R-14. No unit test needed beyond a clean build.

---

## Unit Tests: Cold-Start State (R-05, AC-15)

### `test_typed_graph_state_new_handle_sets_use_fallback_true`
- Arrange: call `TypedGraphState::new_handle()`
- Act: acquire read lock on the returned handle
- Assert: `state.use_fallback == true`
- Assert: `state.all_entries.is_empty()`
- Assert: `state.typed_graph` inner edge count == 0

### `test_typed_graph_state_cold_start_graph_is_empty`
- Assert: `TypedGraphState::new_handle()` returns a handle where the `typed_graph`
  has zero nodes (or is otherwise empty/default)

---

## Unit Tests: Pre-Built Graph Accessibility (FR-22, AC-13)

### `test_typed_graph_state_holds_prebuilt_graph_not_raw_rows`
- Structural assertion: `TypedGraphState` struct contains `typed_graph: TypedRelationGraph`,
  not `all_edges: Vec<GraphEdgeRow>` — verified at compile time by accessing the field
- Act: construct a `TypedGraphState` with a non-empty `typed_graph`; acquire read lock;
  call `graph_penalty(some_id, &state.typed_graph, &state.all_entries)`
- Assert: `graph_penalty` runs without rebuilding the graph — no call to
  `build_typed_relation_graph` on the hot path

### `test_search_path_reads_prebuilt_graph_under_read_lock`
- Arrange: build a `TypedGraphState` with a pre-populated `typed_graph`
  (containing one Supersedes edge A→B)
- Act: acquire read lock, clone `typed_graph` and `all_entries`, release lock,
  call `graph_penalty(A.id, &typed_graph, &all_entries)`
- Assert: returns `CLEAN_REPLACEMENT_PENALTY` (depth-1 chain)
- Assert: no store I/O occurs on the hot path (the graph is already built)

---

## Integration Tests: Rebuild from GRAPH_EDGES (AC-13)

All integration tests use `#[tokio::test]`. Open a fresh `SqlxStore` with
`open_test_store` helper. Seed `GRAPH_EDGES` rows via direct sqlx query or
`AnalyticsWrite::GraphEdge` drain.

### `test_typed_graph_state_rebuild_from_graph_edges`
- Arrange: open fresh store; insert one Supersedes edge into `graph_edges` directly:
  `(source_id=1, target_id=2, relation_type='Supersedes', weight=1.0, bootstrap_only=0, ...)`
- Arrange: insert corresponding entries (id=1, id=2) into `entries` table
- Act: call `TypedGraphState::rebuild(&store).await`
- Assert: returns `Ok(new_state)`
- Assert: `new_state.use_fallback == false`
- Assert: `new_state.typed_graph` contains the expected Supersedes edge
- Assert: `new_state.all_entries` contains the two entries

### `test_typed_graph_state_rebuild_excludes_bootstrap_only_edges` (R-03)
- Arrange: seed `graph_edges` with a `bootstrap_only=1` Supersedes edge
- Act: `TypedGraphState::rebuild(&store).await`
- Assert: `new_state.typed_graph` has zero edges (bootstrap_only=1 edge excluded structurally)

### `test_typed_graph_state_rebuild_failure_preserves_old_state`
- Arrange: a handle with an existing non-empty state; simulate a store read failure
- Act: call `rebuild` with a broken store (or shut down the pool)
- Assert: the handle retains the previous state — old graph is not replaced on `Err`

---

## Integration Tests: Write Lock Swap (AC-13)

### `test_typed_graph_state_handle_write_lock_swap`
- Arrange: `new_handle()` (use_fallback=true, empty graph)
- Act: build a non-empty `TypedGraphState` via `rebuild`; acquire write lock; `*guard = new_state`
- Act: acquire read lock on same handle
- Assert: `state.use_fallback == false`; graph is non-empty

---

## Integration Tests: Poison Recovery

### `test_typed_graph_state_handle_poison_recovery`
- Arrange: spawn a thread that panics while holding the write lock on `TypedGraphStateHandle`
- Act: attempt `.read().unwrap_or_else(|e| e.into_inner())` on the poisoned handle
- Assert: read lock obtained successfully (poison recovery via `unwrap_or_else` works)
- Assert: no panic propagation to the calling thread

---

## Boundary Behavior: bootstrap_only=1 exclusion on search path (R-03, AC-12)

This is fundamentally an engine-types test, but it has an implication for the server-state
search path: the search path must NOT check `bootstrap_only` itself — the exclusion is
structural in `build_typed_relation_graph`. The typed graph presented to `graph_penalty`
via the read lock already has no bootstrap_only edges.

### `inspect_search_path_no_bootstrap_only_check`
- Code review gate: confirm `services/search.rs` does not contain `bootstrap_only` field
  access — the graph read from the handle already excludes bootstrap edges by construction

---

## Compile-Time Verification

The following are structural checks enforced by the compiler — they require no explicit tests:

1. `TypedGraphState` field `typed_graph: TypedRelationGraph` (not `all_edges: Vec<GraphEdgeRow>`)
   — any code that accesses a non-existent `all_edges` field fails to compile
2. `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>` — all handle usages are
   type-checked against the new type

These enforce AC-15 (variance 2 from ALIGNMENT-REPORT: spec governs, struct holds pre-built graph).
