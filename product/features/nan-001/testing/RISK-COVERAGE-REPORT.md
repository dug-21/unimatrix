# Risk Coverage Report: nan-001

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Hardcoded column list diverges from actual schema | test_export_entries_all_26_columns_present, test_export_counters_key_count_and_values, test_export_entry_tags_key_count, test_export_co_access_key_count, test_export_feature_entries_key_count, test_export_outcome_index_key_count, test_export_agent_registry_key_count, test_export_audit_log_key_count, test_entries_all_26_columns (integration) | PASS | Full |
| R-02 | f64 confidence precision loss through JSON serialization | test_export_entries_f64_precision, test_entries_all_26_columns (integration, bitwise check) | PASS | Full |
| R-03 | JSON-in-TEXT columns double-encoded or parsed | test_export_agent_registry_json_in_text_as_string, test_export_audit_log_json_in_text_target_ids | PASS | Full |
| R-04 | NULL columns omitted instead of serialized as null | test_export_entries_null_handling, test_export_agent_registry_null_handling, test_export_entries_empty_string_not_null, test_export_entries_all_nullable_null, test_null_handling_nullable_columns (integration) | PASS | Full |
| R-05 | Transaction not held for full export duration | Code inspection: BEGIN DEFERRED at line 40, COMMIT at line 56 wrapping all do_export calls. Integration tests verify cross-table consistency (test_full_export_representative_data, test_all_8_tables_with_row_counts). | PASS | Full |
| R-06 | JSON key ordering non-deterministic | test_export_entries_key_ordering, test_export_counters_table_key_first, test_header_key_order_preserved, test_deterministic_output (integration, 3 runs) | PASS | Full |
| R-07 | Excluded tables leak into export output | test_excluded_tables_not_present (integration) | PASS | Full |
| R-08 | Row ordering within tables incorrect | test_export_entries_ordered_by_id, test_export_entry_tags_ordered, test_row_ordering_within_tables (integration, entries + tags + co_access) | PASS | Full |
| R-09 | Store::open() migration side-effect | Implicit: all integration tests open DB via Store::open, then export via run_export which opens again -- no crash or schema error indicates re-open is safe. | PASS | Partial |
| R-10 | Output file partial write on error | test_error_on_invalid_output_path, test_error_on_nonexistent_database (integration) | PASS | Partial |
| R-11 | preserve_order breaks existing MCP server serialization | Full workspace test suite: 2164 passed, 0 failed, 18 ignored. Integration smoke tests: 18 passed, 1 xfail (pre-existing GH#111). | PASS | Full |
| R-12 | Empty database export produces invalid output | test_do_export_empty_db, test_do_export_all_lines_valid_json, test_empty_database_export (integration) | PASS | Full |
| R-13 | Unicode content corrupted in JSON serialization | test_export_entries_unicode_cjk_and_emoji, test_export_entry_tags_unicode_accented, test_export_entries_json_special_chars_in_content | PASS | Full |
| R-14 | Large integer values overflow or lose precision | test_export_entries_large_integers, test_export_counters_i64_max | PASS | Full |
| R-15 | --project-dir not wired to export subcommand | test_project_dir_isolation (integration, two separate project dirs with distinct data) | PASS | Full |

## Test Results

### Unit Tests (in export.rs)
- Total: 33
- Passed: 33
- Failed: 0

### Integration Tests (export_integration.rs)
- Total: 16
- Passed: 16
- Failed: 0

### Workspace Regression
- Total: 2164
- Passed: 2164
- Failed: 0
- Ignored: 18
- Note: test_compact_search_consistency (unimatrix-vector) is a known flaky test (GH#188), passed this run

### MCP Integration Smoke Tests (infra-001)
- Total: 19
- Passed: 18
- Deselected: 166
- xfail: 1 (GH#111 -- pre-existing rate limit blocks volume test)

## Gaps

| Risk ID | Gap | Reason |
|---------|-----|--------|
| R-05 | No concurrent-write test proving snapshot isolation | True concurrent modification during export requires spawning a thread that writes between table reads. This is fragile and non-deterministic. Transaction isolation is verified by code inspection (BEGIN DEFERRED wraps all reads) and by the consistency of cross-table data in integration tests. |
| R-09 | No modification-time comparison test | R-09 is medium priority. Store::open() on a current-schema DB is a no-op for migration. The risk is accepted for v1 per the risk strategy. |
| R-10 | No mid-stream write failure test (mock writer) | The internal export functions take `&mut impl Write` but are not public. Testing with a failing writer would require a unit test calling private functions or exposing them. Error paths for invalid output path and non-existent DB are tested. |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | test_output_file_path, test_empty_database_export -- run_export produces valid JSONL |
| AC-02 | PASS | test_output_file_path -- --output writes to file, content is valid JSONL |
| AC-03 | PASS | test_header_validation -- _header: true, schema_version, exported_at (recent), entry_count=3, format_version=1, exactly 5 keys |
| AC-04 | PASS | test_every_non_header_line_has_table -- all non-header lines have _table from allowed set |
| AC-05 | PASS | test_all_8_tables_with_row_counts -- all 8 table types present with correct counts |
| AC-06 | PASS | test_entries_all_26_columns -- all 26 columns verified including confidence (bitwise f64), helpful_count, unhelpful_count, access_count |
| AC-07 | PASS | test_row_ordering_within_tables -- entries by id, tags by (entry_id, tag), co_access by (entry_id_a, entry_id_b) |
| AC-08 | PASS | test_table_emission_order -- counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log |
| AC-09 | PASS | test_null_handling_nullable_columns -- supersedes, superseded_by, pre_quarantine_status all JSON null; allowed_topics/allowed_categories null in agent_registry; key counts unchanged |
| AC-10 | PASS | test_empty_database_export -- header with entry_count=0, only counter rows, all lines valid JSON |
| AC-11 | PASS | test_performance_500_entries -- 500 entries + 1000 tags + 100 co_access exported in <5s |
| AC-12 | PASS | All integration tests run export with no MCP server -- export operates directly on database file |
| AC-13 | PASS | test_project_dir_isolation -- two project dirs with different data produce different exports matching their respective databases |
| AC-14 | PASS | test_deterministic_output -- 3 consecutive exports produce byte-identical output after normalizing exported_at |
| AC-15 | PASS | test_error_on_invalid_output_path (returns Err), test_error_on_nonexistent_database (returns Err) |
| AC-16 | PASS | 33 unit tests cover null fields, empty strings, unicode (CJK, emoji, combining accents), i64::MAX, JSON-in-TEXT columns, newline escaping, JSON special chars |
| AC-17 | PASS | test_full_export_representative_data -- all 8 tables populated, correct row counts, header verified |
| AC-18 | PASS | test_excluded_tables_not_present -- only the 8 allowed _table values appear in output |
