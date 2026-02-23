## ADR-002: Unified Core Error Type

### Context

Each existing crate has its own error type: `StoreError`, `VectorError`, `EmbedError`. Core traits need a single error type so consumers don't handle three different Result types. Options:

1. Use `Box<dyn std::error::Error>` as the error type in traits
2. Create a `CoreError` enum with variants wrapping each crate's error
3. Use `anyhow::Error` for trait methods

Option 1 loses type information. Option 3 adds an external dependency and loses exhaustive matching. Option 2 preserves error specificity while providing a unified interface.

### Decision

Define `CoreError` in `unimatrix-core/src/error.rs`:

```rust
pub enum CoreError {
    Store(StoreError),
    Vector(VectorError),
    Embed(EmbedError),
    JoinError(String),  // tokio spawn_blocking failure
}
```

Implement `From<StoreError>`, `From<VectorError>`, `From<EmbedError>` for ergonomic `?` operator usage in adapters. The `JoinError` variant handles async wrapper failures (task panics, runtime shutdown).

### Consequences

- **Easier**: Consumers use one error type. `?` operator works naturally in adapter implementations.
- **Easier**: Consumers can still match on specific variants when needed (e.g., `CoreError::Store(StoreError::EntryNotFound(id))`).
- **Harder**: Adding a new crate-level error type requires updating CoreError. But this is rare and mechanical.
