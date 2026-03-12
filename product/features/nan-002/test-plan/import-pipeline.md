# nan-002: Test Plan -- import-pipeline

## Component Scope

The `import::run_import()` function and all internal helpers in `crates/unimatrix-server/src/import.rs`. Covers header validation, pre-flight checks, JSONL ingestion, per-table SQL INSERT, hash validation, transaction management, force-drop logic, audit provenance, and progress reporting.

## Unit Tests

### Header Validation

#### test_validate_header_valid (AC-03)
- Valid header with format_version=1, schema_version=11
- Assert Ok

#### test_validate_header_bad_format_version (AC-04)
- Header with format_version=2
- Assert error message contains "2" and "format"

#### test_validate_header_future_schema_version (AC-05)
- Header with schema_version=999
- Assert error message contains "upgrade"

#### test_validate_header_missing_header_flag
- Header with `_header: false` or missing `_header`
- Assert error

#### test_validate_header_format_version_zero
- Header with format_version=0
- Assert error naming version "0"

### Hash Validation

#### test_hash_validation_valid_chain (AC-12)
- Two entries: A (previous_hash="") and B (previous_hash=A.content_hash)
- Both content hashes match recomputed values
- Assert validation passes

#### test_hash_validation_broken_chain (AC-12)
- Entry with previous_hash pointing to nonexistent content_hash
- Assert error contains entry ID

#### test_hash_validation_content_mismatch (AC-13)
- Entry with content_hash that does not match recomputed hash (tampered content)
- Assert error contains entry ID

#### test_hash_validation_empty_previous_hash
- Entry with previous_hash="" (chain root)
- Assert validation skips this entry (no error)

#### test_hash_validation_empty_title_edge_case (R-07)
- Entry with title="" and content="some text"
- Recompute hash, assert it matches export-time computation

#### test_hash_validation_empty_both (R-07)
- Entry with title="" and content=""
- Assert hash computation is consistent with `compute_content_hash("", "")`

### Malformed Input

#### test_malformed_jsonl_line_with_line_number (AC-21)
- 10-line JSONL with corrupt JSON on line 5
- Assert error message contains "line 5" (1-indexed, accounting for header on line 1)

#### test_empty_file_errors
- Zero-length input file
- Assert clear error, not panic

#### test_header_only_file
- File with only the header line, no data
- Assert behavior (either valid empty import or clear error)

### SQL Injection Prevention (R-15)

#### test_sql_injection_in_title
- Entry with `title: "'; DROP TABLE entries; --"`
- Assert entry is inserted successfully with the literal string, tables intact

#### test_sql_injection_in_content
- Entry with SQL metacharacters in content field
- Assert correct insertion, no SQL execution

#### test_duplicate_entry_ids
- Two entries with the same `id`
- Assert PK violation error

## Integration Tests

### Round-Trip (AC-15, AC-24)

#### test_round_trip_export_import_reexport
- Create a populated database with data across all 8 tables (entries, tags, co-access, feature_entries, outcome_index, agent_registry, audit_log, counters)
- Export to JSONL
- Import into a fresh database
- Re-export to a second JSONL
- Compare both exports line-by-line after normalizing `exported_at`
- Assert identical output
- Risks: R-01, R-10

### Force Import (AC-02, AC-06, AC-27)

#### test_force_import_replaces_data
- Populate database with 10 entries (IDs 1-10)
- Create export with 5 different entries (IDs 1-5 with different content)
- Import with --force
- Assert only 5 entries exist, with the imported content (not original)
- Assert stderr contains count of dropped entries
- Risks: R-04

#### test_import_rejected_without_force_on_nonempty (AC-06)
- Populate database with entries
- Attempt import without --force
- Assert error suggesting --force
- Assert database unchanged

#### test_force_on_empty_database
- Fresh (empty) database
- Import with --force
- Assert success (no-op drop, then import)
- Risks: R-04

### Counter Restoration (AC-09)

#### test_counter_restoration_prevents_id_collision
- Export database with entries up to ID 100 (next_entry_id counter = 101)
- Import into fresh database
- Insert a new entry via Store API
- Assert new entry ID >= 101
- Risks: R-03

#### test_counter_values_match_export
- Export, import, then read all counters from DB
- Assert next_entry_id, next_signal_id, schema_version match exported values

#### test_force_import_counter_restoration (R-03)
- Populate DB with entries 1-50, then force-import entries 1-100
- Insert new entry, assert ID > 100

### Atomicity (AC-22)

#### test_atomicity_rollback_on_parse_failure
- Create JSONL with 5 valid entries followed by a corrupt line
- Attempt import
- Assert error
- Assert database has zero entries (transaction rolled back)
- Risks: R-06

#### test_atomicity_rollback_on_fk_violation (R-06)
- Create JSONL with entry_tags referencing non-existent entry IDs
- Attempt import
- Assert FK violation error
- Assert database unchanged

### Hash Validation Integration

#### test_skip_hash_validation_bypass (AC-14)
- Create export, tamper with content of one entry (changing content but keeping original content_hash)
- Import with --skip-hash-validation
- Assert import succeeds
- Assert stderr contains warning about skipped validation

#### test_hash_validation_failure_prevents_commit
- Create export, tamper with content hash
- Import without --skip-hash-validation
- Assert error with entry ID
- Assert database has zero entries (rolled back)

### Empty Import (AC-16)

#### test_empty_export_imports_successfully
- Create JSONL with header + counter lines only (no entries)
- Import into fresh database
- Assert success, database valid, counters set correctly

### Audit Provenance (AC-26)

#### test_audit_provenance_entry_written
- Import a database with audit log entries
- After import, query audit_log for entries with operation containing "import"
- Assert provenance entry exists with correct timestamp and source file info
- Risks: R-13

#### test_audit_provenance_no_id_collision (R-13)
- Import database with audit log entries up to event_id 50
- Assert provenance entry has event_id > 50

### Progress Reporting (AC-25)

#### test_progress_reporting_to_stderr
- Import a multi-entry export
- Capture stderr
- Assert stderr contains progress indicators (entry counts, embedding progress)

### All 8 Tables (AC-07)

#### test_all_eight_tables_restored
- Create database with at least one row in each of the 8 tables
- Export, import, verify row counts per table match

### Per-Column Verification (AC-08)

#### test_entry_columns_preserved_exactly
- Create entries with known values in all 26 columns, including edge values (null optionals, empty strings, unicode, high integers)
- Export, import
- Query each column of each entry, assert exact match with original values
- Risks: R-01

## Risk Coverage

| Risk | Tests | Coverage |
|------|-------|----------|
| R-01 (SQL divergence) | test_round_trip_*, test_entry_columns_preserved_exactly, test_all_eight_tables_restored | Full |
| R-03 (counter/ID collision) | test_counter_restoration_prevents_id_collision, test_counter_values_match_export, test_force_import_counter_restoration | Full |
| R-04 (--force safety) | test_force_import_replaces_data, test_import_rejected_without_force_on_nonempty, test_force_on_empty_database | Full |
| R-06 (FK violation) | test_atomicity_rollback_on_fk_violation | Full |
| R-07 (hash edge cases) | test_hash_validation_*, test_skip_hash_validation_bypass, test_hash_validation_failure_prevents_commit | Full |
| R-08 (concurrent server) | Deferred to manual verification -- PID file warning is advisory only | Partial |
| R-13 (audit provenance collision) | test_audit_provenance_no_id_collision, test_audit_provenance_entry_written | Full |
| R-15 (SQL injection) | test_sql_injection_in_title, test_sql_injection_in_content, test_duplicate_entry_ids | Full |
