# Test Plan: skip-quarantined (C5 — export.rs)

## Scope

`--skip-quarantined` and `--confirm` CLI flags. Skip-set construction inside DEFERRED transaction. `skip_ids: &HashSet<i64>` threaded through `do_export` and 5 affected exporters. Skip-count reporting. Header metadata.

## Unit Tests (in `export.rs` #[cfg(test)])

### test_confirm_safeguard_missing
**Risk**: R-23
**AC**: AC-30

Call `run_export_with_base` with `skip_quarantined=true, confirm=false`. Assert error. Assert error message contains "--confirm". Assert no output file created.

### test_confirm_safeguard_present
**Risk**: R-23
**AC**: AC-30

Call with `skip_quarantined=true, confirm=true`. Assert export succeeds (no error from safeguard).

### test_confirm_alone_ignored
**Risk**: R-23
**AC**: AC-29

Call with `skip_quarantined=false, confirm=true`. Assert export succeeds. Verify output identical to default (confirm silently ignored per ADR-009).

### test_header_skip_quarantined_metadata_active
**Risk**: R-24
**AC**: AC-31

Export with `--skip-quarantined --confirm`. Parse header. Assert `skip_quarantined` key is `true`.

### test_header_skip_quarantined_metadata_inactive
**Risk**: R-24

Export without `--skip-quarantined`. Parse header. Assert `skip_quarantined` key is absent or `false`.

## Integration Tests (extend `export_integration.rs`)

### test_skip_quarantined_entries_filtered
**Risk**: R-16, R-18
**AC**: AC-23

Populate DB with 5 entries: ids 1-3 (status=1 active), ids 4-5 (status=3 quarantined). Export with `--skip-quarantined --confirm`. Parse JSONL. Assert:
- Exactly 3 `entries` rows present (ids 1, 2, 3)
- Zero `entries` rows with id 4 or 5
- Header `entry_count` reflects full DB count (5) or filtered count -- document whichever is implemented

### test_skip_quarantined_entry_tags_filtered
**Risk**: R-16
**AC**: AC-24

Populate DB with entries (3 active, 2 quarantined) and entry_tags for all. Export with `--skip-quarantined --confirm`. Assert:
- entry_tags for active entries (entry_id 1, 2, 3) present
- entry_tags for quarantined entries (entry_id 4, 5) absent

### test_skip_quarantined_feature_entries_filtered
**Risk**: R-16
**AC**: AC-25

Populate feature_entries referencing both active and quarantined entries. Export with `--skip-quarantined --confirm`. Assert:
- feature_entries for active entries present
- feature_entries for quarantined entries absent

### test_skip_quarantined_co_access_dual_column
**Risk**: R-19
**AC**: AC-26

Populate co_access with 4 rows covering all combinations:
- (active, active) -- entry_id_a=1, entry_id_b=2
- (quarantined, active) -- entry_id_a=4, entry_id_b=1
- (active, quarantined) -- entry_id_a=2, entry_id_b=5
- (quarantined, quarantined) -- entry_id_a=4, entry_id_b=5

Export with `--skip-quarantined --confirm`. Assert:
- Only the (active, active) row present in output
- All 3 rows with any quarantined endpoint absent

### test_skip_quarantined_graph_edges_dual_column
**Risk**: R-20
**AC**: AC-27

Populate graph_edges with 4 rows covering all combinations:
- (active_source, active_target) -- source_id=1, target_id=2
- (quarantined_source, active_target) -- source_id=4, target_id=1
- (active_source, quarantined_target) -- source_id=2, target_id=5
- (quarantined_source, quarantined_target) -- source_id=4, target_id=5

Export with `--skip-quarantined --confirm`. Assert:
- Only the (active, active) row present
- All 3 rows with quarantined endpoints absent

### test_skip_quarantined_unaffected_tables
**Risk**: R-21
**AC**: AC-25 (FR-25)

