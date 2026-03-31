# crt-035 Pseudocode: migration.rs (v18→v19 back-fill)

## Purpose

Bump `CURRENT_SCHEMA_VERSION` from 18 to 19 and add a `if current_version < 19` block to
`run_main_migrations` that back-fills the missing reverse CoAccess edge for every forward-only
row in `GRAPH_EDGES` where `relation_type = 'CoAccess' AND source = 'co_access'`.

No DDL changes. No new tables. No `ALTER TABLE`. Pure data migration.

---

## Integration Surface

| Symbol | Current Value | After crt-035 |
|--------|--------------|---------------|
| `CURRENT_SCHEMA_VERSION` | `pub const u64 = 18` | `pub const u64 = 19` |
| `run_main_migrations` signature | `async fn(&mut Transaction<Sqlite>, u64, &Path) -> Result<()>` | Unchanged |
| `migrate_if_needed` signature | `pub(crate) async fn(&mut SqliteConnection, &Path) -> Result<()>` | Unchanged |

---

## Modification 1: `CURRENT_SCHEMA_VERSION` constant

```
// File: crates/unimatrix-store/src/migration.rs, line 19

// BEFORE:
/// Current schema version. Incremented from 17 to 18 by crt-033 (CYCLE_REVIEW_INDEX).
pub const CURRENT_SCHEMA_VERSION: u64 = 18;

// AFTER:
/// Current schema version. Incremented from 18 to 19 by crt-035 (bidirectional CoAccess back-fill).
pub const CURRENT_SCHEMA_VERSION: u64 = 19;
```

---

## Modification 2: Add `if current_version < 19` block to `run_main_migrations`

The new block is inserted immediately after the `if current_version < 18` block and
before the final `INSERT OR REPLACE INTO counters` that sets the version to
`CURRENT_SCHEMA_VERSION`.

### Placement in `run_main_migrations`

```
FUNCTION run_main_migrations(txn: &mut Transaction<Sqlite>, current_version: u64, _db_path: &Path) -> Result<()>:

  ... (existing v3→v17 migration blocks, unchanged) ...

  // v17 → v18: cycle_review_index table (crt-033). [unchanged block]
  IF current_version < 18:
    ... (existing CREATE TABLE IF NOT EXISTS cycle_review_index ...) ...
    UPDATE counters SET value = 18 WHERE name = 'schema_version'

  // v18 → v19: bidirectional CoAccess edges back-fill (crt-035).
  //
  // Inserts a reverse edge (b→a) for every forward-only CoAccess edge (a→b)
  // in GRAPH_EDGES where:
  //   - relation_type = 'CoAccess'   (only CoAccess; Supersedes/Supports/Contradicts untouched)
  //   - source = 'co_access'         (only tick/bootstrap managed edges; excludes any manual rows)
  //   - the reverse (b→a) does not already exist  (NOT EXISTS guard, D4)
  //
  // INSERT OR IGNORE: UNIQUE(source_id, target_id, relation_type) provides a second
  // idempotency layer — concurrent or repeated runs produce no duplicates and no errors.
  //
  // created_by is copied from the forward edge (D1): 'bootstrap' for crt-030/v13 bootstrap
  // edges; 'tick' for crt-034 tick-managed edges. created_by tracks relationship origin,
  // not code path.
  //
  // created_at = strftime('%s','now'): records when the reverse row was written, not
  // when the relationship was first observed (EC-06 design decision).
  //
  // bootstrap_only = 0: reverse edges are always included in build_typed_relation_graph
  // reads (which filter out bootstrap_only=1 rows). bootstrap_only=1 is reserved for
  // edges created solely for the bootstrap analytic snapshot and excluded from live PPR.
  IF current_version < 19:

    // [GATE-3B-03 note: delivery agent must run EXPLAIN QUERY PLAN on this SQL against
    // a real schema and confirm the NOT EXISTS sub-select uses sqlite_autoindex_graph_edges_1
    // (the UNIQUE B-tree on source_id, target_id, relation_type), not a full table scan.
    // If a full scan appears, add a composite index DDL before this INSERT and document the
    // EXPLAIN output as a comment in migration_v18_to_v19.rs.]

    sqlx::query(
      "INSERT OR IGNORE INTO graph_edges
           (source_id, target_id, relation_type, weight, created_at,
            created_by, source, bootstrap_only)
       SELECT
           g.target_id          AS source_id,
           g.source_id          AS target_id,
           'CoAccess'           AS relation_type,
           g.weight             AS weight,
           strftime('%s','now') AS created_at,
           g.created_by         AS created_by,
           'co_access'          AS source,
           0                    AS bootstrap_only
       FROM graph_edges g
       WHERE g.relation_type = 'CoAccess'
         AND g.source = 'co_access'
         AND NOT EXISTS (
           SELECT 1 FROM graph_edges rev
           WHERE rev.source_id = g.target_id
             AND rev.target_id = g.source_id
             AND rev.relation_type = 'CoAccess'
         )"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // Bump the in-transaction schema_version to 19 so that if a subsequent
    // migration block is added later, it observes the correct version baseline.
    sqlx::query("UPDATE counters SET value = 19 WHERE name = 'schema_version'")
      .execute(&mut **txn)
      .await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

  // Final version stamp (always runs, unconditional):
  // Sets schema_version to CURRENT_SCHEMA_VERSION (19) via INSERT OR REPLACE.
  // This is the existing trailing statement in run_main_migrations — unchanged.
  sqlx::query(
    "INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)"
  )
  .bind(CURRENT_SCHEMA_VERSION as i64)
  .execute(&mut **txn)
  .await
  .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

  Ok(())

END FUNCTION
```

