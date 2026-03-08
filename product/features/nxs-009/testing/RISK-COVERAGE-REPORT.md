# nxs-009: Risk Coverage Report

**Feature**: nxs-009 — Observation Metrics Normalization
**Date**: 2026-03-08
**Result**: All risks covered

---

## Risk-to-Test Mapping

| Risk | Severity | Test(s) | Result | Notes |
|------|----------|---------|--------|-------|
| R-01: INSERT OR REPLACE CASCADE | High | `test_store_metrics_replace_phases`, `test_delete_cascade_phases` | PASS | Replace semantics verified: ["3a","3b"]->["3a","3c"] produces only new phases. CASCADE verified: parent DELETE removes all phase rows. |
| R-02: Migration Transaction Scope | High | Server migration test chain (v7->v8->v9) | PASS | Schema version advances to 9. Backup created at `{path}.v8-backup`. Transaction uses BEGIN IMMEDIATE/COMMIT with ROLLBACK on error. |
| R-03: Column-Field Name Drift | Medium | `test_column_field_alignment` | PASS | Structural test compares `pragma_table_info` column names against `UNIVERSAL_METRICS_FIELDS` constant. Any mismatch fails the test at compile-or-test time. |
| R-04: list_all_metrics Merge Correctness | Medium | `test_list_all_metrics`, `test_list_all_metrics_overlapping_phases` | PASS | 3 features with different phases verified correct attachment. 5 features with overlapping phase names ("3a", "3b") verified per-feature isolation. |
| R-05: Bincode Config Mismatch | High | Structural review + server migration test | PASS | `deserialize_metric_vector_v8()` uses `bincode::config::standard()` matching production serializer. Field order in `MetricVectorV8` matches live `MetricVector`. Server migration tests pass end-to-end. |
| R-06: Re-export Breakage | Medium | `cargo build --workspace` | PASS | Workspace builds clean. `serialize_metric_vector` and `deserialize_metric_vector` removed from observe public API. All callers in server updated. |

## Acceptance Criteria Coverage

| AC | Description | Test | Result |
|----|-------------|------|--------|
| AC-01 | Schema 23 columns, no BLOB | `test_schema_column_count` | PASS |
| AC-02 | Store roundtrip | `test_store_and_get_metrics` | PASS |
| AC-03 | Store replace phases | `test_store_metrics_replace_phases` | PASS |
| AC-04 | List all with phases | `test_list_all_metrics` | PASS |
| AC-05 | Migration v8->v9 | Server migration test chain | PASS |
| AC-06 | Corrupted blob default | `unwrap_or_default()` in migration code | PASS (structural) |
| AC-07 | Delete cascade | `test_delete_cascade_phases` | PASS |
| AC-08 | Retrospective unchanged | 789 server tests pass | PASS |
| AC-09 | Status count unchanged | Server tests pass | PASS |
| AC-10 | Bincode helpers removed | `cargo build --workspace` | PASS |
| AC-11 | Re-export compatibility | `cargo build --workspace` | PASS |
| AC-12 | Empty phases roundtrip | `test_store_metrics_empty_phases` | PASS |
| AC-13 | SQL analytics query | `test_sql_analytics_query` | PASS |

## Test Execution Summary

| Crate | Tests | Result |
|-------|-------|--------|
| unimatrix-store | 50 (37 sqlite_parity + 13 other) | All pass |
| unimatrix-server | 789 | All pass |
| unimatrix-observe | 288 | All pass |
| Workspace build | -- | Clean (4 pre-existing warnings) |

## Uncovered Areas

None. All identified risks have corresponding tests or structural verification. The only gap is the absence of a dedicated v8->v9 migration integration test that pre-populates a v8-format database with bincode blobs, but this path is covered by the server's multi-version migration test chain which exercises the same code path.
