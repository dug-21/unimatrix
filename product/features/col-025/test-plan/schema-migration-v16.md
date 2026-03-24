# Test Plan: schema-migration-v16

**Crate**: `unimatrix-store`
**Files modified**: `src/migration.rs`, `src/db.rs`
**New test file**: `tests/migration_v15_to_v16.rs`
**Modified test files**: `tests/migration_v14_to_v15.rs`, `tests/sqlite_parity.rs`, `tests/sqlite_parity_specialized.rs`

**Risks covered**: R-01, R-02, R-08, R-14
**ACs covered**: AC-01, AC-09, AC-16

---

## Pre-Delivery Verification (R-01)

Before modifying `insert_cycle_event`:

```bash
grep -rn "insert_cycle_event" crates/
```

Assert exactly one match in `listener.rs`. Document result. If more than one
call site exists, each must receive the new `goal` parameter at the correct
bind position before the signature change is merged.

---

## Migration Cascade Audit (R-02 / AC-16)

The following files contain `CURRENT_SCHEMA_VERSION` or literal `15` assertions
that must be updated to `16`:

| File | Current assertion | Required change |
|------|------------------|-----------------|
| `tests/migration_v14_to_v15.rs` | `assert_eq!(..., 15)` (multiple) | Update all to `16` |
| `tests/sqlite_parity.rs` | Any `schema_version = 15` assertion | Update to `16` |
| `tests/sqlite_parity_specialized.rs` | Any `schema_version = 15` assertion | Update to `16` |

Post-delivery grep must return no matches:
```bash
grep -rn "schema_version.*15\|CURRENT_SCHEMA_VERSION.*15" crates/unimatrix-store/tests/
```

---

## New Migration Test File: `migration_v15_to_v16.rs`

All tests in this file use `#![cfg(feature = "test-support")]` and the same
`create_v15_database` helper pattern as `migration_v14_to_v15.rs`.

The v15 database builder must include the `cycle_events` table (created by the
v14→v15 migration) WITHOUT the `goal` column — this is the v15 shape.

### Test: `test_current_schema_version_is_16`

```
#[test] fn test_current_schema_version_is_16()
```

Assert `unimatrix_store::migration::CURRENT_SCHEMA_VERSION == 16`.
Catches accidental off-by-one in version bump.

### Test: `test_fresh_db_creates_schema_v16` (AC-09, R-02)

```
#[tokio::test] async fn test_fresh_db_creates_schema_v16()
```

- Arrange: empty path (no prior DB).
- Act: `SqlxStore::open` triggers fresh schema creation.
- Assert: `schema_version == 16`.
- Assert: `pragma_table_info('cycle_events')` contains `goal` column.

### Test: `test_v15_to_v16_migration_adds_goal_column` (AC-09, R-02)

```
#[tokio::test] async fn test_v15_to_v16_migration_adds_goal_column()
```

- Arrange: create v15 database (cycle_events exists, goal column absent).
- Act: `SqlxStore::open` triggers v15→v16 migration.
- Assert: `pragma_table_info('cycle_events')` contains `goal` column.
- Assert: `schema_version == 16`.

### Test: `test_v15_pre_existing_rows_have_null_goal` (AC-09)

```
#[tokio::test] async fn test_v15_pre_existing_rows_have_null_goal()
```

- Arrange: v15 database with a pre-seeded `cycle_events` row (using v15 columns only).
- Act: open triggers migration.
- Assert: `SELECT goal FROM cycle_events WHERE id = <pre-seeded-id>` returns NULL.
- Verifies no backfill occurs on existing rows.

### Test: `test_v15_to_v16_migration_idempotent` (AC-09 / Gate 3c scenario 1)

```
#[tokio::test] async fn test_v15_to_v16_migration_idempotent()
```

- Run 1: apply v15→v16 migration, assert goal column present, `schema_version == 16`.
- Close store.
- Run 2: re-open same database, assert no error.
- Assert: exactly one `goal` column in `pragma_table_info('cycle_events')`.
- Assert: `schema_version == 16`.

This is the **non-negotiable Gate 3c scenario 1**.

### Test: `test_pragma_table_info_guard_prevents_duplicate_goal_column`

```
#[tokio::test] async fn test_pragma_table_info_guard_prevents_duplicate_goal_column()
```

- Arrange: v15 database. Manually add `goal TEXT` column before opening store.
- Act: `SqlxStore::open` — migration guard sees column already exists, skips ALTER TABLE.
- Assert: no error; `schema_version == 16`; exactly one `goal` column.

