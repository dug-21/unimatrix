# EvalServiceLayer — Pseudocode
# File: crates/unimatrix-server/src/eval/profile/layer.rs

## Purpose

Add three changes to the existing `EvalServiceLayer` in `layer.rs`:

1. **Step 5b** — call `TypedGraphState::rebuild(&*store_arc).await` immediately after the
   `SqlxStore` is opened, before `ServiceLayer::with_rate_config()`.
2. **Step 13b** — if rebuild succeeded, write the rebuilt `TypedGraphState` into the handle
   returned by `inner.typed_graph_handle()` via a write-lock swap.
3. **New accessor** — `pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle` that
   delegates to `self.inner.typed_graph_handle()`.

No other fields, functions, or imports change. `ServiceLayer::with_rate_config()` signature is
unchanged (ADR-001, C-03).

---

## Required Imports (additions only)

The following items must be imported at the top of `layer.rs`. All already exist within
`unimatrix-server`; no new crate dependencies are added.

```
use crate::services::typed_graph::{TypedGraphState, TypedGraphStateHandle};
```

`TypedGraphState` is needed for the `rebuild()` call. `TypedGraphStateHandle` is the return
type of the new accessor. If `TypedGraphStateHandle` is already re-exported from
`crate::services`, use that path instead — verify before writing.

---

## Modified Function: `EvalServiceLayer::from_profile()`

Signature (unchanged):
```
pub async fn from_profile(
    db_path: &Path,
    profile: &EvalProfile,
    project_dir: Option<&Path>,
) -> Result<Self, EvalError>
```

### Pseudocode (changes shown relative to existing step numbering)

