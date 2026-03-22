# Agent Report: crt-025-agent-3-schema-migration

**Feature**: crt-025 WA-1 Phase Signal + FEATURE_ENTRIES Tagging
**Component**: Schema Migration (Component 7)
**Agent ID**: crt-025-agent-3-schema-migration

---

## Deliverables

### Files Modified

- `crates/unimatrix-store/src/migration.rs`
  - Bumped `CURRENT_SCHEMA_VERSION` from 14 to 15
  - Added v14→v15 migration block: `CREATE TABLE IF NOT EXISTS cycle_events (...)`, `CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id`, `ALTER TABLE feature_entries ADD COLUMN phase TEXT` with `pragma_table_info` pre-check (C-08 compliant)
  - Updated counter comment from `(14)` to `(15)`

- `crates/unimatrix-store/src/db.rs`
  - Updated `feature_entries` DDL in `create_tables_if_needed` to include `phase TEXT` column
  - Added `cycle_events` DDL and `idx_cycle_events_cycle_id` index after `feature_entries`
  - Replaced hardcoded `schema_version = 14` counter insert with `CURRENT_SCHEMA_VERSION` binding
  - Added `SqlxStore::insert_cycle_event(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp) -> Result<()>` method using direct write pool (ADR-003)
  - Updated `test_schema_version_initialized_to_14_on_fresh_db` to bind `CURRENT_SCHEMA_VERSION`

### Files Created

- `crates/unimatrix-store/tests/migration_v14_to_v15.rs` — 13 integration tests covering:
  - `test_current_schema_version_is_15` (unit)
  - `test_fresh_db_creates_schema_v15`
  - `test_fresh_db_cycle_events_table_schema`
  - `test_v14_to_v15_migration_adds_cycle_events_table`
  - `test_v14_to_v15_migration_adds_phase_column_to_feature_entries`
  - `test_v14_pre_existing_rows_have_null_phase`
  - `test_v14_to_v15_migration_idempotent`
  - `test_pragma_table_info_guard_prevents_duplicate_column`
  - `test_schema_version_is_15_after_migration`
  - `test_v15_feature_entries_round_trip_with_phase`
  - `test_v15_feature_entries_null_phase_row`
  - `test_v15_cycle_events_round_trip`
  - `test_v15_cycle_events_all_nullable_columns_null`

### Schema Version Cascade Updates (pattern #2933)

- `crates/unimatrix-store/tests/migration_v10_to_v11.rs` — 6 `assert_eq!(... 14)` → `assert!(... >= 14)`
- `crates/unimatrix-store/tests/migration_v11_to_v12.rs` — 4 occurrences updated
- `crates/unimatrix-store/tests/migration_v12_to_v13.rs` — 6 occurrences updated
- `crates/unimatrix-store/tests/migration_v13_to_v14.rs` — constant test + schema assertions updated to `>= 14`
- `crates/unimatrix-store/tests/sqlite_parity.rs` — `test_schema_version_is_14` assertion updated to `== 15`

---

## Test Results

```
test result: ok. 136 passed; 0 failed  (lib unit tests)
test result: ok. 13 passed; 0 failed   (migration_v14_to_v15)
test result: ok. 8 passed;  0 failed   (migration_v13_to_v14)
test result: ok. 8 passed;  0 failed   (migration_v12_to_v13)
test result: ok. 16 passed; 0 failed   (migration_v11_to_v12)
test result: ok. 12 passed; 0 failed   (migration_v10_to_v11)
test result: ok. 44 passed; 0 failed   (sqlite_parity)
```

Full workspace build: zero errors.

---

## Design Decisions Followed

- **C-08**: `pragma_table_info` pre-check on `ALTER TABLE feature_entries ADD COLUMN phase TEXT` — not `IF NOT EXISTS` (SQLite unsupported)
- **C-05**: No backfill of pre-existing `feature_entries` rows — `phase = NULL` is correct historical data
- **ADR-003**: `insert_cycle_event` uses direct write pool, not analytics drain
- **ADR-002**: `seq` is advisory; ordering at query time uses `timestamp ASC, seq ASC`
- **Pseudocode 7c**: Verified `create_tables_if_needed` counter insert used hardcoded `14` — corrected to `CURRENT_SCHEMA_VERSION`

---

## Issues Encountered

**Edit tool silent failure in parallel swarm**: Multiple Edit calls appeared to succeed but changes were not persisted due to `cargo fmt` running between Read and Edit calls, invalidating the file fingerprint. Required detecting the failure (by grepping after edit), re-reading, and re-applying. Stored as pattern #3015 for future agents.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store migration` — found pattern #2933 (schema version cascade) and #681 (create-new-then-swap). Both confirmed existing practice followed correctly.
- Stored: entry #3015 "Edit Tool Re-Read Required After Cargo Fmt in Parallel Swarm" via `/uni-store-pattern` — novel runtime discovery about Edit tool behavior in concurrent swarm environments.
