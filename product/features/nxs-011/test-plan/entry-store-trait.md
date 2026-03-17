# Test Plan: EntryStore Trait Migration (unimatrix-core/traits.rs)

**Component**: `crates/unimatrix-core/src/traits.rs` + `crates/unimatrix-core/tests/impl_completeness.rs`
**Risks**: R-07 (RPITIT Send bound failures)
**ACs**: AC-20
**Spec reference**: FR-09, ADR-005

---

## Compile-Time Tests (impl-completeness.rs)

These tests succeed or fail at **compile time** — no runtime assertion is needed. If the file compiles, the test passes.

### ET-C-01: `test_sqlx_store_implements_entry_store` — (AC-20)

**File**: `crates/unimatrix-core/tests/impl_completeness.rs`

```rust
use unimatrix_core::traits::EntryStore;
use unimatrix_store::{SqlxStore, PoolConfig};

fn assert_entry_store_impl<S: EntryStore + Send + Sync>(_: &S) {}

#[tokio::test]
async fn sqlx_store_implements_entry_store() {
    let store = SqlxStore::open(
        std::env::temp_dir().join("impl_completeness_test.db"),
        PoolConfig::test_default(),
    )
    .await
    .expect("store opens");
    assert_entry_store_impl(&store);
    store.close().await;
}
```

- **Assert (compile-time)**: `SqlxStore` implements all 18 `EntryStore` methods with `async fn`; function call compiles
- **Assert (compile-time)**: `SqlxStore: Send + Sync` — required by the bound
- **Assert (runtime)**: Test opens and closes without panic
- **Risk**: R-07

### ET-C-02: `test_sqlx_store_usable_in_tokio_spawn` — (R-07)

**File**: `crates/unimatrix-core/tests/impl_completeness.rs`

```rust
#[tokio::test]
async fn sqlx_store_send_in_spawn_context() {
    use std::sync::Arc;
    let store = Arc::new(
        SqlxStore::open(
            std::env::temp_dir().join("send_in_spawn_test.db"),
            PoolConfig::test_default(),
        )
        .await
        .expect("store opens"),
    );
    let store2 = Arc::clone(&store);
    let handle = tokio::spawn(async move {
        // A representative async method call across a spawn boundary.
        let _ = store2.query(Default::default()).await;
    });
    handle.await.expect("spawn task completed");
    // Unwrap Arc to close
    match Arc::try_unwrap(store) {
        Ok(s) => s.close().await,
        Err(_) => {} // store2 still alive — won't happen after handle.await
    }
}
```

- **Assert (compile-time)**: `Arc<SqlxStore>` is `Send`; future returned by `store2.query()` is `Send`; `tokio::spawn` compiles
- **Risk**: R-07

### ET-C-03: `test_no_dyn_entry_store_in_test_suite`

**Verification**: grep check — `grep -r "dyn EntryStore" crates/` returns zero matches.

- **Assert**: No `dyn EntryStore` trait object construction anywhere in the codebase (AC-20)
- **Risk**: R-07 (dyn EntryStore is non-object-safe with RPITIT; any remaining usage would fail to compile or represent a test that hasn't been migrated)

---

## Unit Tests

### ET-U-01: `test_entry_store_trait_method_count`

**File**: `crates/unimatrix-core/tests/impl_completeness.rs` or `src/traits.rs`

This is a compile-time coverage check, not a runtime assertion:

- **Verify**: The `EntryStore` trait definition has exactly 18 methods (count manually by reading the trait body — `insert`, `update`, `update_status`, `delete`, `get`, `exists`, `query`, `query_by_topic`, `query_by_category`, `query_by_tags`, `query_by_time_range`, `query_by_status`, `put_vector_mapping`, `get_vector_mapping`, `iter_vector_mappings`, `read_counter`, `record_access` — plus any RPITIT-related associated items)
- **Risk**: R-07 (no method accidentally dropped during conversion)

---

## Static Verification

### ET-S-01: No `async_trait` crate in workspace
- **Check**: `grep -r "async.trait" Cargo.toml crates/unimatrix-core/Cargo.toml` returns zero matches
- **Risk**: ADR-005 compliance (no `async_trait` crate introduced)

### ET-S-02: All 18 EntryStore methods are `async fn`
- **Check**: `grep -A2 "fn insert\|fn update\|fn get\|fn exists\|fn query\|fn delete\|fn put_vector\|fn get_vector\|fn iter_vector\|fn read_counter\|fn record_access" crates/unimatrix-core/src/traits.rs` — each match must have `async fn` keyword
- **Risk**: R-07

### ET-S-03: No `where Self: Sized` on trait methods
- **Check**: `grep "where Self: Sized" crates/unimatrix-core/src/traits.rs` returns zero matches
- **Risk**: R-07 (ADR-005: not needed; would indicate incorrect async trait pattern)

---

## Notes

- ET-C-01 and ET-C-02 are the only tests in `crates/unimatrix-core/tests/impl_completeness.rs`. The file is new (does not exist pre-migration).
- The previous `dyn EntryStore` object-safety tests that were in `unimatrix-core/src/traits.rs` must be deleted (not converted). They are replaced by ET-C-01, ET-C-02, and ET-S-03.
- The `assert_entry_store_impl` helper function must be `fn` (not `async fn`) — it takes a reference and does nothing, serving purely as a compile-time bound check.
- If the `StoreAdapter` in `unimatrix-core` also implements `EntryStore`, a second impl-completeness call for `StoreAdapter` should be added to ET-C-01.