---

## Back-fill SQL: Detailed Explanation

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
SELECT
    g.target_id          AS source_id,    -- swap: target becomes new source
    g.source_id          AS target_id,    -- swap: source becomes new target
    'CoAccess'           AS relation_type,
    g.weight             AS weight,        -- same weight as forward edge (OQ-1 resolved)
    strftime('%s','now') AS created_at,    -- migration timestamp (EC-06: intentional)
    g.created_by         AS created_by,   -- copy provenance from forward (D1)
    'co_access'          AS source,
    0                    AS bootstrap_only -- always included in PPR graph reads
FROM graph_edges g
WHERE g.relation_type = 'CoAccess'
  AND g.source = 'co_access'             -- scoped to tick/bootstrap CoAccess only
  AND NOT EXISTS (                        -- D4: explicit intent + efficient re-runs
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = 'CoAccess'
  )
```

### Why NOT EXISTS?

The `NOT EXISTS` guard (D4) makes the SQL self-documenting and makes re-runs efficient:
rows that already have a reverse edge are skipped entirely without attempting the INSERT.
`INSERT OR IGNORE` provides a second safety layer via the UNIQUE constraint for any edge
case where NOT EXISTS races with a concurrent write (impossible in practice since migration
runs on a non-pooled connection, but defensive coding is appropriate here).

### Why `source = 'co_access'` filter?

This scopes the back-fill to edges managed by the tick/bootstrap pipeline. Any hypothetical
manually-inserted CoAccess edge without `source = 'co_access'` is excluded intentionally.

### Columns NOT included

`metadata` is not included in the SELECT — it defaults to NULL in the INSERT (matches the
DDL `DEFAULT NULL`). The forward edge's `metadata` is not copied because CoAccess edges
do not currently use the metadata field; copying it would be surprising and could propagate
future data unexpectedly.

---

## Error Handling

Both SQL statements in the v18→v19 block use `.map_err(|e| StoreError::Migration { source: Box::new(e) })?`.

If either statement fails:
- The enclosing transaction (owned by `migrate_if_needed`) rolls back.
- The database remains at v18.
- `migrate_if_needed` returns `Err(StoreError::Migration { ... })`.
- `SqlxStore::open` propagates this error to the caller.
- The next open attempt re-runs the migration from the beginning (idempotent on success).

This behavior is documented in the risk register as R-09 (low severity, accepted).

---

## File: `crates/unimatrix-store/tests/migration_v18_to_v19.rs`

This is a new file. The pattern follows `migration_v17_to_v18.rs` exactly.

### Preamble

```rust
//! Integration tests for the v18→v19 schema migration (crt-035).
//!
//! Covers: MIG-U-01 (CURRENT_SCHEMA_VERSION = 19), MIG-U-02 (fresh DB creates v19),
//! MIG-U-03 (v18→v19 back-fills bootstrap-era forward CoAccess edges),
//! MIG-U-04 (v18→v19 back-fills tick-era forward CoAccess edges),
//! MIG-U-05 (non-CoAccess edges unmodified), MIG-U-06 (idempotency),
//! MIG-U-07 (empty graph_edges: back-fill is no-op).
//!
//! Pattern: create a v18-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data shape.

