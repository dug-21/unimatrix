# Pseudocode: migration.rs — v26→v27 Schema Migration

## Purpose

Adds the v26→v27 migration block to `crates/unimatrix-store/src/migration.rs` and
bumps `CURRENT_SCHEMA_VERSION` to 27. Documents all 7 mandatory schema cascade
touch points that the delivery agent must complete before Gate 3b.

This is an **index-only migration**. No new tables. No new columns. No data back-fill.
The column-count assertions in `sqlite_parity.rs` do NOT change.

---

## Modified Files (All 7 Schema Cascade Touch Points — ADR-007)

| # | File | Change |
|---|------|--------|
| 1 | `crates/unimatrix-store/src/migration.rs` | Add v26→v27 block; bump `CURRENT_SCHEMA_VERSION` to 27 |
| 2 | `crates/unimatrix-store/src/db.rs` | Add 4 index DDL to `create_tables_if_needed`; bump schema_version literal to 27 |
| 3 | `crates/unimatrix-store/src/sqlite_parity.rs` | `test_schema_version_is_26` → 27; add 4 index-existence assertions |
| 4 | `crates/unimatrix-server/src/server.rs` | All `assert_eq!(version, 26)` → 27 |
| 5 | `crates/unimatrix-store/src/migration_v25_to_v26.rs` | Rename `test_current_schema_version_is_26` → `test_current_schema_version_is_at_least_26`; `assert!(version >= 26)` |
| 6 | `crates/unimatrix-store/src/migration_v26_to_v27.rs` | NEW file: asserts all 4 index names present after migration |
| 7 | `crates/unimatrix-store/src/db.rs` | `test_schema_version_initialized_to_current_on_fresh_db` expected value → 27 |

Note: Touch points 2 and 7 are both in `db.rs` — they are counted separately because
they are distinct changes (DDL addition vs. test assertion update).

**Delivery agent mandatory check**: After completing all 7 touch points, run:
```
grep -r 'schema_version.*== 26' crates/
```
and confirm zero matches. Any remaining `== 26` literal is a missed touch point.

---

## Touch Point 1: migration.rs

### CURRENT_SCHEMA_VERSION Bump

```
// Change:
pub const CURRENT_SCHEMA_VERSION: i64 = 26;
// To:
pub const CURRENT_SCHEMA_VERSION: i64 = 27;
```

### v26→v27 Migration Block

Add this block immediately after the v25→v26 block, following the established
`if current_version < N` pattern:

```
// v26 → v27: indexes for context_graph CTE and neighbor queries (vnc-018, GH #596).
if current_version < 27 {
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type \
         ON graph_edges(source_id, relation_type)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type \
         ON graph_edges(target_id, relation_type)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query("UPDATE counters SET value = 27 WHERE name = 'schema_version'")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    current_version = 27;
}
```

All four indexes are `CREATE INDEX IF NOT EXISTS` — idempotent. Safe to re-run on
any database regardless of whether the indexes already exist.

---

## Touch Point 2: db.rs — 4 Index DDL in create_tables_if_needed

See `store_queries.md` for the full DDL text. Add after the existing index statements:

```sql
CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes);
CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by);
CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type);
```

---

## Touch Point 3: sqlite_parity.rs — Version Assertion + 4 Index Assertions

```
// Change test name and assertion:
// Before:
fn test_schema_version_is_26() {
    // ...
    assert_eq!(version, 26);
}
// After:
fn test_schema_version_is_27() {
    // ...
    assert_eq!(version, 27);
}

// Add 4 index-existence assertions (pattern mirrors existing index assertions in the file):
// For each of the 4 new indexes, add:
let row: (String,) = sqlx::query_as(
    "SELECT name FROM sqlite_master WHERE type='index' AND name=?"
)
.bind("idx_entries_supersedes")
.fetch_one(&pool)
.await
.expect("idx_entries_supersedes should exist after migration");
assert_eq!(row.0, "idx_entries_supersedes");

// Repeat for: idx_entries_superseded_by, idx_graph_edges_source_type, idx_graph_edges_target_type
```

Column-count assertions: unchanged (no new columns added).

---

## Touch Point 4: server.rs — assert_eq!(version, 26) → 27

```
// Find all occurrences of:
assert_eq!(version, 26, "...");
// Replace with:
assert_eq!(version, 27, "...");
```

Run `grep -n 'assert_eq!(version, 26' crates/unimatrix-server/src/server.rs` to
find all occurrences before editing.

---

## Touch Point 5: migration_v25_to_v26.rs — Exact-Version to At-Least

