# Test Plan: schema-migration

## Risk Coverage

- R-02: Schema migration failure (High severity, Low likelihood)

## Test Scenarios

### T-SM-01: Migration v6 to v7 creates observations table
**Type**: Integration
**Risk**: R-02
**AC**: AC-01

Setup: Create a v6 database (schema_version = 6, no observations table)
Action: Call migrate_if_needed()
Assert:
- observations table exists
- idx_observations_session index exists
- idx_observations_ts index exists
- schema_version counter = 7

### T-SM-02: Fresh database includes observations table
**Type**: Integration
**Risk**: R-02
**AC**: AC-01

Setup: Open a new database via Store::open()
Assert:
- observations table exists with correct columns
- AUTOINCREMENT on id column
- Indexes exist
- schema_version = 7

### T-SM-03: Idempotent migration (v7 -> v7 no-op)
**Type**: Integration
**Risk**: R-02

Setup: Open a v7 database
Action: Call migrate_if_needed() again
Assert:
- No error
- observations table still exists
- Data in observations table is unchanged

## Implementation Notes

- Tests go in `crates/unimatrix-store/tests/migration_v7.rs` or inline
- Use Store::open with tempdir for fresh DB test
- For v6 DB test: create a store, manually set schema_version to 6, drop observations table, then re-open
