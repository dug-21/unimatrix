# crt-045: Eval Harness — Wire TypedGraphState rebuild into EvalServiceLayer

## System Overview

The Unimatrix eval harness (`unimatrix eval run`) replays stored query scenarios against a
snapshot database to produce offline MRR/P@5 metrics. Scenarios are executed by
`EvalServiceLayer`, which wraps a `ServiceLayer` built against the read-only snapshot.

The live server populates `TypedGraphState` via a background tick
(`spawn_background_tick → TypedGraphState::rebuild()`). The eval path has no background tick.
Prior to crt-045, `EvalServiceLayer::from_profile()` never called `TypedGraphState::rebuild()`,
leaving the handle in its cold-start state (`use_fallback = true`, empty graph). This silently
disabled Phase 0 (`graph_expand`), Phase 1 (PPR), and graph-derived penalties for every eval
profile, making baseline.toml and ppr-expander-enabled.toml produce bit-identical results.

crt-045 is a single-file fix: add one `TypedGraphState::rebuild()` call inside
`from_profile()`, write the result back into the shared handle, and add one integration test
that verifies a live search observes the rebuilt graph.

## Component Breakdown

### Components Involved

| Component | File | Role |
|-----------|------|------|
| `EvalServiceLayer` | `eval/profile/layer.rs` | Construction site — receives the fix |
| `TypedGraphState` | `services/typed_graph.rs` | Provides `rebuild()` and `new_handle()` |
| `ServiceLayer` | `services/mod.rs` | Creates the `TypedGraphStateHandle` internally; exposes `typed_graph_handle()` |
| `SearchService` | `services/search.rs` | Reads from `TypedGraphStateHandle` at query time under a read lock |
| `ppr-expander-enabled.toml` | `product/research/ass-037/harness/profiles/` | Secondary fix: TOML parse failure |
| `layer_tests.rs` | `eval/profile/layer_tests.rs` | Integration test additions (AC-06, SR-05, SR-06) |

### Component Responsibilities

**`EvalServiceLayer::from_profile()`** — The sole construction entry point for eval service
layers. It opens the snapshot store, builds infrastructure handles, and calls
`ServiceLayer::with_rate_config()`. The fix adds a `TypedGraphState::rebuild()` call
between store construction (Step 5) and `with_rate_config()` (Step 13), then writes the
result back into the handle obtained from the constructed `ServiceLayer`.

**`TypedGraphState::rebuild(store)`** — Async function that queries `GRAPH_EDGES` and all
entries, filters out `Quarantined` entries, and calls `build_typed_relation_graph()`. Returns
`Ok(TypedGraphState { use_fallback: false, ... })` on success; `Err(StoreError::InvalidInput)`
on cycle detection; `Err(StoreError::*)` on I/O failure.

**`ServiceLayer::with_rate_config()`** — Creates `TypedGraphState::new_handle()` at line 399
and stores it as `typed_graph_state: TypedGraphStateHandle`. Passes `Arc::clone(&typed_graph_state)`
to `SearchService::new()` at line 419. Exposes `typed_graph_handle()` which returns
`Arc::clone(&self.typed_graph_state)`.

**`SearchService`** — Holds `typed_graph_state: TypedGraphStateHandle` (an `Arc<RwLock<_>>`).
At query time, acquires a short read lock, clones the graph and entry snapshot, releases the
lock, then executes graph traversal outside the lock. The Arc clone at construction time means
any post-construction write to the handle is immediately visible to `SearchService`.

## Component Interactions

```
from_profile() [async]
  │
  ├─ Step 5: SqlxStore::open_readonly(db_path)  →  store_arc: Arc<Store>
  │
  ├─ Step 5b (NEW): TypedGraphState::rebuild(&store_arc).await
  │     ├─ Ok(state)  →  rebuilt_state stored locally
  │     └─ Err(_)     →  log tracing::warn!, leave rebuilt_state = None
  │
  ├─ Steps 6–12: embed, NLI, rayon, adapt, audit, dedup, vector, categories
  │
  ├─ Step 13: ServiceLayer::with_rate_config(...)
  │     └─ internally: typed_graph_state = TypedGraphState::new_handle()  [cold start]
  │               └─ Arc::clone(&typed_graph_state) → SearchService.typed_graph_state
  │
  ├─ Step 13b (NEW): if let Some(state) = rebuilt_state
  │     └─ let handle = inner.typed_graph_handle()   [Arc::clone of same allocation]
  │        let mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
  │        *guard = state                             [swap — visible to SearchService]
  │
  └─ Ok(EvalServiceLayer { inner, ..., typed_graph_handle: handle })
```

