# Test Plan: import-pipeline (C4 — import/mod.rs)

## Scope

`ImportCounts` gains 3 new fields. `ingest_rows` gains 3 match arms. `drop_all_data` gains 5 DELETE statements (ADR-001). `print_summary` gains 3 lines. `format_version` validation accepts 1 or 2 (ADR-002). `record_provenance` includes new table counts.

## Unit Tests (in `import/mod.rs` #[cfg(test)])

### test_format_version_0_rejected
**Risk**: R-04
**AC**: AC-07

Create header with `format_version: 0`. Call format validation. Assert error containing "0" and supported range.

### test_format_version_1_accepted
**Risk**: R-04
**AC**: AC-05

Create header with `format_version: 1`. Call format validation. Assert `Ok(())`.

### test_format_version_2_accepted
**Risk**: R-04
**AC**: AC-06

Create header with `format_version: 2`. Call format validation. Assert `Ok(())`.

### test_format_version_3_rejected
**Risk**: R-04
**AC**: AC-07

Create header with `format_version: 3`. Call format validation. Assert error containing "3" and supported range "1 and 2".

### test_format_version_999_rejected
**Risk**: R-04
**AC**: AC-07

Create header with `format_version: 999`. Assert error. Boundary value confirmation.

### test_import_counts_default_includes_new_fields
**Risk**: R-13

Assert `ImportCounts::default()` has `graph_edges: 0`, `observations: 0`, `cycle_events: 0`.

## Integration Tests (extend `import_integration.rs`)

### test_import_v1_file_zero_new_table_counts
**Risk**: R-04, R-13
**AC**: AC-05

Craft a v1 JSONL file with only the original 8 table types. Import. Verify:
- Import succeeds
- `graph_edges`, `observations`, `cycle_events` tables have 0 rows in DB

### test_import_v2_file_all_11_types
**Risk**: R-04
**AC**: AC-06

Craft a v2 JSONL file with all 11 table types populated. Import. Verify:
- Import succeeds
- Row counts match for all 11 tables
- `ImportCounts` fields nonzero for all 3 new types

### test_drop_all_data_clears_5_new_tables
**Risk**: R-02
**AC**: AC-13

Populate `graph_edges`, `observations`, `cycle_events`, `observation_metrics`, `observation_phase_metrics`. Run `--force` import. Verify all 5 tables have 0 rows before ingestion begins.

### test_drop_all_data_derived_tables_without_observations
**Risk**: R-02

Populate `observation_phase_metrics` and `observation_metrics` only (no `observations` rows). Run `--force` import. Verify both derived tables cleared. Confirms cleanup happens regardless of parent table state.

### test_drop_all_data_ordering_fk_safe
**Risk**: R-02
**AC**: AC-13

Populate `observation_phase_metrics` with FK referencing `observation_metrics`. Run `--force` import. Verify no FK constraint error occurs during deletion (observation_phase_metrics deleted first per ADR-001).

### test_ingest_routes_new_table_types
**Risk**: R-04
**AC**: AC-06

Craft v2 JSONL with 1 graph_edge, 1 observation, 1 cycle_event. Import. Verify each table has exactly 1 row. Confirms match arms route correctly.

### test_print_summary_new_tables_v2
**Risk**: R-13
**AC**: AC-20

Import a v2 file with known counts. Capture stderr. Assert output contains lines for:
- `graph_edges: {N}`
- `observations: {N}`
- `cycle_events: {N}`

### test_print_summary_new_tables_v1
**Risk**: R-13
**AC**: AC-20

Import a v1 file. Capture stderr. Assert output contains 0 counts for new tables.

### test_record_provenance_includes_new_counts
**Risk**: R-12
**AC**: AC-20 (FR-17)

Import a v2 file with graph_edges=2, observations=3, cycle_events=1. Query `audit_log` for import provenance. Assert detail string mentions all 3 table counts.

### test_round_trip_11_tables
**Risk**: R-08, R-14
**AC**: AC-15

Populate DB with all 11 tables. Export. Import into fresh DB. Re-export. Normalize `exported_at` and filter provenance audit entry. Byte-compare. Extends existing round-trip pattern.

### test_id_collision_observations_non_force
**Risk**: R-05
**AC**: AC-16

Populate observations with id=1 in target DB. Import v2 file with observations id=1 without --force. Assert PRIMARY KEY constraint error. Verify transaction rollback.

### test_id_collision_cycle_events_non_force
**Risk**: R-05
**AC**: AC-17

Same as above for cycle_events.

### test_id_collision_resolved_by_force
**Risk**: R-05
**AC**: AC-13, AC-16, AC-17

Populate observations and cycle_events. Import with --force. Assert drop_all_data clears tables, import succeeds, IDs match import file.

### test_v1_file_with_graph_edges_rows
**Risk**: R-04 (edge case #8)

Craft a v1 file that includes `_table: "graph_edges"` rows (hand-edited scenario). Import. Assert the graph_edges rows are ingested (match arm exists regardless of version). This confirms v1/v2 distinction is header-only.

## Coverage Mapping

| Risk | Scenarios Covered | Tests |
|------|-------------------|-------|
| R-02 | 5 tables cleared, derived without parent, FK ordering | 3 integration tests |
| R-04 | v0 reject, v1 accept, v2 accept, v3 reject, v999 reject | 5 unit + 3 integration |
| R-05 | Observation/cycle_event ID collision; --force resolution | 3 integration tests |
| R-12 | Provenance detail includes new counts | 1 integration test |
| R-13 | Summary output for v1 and v2 | 2 integration tests |
| R-14 | 11-table round-trip consistency | 1 integration test |