```
FUNCTION from_profile(db_path, profile, project_dir) -> Result<Self, EvalError>:

  // Steps 1–4: existing (live-DB guard, NLI validation, weight validation, raw pool).
  // No changes.
  [... existing steps 1–4 ...]

  // -----------------------------------------------------------------------
  // Step 5: Open SqlxStore (existing, unchanged)
  // -----------------------------------------------------------------------
  store <- SqlxStore::open_readonly(db_path).await
           map_err to EvalError::Store
  store_arc <- Arc::new(store)

  // -----------------------------------------------------------------------
  // Step 5b: Rebuild TypedGraphState from snapshot (NEW — crt-045, FR-01)
  //
  // rebuild() is async; called directly with .await.
  // No spawn_blocking (C-01).
  // rebuild() only reads from the snapshot — no write concerns (C-05).
  // -----------------------------------------------------------------------
  rebuilt_state: Option<TypedGraphState> <- None

  MATCH TypedGraphState::rebuild(&*store_arc).await:

    Ok(state):
      // Log at info! — rebuild is the operation that makes the profile meaningful.
      // Log minimum: profile name and entry count. (ADR-002, FR-04, OQ-02)
      tracing::info!(
          profile = %profile.name,
          entries = state.all_entries.len(),
          "eval: TypedGraphState rebuilt"
      )
      rebuilt_state <- Some(state)

    Err(StoreError::InvalidInput { reason, .. }) if reason contains "cycle":
      // Cycle detected. Degrade gracefully; do not abort from_profile(). (ADR-002, FR-03)
      tracing::warn!(
          profile = %profile.name,
          "eval: TypedGraphState rebuild skipped — cycle detected; use_fallback=true"
      )
      // rebuilt_state remains None; handle stays cold-start after Step 13.

    Err(e):
      // Store I/O or other error. Degrade gracefully. (ADR-002, FR-03)
      tracing::warn!(
          profile = %profile.name,
          error = %e,
          "eval: TypedGraphState rebuild failed; use_fallback=true"
      )
      // rebuilt_state remains None; handle stays cold-start after Step 13.

  // Implementation note: The match arms for Err can be collapsed into a single Err(e)
  // arm that inspects the error to choose the log message. The cycle case is
  // StoreError::InvalidInput with reason "supersession cycle detected". Either a nested
  // if-let or a two-arm match is acceptable — choose whichever reads most clearly.

  // -----------------------------------------------------------------------
  // Steps 6–12: existing, unchanged.
  // (embed_handle, NLI handle, rayon pool, adapt_svc, audit, usage_dedup,
  //  vector_adapter, async_vector_store, boosted_categories)
  // -----------------------------------------------------------------------
  [... existing steps 6–12 ...]

  // -----------------------------------------------------------------------
  // Step 13: Build ServiceLayer (existing, unchanged)
  //
  // with_rate_config() creates TypedGraphState::new_handle() internally at
  // services/mod.rs:399 (cold start: use_fallback=true, empty graph).
  // It passes Arc::clone(&typed_graph_state) into SearchService::new() at
  // services/mod.rs:419. Both arcs share the same backing RwLock allocation.
  // -----------------------------------------------------------------------
  inner <- ServiceLayer::with_rate_config(
               Arc::clone(&store_arc),
               Arc::clone(&vector_index),
               Arc::clone(&async_vector_store),
               Arc::clone(&store_arc),         // entry_store
               Arc::clone(&embed_handle),
               Arc::clone(&adapt_svc),
               Arc::clone(&audit),
               Arc::clone(&usage_dedup),
               rate_config,
               boosted_categories,
               Arc::clone(&rayon_pool),
               nli_handle_arc,
               nli_top_k,
               nli_enabled,
               Arc::new(profile.config_overrides.inference.clone()),
               Arc::new(DomainPackRegistry::with_builtin_claude_code()),
               Arc::new(ConfidenceParams::default()),
               eval_category_allowlist,
           )
  // Note: with_rate_config() is NOT async (it returns Self, not Result<Self>).
  // No .await here. Verify in services/mod.rs before implementing.

  // -----------------------------------------------------------------------
  // Step 13b: Write rebuilt state into handle (NEW — crt-045, FR-02, ADR-001)
  //
  // inner.typed_graph_handle() returns Arc::clone(&self.typed_graph_state),
  // which is the same Arc<RwLock<TypedGraphState>> that SearchService holds
  // (confirmed: services/mod.rs:419 uses Arc::clone). Writing through this
  // handle is immediately visible to SearchService at query time.
  //
  // The write lock is acquired AFTER with_rate_config() returns — SearchService
  // is fully constructed before the swap. No concurrency risk (R-10, C-03, NFR-03).
  // -----------------------------------------------------------------------
  IF rebuilt_state is Some(state):
    handle <- inner.typed_graph_handle()
              // Returns Arc::clone of self.typed_graph_state
    guard  <- handle.write().unwrap_or_else(|e| e.into_inner())
              // Poison recovery: consistent with typed_graph.rs conventions
    *guard <- state
              // Swap: cold-start state is dropped; rebuilt state is now live.
              // SearchService.typed_graph_state (same Arc) sees use_fallback=false.
    DROP guard  // release write lock immediately (NFR-03)

  // IF rebuilt_state is None:
  //   handle remains cold-start (use_fallback=true). Eval proceeds in degraded mode.
  //   No log here — warn was already emitted in Step 5b.

  // -----------------------------------------------------------------------
  // Construct and return EvalServiceLayer (existing, unchanged fields)
  // -----------------------------------------------------------------------
  RETURN Ok(EvalServiceLayer {
    inner,
    pool,
    embed_handle,
    db_path: db_resolved,
    profile_name: profile.name.clone(),
    analytics_mode: AnalyticsMode::Suppressed,
    nli_handle,
  })

END FUNCTION
```

---

## New Function: `EvalServiceLayer::typed_graph_handle()`

Location: inside `impl EvalServiceLayer`, after `nli_handle()` (mirrors the pattern of
`embed_handle()` at line 357 and `nli_handle()` at line 379).

