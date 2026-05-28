# Test Plan: export-functions (C2 — export.rs)

## Scope

3 new export functions (`export_graph_edges`, `export_observations`, `export_cycle_events`), format_version bump to 2, `skip_ids` parameter threading through `do_export` and 5 affected exporters. CLI flag parsing for `--skip-quarantined` and `--confirm`.

## Unit Tests (in `export.rs` #[cfg(test)])

### test_export_graph_edges_9_columns
**Risk**: R-08
**AC**: AC-01, AC-08

Insert 3 graph_edges in non-sorted order. Export. Parse JSONL lines with `_table: "graph_edges"`. Assert:
- Each line has 10 keys (9 data + `_table`)
- No `id` field present (ADR-005)
- Rows sorted by (source_id, target_id, relation_type)

### test_export_graph_edges_weight_nan_fallback
**Risk**: R-01
**AC**: AC-11

Insert graph_edge with `weight = f64::NAN`. Export. Parse the JSONL line. Assert `weight` value is `1.0` (ADR-003 fallback), not NaN literal.

### test_export_graph_edges_weight_infinity_fallback
**Risk**: R-01
**AC**: AC-11

Insert graph_edge with `weight = f64::INFINITY`. Export. Assert `weight` is `1.0` fallback.

### test_export_graph_edges_weight_neg_infinity_fallback
**Risk**: R-01
**AC**: AC-11

Insert graph_edge with `weight = f64::NEG_INFINITY`. Export. Assert `weight` is `1.0` fallback.

### test_export_graph_edges_weight_normal_precision
**Risk**: R-01
**AC**: AC-11

Insert graph_edge with `weight = 0.7777777`. Export. Assert exported value preserves full f64 precision (no f32 truncation).

### test_export_graph_edges_weight_zero
**Risk**: R-01 (edge case #7)
**AC**: AC-11

Insert graph_edge with `weight = 0.0`. Export. Assert exported value is `0.0`, not replaced by fallback.

### test_export_graph_edges_nullable_metadata
**Risk**: R-10
**AC**: AC-12

Insert graph_edge with `metadata = NULL`. Export. Assert JSONL line has `"metadata": null` (JSON null, key present).

### test_export_graph_edges_metadata_empty_string
**Risk**: R-10
**AC**: AC-12

Insert graph_edge with `metadata = ""`. Export. Assert JSONL line has `"metadata": ""` (empty string, not null).

### test_export_graph_edges_metadata_populated
**Risk**: R-10
**AC**: AC-12

Insert graph_edge with `metadata = '{"nli_score": 0.8}'`. Export. Assert JSONL preserves exact JSON content.

### test_export_observations_10_columns
**Risk**: R-08
**AC**: AC-02, AC-09

Insert 3 observations with non-sequential ids (5, 2, 8). Export. Parse JSONL lines. Assert:
- Each line has 11 keys (10 data + `_table`)
- `id` field present (ADR-006)
- Rows sorted by id: 2, 5, 8

### test_export_observations_nullable_fields
**Risk**: R-09
**AC**: AC-02

Insert observation with `tool`, `input`, `response_size`, `response_snippet`, `topic_signal`, `phase` all NULL. Export. Assert all nullable fields are JSON null.

### test_export_observations_embedded_newlines
**Risk**: R-09
**AC**: AC-02

Insert observation with `input = "line1\nline2\nline3"` (literal newlines). Export. Assert the JSONL output is a single line (newlines escaped as `\n` in JSON). Count lines in output: must not produce extra JSONL lines.

### test_export_cycle_events_9_columns
**Risk**: R-08
**AC**: AC-03, AC-10, AC-19

Insert 3 cycle_events with non-sequential ids. Export. Parse JSONL lines. Assert:
- Each line has 10 keys (9 data + `_table`)
- `goal_embedding` absent (ADR-004)
- Rows sorted by id

### test_export_cycle_events_nullable_fields
**Risk**: R-06
**AC**: AC-03

Insert cycle_event with `phase`, `outcome`, `next_phase`, `goal` all NULL. Assert all nullable fields are JSON null.

### test_export_header_format_version_2
**Risk**: R-04
**AC**: AC-04

Export a populated DB. Parse header line. Assert `format_version == 2`.

### test_export_table_emission_order_11_tables
**Risk**: R-14
**AC**: AC-14

Export a DB with all 11 tables populated. Extract `_table` values in order of first appearance. Assert ordering: counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, graph_edges, observations, cycle_events.

### test_export_empty_new_tables
**Risk**: none (edge case #1)
**AC**: AC-21

Export a DB where graph_edges, observations, and cycle_events are empty. Assert zero JSONL lines for those `_table` types.

## Integration Tests (extend `export_integration.rs`)

### test_full_export_11_tables
**AC**: AC-01, AC-02, AC-03, AC-14

Extend the existing `test_full_export_representative_data` pattern. Populate all 11 tables. Export. Verify:
- 11 unique `_table` types present
- Row counts match inserted data for all tables
- New tables appear after existing 8

### test_deterministic_output_11_tables
**AC**: AC-08, AC-09, AC-10

Extend existing `test_deterministic_output`. Populate all 11 tables. Export 3 times. Normalize exported_at. Assert byte-identical.

### test_export_unicode_in_new_tables
**Risk**: R-09 (edge case #6)

Insert graph_edge with `metadata` containing Unicode. Insert observation with `tool` containing emoji. Insert cycle_event with `goal` containing CJK characters. Export and import. Verify preserved.

## Coverage Mapping

| Risk | Scenarios Covered | Tests |
|------|-------------------|-------|
| R-01 | NaN, INFINITY, NEG_INFINITY, normal, zero | 5 unit tests |
| R-08 | graph_edges, observations, cycle_events ordering | 3 unit tests + integration |
| R-09 | Embedded newlines, nullable fields | 2 unit tests |
| R-10 | null, empty string, populated metadata | 3 unit tests |
| R-14 | 11-table ordering, transaction isolation | 1 unit test + code review |