#![cfg(feature = "test-support")]

use std::path::Path;
use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;
```

### V18 Database Builder

```
FUNCTION create_v18_database(path: &Path):
  // Open a raw SqliteConnection (no pool, no SqlxStore) to the path.
  // Execute all DDL from migration_v17_to_v18.rs create_v17_database() plus:
  //   - The cycle_review_index table (added by v17→v18 migration).
  // Set schema_version = 18 in counters.
  //
  // This is an EXACT COPY of the v17 DDL from create_v17_database() in
  // migration_v17_to_v18.rs, augmented with:
  //   "CREATE TABLE cycle_review_index (
  //       feature_cycle         TEXT    PRIMARY KEY,
  //       schema_version        INTEGER NOT NULL,
  //       computed_at           INTEGER NOT NULL,
  //       raw_signals_available INTEGER NOT NULL DEFAULT 1,
  //       summary_json          TEXT    NOT NULL
  //   )"
  //
  // Then:
  //   "INSERT INTO counters (name, value) VALUES ('schema_version', 18)"
  //   (and all other counter seeds from create_v17_database)
  //
  // The graph_edges table DDL must be IDENTICAL to the v17 version:
  //   "CREATE TABLE graph_edges (
  //       id             INTEGER PRIMARY KEY AUTOINCREMENT,
  //       source_id      INTEGER NOT NULL,
  //       target_id      INTEGER NOT NULL,
  //       relation_type  TEXT    NOT NULL,
  //       weight         REAL    NOT NULL DEFAULT 1.0,
  //       created_at     INTEGER NOT NULL,
  //       created_by     TEXT    NOT NULL DEFAULT '',
  //       source         TEXT    NOT NULL DEFAULT '',
  //       bootstrap_only INTEGER NOT NULL DEFAULT 0,
  //       metadata       TEXT    DEFAULT NULL,
  //       UNIQUE(source_id, target_id, relation_type)
  //   )"
  //   "CREATE INDEX idx_graph_edges_source_id ON graph_edges(source_id)"
  //   "CREATE INDEX idx_graph_edges_target_id ON graph_edges(target_id)"
  //   "CREATE INDEX idx_graph_edges_relation_type ON graph_edges(relation_type)"
  //
  // NOTE: The UNIQUE constraint creates the implicit B-tree index
  // sqlite_autoindex_graph_edges_1 on (source_id, target_id, relation_type).
  // The three named indexes above are single-column. GATE-3B-03 confirms
  // which index the NOT EXISTS sub-select uses.
END FUNCTION
```

### Helpers

```
FUNCTION read_schema_version(store: &SqlxStore) -> i64:
  sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
  .fetch_one(store.read_pool_test()).await

FUNCTION count_graph_edges(store: &SqlxStore, source_id: i64, target_id: i64,
                            relation_type: &str) -> i64:
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM graph_edges
     WHERE source_id = ? AND target_id = ? AND relation_type = ?"
  )
  .bind(source_id).bind(target_id).bind(relation_type)
  .fetch_one(store.read_pool_test()).await

FUNCTION count_all_coaccess_edges(store: &SqlxStore) -> i64:
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM graph_edges
     WHERE relation_type = 'CoAccess' AND source = 'co_access'"
  )
  .fetch_one(store.read_pool_test()).await

FUNCTION insert_graph_edge(conn, source_id, target_id, relation_type, weight,
                           created_by, source, bootstrap_only):
  sqlx::query(
    "INSERT INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     VALUES (?, ?, ?, ?, 1700000000, ?, ?, ?)"
  )
  .bind(source_id).bind(target_id).bind(relation_type).bind(weight)
  .bind(created_by).bind(source).bind(bootstrap_only)
  .execute(conn).await
```

---

## Test Cases

### MIG-U-01: `test_current_schema_version_is_19`

```
#[test]
fn test_current_schema_version_is_19():
  assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 19,
    "CURRENT_SCHEMA_VERSION must be 19")
