# vnc-018 Test Plan: migration.rs (v26→v27)

## Component Scope

All 7 mandatory schema cascade touch points for the v26→v27 migration (ADR-007):

1. `crates/unimatrix-store/src/migration.rs` — v26→v27 block + `CURRENT_SCHEMA_VERSION = 27`
2. `crates/unimatrix-store/src/db.rs` — 4 index DDL in `create_tables_if_needed` + literal → 27
3. `crates/unimatrix-store/src/sqlite_parity.rs` — version test updated + 4 index assertions
4. `crates/unimatrix-server/src/server.rs` — all `assert_eq!(version, 26)` → 27
5. `crates/unimatrix-store/src/migration_v25_to_v26.rs` — exact-version assertion relaxed to `>=`
6. `crates/unimatrix-store/src/migration_v26_to_v27.rs` (new file) — asserts all 4 index names
7. `crates/unimatrix-store/src/db.rs` — `test_schema_version_initialized_to_current_on_fresh_db` → 27

The 4 new indexes:
- `idx_entries_supersedes ON entries(supersedes)`
- `idx_entries_superseded_by ON entries(superseded_by)`
- `idx_graph_edges_source_type ON graph_edges(source_id, relation_type)`
- `idx_graph_edges_target_type ON graph_edges(target_id, relation_type)`

---

## Unit Test Expectations

### Touch Point 1: `migration.rs` — CURRENT_SCHEMA_VERSION

**Test: `test_current_schema_version_is_27`** (R-05, cascade touch point 1)

```rust
// In migration.rs test module
assert_eq!(CURRENT_SCHEMA_VERSION, 27u32);
// This constant must be updated from 26 → 27.
// If this fails, the migration block does NOT run on existing databases — Critical gap.
```

### Touch Point 2 + 7: `db.rs` — fresh DB schema version

**Test: `test_schema_version_initialized_to_current_on_fresh_db`** (R-05, cascade touch point 7)

```rust
// Arrange: fresh in-memory SQLite
// Act: call create_tables_if_needed
// Assert: schema_version counter == 27
let version: i64 = sqlx::query_scalar(
    "SELECT value FROM counters WHERE name = 'schema_version'"
)
.fetch_one(pool).await?;
assert_eq!(version, 27, "Fresh DB must initialize to schema version 27");
```

### Touch Point 3: `sqlite_parity.rs`

**Test: `test_schema_version_is_27`** (R-05, cascade touch point 3)

```rust
// Renamed from test_schema_version_is_26
// Assert: schema version 27 on a freshly created database
// This test verifies both db.rs and migration.rs are aligned on the version number.
```

**Tests: 4 index existence assertions** (AC-19, R-05)

```rust
// Added to sqlite_parity.rs test suite — one per index:
fn test_index_entries_supersedes_exists(pool: &SqlitePool) {
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name='idx_entries_supersedes'"
    ).fetch_one(pool).await.unwrap();
    assert!(exists, "idx_entries_supersedes must exist");
}

fn test_index_entries_superseded_by_exists(pool: &SqlitePool) { /* same pattern */ }
fn test_index_graph_edges_source_type_exists(pool: &SqlitePool) { /* same pattern */ }
fn test_index_graph_edges_target_type_exists(pool: &SqlitePool) { /* same pattern */ }
```

Alternatively, combine into a single test with four assertions (acceptable):

```rust
fn test_v27_indexes_all_exist(pool: &SqlitePool) {
    let names = ["idx_entries_supersedes", "idx_entries_superseded_by",
                 "idx_graph_edges_source_type", "idx_graph_edges_target_type"];
    for name in names {
        let exists: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name=?"
        ).bind(name).fetch_one(pool).await.unwrap();
        assert!(exists, "Index {name} must exist after v27 migration");
    }
}
```

### Touch Point 4: `server.rs` — no remaining v26 assertions

**Code review check (delivery agent gate, not a unit test):**

```bash
# From project root — delivery agent must run this before Gate 3b:
grep -r 'schema_version.*== 26' crates/
# Expected output: zero matches
# Any match is a schema cascade failure (R-05)
```

If `grep` returns matches, every remaining `assert_eq!(version, 26)` in `server.rs`
must be updated to `assert_eq!(version, 27)`. There must be zero instances of
`version == 26` remaining after the bump.

### Touch Point 5: `migration_v25_to_v26.rs` — exact assertion relaxed

**Test: `test_current_schema_version_is_at_least_26`** (R-05, cascade touch point 5)

