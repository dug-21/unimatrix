## ADR-003: Migration Connection Sequencing

### Context

`migration.rs` (983 lines) currently receives `&Store` (which exposes `lock_conn()` → a
`MutexGuard<Connection>`) and a `&Path`. It reads the schema version, runs SQL migrations
inside a `BEGIN IMMEDIATE` transaction, and commits. The migration logic has never been
tested against sqlx — only against rusqlite.

Risk SR-04 is the primary driver: if pool construction precedes migration, any pool
connection that runs a query before the schema is up to date produces undefined behavior
(wrong column layout, missing tables, stale schema_version).

**Two sequencing options were considered:**

**Option A: Run migration on a connection from `write_pool` after pool construction**

Pool is constructed first. Migration acquires a connection from `write_pool` and runs.

Problems:
- If migration fails (e.g., corruption, version mismatch), the pool has already been
  constructed. Cleanup requires closing the pool, which may not happen cleanly.
- Other code that acquired a pool connection during startup (e.g., lazy initialization
  in a background task) would see a schema mid-migration.
- sqlx pools may open connections eagerly on construction (min_connections > 0). If any
  eager connection runs `PRAGMA schema_version` or queries a table before migration, it
  could observe stale schema.

**Option B: Dedicated non-pooled `SqliteConnection` opened before pool construction**

Migration runs on a `sqlx::SqliteConnection` (non-pooled, opened via
`SqliteConnection::connect()`). On success, the connection is dropped. Pool construction
begins only after the connection is dropped and migration has committed.

This is the approach recommended in SR-04 and mandated in FR-08, C-03.

Advantages:
- Failure to migrate → `StoreError::Migration` returned; pool construction never starts.
  The `Store::open()` caller receives an error and the server does not start.
- No pool connection ever observes a pre-migration schema.
- Migration logic operates on a single `&mut SqliteConnection`, which sqlx supports via
  its `Executor` trait on `&mut SqliteConnection`. This is the direct replacement for
  the current rusqlite `Connection` reference.
- The migration connection is explicitly dropped before pool construction, ensuring SQLite's
  WAL sees a clean commit before the pools begin acquiring connections.

**Migration connection PRAGMAs:**
The migration connection must have the same PRAGMAs applied as the pool connections
(specifically `journal_mode=WAL` and `foreign_keys=ON`) to ensure migration SQL runs with
the same constraints as production queries. A shared `apply_pragmas_connection()` helper
applies all 6 PRAGMAs to any `SqliteConnection` or pool options struct.

**`migrate_if_needed` signature change:**
```rust
// Before (rusqlite, through Store wrapper):
pub(crate) fn migrate_if_needed(store: &Store, db_path: &Path) -> Result<()>

// After (sqlx, direct connection):
pub(crate) async fn migrate_if_needed(
    conn: &mut sqlx::SqliteConnection,
    db_path: &Path,
) -> Result<()>
```

The migration logic (schema version check, all 12 version transitions' SQL) is preserved
verbatim. Only the connection type and execution API change:
- `conn.query_row(...)` → `sqlx::query_scalar!(...).fetch_optional(&mut *conn).await?`
- `conn.execute_batch(...)` → `sqlx::query(...).execute(&mut *conn).await?` (or multiple calls)
- `conn.execute(...)` → `sqlx::query(...).execute(&mut *conn).await?`
- `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` → `let txn = conn.begin().await?; txn.commit().await?`

The `OptionalExtension` rusqlite trait is replaced by `fetch_optional` on sqlx queries.

### Decision

Migration runs on a dedicated non-pooled `sqlx::SqliteConnection` opened before any pool
construction. The sequence in `Store::open()` is:

1. Validate `PoolConfig` (fail fast).
2. Open `SqliteConnection::connect(path)`.
3. Apply all 6 PRAGMAs to the migration connection.
4. Call `migrate_if_needed(&mut migration_conn, path).await`.
5. On failure: return `StoreError::Migration { source }`. Pool construction does not proceed.
6. On success: `drop(migration_conn)` explicitly.
7. Construct `read_pool` and `write_pool`.

`migrate_if_needed` becomes `async fn` accepting `&mut sqlx::SqliteConnection`. Its SQL is
preserved verbatim; only the execution mechanism changes from rusqlite to sqlx.

`create_tables()` (which creates all tables idempotently after migration) is integrated into
the pool initialization sequence — it runs via a `write_pool` connection after pools are
constructed, as a one-time idempotent setup call.

### Consequences

- SR-04 is fully addressed: pool construction is gated behind migration success.
- Migration failures are surfaced as `StoreError::Migration` with the sqlx error as source,
  distinguishable from `StoreError::Open` and `StoreError::Database`.
- The 16 migration integration tests remain valid — they will use the new `async fn` signature
  with `#[tokio::test]` bodies and a temporary `SqliteConnection`.
- AC-17 (migration regression harness covering all 12 version transitions) is implementable
  directly against the adapted `migrate_if_needed` async function.
- The migration connection PRAGMAs are shared with pool construction via a single helper
  function, ensuring consistent SQLite configuration.
- There is no window where a pool connection can observe a schema under migration.
