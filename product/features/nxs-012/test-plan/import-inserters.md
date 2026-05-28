# Test Plan: import-inserters (C3 — import/inserters.rs)

## Scope

3 new inserter functions: `insert_graph_edge`, `insert_observation`, `insert_cycle_event`. Each uses parameterized sqlx queries on `&mut SqliteConnection`.

## Unit Tests (in `inserters.rs` #[cfg(test)])

### test_insert_graph_edge_all_columns
**Risk**: R-07
**AC**: AC-01

Insert a `GraphEdgeRow` with all 9 fields populated. Query `graph_edges` table. Assert all 9 column values match. Assert the synthetic `id` column was auto-assigned (not controlled by the inserter).

### test_insert_graph_edge_nullable_metadata_null
**Risk**: R-10
**AC**: AC-12

Insert a `GraphEdgeRow` with `metadata: None`. Query. Assert `metadata IS NULL`.

### test_insert_graph_edge_nullable_metadata_populated
**Risk**: R-10

Insert a `GraphEdgeRow` with `metadata: Some("{\"score\": 0.9}")`. Query. Assert exact string match.

### test_insert_graph_edge_plain_insert_not_ignore
**Risk**: R-03
**AC**: AC-18

Insert a `GraphEdgeRow`. Insert a second row with the same `(source_id, target_id, relation_type)`. Assert the second INSERT returns a UNIQUE constraint violation error. This confirms plain INSERT is used, not INSERT OR IGNORE.

### test_insert_graph_edge_duplicate_different_relation
**Risk**: R-03

Insert two `GraphEdgeRow` with same (source_id, target_id) but different `relation_type`. Assert both succeed (UNIQUE key includes relation_type).

### test_insert_observation_all_columns
**Risk**: none (structural)
**AC**: AC-02, AC-16

Insert an `ObservationRow` with all 10 fields including id=42. Query `observations`. Assert `id == 42` (explicit id preserved, ADR-006). Assert all other columns match.

### test_insert_observation_nullable_fields_null
**Risk**: R-09
**AC**: AC-02

Insert `ObservationRow` with `tool`, `input`, `response_size`, `response_snippet`, `topic_signal`, `phase` all `None`. Query. Assert all are SQL NULL.

### test_insert_observation_id_preserved
**Risk**: R-05
**AC**: AC-16

Insert `ObservationRow` with `id = 999`. Query `SELECT id FROM observations`. Assert returns 999. This verifies explicit id binding in the INSERT statement.

### test_insert_observation_id_collision
**Risk**: R-05
**AC**: AC-16

Insert `ObservationRow` with `id = 1`. Insert a second `ObservationRow` with `id = 1`. Assert PRIMARY KEY constraint error.

### test_insert_cycle_event_all_columns
**Risk**: R-06
**AC**: AC-03, AC-17

Insert a `CycleEventRow` with all 9 fields including id=77. Query `cycle_events`. Assert `id == 77`. Assert all 8 other columns match. Assert `goal_embedding IS NULL` (inserter binds NULL, ADR-004).

### test_insert_cycle_event_goal_embedding_null
**Risk**: R-15
**AC**: AC-19

Insert a `CycleEventRow`. Query `SELECT goal_embedding FROM cycle_events`. Assert the result is NULL. This verifies the inserter explicitly binds NULL for the excluded BLOB column.

### test_insert_cycle_event_nullable_fields_null
**Risk**: R-06
**AC**: AC-03

Insert `CycleEventRow` with `phase`, `outcome`, `next_phase`, `goal` all `None`. Query. Assert all are SQL NULL.

### test_insert_cycle_event_id_preserved
**Risk**: R-05
**AC**: AC-17

Insert `CycleEventRow` with `id = 888`. Query `SELECT id FROM cycle_events`. Assert returns 888.

### test_insert_cycle_event_id_collision
**Risk**: R-05

Insert two `CycleEventRow` with same `id`. Assert PRIMARY KEY constraint error.

## Integration Boundary Tests

These tests are implemented in `import_integration.rs` because they require full export -> import -> verify round-trips:

### test_graph_edge_round_trip
**AC**: AC-15

Export DB with graph_edges. Import into fresh DB. Query graph_edges in target DB. Assert all fields match source. Verify the auto-assigned `id` in the target may differ (ADR-005).

### test_observation_round_trip
**AC**: AC-15, AC-16

Export DB with observations. Import into fresh DB. Assert `id` values preserved exactly.

### test_cycle_event_round_trip
**AC**: AC-15, AC-17

Export DB with cycle_events. Import into fresh DB. Assert `id` values preserved. Assert `goal_embedding IS NULL` in target.

## Coverage Mapping

| Risk | Scenarios Covered | Tests |
|------|-------------------|-------|
| R-03 | Duplicate natural key -> UNIQUE error; different relation_type -> success | 2 unit tests |
| R-05 | ID collision without --force; ID preserved | 4 unit tests |
| R-06 | goal_embedding NULL on insert; nullable fields | 2 unit tests |
| R-07 | Valid edge references | test_insert_graph_edge_all_columns |
| R-10 | NULL metadata, populated metadata | 2 unit tests |
| R-15 | goal_embedding NULL in DB | test_insert_cycle_event_goal_embedding_null |