```

### MIG-U-02: `test_fresh_db_creates_schema_v19`

```
#[tokio::test]
async fn test_fresh_db_creates_schema_v19():
  dir = TempDir::new()
  store = SqlxStore::open(dir.path().join("test.db"), PoolConfig::default()).await

  // Assert schema_version == 19 (fresh DB skips migration, uses create_tables_if_needed)
  assert_eq!(read_schema_version(&store).await, 19)
  store.close().await
```

### MIG-U-03: `test_v18_to_v19_back_fills_bootstrap_era_edges`

```
#[tokio::test]
async fn test_v18_to_v19_back_fills_bootstrap_era_edges():
  dir = TempDir::new()
  db_path = dir.path().join("test.db")
  create_v18_database(&db_path).await

  // Arrange: insert a bootstrap-era forward edge (1→2) with created_by='bootstrap'
  {
    conn = open_raw_sqlite_conn(db_path).await
    insert_graph_edge(&conn, 1, 2, 'CoAccess', 0.75, 'bootstrap', 'co_access', 0).await
    // Assert pre-condition: only forward edge exists
    count = count_graph_edges_raw(&conn, 2, 1, 'CoAccess').await
    assert_eq!(count, 0, "reverse must not exist in v18 shape")
  }

  // Act: open triggers v18→v19 migration
  store = SqlxStore::open(&db_path, PoolConfig::default()).await

  // Assert: schema_version == 19
  assert_eq!(read_schema_version(&store).await, 19)

  // Assert: reverse edge (2→1) was inserted
  rev_count = count_graph_edges(&store, 2, 1, "CoAccess").await
  assert_eq!(rev_count, 1, "reverse edge (2→1) must exist after back-fill (AC-09)")

  // Assert: reverse edge carries same weight and created_by='bootstrap'
  // Use SELECT to verify weight and created_by on the reverse row
  rev_row = fetch_one_graph_edge(&store, 2, 1, "CoAccess").await
  assert!(|rev_row.weight - 0.75| < 1e-9, "reverse edge weight must match forward")
  assert_eq!(rev_row.created_by, "bootstrap", "created_by must be copied from forward (D1)")
  assert_eq!(rev_row.source, "co_access")
  assert_eq!(rev_row.bootstrap_only, 0)

  // Assert: forward edge (1→2) still exists (not touched by back-fill)
  fwd_count = count_graph_edges(&store, 1, 2, "CoAccess").await
  assert_eq!(fwd_count, 1, "forward edge must survive migration unchanged")

  store.close().await
```

### MIG-U-04: `test_v18_to_v19_back_fills_tick_era_edges`

```
#[tokio::test]
async fn test_v18_to_v19_back_fills_tick_era_edges():
  dir = TempDir::new()
  db_path = dir.path().join("test.db")
  create_v18_database(&db_path).await

  // Arrange: insert a tick-era forward edge (5→10) with created_by='tick'
  {
    conn = open_raw_sqlite_conn(db_path).await
    insert_graph_edge(&conn, 5, 10, 'CoAccess', 0.60, 'tick', 'co_access', 0).await
  }

  // Act
  store = SqlxStore::open(&db_path, PoolConfig::default()).await

  // Assert: schema_version == 19
  assert_eq!(read_schema_version(&store).await, 19)

  // Assert: reverse edge (10→5) created with created_by='tick'
  rev_row = fetch_one_graph_edge(&store, 10, 5, "CoAccess").await
  assert!(|rev_row.weight - 0.60| < 1e-9)
  assert_eq!(rev_row.created_by, "tick")
  assert_eq!(rev_row.source, "co_access")

  store.close().await
