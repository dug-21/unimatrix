## ADR-001: Post-Construction Write-Back Rather Than Pre-Populated Handle Parameter

### Context

`EvalServiceLayer::from_profile()` must call `TypedGraphState::rebuild()` and make the result
visible to `SearchService` before scenario replay begins. Two implementation options exist:

**Option A — Pre-populated handle parameter:** Call `rebuild()` before `ServiceLayer::with_rate_config()`.
Construct a pre-populated `TypedGraphStateHandle` via `Arc::new(RwLock::new(rebuilt_state))`
and pass it into `with_rate_config()` as a new parameter, bypassing the internal
`TypedGraphState::new_handle()` call.

**Option B — Post-construction write-back:** Call `rebuild()` after `with_rate_config()`
completes. Obtain the handle via `layer.inner.typed_graph_handle()` and write the rebuilt
state in via a write lock swap.

`with_rate_config()` currently creates `TypedGraphState::new_handle()` at line 399 and passes
`Arc::clone` into `SearchService::new()` at line 419. It does not accept an external handle
parameter — adding one would change the function signature, which is a `pub(crate)` method
called from `main.rs`, all integration test harnesses, and `from_profile()`.

The SR-01 risk was: if `SearchService` holds a value copy of `TypedGraphState` rather than an
Arc clone, a post-construction write through the `ServiceLayer` handle would not propagate.
Code inspection of `services/mod.rs:419` confirms: `SearchService::new()` receives
`Arc::clone(&typed_graph_state)` — the same `Arc<RwLock<TypedGraphState>>` allocation that
`ServiceLayer` retains as `self.typed_graph_state`. SR-01 is resolved: Option B is safe.

### Decision

Use Option B (post-construction write-back). After `ServiceLayer::with_rate_config()` returns:

1. If rebuild succeeded, call `inner.typed_graph_handle()` to obtain an `Arc::clone` of the
   shared handle.
2. Acquire the write lock: `handle.write().unwrap_or_else(|e| e.into_inner())`.
3. Swap in the rebuilt state: `*guard = rebuilt_state`.

No signature changes to `with_rate_config()`, `ServiceLayer::new()`, or any test harness.
The write propagates to `SearchService` because all three Arcs — `ServiceLayer.typed_graph_state`,
`SearchService.typed_graph_state`, and the one returned by `typed_graph_handle()` — are clones
of the same backing `Arc<RwLock<TypedGraphState>>` allocation.

The `rebuild()` call is placed between Step 5 (store construction) and Step 13
(`with_rate_config()`), so the rebuilt state is ready before `ServiceLayer` is constructed.
The write-back occurs immediately after Step 13 returns.

### Consequences

Easier:
- No signature change to `with_rate_config()` — zero changes to call sites in `main.rs` and
  test harnesses.
- The fix is entirely local to `from_profile()` — no refactoring of `ServiceLayer` internals.
- The write-lock swap pattern is identical to what the background tick does on every interval;
  no new pattern is introduced.

Harder:
- The `rebuild()` call and the write-back are separated by ~20 lines of `with_rate_config()`
  construction. A reader of `from_profile()` must understand that the cold-start handle created
  by `with_rate_config()` is immediately overwritten by Step 13b. A `// Step 13b` comment with
  explicit rationale is required.
- If `with_rate_config()` is ever refactored to accept an external typed graph handle, this
  decision should be revisited via ADR supersession.
