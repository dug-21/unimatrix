## ADR-003: Feature-Gated Async Wrappers

### Context

The MCP server (vnc-001) will be async (tokio-based), but all three foundation crates are synchronous. The established pattern from ASS-003 research is `Arc<T> + tokio::task::spawn_blocking`. Options:

1. Add async methods directly to core traits (requires `async_trait` or native async traits)
2. Create a separate `unimatrix-async` crate for async wrappers
3. Feature-gate async wrappers in `unimatrix-core` behind `async = ["tokio"]`

Option 1 forces async on all trait implementations, even sync-only consumers. Rust's native async traits have limitations around object safety and `Send` bounds. Option 2 creates another crate for thin delegation code. Option 3 keeps the base crate async-free while providing opt-in async support.

### Decision

Feature-gate async wrappers in `unimatrix-core`:

```toml
[features]
async = ["dep:tokio"]

[dependencies]
tokio = { version = "1", features = ["rt"], optional = true }
```

The `async_wrappers` module is conditionally compiled:
```rust
#[cfg(feature = "async")]
pub mod async_wrappers;
```

Async wrapper structs (`AsyncEntryStore<T>`, `AsyncVectorStore<T>`, `AsyncEmbedService<T>`) take `Arc<T>` and delegate via `spawn_blocking`.

### Consequences

- **Easier**: Core traits stay synchronous and object-safe. No `async_trait` dependency needed.
- **Easier**: Sync-only consumers (tests, CLI tools) don't pay for tokio. Async consumers add one feature flag.
- **Easier**: vnc-001 adds `unimatrix-core = { features = ["async"] }` and gets everything.
- **Harder**: Async wrappers are generic structs, not trait methods. This is a different API shape than having async directly on traits. But it's the established Rust pattern for wrapping sync code.