```

### MIG-U-05: `test_v18_to_v19_does_not_touch_non_coaccess_edges`

```
#[tokio::test]
async fn test_v18_to_v19_does_not_touch_non_coaccess_edges():
  dir = TempDir::new()
  db_path = dir.path().join("test.db")
  create_v18_database(&db_path).await

  // Arrange: insert a Supersedes edge and a Supports edge alongside one CoAccess forward edge
  {
    conn = open_raw_sqlite_conn(db_path).await
    insert_graph_edge(&conn, 1, 2, 'Supersedes', 1.0, '', '', 0).await
    insert_graph_edge(&conn, 3, 4, 'Supports', 1.0, '', '', 0).await
    insert_graph_edge(&conn, 1, 2, 'CoAccess', 0.5, 'tick', 'co_access', 0).await
  }

  // Act
  store = SqlxStore::open(&db_path, PoolConfig::default()).await

  // Assert: Supersedes edge (1→2) still has count 1 (no reverse added)
  sup_fwd = count_graph_edges(&store, 1, 2, "Supersedes").await
  sup_rev = count_graph_edges(&store, 2, 1, "Supersedes").await
  assert_eq!(sup_fwd, 1)
  assert_eq!(sup_rev, 0, "Supersedes reverse must NOT be back-filled (EC-03)")

  // Assert: Supports edge (3→4) still has count 1 (no reverse added)
  spt_rev = count_graph_edges(&store, 4, 3, "Supports").await
  assert_eq!(spt_rev, 0, "Supports reverse must NOT be back-filled (EC-03)")

  // Assert: CoAccess reverse edge (2→1) was created
  ca_rev = count_graph_edges(&store, 2, 1, "CoAccess").await
  assert_eq!(ca_rev, 1, "CoAccess reverse edge must be back-filled")

  // Assert total edge count: 1 Supersedes + 1 Supports + 2 CoAccess (fwd + rev) = 4
  total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
      .fetch_one(store.read_pool_test()).await
  assert_eq!(total, 4)

  store.close().await
```

### MIG-U-06: `test_v18_to_v19_migration_idempotent`

```
#[tokio::test]
async fn test_v18_to_v19_migration_idempotent():
  dir = TempDir::new()
  db_path = dir.path().join("test.db")
  create_v18_database(&db_path).await

  // Arrange: one forward CoAccess edge
  {
    conn = open_raw_sqlite_conn(db_path).await
    insert_graph_edge(&conn, 1, 2, 'CoAccess', 0.5, 'bootstrap', 'co_access', 0).await
  }

  // Run 1: applies v18→v19 back-fill
  {
    store = SqlxStore::open(&db_path, PoolConfig::default()).await
    assert_eq!(read_schema_version(&store).await, 19)
    count = count_all_coaccess_edges(&store).await
    assert_eq!(count, 2, "two CoAccess edges after first open")
    store.close().await
  }

  // Run 2: must be a no-op (NOT EXISTS guard + INSERT OR IGNORE)
  store = SqlxStore::open(&db_path, PoolConfig::default()).await
  assert_eq!(read_schema_version(&store).await, 19)
  count_after = count_all_coaccess_edges(&store).await
  assert_eq!(count_after, 2,
    "second open must not duplicate edges; NOT EXISTS + UNIQUE guard must be idempotent (NFR-02)")
  // [R-02 note: even count (2) is the invariant — any odd value here is a bug]
  store.close().await
```

### MIG-U-07: `test_v18_to_v19_empty_graph_edges_is_noop`

```
#[tokio::test]
async fn test_v18_to_v19_empty_graph_edges_is_noop():
  dir = TempDir::new()
  db_path = dir.path().join("test.db")
  create_v18_database(&db_path).await
  // No rows inserted into graph_edges — EC-01: empty table at migration time

  store = SqlxStore::open(&db_path, PoolConfig::default()).await.expect("must not error")

  assert_eq!(read_schema_version(&store).await, 19)
  total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
      .fetch_one(store.read_pool_test()).await
  assert_eq!(total, 0, "back-fill on empty table must be a no-op (EC-01)")

  store.close().await
```

---

## Error Handling Summary

| Failure Point | Behavior |
|--------------|----------|
| `INSERT OR IGNORE` SQL error | `?` propagates `StoreError::Migration`; transaction rolls back; DB stays at v18 |
| `UPDATE counters SET value = 19` error | Same rollback behavior |
| Repeated open after rollback | Migration re-runs from `current_version < 19`; idempotent on success |
| Empty `graph_edges` table | SELECT returns zero rows; INSERT inserts nothing; no error |
| All edges already bidirectional | NOT EXISTS filters everything; INSERT inserts nothing; no error |

---

## Knowledge Stewardship

- Pattern #3889 (unimatrix-store): back-fill reverse GRAPH_EDGES uses `INSERT OR IGNORE` with
  SELECT that swaps source_id and target_id — confirmed, followed exactly.
- ADR-001 (crt-035, #3890): eventual consistency; back-fill runs inside main transaction.
- ADR-006 (crt-034, #3891): forward-only was v1 intentional; crt-035 fulfills follow-up contract.
- Deviations from established patterns: none. The v18→v19 block follows the identical structure
  as the v17→v18 block. Pure data migration, no DDL.
