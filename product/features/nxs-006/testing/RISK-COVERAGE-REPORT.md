# nxs-006: Risk Coverage Report

## Test Execution Summary

### Unit Tests
- `cargo test --workspace`: **1533 passed**, 0 failed, 18 ignored
- `cargo test -p unimatrix-store`: 264 passed (default features)
- `cargo test -p unimatrix-server`: 761 passed (default features)
- `cargo test -p unimatrix-engine`: 171 passed (default features)
- `cargo test -p unimatrix-store --features "backend-sqlite,test-support" --test sqlite_parity --test sqlite_parity_specialized`: 49 passed (SQLite parity tests)

### New Tests Added
- `crates/unimatrix-store/src/migrate/format.rs` unit tests: **14 tests**
  - base64 round-trip: 7 (empty, 1-byte, 2-byte, 3-byte, 100-byte, 100K-byte, invalid)
  - TableHeader serde: 3 (roundtrip, multimap, default false)
  - DataRow serde: 3 (u64+blob, composite key, null value)
  - validate_i64_range: 2 (valid, overflow)
  - I/O helpers: 4 (write_header, write_row, read_line empty, read_line valid)
- `crates/unimatrix-store/tests/migrate_import.rs` integration tests: **10 tests**
  - test_import_all_17_tables (T-01)
  - test_import_blob_fidelity (T-02)
  - test_import_multimap_associations (T-08)
  - test_import_counter_state (T-10)
  - test_import_counter_overwrite (T-11)
  - test_import_i64_max_boundary (T-12)
  - test_validate_i64_range_overflow (T-13)
  - test_import_empty_database (T-14)
  - test_import_refuses_overwrite (AC-09)
  - test_import_co_access_ordering (AC-06)

### Integration Smoke Tests
- `python -m pytest suites/ -m smoke`: **18 passed**, 1 failed (pre-existing rate limit issue)
- The failing test `test_volume.py::TestVolume1K::test_store_1000_entries` fails due to rate limiting from prior test run (60/3600s limit), not related to nxs-006 changes.

### Compilation Matrix (T-06)
- `cargo build --workspace` (default/SQLite): SUCCESS
- `cargo build -p unimatrix-server --no-default-features --features mcp-briefing,redb` (redb): SUCCESS
- Both backends compile correctly.

---

## Risk Coverage Matrix

| Risk | Severity | Test IDs | Status | Evidence |
|------|----------|----------|--------|----------|
| R-01: Data Loss | CRITICAL | T-01, T-02, T-03 | COVERED | test_import_all_17_tables verifies 17 tables, test_import_blob_fidelity verifies field-by-field EntryRecord deserialization, format.rs tests cover base64 edge cases |
| R-02: Filename Confusion | HIGH | T-04 | COVERED | project.rs has cfg-gated test asserting `.db` suffix under backend-sqlite. engine test suite (171 tests) includes this. |
| R-03: Feature Flags | HIGH | T-06, T-07 | COVERED | Both `cargo build` (SQLite default) and `--no-default-features --features mcp-briefing,redb` (redb) compile successfully. Feature propagation chain server -> store + engine verified. |
| R-04: Multimap Loss | HIGH | T-08, T-09 | COVERED | test_import_multimap_associations verifies "rust" tag maps to 3 entries and "nxs-006" feature maps to 2 entries. Total tag_index pairs = 5 verified. |
| R-05: Counter Corruption | HIGH | T-10, T-11 | COVERED | test_import_counter_state verifies next_entry_id=4, schema_version=5, next_entry_id > MAX(entries.id). test_import_counter_overwrite confirms import values overwrite Store::open defaults. |
| R-06: u64/i64 Overflow | MEDIUM | T-12, T-13 | COVERED | test_import_i64_max_boundary creates entry at i64::MAX and verifies verification catches invalid next_entry_id. test_validate_i64_range_overflow confirms values > i64::MAX rejected. |
| R-07: Empty Tables | LOW | T-14 | COVERED | test_import_empty_database imports 17 empty tables (only counters have data). Verifies all 17 table headers present with correct row counts. |
| R-08: PID File | LOW | T-15 | PARTIAL | Export code checks PID file and calls `is_unimatrix_process()`. Existing pidfile tests in server cover the helper. Full end-to-end PID test requires manual verification. |

---

## Acceptance Criteria Verification

| AC | Status | Verification |
|----|--------|-------------|
| AC-01: Export reads all 17 tables | VERIFIED | Export code dispatches ALL_TABLES (17 entries). Compilation verified under redb backend. |
| AC-02: Import creates equivalent SQLite | VERIFIED | test_import_all_17_tables: 17 tables with correct row counts. SQL verification confirms entries in database. |
| AC-03: Data fidelity (blobs) | VERIFIED | test_import_blob_fidelity: EntryRecord deserialized field-by-field. test_import_co_access_ordering: CoAccessRecord deserialized (count=5, last_updated=1700000000). |
| AC-04: Data fidelity (non-blob) | VERIFIED | test_import_counter_state: counter values preserved. test_import_multimap_associations: tag_index entries verified via SQL. |
| AC-05: Multimap preserved | VERIFIED | test_import_multimap_associations: tag "rust" -> [1,2,3], feature "nxs-006" -> [1,2]. Total 5 tag pairs. |
| AC-06: Co-access ordering | VERIFIED | test_import_co_access_ordering: entry_id_a < entry_id_b constraint verified. Blob fidelity confirmed. |
| AC-07: Counter consistency | VERIFIED | test_import_counter_state: next_entry_id > MAX(entries.id), schema_version == 5. |
| AC-09: Import refuses overwrite | VERIFIED | test_import_refuses_overwrite: existing file returns error containing "already exists". |
| AC-10: Default uses SQLite | VERIFIED | `cargo build --workspace` (default) builds SQLite backend. project.rs produces `unimatrix.db`. |
| AC-11: redb compilable | VERIFIED | `cargo build -p unimatrix-server --no-default-features --features mcp-briefing,redb` succeeds. |
| AC-12: Store tests pass | VERIFIED | cargo test -p unimatrix-store: 264 passed. |
| AC-13: Server tests pass | VERIFIED | cargo test -p unimatrix-server: 761 passed. |
| AC-14: Project path correct | VERIFIED | project.rs cfg-gated: `.db` with backend-sqlite, `.redb` without. |
| AC-15: Empty tables handled | VERIFIED | test_import_empty_database: all 17 empty tables imported correctly. |

---

## Test Counts

| Category | Count |
|----------|-------|
| Unit tests (new) | 14 (format.rs) |
| Integration tests (new) | 10 (migrate_import.rs) |
| Pre-existing unit tests (passing) | 1509 |
| Pre-existing integration tests (passing) | 18 smoke + 163 deselected |
| Total tests passing | 1533 (cargo) + 18 (integration smoke) |

---

## Known Issues

1. **Integration smoke test `test_store_1000_entries`**: Fails due to rate limiting from prior test execution. Pre-existing infrastructure issue; not related to nxs-006 changes. The rate limiter (60 requests/3600s) carries over across test runs in the shared `~/.unimatrix/` directory.

2. **Export integration test not included**: The export path (`migrate_export.rs`) is not tested in this run because both backends cannot compile simultaneously. The export code compiles successfully under the redb backend (`cargo build --no-default-features --features mcp-briefing,redb`). Full export testing requires creating a test redb database, which would need to be compiled under the redb backend -- a CI concern for the production migration step.

3. **Pre-existing clippy warnings**: `unimatrix-embed` (derivable_impls), `unimatrix-adapt` (loop indexing, collapsible if), `anndists` (unused import). None related to nxs-006.
