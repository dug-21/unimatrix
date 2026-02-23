# Pseudocode: C7 Server State Extension

## File: `crates/unimatrix-server/src/server.rs`

### Change: Add `vector_index` field to `UnimatrixServer`

```rust
pub struct UnimatrixServer {
    // ... existing fields ...
    pub(crate) entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    pub(crate) vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    pub(crate) embed_service: Arc<EmbedServiceHandle>,
    pub(crate) registry: Arc<AgentRegistry>,
    pub(crate) audit: Arc<AuditLog>,
    pub(crate) categories: Arc<CategoryAllowlist>,
    pub(crate) store: Arc<Store>,
    // NEW:
    pub(crate) vector_index: Arc<VectorIndex>,
    // ... tool_router, server_info unchanged ...
}
```

### Change: Update `UnimatrixServer::new()` signature

```
pub fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,  // NEW parameter
) -> Self:
    // ... existing server_info construction ...
    UnimatrixServer {
        entry_store,
        vector_store,
        embed_service,
        registry,
        audit,
        categories,
        store,
        vector_index,  // NEW
        tool_router: Self::tool_router(),
        server_info,
    }
```

### Change: Update `make_server()` test helper

```
pub(crate) fn make_server() -> UnimatrixServer:
    // ... existing setup ...
    let vector_index = Arc::new(
        unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap(),
    );
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));  // Clone Arc
    let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // ... existing embed_service, registry, audit, categories setup ...

    UnimatrixServer::new(
        entry_store,
        vector_store,
        embed_service,
        registry,
        audit,
        categories,
        Arc::clone(&store),
        vector_index,  // NEW: pass vector_index to server
    )
```

### Change: Update `main.rs` server construction

```
let server = UnimatrixServer::new(
    async_entry_store,
    async_vector_store,
    Arc::clone(&embed_handle),
    Arc::clone(&registry),
    Arc::clone(&audit),
    categories,
    Arc::clone(&store),
    Arc::clone(&vector_index),  // NEW
);
```

### Import Required

```rust
use unimatrix_core::VectorIndex;  // or unimatrix_vector::VectorIndex
```

Note: VectorIndex is re-exported through unimatrix_core, so the import path
depends on what is already in scope. The existing code uses `unimatrix_core::VectorIndex`
in the test helper.
