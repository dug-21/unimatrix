# Risk Coverage Report: nan-002 (Knowledge Import)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Direct SQL INSERT diverges from schema DDL | test_entry_row_all_26_fields_present, test_entry_row_field_count_matches_ddl, test_round_trip_export_import_reexport, test_entry_columns_preserved_exactly, test_all_eight_tables_restored | PASS | Full |
| R-02 | Format deserialization edge cases (null, empty, unicode, max int, JSON-in-TEXT) | test_entry_row_null_optionals, test_entry_row_empty_strings, test_entry_row_unicode_content, test_entry_row_max_integers, test_agent_registry_row_deserialize, test_agent_registry_row_with_topics, test_audit_log_row_deserialize, test_malformed_jsonl_line_with_line_number | PASS | Full |
| R-03 | Counter restoration / ID collision | test_counter_restoration_prevents_id_collision, test_counter_values_match_export, test_force_import_counter_restoration | PASS | Full |
| R-04 | Destructive --force without safety net | test_force_import_replaces_data, test_import_rejected_without_force_on_nonempty, test_force_on_empty_database | PASS | Full |
| R-05 | Embedding fails after DB commit | test_embed_batch_size_constant, test_batch_count_calculation, test_read_entries_empty_db, test_read_entries_returns_id_title_content, test_read_entries_ordered_by_id, test_read_entries_multiple_entries | PASS | Partial |
| R-06 | Foreign key violation on misordered JSONL | test_atomicity_rollback_on_fk_violation | PASS | Full |
| R-07 | Hash chain validation edge cases | test_hash_validation_valid_chain, test_hash_validation_broken_chain, test_hash_validation_content_mismatch, test_hash_validation_empty_previous_hash, test_hash_validation_empty_title_edge_case, test_hash_validation_empty_both, test_skip_hash_validation_bypass, test_hash_validation_failure_prevents_commit | PASS | Full |
| R-08 | Concurrent MCP server during import | (PID file logic implemented; advisory warning only) | N/A | Partial |
| R-09 | ONNX model unavailable | (Error messaging implemented; requires model-absent environment to test) | N/A | Partial |
| R-10 | Floating-point round-trip fidelity | test_entry_row_confidence_precision, test_entry_row_confidence_boundaries, test_entry_columns_preserved_exactly (f64 bit-exact assertion) | PASS | Full |
| R-11 | Unknown _table discriminator | test_export_row_unknown_table_errors | PASS | Full |
| R-12 | Large import performance | (Requires ONNX model; deferred to environment with model) | N/A | None |
| R-13 | Audit log provenance event_id collision | test_audit_provenance_entry_written, test_audit_provenance_no_id_collision | PASS | Full |
| R-14 | --project-dir resolution mismatch | test_round_trip_export_import_reexport (uses project_dir), test_entry_columns_preserved_exactly (uses project_dir) | PASS | Partial |
| R-15 | Malicious input / SQL injection | test_sql_injection_in_title, test_sql_injection_in_content, test_duplicate_entry_ids | PASS | Full |

## Test Results

### Unit Tests
- Total: 40 (format.rs: 20, import/mod.rs: 15, embed_reconstruct.rs: 5)
- Passed: 40
- Failed: 0

### Integration Tests (Rust -- import_integration.rs)
- Total: 16
- Passed: 16
- Failed: 0

### Integration Tests (pipeline_e2e.rs)
- Total: 7
- Passed: 7
- Failed: 0