```rust
// In migration_v25_to_v26.rs
// BEFORE (must be changed):
//   assert_eq!(version, 26u32);
// AFTER:
assert!(version >= 26, "Schema must be at least v26 after v25→v26 migration, got {version}");
```

This change is required because `CURRENT_SCHEMA_VERSION` is now 27 — a fresh DB will
initialize to 27, making the old `== 26` assertion fail on fresh databases.

### Touch Point 6: `migration_v26_to_v27.rs` — new test file (AC-19)

**Test: `test_migration_v26_to_v27_creates_four_indexes`** (AC-19, R-05)

```rust
// File: crates/unimatrix-store/src/migration_v26_to_v27.rs (new)
// This is the definitive AC-19 test.

#[tokio::test]
async fn test_migration_v26_to_v27_creates_four_indexes() {
    // Arrange: create a v26-schema database (before migration)
    // This requires setting up a DB at v26 state — see pattern in migration_v25_to_v26.rs
    let pool = create_v26_test_db().await;
    
    // Act: run migration from v26 to v27
    let mut txn = pool.begin().await.unwrap();
    // Run only the v26→v27 migration block
    run_migration_v26_to_v27(&mut txn).await.unwrap();
    txn.commit().await.unwrap();
    
    // Assert: all four indexes exist in sqlite_master
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (?,?,?,?)"
    )
    .bind("idx_entries_supersedes")
    .bind("idx_entries_superseded_by")
    .bind("idx_graph_edges_source_type")
    .bind("idx_graph_edges_target_type")
    .fetch_one(&pool).await.unwrap();
    
    assert_eq!(count, 4,
        "All four v27 indexes must be present after migration; found {count}");
    
    // Assert: schema_version updated to 27
    let version: i64 = sqlx::query_scalar(
        "SELECT value FROM counters WHERE name = 'schema_version'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(version, 27i64, "schema_version must be 27 after v26→v27 migration");
}
```

The `create_v26_test_db()` helper must produce a database at exactly v26 schema state
(before v27 indexes exist). Follow the pattern established in `migration_v25_to_v26.rs`
for creating a versioned test database.

---

## Integration Test Expectations

### Restart persistence with v27 schema (infra-001 lifecycle suite)

The `lifecycle` suite's restart persistence tests verify that the server restarts
correctly after a clean shutdown. With the v27 migration, the suite validates:

- Server starts from a v26 database → migration runs → v27 schema established → server ready
- Server starts from a v27 database → migration block skips (idempotent `IF NOT EXISTS`) → normal startup

The lifecycle tests do not need modification — they exercise the full migration path
as a side effect of server startup. If any lifecycle test fails after the v27 change,
it indicates a migration block error (not expected).

---

## Schema Cascade Completeness Checklist

This checklist must be verified before Gate 3b sign-off:

| Touch Point | File | Change | Verification |
|-------------|------|--------|-------------|
| 1 | `migration.rs` | `CURRENT_SCHEMA_VERSION = 27` | `test_current_schema_version_is_27` |
| 2 | `db.rs` | 4 index DDL in `create_tables_if_needed` + literal → 27 | `test_create_tables_creates_four_indexes` |
| 3 | `sqlite_parity.rs` | version test → 27 + 4 index assertions | `test_schema_version_is_27` + 4 index tests |
| 4 | `server.rs` | All `assert_eq!(version, 26)` → 27 | `grep -r 'schema_version.*== 26' crates/` → zero matches |
| 5 | `migration_v25_to_v26.rs` | `== 26` → `>= 26` | `test_current_schema_version_is_at_least_26` |
| 6 | `migration_v26_to_v27.rs` (new) | Assert all 4 indexes | `test_migration_v26_to_v27_creates_four_indexes` |
| 7 | `db.rs` test | Expected version → 27 | `test_schema_version_initialized_to_current_on_fresh_db` |

---

## Migration Idempotency

**Test: `test_v27_migration_is_idempotent`** (R-05)

```rust
// Arrange: v27 database (already migrated)
// Act: run the v26→v27 migration block again (simulating re-run)
// Assert: no error (CREATE INDEX IF NOT EXISTS is idempotent)
// Assert: still exactly 4 indexes with the expected names (no duplicates)
```

The `CREATE INDEX IF NOT EXISTS` DDL guarantees idempotency. This test confirms
the implementation uses `IF NOT EXISTS` (not bare `CREATE INDEX`).

---

## Risks Specifically Addressed in This Component

- R-05: All 7 cascade touch points verified, including the grep gate check
- AC-19: `migration_v26_to_v27.rs` asserts all 4 index names (mandatory non-negotiable test)
- Migration idempotency: `IF NOT EXISTS` prevents duplicate-index errors on re-run
