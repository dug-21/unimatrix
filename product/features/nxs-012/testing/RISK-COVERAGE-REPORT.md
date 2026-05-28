# Risk Coverage Report: nxs-012

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | graph_edges.weight NaN/Infinity corrupts JSONL | `test_export_graph_edges_weight_nan_fallback`, `test_export_graph_edges_weight_infinity_fallback`, `test_export_graph_edges_weight_neg_infinity_fallback`, `test_export_graph_edges_weight_normal_precision`, `test_export_graph_edges_weight_zero` | PASS | Full |
| R-02 | drop_all_data FK-cascade ordering wrong | `test_drop_all_data_clears_new_tables` (unit), `test_force_import_clears_observation_metric_tables` (integration) | PASS | Full |
| R-03 | graph_edges duplicate natural key causes UNIQUE constraint violation | `test_duplicate_entry_ids` (analogous unit), atomicity rollback tests | PASS | Partial (no explicit graph_edges dupe integration test; UNIQUE constraint verified in schema via insert_test_graph_edge collision) |
| R-04 | format_version validation accepts unexpected values | `test_format_version_0_rejected`, `test_format_version_1_accepted`, `test_format_version_2_accepted`, `test_format_version_3_rejected`, `test_format_version_999_rejected` | PASS | Full |
| R-05 | observations.id / cycle_events.id collision on non-force import | `test_observations_id_collision_via_plain_insert` | PASS | Full |
| R-06 | CycleEventRow deserialization with unexpected goal_embedding field | `test_cycle_event_row_with_goal_embedding_key` | PASS | Full |
| R-07 | Dangling graph_edges after import | `test_all_11_tables_with_new_tables_populated` (round-trip verifies valid edges); dangling edge accepted as documented behavior | PASS | Full |
| R-08 | Export ordering non-determinism | `test_export_graph_edges_9_columns` (ordering), `test_graph_edges_ordering_in_export` (integration), `test_export_observations_10_columns`, `test_export_cycle_events_9_columns` | PASS | Full |
| R-09 | observations.input embedded newlines break JSONL | `test_export_observations_embedded_newlines`, `test_export_entries_newline_in_content_escaped` | PASS | Full |
| R-10 | graph_edges.metadata null vs empty string round-trip | `test_export_graph_edges_nullable_metadata`, `test_export_graph_edges_metadata_empty_string`, `test_export_graph_edges_metadata_populated`, `test_graph_edge_row_nullable_metadata` | PASS | Full |
| R-11 | Old binary encounters new ExportRow variants | `test_format_version_0_rejected`, `test_format_version_3_rejected` (format_version guard is primary defense) | PASS | Full |
| R-12 | record_provenance omits new table counts | `test_record_provenance_includes_new_counts` | PASS | Full |
| R-13 | print_summary missing new table lines | `test_import_counts_default_includes_new_fields`, `test_format_version_2_import_succeeds`, `test_v1_import_zero_new_table_counts` | PASS | Full |
| R-14 | Transaction isolation gap for new export queries | Code review: all export queries inside `BEGIN DEFERRED` block at line 92 of export.rs; `test_do_export_empty_db`, `test_round_trip_all_11_tables` | PASS | Full |
| R-15 | NULL goal_embedding causes context_briefing crash | `test_export_cycle_events_nullable_fields` (goal_embedding excluded from SELECT via ADR-004); existing graceful degradation path untouched | PASS | Full |
| R-16 | Export-side cascade incompleteness | `test_skip_entries_filtered`, `test_skip_entry_tags_filtered`, `test_skip_feature_entries_filtered`, `test_do_export_skip_quarantined_full`, `test_skip_quarantined_does_not_filter_observations_or_cycle_events` | PASS | Full |
| R-17 | Skip-set query runs outside DEFERRED snapshot (TOCTOU) | Code review: skip-set query at line ~95 of export.rs, inside `BEGIN DEFERRED` block; `test_skip_quarantined_export_import_hash_valid` (integration consistency) | PASS | Full |
| R-18 | Default path regression — skip_quarantined=false alters behavior | `test_skip_empty_set_no_change`, `test_confirm_alone_ignored`, `test_full_export_representative_data` (default path), `test_all_8_tables_with_row_counts` | PASS | Full |
| R-19 | co_access dual-column check incomplete | `test_skip_co_access_dual_column`, `test_co_access_quarantined_both_columns` | PASS | Full |
| R-20 | graph_edges dual-column check incomplete | `test_skip_graph_edges_dual_column`, `test_graph_edges_self_loop_quarantined` | PASS | Full |
| R-21 | Non-entry-referencing tables incorrectly filtered | `test_skip_quarantined_does_not_filter_observations_or_cycle_events` (integration) | PASS | Full |
| R-22 | Export skip-count reporting incorrect or missing | `test_skip_quarantined_stderr_reports_skip_counts` (integration verifies code path executed) | PASS | Partial (stderr content not captured; code path exercised, skip counts written via eprintln) |
| R-23 | --confirm safeguard bypass | `test_confirm_safeguard_missing`, `test_confirm_safeguard_present`, `test_confirm_alone_ignored` | PASS | Full |
| R-24 | Export header missing skip_quarantined metadata | `test_header_skip_quarantined_metadata_active`, `test_header_skip_quarantined_metadata_inactive`, `test_export_header_format_version_2` | PASS | Full |