Populate DB with quarantined entries plus rows in observations, cycle_events, counters, outcome_index, agent_registry, audit_log. Export with `--skip-quarantined --confirm`. Assert:
- observations row count matches `SELECT COUNT(*) FROM observations`
- cycle_events row count matches source DB
- counters row count matches source DB
- outcome_index row count matches source DB
- agent_registry row count matches source DB
- audit_log row count matches source DB (including entries mentioning quarantined IDs)

### test_skip_quarantined_default_path_no_change
**Risk**: R-18
**AC**: AC-29

Populate DB with quarantined entries (status=3) and all dependent rows. Export WITHOUT `--skip-quarantined`. Assert:
- All entries including quarantined present in output
- All entry_tags for quarantined entries present
- All co_access rows referencing quarantined entries present
- All graph_edges with quarantined endpoints present
- Row counts match `SELECT COUNT(*)` for every table

### test_skip_quarantined_cascade_completeness
**Risk**: R-16
**AC**: AC-23, AC-24, AC-25, AC-26, AC-27

Critical integrated test. Populate DB with 2 quarantined entries, each having:
- 2 entry_tags
- 1 feature_entry
- 1 co_access (with active entry)
- 1 graph_edge (as source, to active target)

Plus 3 active entries with their own dependents. Export with `--skip-quarantined --confirm`. Scan the entire export file: assert zero occurrences of quarantined entry IDs across ALL `_table` types.

### test_skip_quarantined_round_trip_import
**Risk**: R-16, R-17
**AC**: AC-31

Export with `--skip-quarantined --confirm`. Import the resulting file into a fresh DB without `--skip-hash-validation`. Assert:
- Import succeeds (hash integrity valid)
- No entry with status=3 in target DB
- No orphaned entry_tags, feature_entries, co_access, graph_edges referencing missing entries

### test_skip_quarantined_skip_counts_reported
**Risk**: R-22
**AC**: AC-28

Export with `--skip-quarantined --confirm` from DB with quarantined entries and dependents. Capture stderr. Assert:
- Line reporting skipped entry count (e.g., "skipped 2 quarantined entries")
- Per-table skipped dependent row counts present

### test_skip_quarantined_no_skip_counts_default
**Risk**: R-22
**AC**: AC-28

Export WITHOUT `--skip-quarantined`. Capture stderr. Assert no skip-related lines in output.

### test_skip_quarantined_zero_quarantined_entries
**Risk**: R-18 (edge case #9)

Flag active but no entries have status=3. Assert export produces identical output to default path (all rows exported, skip counts are 0).

### test_skip_quarantined_all_entries_quarantined
**Risk**: R-16 (edge case #10)

All entries have status=3. Export with `--skip-quarantined --confirm`. Assert:
- Zero entries rows in output
- Zero entry_tags, feature_entries, co_access, graph_edges rows
- Counters, audit_log, observations, cycle_events still present

### test_co_access_self_referencing_quarantined
**Risk**: R-19 (edge case #11)

co_access row with `entry_id_a == entry_id_b` where the entry is quarantined. Assert filtered.

### test_graph_edges_self_loop_quarantined
**Risk**: R-20 (edge case #12)

graph_edge with `source_id == target_id` where the entry is quarantined. Assert filtered.

## Coverage Mapping

| Risk | Scenarios Covered | Tests |
|------|-------------------|-------|
| R-16 | Full cascade, per-table, integrated | 4 integration tests |
| R-17 | Round-trip import with hash validation | 1 integration test + code review |
| R-18 | Default path unchanged, zero quarantined, confirm-alone | 3 tests |
| R-19 | 4-combination matrix, self-referencing | 2 integration tests |
| R-20 | 4-combination matrix, self-loop | 2 integration tests |
| R-21 | 6 unaffected tables at full count | 1 integration test |
| R-22 | Skip counts present when active, absent when inactive | 2 integration tests |
| R-23 | 4 flag combinations (neither, skip-only, both, confirm-only) | 3 unit + 1 integration |
| R-24 | Header metadata present/absent | 2 unit tests |
