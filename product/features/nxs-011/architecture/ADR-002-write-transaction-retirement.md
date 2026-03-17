## ADR-002: SqliteWriteTransaction Retirement Strategy

### Context

`SqliteWriteTransaction<'a>` in `txn.rs` holds a `MutexGuard<'a, Connection>` as a public
field (`pub guard: MutexGuard<'a, Connection>`). This design has two problems that make it
incompatible with the async migration:

1. **Lifetime incompatibility**: `MutexGuard<'a, Connection>` cannot cross `.await` points.
   Any code that holds a `SqliteWriteTransaction` across an await is a compile error.
2. **Public `guard` field**: Downstream crates access `txn.guard` directly to execute SQL,
   which means the `Connection` lifetime escapes the transaction abstraction entirely.

The 5 call sites that use `begin_write()` + `txn.guard`:
- `server.rs` ×3 (bulk writes, transactional corrections, audit log writes)
- `store_correct.rs` (correction chain update)
- `store_ops.rs` (multi-table atomic operations)
- `audit.rs` (transactional audit event write)

**Two options were considered:**

**Option A: Typed wrapper `WriteTransaction<'_>` over `sqlx::Transaction<'_, Sqlite>`**

Introduce a new `WriteTransaction<'_>` struct that wraps `sqlx::Transaction<'_, Sqlite>` and
exposes a helper API:

```rust
pub struct WriteTransaction<'c> {
    inner: sqlx::Transaction<'c, sqlx::Sqlite>,
}

impl<'c> WriteTransaction<'c> {
    pub async fn execute(&mut self, query: &str) -> Result<()> { ... }
    pub async fn commit(self) -> Result<()> { ... }
}
```

Pros: ergonomic, limits surface area, familiar API shape.
Cons: introduces a new type that must be maintained; the 5 call sites need raw query access
for varied SQL operations — the wrapper would either need to be very thin (defeating its
purpose) or replicate the complexity of `sqlx::Transaction` with a custom interface. Five
call sites do not constitute a pattern worth abstracting.

**Option B: Direct `pool.begin().await?` at each call site**

Each of the 5 call sites acquires a transaction directly:

```rust
let mut txn = store.write_pool.begin().await?;
sqlx::query!("...").execute(&mut *txn).await?;
txn.commit().await?;
// Rollback on Drop: sqlx::Transaction drops with ROLLBACK if not committed.
```

Pros: idiomatic sqlx; no new type; each call site is self-contained; rollback on `?`-early-exit
is automatic (sqlx::Transaction's Drop impl rolls back uncommitted transactions); no wrapper
to maintain; 5 call sites do not warrant an abstraction layer.
Cons: slightly more verbose at each call site; callers need `&mut *txn` syntax to pass the
transaction as an `Executor`.

The `&mut *txn` dereference syntax (`execute(&mut *txn)`) is standard sqlx idiom documented
in the sqlx transaction guide and will be familiar to implementers reading the migrated code.

### Decision

**Option B: direct `pool.begin().await?` at each call site.** No typed wrapper is introduced.
`txn.rs` is deleted.

Call sites that previously acquired a guard and ran SQL within the transaction scope will
use `sqlx::Transaction<'_, Sqlite>` directly, passed as `&mut *txn` to `sqlx::query!()`
calls.

The `SqliteWriteTransaction` type is removed from the public API of `unimatrix-store`. The
`Store::begin_write()` method is removed. All downstream call sites that imported or called
these are updated.

Rollback semantics are preserved: `sqlx::Transaction` rolls back automatically on `Drop`
when not committed, mirroring the behavior of the old `Drop` impl in `SqliteWriteTransaction`.

### Consequences

- `txn.rs` is deleted. `SqliteWriteTransaction` and `begin_write()` are gone from the
  public API.
- All 5 call sites are updated to `pool.begin().await?` + `txn.commit().await?`.
- AC-16 is satisfied: no `MutexGuard` lifetime escapes any function boundary.
- `async fn` call sites that use `?` inside a transaction body trigger automatic rollback
  via `Drop` — this is the correct async behavior (same semantics as the old sync `Drop`).
- No new maintenance surface is created for a wrapper type.
- The 5 call sites are slightly more verbose (~3 lines each vs. 2 with a wrapper) — an
  acceptable trade for no new abstraction.