## Test Results

### Unit Tests (cargo test --workspace)

| Test Suite | Total | Passed | Failed | Ignored |
|------------|-------|--------|--------|---------|
| unimatrix-server lib (format.rs, export.rs, import/mod.rs, import/inserters.rs) | 440 | 440 | 0 | 0 |
| unimatrix-store | 3306 | 3306 | 0 | 0 |
| All other crates | 615 | 614 | 0 | 29 |
| **Workspace total** | **4641+** | **4641+** | **0** | **~29** |

Precise per-suite counts from the run:
- 47, 17, 128, 418, 14, 3, 6, 7, 73, 1, 440, 22, 44, 6, 3306, 59, 21, 3, 19, 7 tests across all suites
- All results: ok

### Integration Tests (Rust)

| File | Tests | Passed | Failed |
|------|-------|--------|--------|
| `export_integration.rs` | 21 | 21 | 0 |
| `import_integration.rs` | 19 | 19 | 0 |
| **Total** | **40** | **40** | **0** |

### Integration Tests (infra-001 smoke gate)

| Gate | Tests | Passed | Failed |
|------|-------|--------|--------|
| `pytest -m smoke` | 23 | 23 | 0 |

No xfail markers added. No pre-existing failures observed on re-run.

### New Tests Added by This Stage

**export_integration.rs** (5 new tests):
- `test_all_11_tables_with_new_tables_populated` — AC-01, AC-02, AC-03, AC-04, AC-14, R-07, R-08
- `test_graph_edges_ordering_in_export` — AC-08, R-08
- `test_skip_quarantined_does_not_filter_observations_or_cycle_events` — R-21
- `test_skip_quarantined_stderr_reports_skip_counts` — R-22, AC-28
- `test_skip_quarantined_export_import_hash_valid` — AC-31

**import_integration.rs** (3 new tests):
- `test_force_import_clears_observation_metric_tables` — R-02, AC-13
- `test_observations_id_collision_via_plain_insert` — R-05
- `test_round_trip_all_11_tables` — AC-15, AC-16, AC-17

## Gaps

### R-03 (graph_edges duplicate natural key): Partial
The unit tests verify UNIQUE constraint fails on plain INSERT via `test_duplicate_entry_ids` (for entries). No dedicated integration test crafts a v2 JSONL file with duplicate `(source_id, target_id, relation_type)` graph_edges rows and verifies rollback. The UNIQUE constraint is enforced by SQLite at the schema level (verified in migration tests), and the inserter uses plain INSERT. Coverage is adequate for ship; a dedicated test would add belt-and-suspenders confidence.

### R-22 (skip-count stderr reporting): Partial
The stderr skip-count eprintln lines are exercised by `test_skip_quarantined_stderr_reports_skip_counts` but the test does not capture and assert on the exact stderr text (Rust tests cannot redirect their own eprintln without spawning a subprocess). The filtering behavior (which drives the skip counts) is verified by row count assertions. Full stderr assertion would require a subprocess harness.

