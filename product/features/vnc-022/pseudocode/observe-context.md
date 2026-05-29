# observe-context: Service Handle Bundle for /observe Handler

## Purpose

Define the `ObserveContext` struct that bundles the 9 Arc-cloned service handles needed by `dispatch_request`. Store it as a single field on `PathRouter`. Construct it in `main.rs` from `UnimatrixServer` fields before rmcp wrapping. This solves SR-07 (PathRouter cannot reach service handles) and SR-01 (parameter sprawl).

## File 1: `crates/unimatrix-server/src/http/router.rs`

### New Struct: ObserveContext

Insert after imports, before the `PathRouter` struct definition (before line 44):

```
/// Service handle bundle for the /observe handler (ADR-001).
///
/// Holds Arc-cloned references to the subset of UnimatrixServer fields
/// needed by dispatch_request(). Constructed once in main.rs, stored
/// on PathRouter, referenced by the /observe handler.
///
/// Intentionally NOT the same as UnimatrixServer -- carries only what
/// dispatch_request needs, not MCP-specific state.
#[derive(Clone)]
pub struct ObserveContext {
    pub store: Arc<Store>,
    pub embed_service: Arc<EmbedServiceHandle>,
    pub vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    pub entry_store: Arc<Store>,
    pub adapt_service: Arc<AdaptationService>,
    pub server_version: String,
    pub session_registry: Arc<SessionRegistry>,
    pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    pub services: ServiceLayer,
}
```

### Required Imports in router.rs

Add to the existing import block:

```
use std::sync::Mutex;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceLayer;
use unimatrix_adapt::AdaptationService;
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{Store, VectorAdapter};
```

Note: Check which of these are already imported. `Arc` is already imported via `std::sync::Arc`. `Store` may need to be added.

### Modified Struct: PathRouter

**Current** (line 44-51):
```
pub struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    project_router: ProjectRouter<ReqBody>,
}
```

**After**:
```
pub struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    project_router: ProjectRouter<ReqBody>,
    observe_ctx: ObserveContext,
}
```

### Modified: PathRouter Debug impl

Add `observe_ctx` to the debug output:
```
fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("PathRouter")
        .field("project_router", &self.project_router)
        .field("observe_ctx", &"ObserveContext{..}")
        .finish()
}
```

Note: ObserveContext does not derive Debug (its fields include types like ServiceLayer that may not implement Debug). Use a string placeholder in the Debug impl.

### Modified: PathRouter::new

**Current** (line 73):
```
pub fn new(project_router: ProjectRouter<ReqBody>) -> Self {
    PathRouter { project_router }
}
```

**After**:
```
pub fn new(project_router: ProjectRouter<ReqBody>, observe_ctx: ObserveContext) -> Self {
    PathRouter { project_router, observe_ctx }
}
```

### Modified: PathRouter Clone impl

**Current** (line 84-88):
```
fn clone(&self) -> Self {
    PathRouter {
        project_router: self.project_router.clone(),
    }
}
```

**After**:
```
fn clone(&self) -> Self {
    PathRouter {
        project_router: self.project_router.clone(),
        observe_ctx: self.observe_ctx.clone(),
    }
}
```

`ObserveContext` derives Clone. All fields are Arc (cheap clone) or String (small, immutable) or ServiceLayer (implements Clone). This is correct for tower::Service which requires Clone.

## File 2: `crates/unimatrix-server/src/main.rs`

### Modified: tokio_main_daemon (HTTP listener block, ~line 810)

**Current** (line 827-828):
```
let project_router = ProjectRouter::new(server.clone(), config.http.max_request_body_bytes);
let path_router = PathRouter::new(project_router);
```

