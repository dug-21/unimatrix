## ADR-005: Native async fn in EntryStore Trait (RPITIT)

### Context

The `EntryStore` trait in `unimatrix-core/src/traits.rs` has 18 synchronous methods.
The `AsyncEntryStore<T>` wrapper in `async_wrappers.rs` wraps each method in
`tokio::task::spawn_blocking` to make them callable from async contexts. This is the
structural bridge this feature retires.

To make `EntryStore` methods async-native, two approaches exist:

**Option A: `async_trait` crate**

The `async_trait` proc-macro rewrites `async fn` in traits to return
`Pin<Box<dyn Future<Output = T> + Send>>`. This makes the trait object-safe.

Pros: Object-safe (`dyn EntryStore` works). Widely used in pre-1.75 Rust.
Cons:
- Introduces a new crate dependency (`async_trait`). The workspace is on Rust 1.89 where
  this is no longer needed.
- Every method invocation allocates a `Box<dyn Future>`. On the hot path (every MCP tool
  call invokes multiple store methods), this is measurable allocation overhead.
- The crate must not be introduced per constraint C-02 in SPECIFICATION.md.

**Option B: Native RPITIT (return-position `impl Trait` in traits)**

Rust 1.75+ allows `async fn` in trait definitions natively. Each method's return type is
an opaque future type inferred by the compiler. The workspace is pinned to `rust-version =
"1.89"` (per Cargo.toml), so this is stable and available.

Pros:
- Zero-cost: no boxing, no allocation per call.
- No external dependency.
- The trait is expressive and idiomatic Rust 1.89+ style.
- Consistent with the project's Rust version commitment.

Cons:
- The trait is **not object-safe**. `dyn EntryStore` is a compile error. This means:
  - No `Arc<dyn EntryStore>` patterns in production code.
  - Generic bounds (`T: EntryStore`) must replace `dyn EntryStore` everywhere.
  - The existing `dyn EntryStore` compile-tests in `traits.rs` must be removed (they
    would now fail to compile as expected, but for the wrong reason — they would become
    genuine compile errors, not "this compiles, therefore the trait is object-safe" tests).

**Object-safety: current usage audit**

The existing tests in `traits.rs` assert `dyn EntryStore` compiles (object-safety check).
These are test-only patterns; no production code holds `dyn EntryStore` in an
`Arc` and calls methods polymorphically across different `EntryStore` implementations.
`SqlxStore` is the **sole production implementor** of `EntryStore`. There is no
production site that needs to swap implementations at runtime via `dyn EntryStore`.

The `AsyncEntryStore<T>` wrapper was the only consumer of the trait's object-safety surface
— and it is being retired. After retirement, no production code requires `dyn EntryStore`.

**Replacement for object-safety tests: impl-completeness tests**

RPITIT async traits cannot be used as `dyn T` trait objects, so the old compile-check
pattern:
```rust
fn _check(_: &dyn EntryStore) {}
```
becomes a hard compile error (not a valid test). It is replaced with an
impl-completeness test:

```rust
// Asserts that SqlxStore provides concrete implementations for all 18 methods.
// Fails to compile if any method is missing.
fn assert_entry_store_impl<S: EntryStore>(_: &S) {}

#[tokio::test]
async fn sqlx_store_implements_entry_store() {
    let store = SqlxStore::open(temp_db_path(), PoolConfig::test_default()).await.unwrap();
    assert_entry_store_impl(&store);
    store.close().await;
}
```

This test is a compile-time correctness gate: if `SqlxStore` is missing any `EntryStore`
method, `assert_entry_store_impl(&store)` fails to compile with a specific "method not found
in `SqlxStore`" error pointing to the missing method.

**`where Self: Sized` bounds:** Not needed. RPITIT async traits in Rust 1.89 do not
require `where Self: Sized` on individual methods unless you need to selectively restore
object-safety for specific methods. Since the entire trait is non-object-safe (by design),
no such bounds are needed.

**SR-02 documentation:** The architect must add a doc comment to the `EntryStore` trait:

```rust
/// # Object Safety
///
/// This trait uses native `async fn` (RPITIT, Rust 1.89+) and is **not object-safe**.
/// `dyn EntryStore` is not supported. Use generic bounds `T: EntryStore` or refer to
/// the concrete `SqlxStore` type. This is intentional: the trait has a single production
/// implementor (`SqlxStore`) and boxing futures on every call would incur unnecessary
/// allocation overhead on the MCP hot path.
```

### Decision

All 18 `EntryStore` methods become native `async fn` using RPITIT (Rust 1.89). The
`async_trait` crate is not introduced. `dyn EntryStore` is not supported and must not
appear in production code.

The object-safety compile tests in `unimatrix-core/src/traits.rs` are deleted and replaced
with impl-completeness tests in `unimatrix-core/tests/impl_completeness.rs` (AC-20).

A doc comment on the `EntryStore` trait documents the non-object-safe design decision
(SR-02).

### Consequences

- Zero heap allocation on every `EntryStore` method call (compared to `async_trait`'s
  `Box<dyn Future>`). Directly benefits the MCP hot path.
- `async_trait` crate is not a dependency. Constraint C-02 is satisfied.
- All call sites that used `Arc<dyn EntryStore>` or `Box<dyn EntryStore>` are updated to
  use `Arc<SqlxStore>` (concrete type) or `impl EntryStore` generic bounds where needed.
  In practice there are zero such sites in production code (audit confirmed above).
- Server code that currently calls `async_store.method().await` (via `AsyncEntryStore`)
  calls `store.method().await` directly on `Arc<SqlxStore>`.
- Future code that needs to mock `EntryStore` in tests cannot use `dyn EntryStore`. Test
  mocks must implement `EntryStore` as a concrete type. This is a minor ergonomic cost
  for test writing that is outweighed by zero-cost async dispatch in production.
- AC-20 is satisfied by the impl-completeness test.