### Test: `test_schema_version_is_16_after_migration` (AC-16)

```
#[tokio::test] async fn test_schema_version_is_16_after_migration()
```

- Assert `schema_version` counter = 16 after migration.
- Assert `unimatrix_store::migration::CURRENT_SCHEMA_VERSION == 16`.

---

## DB Helper Tests: `insert_cycle_event` and `get_cycle_start_goal`

These tests live in `tests/migration_v15_to_v16.rs` or a dedicated
`tests/db_cycle_events.rs`. They require a v16 database (open fresh store).

### Test: `test_insert_cycle_event_full_column_assertion` (R-08 / AC-01 / Gate 3c scenario 6)

```
#[tokio::test] async fn test_insert_cycle_event_full_column_assertion()
```

Write a `cycle_start` event with a known goal string. Read back the full row.
Assert every column by name (not by position):

```
event_type  == "cycle_start"
phase       == Some("scope")
outcome     == None
next_phase  == Some("design")
goal        == Some("Implement feature goal signal for col-025.")
```

This is the **non-negotiable Gate 3c scenario 6**. Full column assertion
detects binding transposition (R-08).

### Test: `test_insert_cycle_event_goal_null_for_non_start_events` (FR-01)

```
#[tokio::test] async fn test_insert_cycle_event_goal_null_for_non_start_events()
```

Insert a `cycle_phase_end` row and a `cycle_stop` row with `goal = None`.
Assert both rows have `goal IS NULL`. Confirms goal is only written on start rows.

### Test: `test_insert_cycle_event_goal_none_writes_null`

```
#[tokio::test] async fn test_insert_cycle_event_goal_none_writes_null()
```

Insert a `cycle_start` row with `goal = None`.
Assert `goal IS NULL` in the row and no other column is displaced.

### Test: `test_get_cycle_start_goal_returns_stored_goal` (R-03)

```
#[tokio::test] async fn test_get_cycle_start_goal_returns_stored_goal()
```

Insert `cycle_start` row with `goal = Some("test goal")`.
Call `get_cycle_start_goal(cycle_id)`.
Assert `Ok(Some("test goal"))`.

### Test: `test_get_cycle_start_goal_returns_none_for_unknown_cycle_id` (R-03)

```
#[tokio::test] async fn test_get_cycle_start_goal_returns_none_for_unknown_cycle_id()
```

Call `get_cycle_start_goal("nonexistent-cycle-id")` on an empty DB.
Assert `Ok(None)`.

### Test: `test_get_cycle_start_goal_returns_none_when_goal_is_null` (R-03)

```
#[tokio::test] async fn test_get_cycle_start_goal_returns_none_when_goal_is_null()
```

Insert `cycle_start` row with `goal = None` (NULL in DB).
Call `get_cycle_start_goal(cycle_id)`.
Assert `Ok(None)` — the `NULL` column maps to `None`.

### Test: `test_get_cycle_start_goal_multiple_start_rows_returns_first` (R-10)

```
#[tokio::test] async fn test_get_cycle_start_goal_multiple_start_rows_returns_first()
```

Insert two `cycle_start` rows for the same `cycle_id` with different goals
(simulated corrupted state). Assert `get_cycle_start_goal` returns the first
row's goal (LIMIT 1 semantics).

---

## Existing Test File Updates

### `migration_v14_to_v15.rs`

- `test_current_schema_version_is_15` → rename to `test_current_schema_version_is_16`
  and update assertion from `15` to `16`. **Or**: delete this test from the v14→v15
  file and add the canonical version check to `migration_v15_to_v16.rs` only.
- `test_fresh_db_creates_schema_v15` → update `schema_version == 15` to `16`.
- `test_v14_to_v15_migration_adds_cycle_events_table` → update assertion to `16`.
- `test_schema_version_is_15_after_migration` → update assertion to `16`.
- All other assertions in this file asserting `15` → update to `16`.

### `sqlite_parity.rs` and `sqlite_parity_specialized.rs`

Audit for any assertion on `CURRENT_SCHEMA_VERSION` or literal schema version `15`.
Update each to `16`. If these files do not contain such assertions, document that
the audit found nothing to update.

---

## Edge Cases

| Edge Case | Test |
|-----------|------|
| `goal` exactly `MAX_GOAL_BYTES` bytes → stored verbatim, no truncation | `test_insert_cycle_event_goal_at_exact_max_bytes` (write via store helper with full-length string) |
| `cycle_id` with special chars (underscores, hyphens) | Standard store contract; no dedicated test needed |