```
/// Return the TypedGraphState handle for inspection and pre-replay diagnostics.
///
/// Used by runner.rs to verify the graph was populated before scenario replay,
/// and by layer_tests.rs to assert post-construction state (AC-06, crt-045).
///
/// pub(crate): mirrors embed_handle() and nli_handle() visibility.
/// No #[cfg(test)] guard: also used by runner.rs for diagnostics (ADR-004, C-10).
pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle {
    self.inner.typed_graph_handle()
    // Delegates to ServiceLayer::typed_graph_handle() which returns
    // Arc::clone(&self.typed_graph_state). No new state on EvalServiceLayer. (C-08)
}
```

---

## State Machine

`EvalServiceLayer::from_profile()` has a single new conditional branch, not a full state
machine. The simplified flow:

```
State: RebuildOutcome
  Pending     -- before Step 5b
  Succeeded   -- rebuild() returned Ok(state)
  Failed      -- rebuild() returned Err(_)

Transition: Step 5b
  Pending -> Succeeded  if rebuild Ok
  Pending -> Failed     if rebuild Err (log warn, do not abort)

Effect at Step 13b:
  Succeeded -> write state into handle (use_fallback becomes false in handle)
  Failed    -> no write; handle stays cold-start (use_fallback remains true)
```

The `EvalServiceLayer` itself has no state enum. `RebuildOutcome` is represented by
`rebuilt_state: Option<TypedGraphState>` (a local variable inside `from_profile()`).

---

## Error Handling

| Error Source | Error Type | Handler |
|-------------|-----------|---------|
| `TypedGraphState::rebuild()` — cycle | `StoreError::InvalidInput` | log warn, set `rebuilt_state = None`, continue |
| `TypedGraphState::rebuild()` — I/O | `StoreError::*` | log warn, set `rebuilt_state = None`, continue |
| `handle.write()` — lock poisoned | `PoisonError<_>` | `.unwrap_or_else(|e| e.into_inner())` — recover from poison (consistent with typed_graph.rs tests) |

Rebuild errors MUST NOT propagate as `Err(EvalError)` from `from_profile()` (ADR-002, FR-03,
C-02). All other error paths in `from_profile()` are unchanged.

---

## Key Test Scenarios (for implementation agent awareness)

1. **Happy path (AC-01, AC-06):** Snapshot with two Active entries and one S1/S2/S8 edge.
   `from_profile()` succeeds; `typed_graph_handle()` shows `use_fallback=false`; graph
   non-empty. Three-layer assertion in `layer_tests.rs`.

2. **Degraded path — cycle (AC-05):** Snapshot with a Supersedes cycle (A→B→A). `rebuild()`
   returns `StoreError::InvalidInput`. `from_profile()` returns `Ok(layer)`. Handle shows
   `use_fallback=true`.

3. **Degraded path — no edges (R-06, AC-04):** Snapshot with entries but no graph edges.
   `rebuild()` returns `Ok(state)` with `use_fallback=false` but empty `typed_graph`. Baseline
   profile behavior is unchanged. This is not tested by the new test — it is covered by
   existing tests.

4. **Accessor delegation (C-08):** `layer.typed_graph_handle()` returns an Arc that points to
   the same `RwLock` as `layer.inner.typed_graph_handle()`. Verify via `Arc::ptr_eq` in test.

---

## Constraints Checklist

- [x] `rebuild()` called with `.await` — no `spawn_blocking` (C-01)
- [x] Rebuild errors produce `Ok(layer)` not `Err(...)` (C-02)
- [x] `with_rate_config()` signature unchanged (C-03)
- [x] Accessor is `pub(crate)`, not `pub` (C-04)
- [x] No writes to snapshot store (C-05)
- [x] Accessor delegates to `self.inner.typed_graph_handle()`, no new fields (C-08)
- [x] No `#[cfg(test)]` guard on accessor (C-10)
- [x] Write lock uses `.unwrap_or_else(|e| e.into_inner())` for poison recovery
- [x] Info log on success; warn log on failure — no error! or debug!-only (NFR-04)
