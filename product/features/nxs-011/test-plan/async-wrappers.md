# Test Plan: AsyncEntryStore Retirement (async_wrappers.rs)

**Component**: `crates/unimatrix-core/src/async_wrappers.rs`
**Risks**: R-07 (RPITIT Send bound failures), R-15 (spawn_blocking residual)
**ACs**: AC-04
**Spec reference**: FR-10, ARCHITECTURE.md §9

---

## Scope Boundary

Only `AsyncEntryStore<T>` is retired. The following are explicitly OUT OF SCOPE and must not be touched:

- `AsyncVectorStore<T>` — remains in `async_wrappers.rs` (C-06)
- `AsyncEmbedService<T>` — remains in `async_wrappers.rs` (C-06)

The file `async_wrappers.rs` is NOT deleted; it continues to exist with `AsyncVectorStore` and `AsyncEmbedService` intact.

---

## Static Verification (Primary Test Method for this Component)

### AW-S-01: `AsyncEntryStore` is fully removed — (AC-04)
- **Check**: `grep -r "AsyncEntryStore" crates/` returns zero matches
- **Scope**: All crates (unimatrix-core, unimatrix-server, unimatrix-observe, unimatrix-store)
- **Risk**: R-07, R-15

### AW-S-02: `async_wrappers.rs` still exists and compiles
- **Check**: `test -f crates/unimatrix-core/src/async_wrappers.rs` passes
- **Check**: `cargo build -p unimatrix-core` succeeds
- **Assert**: `AsyncVectorStore` and `AsyncEmbedService` are present and unchanged
- **Risk**: C-06 compliance

### AW-S-03: No `spawn_blocking` in `async_wrappers.rs` for store methods
- **Check**: `grep "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs` returns zero matches
- **Note**: If `AsyncVectorStore`/`AsyncEmbedService` still use `spawn_blocking` for CPU-bound operations, that is expected and acceptable (C-06). The check is that NO store-related `spawn_blocking` remains.
- **Risk**: R-15

### AW-S-04: No import of `AsyncEntryStore` in server.rs or observe crates — (AC-04)
- **Check**: `grep -rn "AsyncEntryStore" crates/unimatrix-server/src/ crates/unimatrix-observe/src/` returns zero matches
- **Risk**: AC-04

---

## Compile-Time Test

### AW-C-01: `async_wrappers.rs` module compiles after `AsyncEntryStore` deletion
- **Test**: `cargo test -p unimatrix-core` passes (file compiles without `AsyncEntryStore`)
- **Assert**: `AsyncVectorStore` and `AsyncEmbedService` remain functional (no accidental removal)
- **Risk**: R-07

---

## Unit Test

### AW-U-01: `test_async_entry_store_absent_from_module`
- **File**: `crates/unimatrix-core/tests/async_wrappers_tests.rs` (or inline module test)
- **Method**: This is a compile-time test — the test body simply imports `async_wrappers` and confirms `AsyncVectorStore` is accessible

```rust
#[test]
fn async_vector_store_still_accessible() {
    // Compile-time only: if AsyncVectorStore was accidentally deleted, this import fails
    use unimatrix_core::async_wrappers::AsyncVectorStore;
    let _ = std::marker::PhantomData::<AsyncVectorStore<()>>;
}
```

- **Assert (compile-time)**: `AsyncVectorStore` is still exported
- **Assert (compile-time)**: `AsyncEntryStore` is NOT exported (attempting to use it would fail to compile — the absence is verified by AW-S-01)
- **Risk**: C-06 compliance

---

## Notes

- The primary verification for this component is grep-based (AW-S-01 through AW-S-04). The compile-time tests are secondary.
- AW-S-01 is the definitive AC-04 gate. If grep returns any match, the AC fails regardless of compile status.
- The 18 `spawn_blocking`-wrapped methods in the original `AsyncEntryStore` are deleted, not migrated. There is no equivalent test to "verify the old behavior" — the behavior is now tested through direct `SqlxStore` method calls in the sqlx-store.md tests.
