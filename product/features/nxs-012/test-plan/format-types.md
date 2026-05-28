# Test Plan: format-types (C1 — format.rs)

## Scope

3 new row structs (`GraphEdgeRow`, `ObservationRow`, `CycleEventRow`), 3 new `ExportRow` enum variants. Deserialization contract for format_version 2.

## Unit Tests (in `format.rs` #[cfg(test)])

### test_graph_edge_row_deserialize_all_fields
**Risk**: R-01, R-10
**AC**: AC-01, AC-12

Deserialize a known JSON string into `GraphEdgeRow`. Assert all 9 fields parse correctly:
- `source_id: 1`, `target_id: 2`, `relation_type: "Supports"`, `weight: 0.85`, `created_at: 1700000000`, `created_by: "agent-x"`, `source: "runtime"`, `bootstrap_only: 0`, `metadata: Some("{}"`

### test_graph_edge_row_nullable_metadata
**Risk**: R-10
**AC**: AC-12

Deserialize `GraphEdgeRow` with `"metadata": null`. Assert `metadata == None`.

### test_observation_row_deserialize_all_fields
**Risk**: R-09
**AC**: AC-02

Deserialize a known JSON string into `ObservationRow` with all 10 fields populated:
- `id: 5`, `session_id: "sess-1"`, `ts_millis: 1700000000`, `hook: "on_tool"`, `tool: Some("context_store")`, `input: Some("test input")`, `response_size: Some(1024)`, `response_snippet: Some("ok")`, `topic_signal: Some("testing")`, `phase: Some("active")`

### test_observation_row_nullable_fields
**Risk**: R-09
**AC**: AC-02

Deserialize `ObservationRow` with `tool`, `input`, `response_size`, `response_snippet`, `topic_signal`, `phase` all set to `null`. Assert all are `None`.

### test_cycle_event_row_deserialize_all_fields
**Risk**: R-06
**AC**: AC-03, AC-19

Deserialize a known JSON string into `CycleEventRow` with all 9 fields:
- `id: 10`, `cycle_id: "nxs-012"`, `seq: 1`, `event_type: "cycle_start"`, `phase: Some("design")`, `outcome: Some("complete")`, `next_phase: Some("delivery")`, `timestamp: 1700000000`, `goal: Some("extend export")`

### test_cycle_event_row_nullable_fields
**Risk**: R-06
**AC**: AC-03

Deserialize with `phase`, `outcome`, `next_phase`, `goal` all `null`. Assert all are `None`.

### test_cycle_event_row_with_goal_embedding_key
**Risk**: R-06
**AC**: AC-19

Deserialize a JSON string that includes `"goal_embedding": null` alongside the 9 expected fields. Verify behavior: either succeeds (serde ignores unknown field) or produces a clear error. Document the outcome. If serde's default mode (deny_unknown_fields not set) is used, this should succeed silently.

### test_export_row_graph_edge_variant
**Risk**: none (structural)
**AC**: AC-01

Deserialize `{"_table": "graph_edges", "source_id": 1, ...}` into `ExportRow`. Assert matches `ExportRow::GraphEdge(r)` where `r.source_id == 1`.

### test_export_row_observation_variant
**Risk**: none (structural)
**AC**: AC-02

Deserialize `{"_table": "observations", "id": 5, ...}` into `ExportRow`. Assert matches `ExportRow::Observation(r)` where `r.id == 5`.

### test_export_row_cycle_event_variant
**Risk**: none (structural)
**AC**: AC-03

Deserialize `{"_table": "cycle_events", "id": 10, ...}` into `ExportRow`. Assert matches `ExportRow::CycleEvent(r)` where `r.id == 10`.

### test_export_row_unknown_table_error
**Risk**: R-11
**AC**: AC-07 (format guard is primary defense)

Deserialize `{"_table": "unknown_table", "data": 1}` into `ExportRow`. Assert deserialization error.

### test_graph_edge_weight_f64_precision
**Risk**: R-01
**AC**: AC-11

Deserialize `GraphEdgeRow` with `"weight": 0.7777777777777`. Assert `weight.to_bits() == 0.7777777777777_f64.to_bits()` (no f32 truncation).

## Edge Cases

- `source_id` and `target_id` at `i64::MAX` (edge case #5 from Risk Strategy)
- Unicode in `metadata`, `goal`, `tool` fields (edge case #6)
- `weight: 0.0` -- must parse as 0.0, not confused with NaN fallback (edge case #7)

## Coverage Mapping

| Risk | Scenarios Covered | Tests |
|------|-------------------|-------|
| R-01 | f64 precision in deserialization | test_graph_edge_weight_f64_precision |
| R-06 | Unknown goal_embedding field | test_cycle_event_row_with_goal_embedding_key |
| R-10 | Nullable metadata | test_graph_edge_row_nullable_metadata |
| R-11 | Unknown _table tag | test_export_row_unknown_table_error |