**After**:
```
// vnc-022: Construct ObserveContext from server fields before rmcp wrapping.
// All fields are Arc::clone -- cheap, no logic.
let observe_ctx = unimatrix_server::http::ObserveContext {
    store: Arc::clone(&store),
    embed_service: Arc::clone(&embed_handle),
    vector_store: Arc::clone(&async_vector_store),
    entry_store: Arc::clone(&store),
    adapt_service: Arc::clone(&adapt_service),
    server_version: env!("CARGO_PKG_VERSION").to_string(),
    session_registry: Arc::clone(&session_registry),
    pending_entries_analysis: Arc::clone(&pending_entries_analysis),
    services: services.clone(),
};

let project_router = ProjectRouter::new(server.clone(), config.http.max_request_body_bytes);
let path_router = PathRouter::new(project_router, observe_ctx);
```

### Field Mapping (UnimatrixServer -> ObserveContext)

| ObserveContext field | Source variable in main.rs | Type |
|---------------------|---------------------------|------|
| store | `store` | `Arc<Store>` |
| embed_service | `embed_handle` | `Arc<EmbedServiceHandle>` |
| vector_store | `async_vector_store` | `Arc<AsyncVectorStore<VectorAdapter>>` |
| entry_store | `store` (same as store) | `Arc<Store>` |
| adapt_service | `adapt_service` | `Arc<AdaptationService>` |
| server_version | `env!("CARGO_PKG_VERSION")` | `String` |
| session_registry | `session_registry` | `Arc<SessionRegistry>` |
| pending_entries_analysis | `pending_entries_analysis` | `Arc<Mutex<PendingEntriesAnalysis>>` |
| services | `services` | `ServiceLayer` |

Note: `store` and `entry_store` both point to the same `Arc<Store>`, matching the existing `dispatch_request` call site at line 478-489 where `&store` and `&store` (aliased as `entry_store`) are both `&Arc<Store>`.

### Visibility: pub Re-export

`main.rs` is a binary target in the same Cargo.toml as the library. In Rust 2021, binaries see the library as an external crate via `use unimatrix_server::...`. Therefore `pub(crate)` items are NOT visible to `main.rs`.

The struct must be `pub` (not `pub(crate)`) and re-exported as `pub` from `http/mod.rs`, following the same pattern as `PathRouter` and `ProjectRouter`.

In `router.rs`, change the struct visibility:
```
#[derive(Clone)]
pub struct ObserveContext { ... }
```

All fields remain `pub(crate)` -- only the struct itself needs to be `pub` for construction from `main.rs`. Field access from `main.rs` works because struct literal construction requires field visibility, so fields must be `pub` too, OR provide a constructor.

**Preferred approach**: Make fields `pub` (matching PathRouter's pattern where fields are accessed directly). The struct is not part of a stable API -- it is an internal implementation detail used only by `main.rs` and `router.rs`.

Add to `crates/unimatrix-server/src/http/mod.rs` (line 17, alongside existing re-exports):
```
pub use router::ObserveContext;
```

### Placement: Before or After server construction?

The ObserveContext must be constructed AFTER `services` is created (line 699-717) and BEFORE `services` is moved into `LifecycleHandles` (line 875). The current insertion point (line 827) is inside the `if config.http.enabled` block, which is after services creation and before lifecycle_handles construction. This is correct.

Important: `services.clone()` is used (not move), because `services` is still needed later for `lifecycle_handles.services = Some(services)`.

## Error Handling

No new error paths. Construction is purely mechanical Arc::clone calls. If any field is missing or has the wrong type, this is a compile-time error.

## Key Test Scenarios

1. **Compilation**: PathRouter with ObserveContext compiles and implements tower::Service (R-11).
2. **Clone works**: PathRouter::clone() produces a valid clone (tower::Service requires Clone). Verified by any integration test that handles multiple concurrent requests.
3. **Field wiring E2E**: Integration test sends ContextSearch via HTTP -> exercises embed_service, vector_store, services handles -> returns Entries (R-01).
4. **Field wiring E2E**: Integration test sends SessionRegister via HTTP -> exercises session_registry handle -> session visible in registry (R-01).
5. **Field wiring E2E**: Integration test sends CompactPayload via HTTP -> exercises adapt_service, services handles -> returns BriefingContent (R-01).