### Integration Tests (infra-001 smoke)
- Total: 19 (18 passed, 1 xfail)
- Passed: 18
- xfail: 1 (pre-existing GH#111 -- volume test rate limit, unrelated to nan-002)
- Failed: 0

### Full Workspace
- Total: 2225
- Passed: 2225
- Failed: 0
- Ignored: 18

## Gaps

### R-05 (Embedding after DB commit) -- Partial Coverage
The embedding reconstruction component (`embed_reconstruct.rs`) is tested at the unit level for `read_entries()` and batch calculation logic. Full end-to-end embedding verification (AC-10: vector index file creation, AC-11: semantic search post-import) requires the ONNX model, which is environment-dependent. The code path includes clear error messaging per ADR-004 (database valid regardless of embedding outcome). The structural correctness of the embedding orchestration is verified by unit tests.

### R-08 (Concurrent server) -- Partial Coverage
PID file warning is implemented (check `pid_path.exists()` in `check_preflight`). This is advisory-only by design (SR-07). Not tested because it requires a running server process during test execution. The behavior is observable via manual testing.

### R-09 (ONNX model unavailable) -- Partial Coverage
Error messaging is implemented with actionable guidance (model name, suggestion to pre-cache). Testing requires an environment without the ONNX model cached. The code path is verified by code review (the `map_err` in `reconstruct_embeddings` produces a descriptive message).

### R-12 (Performance) -- No Coverage
AC-17 (500-entry import under 60s) requires the ONNX model for embedding. Performance characteristics are bounded by design: line-by-line JSONL reading, 64-entry embedding batches, incremental HNSW construction.

### R-14 (--project-dir resolution) -- Partial Coverage
All integration tests use `project_dir` via `setup_project()` and pass it to `run_import()`, validating that import writes to the specified directory. A dedicated test verifying export-from-A/import-into-B path isolation would strengthen this, but the round-trip test implicitly validates separate project directories (project_a and project_b).

### AC-19 (--project-dir flag) -- Partial
The CLI parsing is covered by the `Command::Import` variant registration in main.rs. Integration tests use project_dir via `run_import()` function calls. A binary-level test invoking `unimatrix-server import --project-dir <path> --input <file>` would provide stronger coverage but is not strictly required since the dispatch is trivial.

### AC-20 (Exit codes) -- Partial
Exit codes are implicitly tested via `Result::is_err()` / `Result::is_ok()` assertions on `run_import()` return values. The binary-level exit code (process exit status) is not tested since tests call the library function directly.

### AC-25 (Progress reporting to stderr) -- Not Tested
Progress output to stderr is implemented (`eprintln!` calls in `ingest_rows` and `reconstruct_embeddings`). Capturing stderr in Rust tests requires process-level test execution, which is out of scope for the current test infrastructure.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | test_round_trip_export_import_reexport, test_all_eight_tables_restored |
| AC-02 | PASS | test_force_import_replaces_data |
| AC-03 | PASS | test_validate_header_valid |
| AC-04 | PASS | test_validate_header_bad_format_version |
| AC-05 | PASS | test_validate_header_future_schema_version |
| AC-06 | PASS | test_import_rejected_without_force_on_nonempty |
| AC-07 | PASS | test_all_eight_tables_restored (verifies row counts for all 8 tables) |
| AC-08 | PASS | test_entry_columns_preserved_exactly (all 26 columns bit-exact, including f64 confidence) |
| AC-09 | PASS | test_counter_restoration_prevents_id_collision, test_counter_values_match_export |
| AC-10 | PARTIAL | embed_reconstruct unit tests verify read_entries and batch logic; full vector index verification requires ONNX model |
| AC-11 | PARTIAL | infra-001 smoke tests pass (server functional); post-import MCP verification requires ONNX model for import |
| AC-12 | PASS | test_hash_validation_valid_chain, test_hash_validation_broken_chain |
| AC-13 | PASS | test_hash_validation_content_mismatch |
| AC-14 | PASS | test_skip_hash_validation_bypass |
| AC-15 | PASS | test_round_trip_export_import_reexport (line-by-line comparison after normalizing exported_at, filtering provenance audit entry) |
| AC-16 | PASS | test_empty_export_imports_successfully |
| AC-17 | NOT TESTED | Requires ONNX model (environment-dependent) |
| AC-18 | PASS | By design: import calls Store::open() directly, no server dependency in code path |
| AC-19 | PARTIAL | Integration tests use project_dir; CLI flag registered in main.rs |
| AC-20 | PARTIAL | Result::is_err/is_ok verified for success and 3+ failure cases (bad header, non-empty DB, parse error, hash mismatch, FK violation) |
| AC-21 | PASS | test_malformed_jsonl_line_with_line_number |
| AC-22 | PASS | test_atomicity_rollback_on_parse_failure, test_atomicity_rollback_on_fk_violation, test_hash_validation_failure_prevents_commit |
| AC-23 | PASS | format.rs unit tests: null optionals, empty strings, unicode, max integers, JSON-in-TEXT for all 8 table types |
| AC-24 | PASS | test_round_trip_export_import_reexport (all 8 tables populated), test_all_eight_tables_restored |
| AC-25 | NOT TESTED | Progress eprintln! calls implemented; stderr capture not in test scope |
| AC-26 | PASS | test_audit_provenance_entry_written, test_audit_provenance_no_id_collision |
| AC-27 | PASS | test_force_import_replaces_data (10 entries replaced by 5, content verified) |
