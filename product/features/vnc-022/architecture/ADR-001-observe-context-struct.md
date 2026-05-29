## ADR-001: ObserveContext Struct for Service Handle Passing

### Context

SR-07 (HIGH): `PathRouter` cannot reach `UnimatrixServer` service handles. The `UnimatrixServer` instance is wrapped inside rmcp's `StreamableHttpService` (via `McpAdapter`) and not directly accessible. The `/observe` handler needs the same 10 `Arc`-wrapped service handles that `dispatch_request()` uses.

SR-01 (HIGH): `dispatch_request` already has 10 parameters. Adding capabilities makes 11. Threading 11 individual `Arc` fields through `PathRouter` -> `ProjectRouter` -> handler is fragile and couples every layer to the parameter list.

Three options considered:
- **(A) Store full `UnimatrixServer` on `PathRouter`**: Simple, but `UnimatrixServer` is 20+ fields and carries the MCP tool router, server_info, and other MCP-specific state irrelevant to `/observe`. Leaks concerns.
- **(B) Store individual Arc fields on `PathRouter`**: 10 fields on `PathRouter`, 10 on its `Clone` impl, 10 in `new()`. Grows linearly with dispatch_request parameters.
- **(C) Define `ObserveContext` struct bundling the required handles**: One field on `PathRouter`, one in `Clone`, one in `new()`. Internal field changes are isolated to `ObserveContext`.

### Decision

Option (C): Define `ObserveContext` in `http/router.rs`:

```rust
#[derive(Clone)]
pub(crate) struct ObserveContext {
    pub(crate) store: Arc<Store>,
    pub(crate) embed_service: Arc<EmbedServiceHandle>,
    pub(crate) vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    pub(crate) entry_store: Arc<Store>,
    pub(crate) adapt_service: Arc<AdaptationService>,
    pub(crate) server_version: String,
    pub(crate) session_registry: Arc<SessionRegistry>,
    pub(crate) pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    pub(crate) services: ServiceLayer,
}
```

Constructed in `main.rs` from `UnimatrixServer` fields (all `Arc::clone`). Passed to `PathRouter::new(project_router, observe_ctx)`. `PathRouter` stores it as a single field and passes references to the `/observe` handler.

`server_version` is `String` (not `Arc`) because it is `env!("CARGO_PKG_VERSION").to_string()` — small, cheap to clone, immutable.

### Consequences

- `PathRouter` gains one field (`observe_ctx: ObserveContext`), not 10. Clone impl stays trivial (derive).
- If `dispatch_request` gains or loses a parameter, only `ObserveContext` definition and `main.rs` construction change. `PathRouter`, `ProjectRouter`, and the handler signature are insulated.
- `main.rs` constructor is ~12 lines of `Arc::clone` — mechanical, no logic.
- The struct is `pub(crate)` — not part of any public API. Internal refactoring is free.
- `ObserveContext` is intentionally NOT the same as `UnimatrixServer` — it carries only what `dispatch_request` needs, not MCP-specific state (tool_router, server_info, registry, audit, categories, etc.).
