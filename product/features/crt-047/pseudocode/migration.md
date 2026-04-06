# crt-047: Pseudocode — migration v23→v24

## Purpose

Add seven new `INTEGER NOT NULL DEFAULT 0` columns to `cycle_review_index`.
Bump `CURRENT_SCHEMA_VERSION` from `23` to `24`. Both migration paths must
be updated: `migration.rs` (incremental) and `db.rs` (fresh-schema DDL).

---

## Pre-Delivery Check (SR-02, ADR-004)

Before implementing, the SM must run:
```
grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs
```
Confirm output is `23`. If another feature has claimed `24`, all version references
must be renumbered before pseudocode applies.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Add `v23→v24` block; bump `CURRENT_SCHEMA_VERSION` to 24 |
| `crates/unimatrix-store/src/db.rs` | Update `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL |

---

## migration.rs Changes

### Constant bump

```
// Change from:
pub const CURRENT_SCHEMA_VERSION: u64 = 23;

// To:
pub const CURRENT_SCHEMA_VERSION: u64 = 24;
```

### New migration block (add after the v22→v23 block, before the final schema_version update)

The v22→v23 block ends at line ~915 with the `schema_version` counter update to 23.
Insert the v23→v24 block immediately after. The existing final
`INSERT OR REPLACE INTO counters ... CURRENT_SCHEMA_VERSION` update at the end of
`run_main_migrations` then correctly stamps 24.

```
// v23 → v24: curation health metrics columns on cycle_review_index (crt-047).
//
// Adds seven INTEGER NOT NULL DEFAULT 0 columns:
//   corrections_total, corrections_agent, corrections_human, corrections_system,
//   deprecations_total, orphan_deprecations, first_computed_at
//
// Each column uses pragma_table_info pre-check (SQLite has no ADD COLUMN IF NOT EXISTS).
// All seven pre-checks run BEFORE any ALTER TABLE executes (pattern from v20→v21).
// All seven ALTER TABLEs run in the same outer transaction as the version bump.
// If any ALTER TABLE fails, the transaction rolls back; schema_version stays at 23.
//
// first_computed_at DEFAULT 0: existing rows have first_computed_at = 0 (no temporal anchor).
// get_curation_baseline_window() excludes these rows (WHERE first_computed_at > 0).
// Operators who want historical cycles in the baseline must force=true each one.
//
// SUMMARY_SCHEMA_VERSION in cycle_review_index.rs is bumped to 2 separately.
// All historical cycle_review_index rows will show the stale-record advisory
// on force=false calls after deployment (designed behavior per crt-033 ADR-002).
if current_version < 24 {
    // --- Pre-check phase: read all seven column states before any ALTER ---
    // Pattern: all pre-checks before any ALTER (from v20→v21 block, entry #4088).
    // This ensures that a partial migration (some columns present) is correctly
    // recovered on retry without executing unnecessary ALTER TABLE statements.

    has_corrections_total: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'corrections_total'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_corrections_agent: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'corrections_agent'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_corrections_human: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'corrections_human'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_corrections_system: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'corrections_system'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_deprecations_total: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'deprecations_total'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_orphan_deprecations: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'orphan_deprecations'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    has_first_computed_at: bool =
      sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
         WHERE name = 'first_computed_at'"
      )
      .fetch_one(&mut **txn).await
      .map(|count| count > 0)
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // --- ALTER TABLE phase: execute only for absent columns ---

    if !has_corrections_total:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN corrections_total INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_corrections_agent:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN corrections_agent INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_corrections_human:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN corrections_human INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_corrections_system:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN corrections_system INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_deprecations_total:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN deprecations_total INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_orphan_deprecations:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN orphan_deprecations INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    if !has_first_computed_at:
      sqlx::query(
        "ALTER TABLE cycle_review_index
         ADD COLUMN first_computed_at INTEGER NOT NULL DEFAULT 0"
      )
      .execute(&mut **txn).await
      .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // Bump schema_version to 24 AFTER all columns are verified/added.
    // This is the in-transaction version bump — the final INSERT OR REPLACE
    // at the end of run_main_migrations also stamps CURRENT_SCHEMA_VERSION.
    // Bumping here ensures that a subsequent migration block (if added later)
    // observes the correct intermediate version.
    sqlx::query(
      "UPDATE counters SET value = 24 WHERE name = 'schema_version'"
    )
    .execute(&mut **txn).await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?
}
```

---

## db.rs Changes

Update the `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL to include all seven
new columns. The DDL must be byte-consistent with the post-migration schema (pattern
per ADR-004 and existing `db.rs` line 952-953 comment "DDL must be byte-identical
to the vNN migration block").

```
// Change from:
"CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL
)"

// To:
"CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL,
    corrections_total     INTEGER NOT NULL DEFAULT 0,
    corrections_agent     INTEGER NOT NULL DEFAULT 0,
    corrections_human     INTEGER NOT NULL DEFAULT 0,
    corrections_system    INTEGER NOT NULL DEFAULT 0,
    deprecations_total    INTEGER NOT NULL DEFAULT 0,
    orphan_deprecations   INTEGER NOT NULL DEFAULT 0,
    first_computed_at     INTEGER NOT NULL DEFAULT 0
)"
```

---

## Cascade Test Updates Required

These existing tests will fail after the version bump and must be updated:

| Test | Location | Change Required |
|------|----------|-----------------|
| `test_summary_schema_version_is_one` | `cycle_review_index.rs` | Rename to `_is_two`; assert `2u32` |
| Schema version equality assertions | `sqlite_parity.rs` | Update `== 23` to `== 24` |
| Column count assertions | `sqlite_parity.rs` | Update count to include 7 new columns |
| Schema version assertions | `server.rs` | Update any `schema_version == 23` to `24` |

Pre-delivery cascade check:
```
grep -r 'schema_version.*== 23' crates/   -- must return zero matches after bump
grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs  -- must show 24
```

---

## Atomicity and Recovery

- All seven `pragma_table_info` pre-checks run before any `ALTER TABLE`.
- All `ALTER TABLE` statements and the version counter update execute within the
  existing outer transaction (opened by `migrate_if_needed` calling `conn.begin()`).
- On any failure: the transaction rolls back; `schema_version` stays at 23.
- On retry: pre-checks detect already-added columns and skip their `ALTER TABLE`.
- This matches the v20→v21 pattern (two-column pre-check-all-then-alter).

---

## Failure Modes

**FM-01: Migration fails mid-run (e.g., process crash after 3 of 7 ALTERs)**:
The outer transaction rolls back on crash (SQLite WAL durability), leaving the DB at
schema_version 23. On next `Store::open()`, pre-checks find the 3 existing columns
(skipped) and add the remaining 4. The version counter is bumped to 24 only after
all 7 are verified/present.

**FM-02: `CURRENT_SCHEMA_VERSION` claimed by parallel feature**:
Pre-delivery grep check catches this before implementation. All version references
in design artifacts must be renumbered.

---

## Key Test Scenarios

**T-MIG-01 (AC-01, AC-14)**: Integration test — synthetic v23 database through `Store::open()`.
- Create in-memory or tempfile DB with v23 `cycle_review_index` DDL (no new columns).
- Insert at least one pre-existing row.
- Call `Store::open()` (not `migrate_if_needed` in isolation).
- Assert:
  - `pragma_table_info('cycle_review_index')` returns all 7 new columns
  - Pre-existing row has value `0` for all 7 new columns
  - `CURRENT_SCHEMA_VERSION` counter equals `24`

**T-MIG-02 (R-03)**: Fresh-schema database has same schema as migrated database.
- Open a fresh database (no prior tables).
- Query `pragma_table_info('cycle_review_index')`.
- Assert column list matches the migrated database schema from T-MIG-01.

**T-MIG-03 (R-03)**: Idempotency — re-running migration on v24 database is a no-op.
- Open a v24 database.
- Call `Store::open()` again.
- Assert no errors; `CURRENT_SCHEMA_VERSION` remains 24.

**T-MIG-04 (ADR-004)**: Partial migration recovery.
- Manually add 3 of 7 columns to a v23 database.
- Call `Store::open()`.
- Assert migration completes successfully (all 7 columns present, version = 24).
