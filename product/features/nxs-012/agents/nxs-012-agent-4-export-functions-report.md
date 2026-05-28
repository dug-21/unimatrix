# Agent Report: nxs-012-agent-4-export-functions

## Task
Implement export_graph_edges, export_observations, export_cycle_events functions in export.rs. Integrate into do_export. Bump format_version from 1 to 2.

## Files Modified
- `crates/unimatrix-server/src/export.rs`

## Changes
1. Bumped format_version from 1 to 2 in write_header
2. Added `export_graph_edges` -- 9 columns, ORDER BY (source_id, target_id, relation_type), id omitted (ADR-005), NaN/Inf weight fallback to 1.0 (ADR-003), skip_ids support for quarantine filtering
3. Added `export_observations` -- 10 columns, ORDER BY id, id preserved (ADR-006)
4. Added `export_cycle_events` -- 9 columns, ORDER BY id, id preserved (ADR-006), goal_embedding excluded from SELECT (ADR-004)
5. Integrated 3 new calls into do_export after existing 8 tables
6. Added 20 unit tests per component test plan

Note: The skip-quarantined component (agent-7) modified the same file concurrently, adding skip_ids parameter threading and --confirm/--skip-quarantined CLI flags. The commit includes both sets of changes since they are interleaved in the same file.

## Tests
- 57 unit tests passed (including 20 new), 0 failed
- 2 export integration tests passed
- 3 import integration tests passed
- Total: 63 tests, all passing

## Test Coverage per Test Plan

| Test | Status |
|------|--------|
| test_export_graph_edges_9_columns | PASS |
| test_export_graph_edges_weight_nan_fallback | PASS |
| test_export_graph_edges_weight_infinity_fallback | PASS |
| test_export_graph_edges_weight_neg_infinity_fallback | PASS |
| test_export_graph_edges_weight_normal_precision | PASS |
| test_export_graph_edges_weight_zero | PASS |
| test_export_graph_edges_nullable_metadata | PASS |
| test_export_graph_edges_metadata_empty_string | PASS |
| test_export_graph_edges_metadata_populated | PASS |
| test_export_observations_10_columns | PASS |
| test_export_observations_nullable_fields | PASS |
| test_export_observations_embedded_newlines | PASS |
| test_export_cycle_events_9_columns | PASS |
| test_export_cycle_events_nullable_fields | PASS |
| test_export_header_format_version_2 | PASS |
| test_export_table_emission_order_11_tables | PASS |
| test_export_empty_new_tables | PASS |

## Design Deviations
- NaN/Infinity/NEG_INFINITY weight tests use direct `Number::from_f64` testing instead of DB round-trip, because SQLite NOT NULL REAL columns cannot store NaN (treated as NULL). The defensive code pattern is still exercised. Infinity tests could use DB path (SQLite stores Infinity) but were kept consistent with the NaN approach.

## Pre-existing Issues
- `import::tests::test_drop_all_data_clears_new_tables` fails on the branch before my changes (audit_log UNIQUE constraint). This is in the import-pipeline component scope.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR entries for nxs-012, NaN safety patterns (#4133, #4533), crate architecture patterns. Applied ADR-003 (NaN->1.0 fallback), ADR-004 (goal_embedding exclusion), ADR-005 (graph_edges id omission), ADR-006 (observations/cycle_events id preservation).
- Queried: mcp__unimatrix__context_search -- found NaN safety patterns confirming is_finite() guards and Number::from_f64 fallback approach. No novel patterns to store.
- Stored: nothing novel to store -- implementation followed established patterns from nan-001/nan-002 export/import codebase. The NaN-in-SQLite-NOT-NULL discovery (NaN treated as NULL, preventing insertion) is a SQLite behavior detail rather than a reusable pattern.
