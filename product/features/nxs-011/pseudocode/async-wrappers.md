# Component: AsyncEntryStore Retirement (async_wrappers.rs)
## File: `crates/unimatrix-core/src/async_wrappers.rs` (partial rewrite)

---

## Purpose

Deletes `AsyncEntryStore<T>` (18 `spawn_blocking`-wrapped methods) from `async_wrappers.rs`.
The file is NOT deleted — it retains `AsyncVectorStore<T>` and `AsyncEmbedService<T>` which
are explicitly out of scope (C-06). The file continues to exist with two structs instead of
three.

This component depends on `EntryStore` being async (Wave 3 entry-store-trait) because
`AsyncEntryStore` is used in the server crate which must compile after both changes.

---

## What Is Deleted

The entire `AsyncEntryStore<T>` struct and its `impl` block:

```rust
// DELETE ENTIRELY from async_wrappers.rs:

pub struct AsyncEntryStore<T: EntryStore> {
    inner: Arc<T>,
}

impl<T: EntryStore + Send + Sync + 'static> AsyncEntryStore<T> {
    pub fn new(inner: Arc<T>) -> Self { ... }

    // All 18 spawn_blocking-wrapped methods:
    pub async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.insert(entry)).await...
    }
    // ... 17 more methods ...
}
```

---

## What Is Retained (untouched per C-06)

```rust
// KEEP UNCHANGED — do not modify, move, or remove:

pub struct AsyncVectorStore<T: VectorStore> {
    // ... existing fields ...
}

impl<T: VectorStore + Send + Sync + 'static> AsyncVectorStore<T> {
    // ... all existing methods unchanged ...
}

pub struct AsyncEmbedService<T: EmbedService> {
    // ... existing fields ...
}

impl<T: EmbedService + Send + Sync + 'static> AsyncEmbedService<T> {
    // ... all existing methods unchanged ...
}
```

---

## File Structure After Change

```rust
// crates/unimatrix-core/src/async_wrappers.rs — after nxs-011

// [module doc comment]
// Note: AsyncEntryStore was removed in nxs-011. EntryStore is now async-native (RPITIT).
// AsyncVectorStore and AsyncEmbedService remain as spawn_blocking wrappers for
// CPU-bound HNSW and ONNX operations (out of scope for nxs-011, C-06).

// AsyncVectorStore (unchanged)
pub struct AsyncVectorStore<T: VectorStore> { ... }
impl<T: VectorStore + Send + Sync + 'static> AsyncVectorStore<T> { ... }

// AsyncEmbedService (unchanged)
pub struct AsyncEmbedService<T: EmbedService> { ... }
impl<T: EmbedService + Send + Sync + 'static> AsyncEmbedService<T> { ... }
```

---

## Caller Update (server.rs — simplified)

The server's startup code that previously held an `AsyncEntryStore` is simplified:

```rust
// BEFORE (server.rs startup):
let store = Arc::new(Store::open(db_path)?);
let async_store = AsyncEntryStore::new(Arc::clone(&store));
// ... pass async_store to tool handlers ...

// AFTER (server.rs startup):
let store = Arc::new(SqlxStore::open(db_path, PoolConfig::default()).await?);
// No async_store wrapper needed. Pass Arc::clone(&store) directly to handlers.
```

All function signatures that previously accepted `async_store: AsyncEntryStore<Arc<Store>>`
are updated to accept `store: Arc<SqlxStore>` and call methods via `.await` directly.

The full list of affected server functions is in `server-migration.md`.

---

## Error Handling

No new error types introduced. The deletion cannot introduce errors — it only removes code.
Any compile error from the deletion is a call-site issue that server-migration.md addresses.

---

## Key Test Scenarios

1. **`test_no_async_entry_store_imports`** (AC-04): CI grep check —
   `grep -r "AsyncEntryStore" crates/` returns zero matches. This is the definitive gate.

2. **`test_async_vector_store_unchanged`**: Run existing tests for `AsyncVectorStore`;
   assert all pass without modification. If any test in `async_wrappers.rs` currently tests
   `AsyncEntryStore`, delete that test.

3. **`test_async_embed_service_unchanged`**: Same for `AsyncEmbedService`.

4. **Compile-time**: The server crate must compile cleanly after `AsyncEntryStore` is
   removed. Any remaining `use unimatrix_core::async_wrappers::AsyncEntryStore` causes a
   compile error that the server-migration wave must resolve first.

---

## OQ-DURING Items Affecting This Component

None. C-06 is a hard constraint; `AsyncVectorStore` and `AsyncEmbedService` are untouched.
The deletion of `AsyncEntryStore` is a mechanical removal with no design decisions.