```
// In the existing test file migration_v25_to_v26.rs:
// Find the assertion:
assert_eq!(version, 26, "...");
// Or the test function:
fn test_current_schema_version_is_26() { ... }

// Change to:
fn test_current_schema_version_is_at_least_26() {
    // ...
    assert!(version >= 26, "Expected schema version >= 26, got {version}");
}
```

This is the established pattern: each migration test asserts `>= N` (not `== N`)
so that subsequent migrations do not break earlier migration tests.

---

## Touch Point 6: migration_v26_to_v27.rs (NEW FILE)

New file: `crates/unimatrix-store/src/migration_v26_to_v27.rs`

```
// Integration test: verifies the v26→v27 migration creates all 4 required indexes.

#[cfg(test)]
mod tests {
    use super::*;  // or appropriate imports

    #[tokio::test]
    async fn test_migration_v26_to_v27_creates_all_indexes() {
        // 1. Create a fresh in-memory database
        // 2. Apply all migrations up to and including v26→v27 (migrate_if_needed)
        // 3. Query sqlite_master for each of the 4 index names
        // 4. Assert all 4 indexes exist

        let pool = /* create test pool, migrate, etc. */

        let expected_indexes = [
            "idx_entries_supersedes",
            "idx_entries_superseded_by",
            "idx_graph_edges_source_type",
            "idx_graph_edges_target_type",
        ];

        for index_name in &expected_indexes {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT name FROM sqlite_master WHERE type='index' AND name=?"
            )
            .bind(*index_name)
            .fetch_optional(&pool)
            .await
            .expect("sqlite_master query failed");

            assert!(
                row.is_some(),
                "Expected index '{index_name}' to exist after v26→v27 migration, but it was not found"
            );
        }

        // Assert schema version is 27
        let version: (i64,) = sqlx::query_as(
            "SELECT value FROM counters WHERE name = 'schema_version'"
        )
        .fetch_one(&pool)
        .await
        .expect("schema_version query failed");
        assert!(version.0 >= 27, "Expected schema version >= 27, got {}", version.0);
    }
}
```

Add `mod migration_v26_to_v27;` to `lib.rs` (or wherever migration test files are
registered, following the pattern for `migration_v25_to_v26.rs`).

---

## Touch Point 7: db.rs — test_schema_version_initialized_to_current_on_fresh_db

```
// Find in db.rs:
assert_eq!(version, 26, "fresh database should start at schema version 26");
// Change to:
assert_eq!(version, 27, "fresh database should start at schema version 27");
```

---

## Initialization Sequence

The migration sequencing guarantee (from ARCHITECTURE.md):

```
1. migrate_if_needed(pool) runs to completion
   → v26→v27 block creates all 4 indexes
   → schema_version updated to 27 in COUNTERS table

2. Connection pools are constructed after migration completes

3. MCP server starts accepting connections
   → context_graph handler is reachable
   → all 4 indexes are guaranteed present
```

This is SR-01's resolution: the indexes are in place before any `context_graph`
handler can execute.

---

## Error Handling

| Error | Type | Handling |
|-------|------|---------|
| `CREATE INDEX` fails (DDL error) | `StoreError::Migration` | Propagated from `migrate_if_needed` → server startup aborts |
| `UPDATE counters` fails | `StoreError::Migration` | Same — server startup aborts |
| Migration runs on a database already at v27 | `if current_version < 27` = false | Block skipped; no-op |

Migration errors are fatal to server startup. There is no partial-migration recovery.
The `CREATE INDEX IF NOT EXISTS` idiom ensures idempotency — re-running the migration
on an already-v27 database is a no-op at the SQL level (even if `current_version` was
somehow wrong).

---

## Key Test Scenarios

1. **AC-19**: After `migrate_if_needed` on a v26 database, all four indexes present
   in `sqlite_master`. Covered by `migration_v26_to_v27.rs` test (Touch Point 6).

2. **Schema cascade completeness** (R-05): Run `grep -r 'schema_version.*== 26' crates/`
   after all 7 touch points → zero matches. This is the delivery gate check.

3. **Fresh database at v27** (Touch Point 7): `create_tables_if_needed` on a new
   database produces schema_version = 27 with all 4 indexes. The test in `db.rs`
   asserts this.

4. **Idempotency**: Run `migrate_if_needed` twice on the same v26 database → no
   error (second run: `if current_version < 27` is false after first run; but the
   entire migration runs in a transaction — need to verify re-entrant behavior
   matches existing migration patterns in the codebase).

5. **server.rs assertions**: All `assert_eq!(version, 26)` replaced with 27 →
   integration test suite passes (no assertion failures on server startup).

6. **migration_v25_to_v26.rs**: `test_current_schema_version_is_at_least_26` passes
   (using `>= 26` not `== 26` so that v27 schemas still pass this test).
