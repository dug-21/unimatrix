## ADR-004: typed_graph_handle() Accessor on EvalServiceLayer is pub(crate), Not pub

### Context

`EvalServiceLayer` must expose a `typed_graph_handle()` accessor so that:
1. The integration test in `layer_tests.rs` can inspect `TypedGraphState` post-construction
   (AC-06).
2. `runner.rs` (Wave 2 runner) can potentially check `use_fallback` before scenario replay to
   emit a diagnostic warning.

The existing accessors on `EvalServiceLayer` set the precedent:
- `embed_handle()` — `pub(crate)` (used by runner.rs for readiness polling)
- `nli_handle()` — `pub(crate)` (used by runner.rs for NLI readiness polling)
- `profile_name()` — `pub` (used by external report formatting)
- `db_path()` — `pub`
- `analytics_mode()` — `pub`

SR-03 identifies that making the accessor `pub` widens the API surface beyond test use and may
conflict with future encapsulation of `ServiceLayer` internals. The `TypedGraphStateHandle` is
an `Arc<RwLock<TypedGraphState>>` — a writable reference to internal service state. Exposing
it publicly would allow callers outside the crate to modify the graph state, undermining the
invariant that only the background tick (or eval construction) writes to it.

The accessor is needed within the crate (runner.rs, layer_tests.rs, future eval gates). It is
not part of the public API contract of `EvalServiceLayer`.

### Decision

Define the accessor as `pub(crate)`:

```rust
/// Return the TypedGraphState handle for inspection and readiness checking.
///
/// Used by runner.rs to verify the graph was populated before scenario replay,
/// and by layer_tests.rs to assert post-construction state (AC-06, crt-045).
pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle {
    self.inner.typed_graph_handle()
}
```

The implementation delegates to `ServiceLayer::typed_graph_handle()` (which is already `pub`)
via `self.inner`. No new field is required on `EvalServiceLayer` — the handle is owned by
`self.inner` and accessed through the existing delegation path.

No `#[cfg(test)]` guard is applied. Although the primary consumer in crt-045 is the test, the
accessor is also useful to `runner.rs` for pre-replay diagnostics. Restricting it to
`#[cfg(test)]` would prevent that use.

### Consequences

Easier:
- `pub(crate)` keeps the accessor invisible outside `unimatrix-server`, preventing external
  mutation of graph state.
- Delegation to `self.inner.typed_graph_handle()` requires zero new state on `EvalServiceLayer`.
- Pattern is identical to `nli_handle()` and `embed_handle()` — consistent codebase style.

Harder:
- If a future external consumer (e.g., a CLI diagnostic command) needs to inspect
  `TypedGraphState`, this accessor would need to be promoted to `pub`. That requires an ADR
  update at that time — acceptable given crt-045 scope.