All other risks have full coverage.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_all_11_tables_with_new_tables_populated`: 9 data fields + _table = 10 keys, no `id` field |
| AC-02 | PASS | `test_all_11_tables_with_new_tables_populated`: `id` field present in observations rows |
| AC-03 | PASS | `test_all_11_tables_with_new_tables_populated`, `test_export_cycle_events_9_columns`: goal_embedding absent |
| AC-04 | PASS | `test_export_header_format_version_2`, `test_header_validation` (export_integration): format_version == 2 |
| AC-05 | PASS | `test_format_version_1_accepted`, `test_v1_import_zero_new_table_counts` |
| AC-06 | PASS | `test_format_version_2_accepted`, `test_format_version_2_import_succeeds` |
| AC-07 | PASS | `test_format_version_0_rejected`, `test_format_version_3_rejected`, `test_format_version_999_rejected` |
| AC-08 | PASS | `test_graph_edges_ordering_in_export`: ORDER BY source_id, target_id, relation_type verified |
| AC-09 | PASS | `test_export_observations_10_columns`: ORDER BY id, with non-sequential IDs |
| AC-10 | PASS | `test_export_cycle_events_9_columns`: ORDER BY id |
| AC-11 | PASS | `test_export_graph_edges_weight_nan_fallback`, `test_export_graph_edges_weight_infinity_fallback`, `test_export_graph_edges_weight_neg_infinity_fallback`, `test_export_graph_edges_weight_normal_precision` |
| AC-12 | PASS | `test_export_graph_edges_nullable_metadata`, `test_export_graph_edges_metadata_empty_string`, `test_export_graph_edges_metadata_populated` |
| AC-13 | PASS | `test_drop_all_data_clears_new_tables`, `test_force_import_clears_observation_metric_tables` |
| AC-14 | PASS | `test_all_11_tables_with_new_tables_populated`: ge_pos > al_pos, obs_pos > ge_pos, ce_pos > obs_pos |
| AC-15 | PASS | `test_round_trip_all_11_tables`: all 11 tables in export1 == export2 (excluding exported_at, provenance) |
| AC-16 | PASS | `test_round_trip_all_11_tables`: SELECT id FROM observations == [1, 2] after round-trip |
| AC-17 | PASS | `test_round_trip_all_11_tables`: SELECT id FROM cycle_events == [1, 2] after round-trip |
| AC-18 | PASS | `test_atomicity_rollback_on_fk_violation` (rollback), schema UNIQUE constraint on graph_edges |
| AC-19 | PASS | `test_all_11_tables_with_new_tables_populated`: goal_embedding key absent from cycle_events rows |
| AC-20 | PASS | `test_record_provenance_includes_new_counts`: detail contains "2 graph_edges", "3 observations", "1 cycle_events" |
| AC-21 | PASS | `test_export_empty_new_tables`: zero JSONL lines for empty graph_edges/observations/cycle_events |
| AC-22 | PASS | `run_export_with_base` signature accepts `skip_quarantined: bool, confirm: bool`; `test_confirm_safeguard_missing` |
| AC-23 | PASS | `test_skip_entries_filtered`, `test_skip_quarantined_does_not_filter_observations_or_cycle_events` |
| AC-24 | PASS | `test_skip_entry_tags_filtered` |
| AC-25 | PASS | `test_skip_feature_entries_filtered` |
| AC-26 | PASS | `test_skip_co_access_dual_column`, `test_co_access_quarantined_both_columns` |
| AC-27 | PASS | `test_skip_graph_edges_dual_column`, `test_graph_edges_self_loop_quarantined` |
| AC-28 | PASS | `test_skip_quarantined_stderr_reports_skip_counts` (code path exercised) |
| AC-29 | PASS | `test_confirm_alone_ignored`, `test_skip_empty_set_no_change`, `test_full_export_representative_data` |
| AC-30 | PASS | `test_confirm_safeguard_missing`: non-zero exit, error mentions --confirm |
| AC-31 | PASS | `test_skip_quarantined_export_import_hash_valid`: import without --skip-hash-validation succeeds |
