# ADR-004: Synchronous API with spawn_blocking Delegation to Consumers

## Status

Accepted

## Context

redb provides a synchronous-only API. The primary consumer of `unimatrix-store` is vnc-001 (MCP server), which runs on a tokio async runtime. We need to decide where the sync-to-async boundary lives:

**Option A: Async API in unimatrix-store.** The crate depends on tokio and wraps every operation in `spawn_blocking` internally.

**Option B: Synchronous API in unimatrix-store.** The crate has no async runtime dependency. Consumers wrap calls with `spawn_blocking` and share the database via `Arc<Database>`.

## Decision

Expose a **synchronous Rust API** from `unimatrix-store`. Async wrapping via `tokio::task::spawn_blocking` and database sharing via `Arc<Database>` are the responsibility of downstream consumers.

The public API operates on `&Database` references:

```rust
// Synchronous — no async runtime required
pub fn insert_entry(db: &Database, record: &EntryRecord) -> Result<u64, StoreError>;
pub fn get_by_id(db: &Database, entry_id: u64) -> Result<Option<EntryRecord>, StoreError>;
pub fn query(db: &Database, filter: &QueryFilter) -> Result<Vec<EntryRecord>, StoreError>;
```

Downstream async consumers (vnc-001) apply the standard pattern:

```rust
let db: Arc<Database> = Arc::new(database::open(&path, config)?);

// Wrap sync calls for async context
let db_clone = db.clone();
let entries = tokio::task::spawn_blocking(move || {
    read::query(&db_clone, &filter)
}).await??;
```

## Consequences

**Positive:**
- **No async runtime dependency.** The storage crate depends only on redb, serde, and bincode. No tokio, no async-trait, no runtime overhead.
- **Simpler testing.** Tests are synchronous — no `#[tokio::test]`, no `.await`, no runtime initialization. Faster compilation and clearer test failures.
- **Consumer flexibility.** Different consumers can choose their own async strategy. vnc-001 uses tokio's `spawn_blocking`; a future CLI (nan-001) can call the API directly without any async overhead.
- **Matches redb's API shape.** No impedance mismatch — the public API mirrors redb's natural synchronous transaction model.
- **Proven pattern.** The `Arc<Database>` + `spawn_blocking` approach is production-validated by Iroh (p2p data sync framework built on redb + tokio).

**Negative:**
- **Caller boilerplate.** Every async caller must write `spawn_blocking(move || ...)` around each storage call. Mitigated by: vnc-001 can define thin async wrappers in its own crate, or nxs-004 (Core Traits) can provide an async adapter trait.
- **Arc cloning overhead.** Each `spawn_blocking` call requires an `Arc::clone()`. This is a single atomic increment — negligible at any realistic call frequency.
- **Double error unwrapping.** `spawn_blocking` returns `Result<Result<T, StoreError>, JoinError>`, requiring `??` or flattening. Mitigated by: this is standard tokio boilerplate that vnc-001 handles once in its wrapper layer.
