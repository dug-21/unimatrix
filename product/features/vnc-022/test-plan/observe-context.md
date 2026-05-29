# Test Plan: observe-context

Component: `crates/unimatrix-server/src/http/router.rs` — ObserveContext struct + PathRouter integration

Covers: R-01, R-11

## Design Constraints

ObserveContext is a `pub(crate)` struct with 9 `Arc`-wrapped fields. It must:
1. Derive `Clone` (tower::Service requires Clone on PathRouter)
2. Contain every field that dispatch_request needs
3. Be constructible from UnimatrixServer fields in main.rs

## Compilation Gate Tests

### PathRouter with ObserveContext compiles as tower::Service

Assert: `cargo build --workspace` succeeds. PathRouter<ReqBody> implements `tower::Service<Request<ReqBody>>`. If ObserveContext doesn't derive Clone, this fails.

This is the primary R-11 gate.

### ObserveContext fields match dispatch_request parameters

Structural verification: The ObserveContext struct fields must map 1:1 to dispatch_request parameters (excluding `request`, `capabilities`, and `server_version` which is a String not an Arc).

Expected fields (from IMPLEMENTATION-BRIEF.md):
```
store: Arc<Store>
embed_service: Arc<EmbedServiceHandle>
vector_store: Arc<AsyncVectorStore<VectorAdapter>>
entry_store: Arc<Store>
adapt_service: Arc<AdaptationService>
server_version: String
session_registry: Arc<SessionRegistry>
pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>
services: ServiceLayer
```

Test: Code review + compiler will catch type mismatches when the handler calls dispatch_request with ObserveContext fields.

## Unit Tests

Location: `crates/unimatrix-server/src/http/router/tests.rs` (extend existing module)

### test_observe_context_clone

Arrange: If test infrastructure allows constructing an ObserveContext (may require TestHarness), construct one.
Act: `let cloned = ctx.clone()`
Assert: Clone succeeds. All Arc fields point to same allocation (Arc::strong_count increases by 1 for each field).

Note: If constructing a full ObserveContext in unit tests is impractical (requires store, embed_service, etc.), this is validated by the compilation gate alone. All fields are Arc-wrapped, and `#[derive(Clone)]` on a struct of Arc fields always works.

### test_path_router_new_accepts_observe_context

Arrange: Construct PathRouter with `ProjectRouter` and `ObserveContext`.
Act: `PathRouter::new(project_router, observe_ctx)`
Assert: Construction succeeds, PathRouter has `observe_ctx` field.

## Integration Tests

### test_observe_handler_exercises_all_service_handles (R-01)

This is the key R-01 test. Covered in observe-handler.md but traced here because R-01 is about ObserveContext correctness:

1. ContextSearch -> exercises `embed_service`, `services.search` (store, vector_store, entry_store)
2. SessionRegister -> exercises `session_registry`
3. CompactPayload -> exercises `services`, `adapt_service`, `session_registry`
4. RecordEvent -> exercises `store` (observation persistence), `session_registry`, `pending_entries_analysis`

If any ObserveContext field is missing or incorrectly wired, the corresponding integration test fails.

## Risk Trace

| Risk | Scenario | Test |
|------|----------|------|
| R-01 | Missing field on ObserveContext | Compilation failure + integration tests per service handle |
| R-01 | Stale/wrong Arc field | Integration test produces wrong result or panic |
| R-11 | Clone not derivable | Compilation gate: PathRouter as tower::Service requires Clone |
| R-11 | Expensive Clone degrades throughput | All fields are Arc (cheap clone). No runtime test needed. |
