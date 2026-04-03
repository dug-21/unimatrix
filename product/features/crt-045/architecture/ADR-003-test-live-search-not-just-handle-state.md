## ADR-003: Integration Test Must Invoke Live Search, Not Only Assert Handle State

### Context

The primary acceptance criterion (AC-06) requires asserting that `use_fallback == false` and
`typed_graph` is non-empty after `EvalServiceLayer::from_profile()` against a graph-seeded
snapshot. This can be satisfied by inspecting the handle directly:

```rust
let guard = layer.typed_graph_handle().read().unwrap_or_else(|e| e.into_inner());
assert!(!guard.use_fallback);
assert!(!guard.all_entries.is_empty());
```

SR-05 (High risk) identifies the wired-but-unused anti-pattern (entry #1495, crt-019): struct
wiring at construction time does not guarantee behavioral wiring at the use site. If
`SearchService` stores the handle as `typed_graph_state` but the `if !use_fallback` guard in
`search.rs` Step 6d reads a different field (or reads from a different handle clone), the
handle inspection test passes while the behavioral path remains broken.

The test must verify that `SearchService` observes the rebuilt graph at actual query time —
not only that the handle was written at construction time. This means invoking a real search
operation through `layer.inner.search` (or equivalent) and asserting the result is not the
cold-start fallback path.

The embedding model is unavailable in CI, so the search will not produce embedding-based
results. The test can instead assert:
- The search call returns `Ok(_)` without error (graph-path code does not panic on a real graph).
- Or: assert the number of graph nodes reachable from a known entry is non-zero by inspecting
  `find_terminal_active` on the rebuilt state.

A combined assertion strategy satisfies both AC-06 and SR-05:
1. Handle inspection: `use_fallback == false` and `all_entries.len() >= 2`.
2. Graph connectivity: `find_terminal_active(entry_id, &guard.typed_graph, &guard.all_entries)`
   returns `Some(entry_id)` for an Active entry seeded into the snapshot.

This exercises the actual `TypedRelationGraph` data structure that `SearchService` will use at
query time, confirming the write-back propagated and the graph is structurally sound.

### Decision

The AC-06 integration test asserts three things in sequence:

1. **Handle state** — `use_fallback == false` and `all_entries.len() >= 2` (at least the two
   seeded entries are present).
2. **Graph connectivity** — `find_terminal_active(seeded_entry_id, &guard.typed_graph, &guard.all_entries)`
   returns `Some(seeded_entry_id)` for each Active entry (confirms the graph has nodes, not
   just that `use_fallback` was flipped).
3. **Behavioral wiring** — Call `layer.inner.search.search(params).await` with a minimal
   `ServiceSearchParams` and assert `Ok(_)` is returned. The embedding model will not be
   loaded in CI; the search will return ANN results or an empty result set, not an error.
   This confirms the search path does not panic or error when `use_fallback == false`.

The test follows the existing seeding pattern from `test_rebuild_excludes_quarantined_entries`
in `typed_graph.rs` (uses `SqlxStore::open()` for full migration + `store.insert()`).

Graph edges are inserted via raw SQL (`sqlx::query INSERT INTO graph_edges ...`) following the
pattern established in `test_reverse_coaccess_high_id_to_low_id_ppr_regression`.

### Consequences

Easier:
- The three-layer assertion catches both the wired-but-unused anti-pattern and structural graph
  issues (empty graph despite `use_fallback = false`).
- No live embedding model required — the test uses `find_terminal_active` as the behavioral
  proxy rather than full PPR execution.
- Pattern is consistent with existing graph integration tests in `typed_graph.rs`.

Harder:
- The test requires seeding graph edges via raw SQL, which requires access to the store's
  write pool (`store.write_pool_server()`). This is an internal method used by other tests in
  the same crate — acceptable for `#[cfg(test)]` use.
- `ServiceSearchParams` must be constructed with valid values; the test author must reference
  an existing test that calls `search()` directly to avoid inventing field values.