**Why post-construction write is safe (SR-01 resolution):** `services/mod.rs:399` creates
`typed_graph_state = TypedGraphState::new_handle()`. Line 419 passes
`Arc::clone(&typed_graph_state)` to `SearchService::new()`. `ServiceLayer` stores the original
Arc in `self.typed_graph_state`. `ServiceLayer::typed_graph_handle()` returns
`Arc::clone(&self.typed_graph_state)`. All three Arcs — ServiceLayer's, SearchService's, and
the one returned by `typed_graph_handle()` — point to the same `RwLock<TypedGraphState>`
allocation. A write through any clone is immediately visible to all others.

## Technology Decisions

See ADR files:
- `ADR-001-post-construction-write-vs-parameter.md` — Why write-after-construction rather than
  pre-populated handle parameter
- `ADR-002-rebuild-error-handling.md` — Degraded mode vs. abort on rebuild failure
- `ADR-003-test-live-search-not-just-handle-state.md` — Why the integration test must invoke
  a live search, not only inspect the handle
- `ADR-004-typed-graph-handle-accessor-visibility.md` — `pub(crate)` scope for
  `typed_graph_handle()` on `EvalServiceLayer`
- `ADR-005-toml-distribution-change-false.md` — Why `ppr-expander-enabled.toml` sets
  `distribution_change = false`

## Integration Points

### Existing Interfaces Used

| Interface | Location | Usage |
|-----------|----------|-------|
| `TypedGraphState::rebuild(&Store) -> Result<Self, StoreError>` | `services/typed_graph.rs:91` | Called once after store open |
| `ServiceLayer::typed_graph_handle() -> TypedGraphStateHandle` | `services/mod.rs:297` | Obtains write-target handle post-construction |
| `TypedGraphStateHandle` = `Arc<RwLock<TypedGraphState>>` | `services/typed_graph.rs:161` | Write lock for state swap |
| `TypedGraphState::use_fallback: bool` | `services/typed_graph.rs:53` | Test assertion target |
| `TypedGraphState::typed_graph: TypedRelationGraph` | `services/typed_graph.rs:44` | Test non-empty assertion target |
| `EvalServiceLayer::nli_handle()` | `eval/profile/layer.rs:379` | Pattern to mirror for new accessor |
| `EmbedServiceHandle` pattern | `eval/profile/layer.rs:214–216` | Construction pattern to mirror |

### New Interfaces Introduced

| Interface | Visibility | Purpose |
|-----------|------------|---------|
| `EvalServiceLayer::typed_graph_handle() -> TypedGraphStateHandle` | `pub(crate)` | Exposes the inner handle for test assertions and runner.rs |

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `TypedGraphState::rebuild` | `async fn rebuild(store: &Store) -> Result<TypedGraphState, StoreError>` | `services/typed_graph.rs:91` |
| `ServiceLayer::typed_graph_handle` | `pub fn typed_graph_handle(&self) -> TypedGraphStateHandle` | `services/mod.rs:297` |
| `TypedGraphStateHandle` | `Arc<RwLock<TypedGraphState>>` | `services/typed_graph.rs:161` |
| `EvalServiceLayer::typed_graph_handle` | `pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle` | NEW in `eval/profile/layer.rs` |
| Write-lock swap idiom | `*guard = rebuilt_state` inside `handle.write().unwrap_or_else(|e| e.into_inner())` | Established by `typed_graph.rs` tests |

## Existing Pattern: NLI Handle Init (crt-023)

The NLI handle init in `from_profile()` (Steps 6b, 13) is the direct precedent:

