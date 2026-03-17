# Component: EntryStore Trait Migration
## File: `crates/unimatrix-core/src/traits.rs` (rewrite)
## New File: `crates/unimatrix-core/tests/impl_completeness.rs`

---

## Purpose

Converts all 18 `EntryStore` methods from synchronous to `async fn` using RPITIT (Rust 1.89).
Removes `dyn EntryStore` compile-tests. Adds impl-completeness test in a new test file.
The trait becomes intentionally non-object-safe (ADR-005).

This component depends on `SqlxStore` existing (Wave 2) — `SqlxStore` is the sole production
implementor. Once the trait is async, `AsyncEntryStore` in `async_wrappers.rs` becomes
compilable for deletion (Wave 3).

---

## Trait Definition

```rust
/// Async-native entry store trait. All 18 methods are `async fn` via RPITIT (Rust 1.89).
///
/// # Object Safety
///
/// This trait uses native `async fn` (RPITIT, Rust 1.89+) and is **not object-safe**.
/// `dyn EntryStore` is not supported. Use generic bounds `T: EntryStore` or refer to
/// the concrete `SqlxStore` type directly. This is intentional: `SqlxStore` is the sole
/// production implementor, and boxing futures on every call would incur unnecessary
/// allocation overhead on the MCP hot path. (ADR-005, SR-02)
pub trait EntryStore: Send + Sync {
    // --- Core CRUD ---

    async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;

    async fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;

    async fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError>;

    async fn delete(&self, id: u64) -> Result<(), CoreError>;

    // --- Read operations ---

    async fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;

    async fn exists(&self, id: u64) -> Result<bool, CoreError>;

    async fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>;

    async fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError>;

    async fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError>;

    async fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>;

    async fn query_by_time_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<EntryRecord>, CoreError>;

    async fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError>;

    // --- Vector mapping (integrity writes) ---

    async fn put_vector_mapping(
        &self,
        entry_id: u64,
        hnsw_data_id: u64,
    ) -> Result<(), CoreError>;

    async fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError>;

    async fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError>;

    // --- Counters (integrity reads) ---

    async fn read_counter(&self, name: &str) -> Result<u64, CoreError>;

    // --- Access recording (analytics write via enqueue_analytics) ---

    async fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;

    // --- Confidence update (integrity write) ---

    async fn update_confidence(&self, id: u64, confidence: f64) -> Result<(), CoreError>;
}
```

Note: `record_access` on the trait is implemented in `SqlxStore` by calling
`self.enqueue_analytics(AnalyticsWrite::CoAccess { ... })`. The trait method signature is
`async fn` for consistency, but the implementation body is synchronous internally
(enqueue_analytics is non-async). The `async` on the trait is correct — the caller awaits
the trait method; the implementation can return immediately.

The exact 18 methods: verify against the current `traits.rs` and `async_wrappers.rs`.
The list above is representative. If the current trait has different method names, the
implementation follows the existing names exactly.

---

## Deletion: Object-Safety Tests

The following pattern in the current `traits.rs` is deleted:

```rust
// DELETE THESE — they would become compile errors (not tests) after RPITIT conversion:
#[test]
fn entry_store_is_object_safe() {
    fn _check(_: &dyn EntryStore) {}
}
```

---

## New File: `impl_completeness.rs`

```rust
// crates/unimatrix-core/tests/impl_completeness.rs

use unimatrix_core::traits::EntryStore;
use unimatrix_store::{SqlxStore, PoolConfig};

/// Compile-time gate: asserts that S implements all EntryStore methods.
/// Fails to compile with "method not found in S" if any method is missing.
/// Does NOT test runtime behavior — it is a compile-time completeness check.
fn assert_entry_store_impl<S: EntryStore + Send + Sync>(_: &S) {}

/// Compile-time gate: asserts that Arc<S> is Send + Sync (required for tokio::spawn contexts).
fn assert_send_sync<T: Send + Sync>(_: T) {}

/// Verifies SqlxStore satisfies EntryStore + Send + Sync (AC-20, R-07).
#[tokio::test]
async fn test_sqlx_store_implements_entry_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("impl_completeness_test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open store");

    assert_entry_store_impl(&store);
    store.close().await;
}

/// Verifies Arc<SqlxStore> is Send + Sync (required for tokio::spawn in background.rs).
#[tokio::test]
async fn test_arc_sqlx_store_is_send_sync() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("send_sync_test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open store");
    let arc_store = std::sync::Arc::new(store);
    assert_send_sync(Arc::clone(&arc_store));

    // Ensure arc can be moved into tokio::spawn (compile-time verification via closure).
    let store_clone = Arc::clone(&arc_store);
    let handle = tokio::spawn(async move {
        let _ = store_clone.shed_events_total();
    });
    handle.await.unwrap();

    // Close via unwrap (Arc has single owner after join).
    Arc::try_unwrap(arc_store).unwrap().close().await;
}
```

---

## `SqlxStore` implements `EntryStore` (in unimatrix-store/src/db.rs)

```rust
impl EntryStore for SqlxStore {
    async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        self.write_entry(entry)
            .await
            .map_err(CoreError::Store)
    }

    async fn update(&self, entry: EntryRecord) -> Result<(), CoreError> {
        self.update_entry(entry)
            .await
            .map_err(CoreError::Store)
    }

    async fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        // Enqueue co_access pairs for all combinations (analytics path).
        for i in 0..entry_ids.len() {
            for j in (i + 1)..entry_ids.len() {
                self.enqueue_analytics(AnalyticsWrite::CoAccess {
                    id_a: entry_ids[i],
                    id_b: entry_ids[j],
                });
            }
        }
        Ok(())
    }

    // ... implement all 18 methods delegating to self's read/write methods ...
}
```

---

## Error Handling

All `EntryStore` trait methods return `Result<T, CoreError>`. The `SqlxStore` implementation
maps `StoreError` to `CoreError::Store(store_error)`. No panics.

The caller (server crate tools) receives `CoreError` which maps to `ServerError` via the
existing error conversion chain.

---

## Key Test Scenarios

1. **`test_sqlx_store_implements_entry_store`** (AC-20): Compile-time completeness gate.
   Fails at compile time if any method is missing. No assertion needed — if it compiles, it passes.

2. **`test_arc_sqlx_store_is_send_sync`** (R-07): Verifies Send + Sync bounds required for
   `tokio::spawn` contexts in `background.rs`.

3. **`test_entry_store_insert_delegates_to_write_entry`**: Call `EntryStore::insert()` on
   `SqlxStore`; assert entry is readable via `get()`.

4. **`test_entry_store_record_access_enqueues_co_access`**: Call `record_access(&[1, 2, 3])`;
   close store; assert co_access table has 3 pairs: (1,2), (1,3), (2,3).

5. **`test_no_dyn_entry_store_in_codebase`** (AC-04): Grep-level CI check that
   `dyn EntryStore` produces zero matches in production code. Verified by CI, not a unit test.

---

## OQ-DURING Items Affecting This Component

None. ADR-005 fully specifies the RPITIT approach. No `async_trait` crate is introduced.