1. A condition is checked (`nli_enabled`).
2. A handle is created and started conditionally (Step 6b).
3. The handle is passed into `with_rate_config()` (Step 13).

The typed graph fix follows the same conditional-init pattern but deviates at Step 3: because
`with_rate_config()` always creates its own cold-start handle internally and cannot accept a
pre-built one without a signature change, the write is post-construction (Step 13b) rather
than via parameter. The NLI handle is passed in as a parameter because `with_rate_config()`
was designed to accept it; the typed graph handle was not.

## Test Architecture (AC-06, SR-05, SR-06)

The new integration test in `layer_tests.rs` must satisfy three constraints:

**SR-06** — Seed real graph edges. The test must insert at least two `Active` entries and one
`S1/S2/S8` edge between them via raw SQL into `graph_edges`. A snapshot with only entries and
no edges produces an empty `TypedRelationGraph` even after a successful rebuild, making the
non-empty graph assertion vacuously false.

**SR-05** — Invoke a live search. The test must call a real search operation through the
`EvalServiceLayer` (not just read `use_fallback` from the handle) to confirm the rebuilt graph
is observed at query time — not only at construction time. This guards against the
wired-but-unused anti-pattern (entry #1495).

**AC-06** — Assert both `use_fallback = false` and non-empty `typed_graph`. Access the state
via the new `typed_graph_handle()` accessor.

**Test structure:**
```
test_from_profile_typed_graph_rebuilt_after_construction
  1. Open SqlxStore (temp dir, full migration via open())
  2. Insert two Active entries (store.insert())
  3. Insert one CoAccess or Supports edge between them (raw sqlx INSERT into graph_edges)
  4. Dump a VectorIndex with embeddings for both entries (vi.dump())
  5. Call EvalServiceLayer::from_profile() against the seeded snapshot
  6. Assert layer.typed_graph_handle() guard: use_fallback == false
  7. Assert guard.typed_graph is non-empty (at least one node or edge reachable)
  8. Call layer.inner.search.search(...) with a query that would use the graph path
  9. Assert the search completes without error (live search path validation, SR-05)
```

The embedding model is not available in CI, so the full PPR path will not execute. The test
therefore focuses on: (a) handle state after construction, (b) that the search call does not
panic or error on the graph-enabled path (it will fall back to ANN-only if no embedding).

## Open Questions

None. All open questions from SCOPE.md are resolved:

- **OQ-01 (RESOLVED):** `ppr-expander-enabled.toml` — `distribution_change = false`, gate on
  `mrr_floor = 0.2651` and `p_at_5_min = 0.1083`.
- **OQ-02 (RESOLVED):** Log at `tracing::info!` in eval context (significant operation).
- **OQ-03/OQ-04 (RESOLVED non-blocking):** Post-construction write propagates; verify via harness post-delivery.

## Constraints Checklist

| ID | Constraint | Architecture Response |
|----|-----------|----------------------|
| C-01 | `rebuild()` is async — call from async `from_profile()` body | Called with `.await` directly; no `spawn_blocking` |
| C-02 | Rebuild errors must not abort | Match arm: log `tracing::warn!`, set `rebuilt_state = None`, continue |
| C-03 | `with_rate_config()` creates its own handle internally | Post-construction write via `typed_graph_handle()` accessor |
| C-04 | `EvalServiceLayer` must expose `typed_graph_handle()` accessor | New `pub(crate)` method delegating to `self.inner.typed_graph_handle()` |
| C-05 | Snapshot is read-only; rebuild only reads | No write concerns; confirmed by `TypedGraphState::rebuild()` impl |
| C-06 | TOML thresholds grounded in current baseline | `mrr_floor = 0.2651`, `p_at_5_min = 0.1083` per OQ-01 resolution |
| C-07 | No changes to `ScenarioResult`, `ProfileResult`, or runner/report types | Fix is eval layer construction only |
| C-08 | `typed_graph_handle()` on `EvalServiceLayer` delegates to `self.inner` | Mirrors `embed_handle()` and `nli_handle()` delegation pattern |
