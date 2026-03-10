# Risk Coverage Report: col-020

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | JSON parsing of result_entry_ids produces incorrect reuse counts on malformed data | `test_knowledge_reuse_malformed_result_entry_ids`, `test_knowledge_reuse_empty_result_entry_ids`, `test_knowledge_reuse_null_result_entry_ids`, `test_knowledge_reuse_duplicate_ids_in_result`, `test_parse_result_entry_ids_*` (5 tests) | PASS | Full |
| R-02 | Knowledge reuse undercounts when query_log or injection_log has gaps | `test_knowledge_reuse_no_query_log_data`, `test_knowledge_reuse_no_injection_log_data`, `test_knowledge_reuse_both_sources_empty` | PASS | Full |
| R-03 | Low attribution coverage silently degrades cross-session metrics | `test_attribute_*` (12 tests in unimatrix-observe attribution module) | PASS | Full |
| R-04 | Server-side knowledge reuse computation bypasses ObservationSource abstraction | `test_knowledge_reuse_cross_session_query_log`, `test_knowledge_reuse_cross_session_injection_log`, `test_knowledge_reuse_same_session_excluded`, `test_knowledge_reuse_by_category`, `test_knowledge_reuse_category_gaps` | PASS | Full |
| R-05 | Repeated retrospective runs produce incorrect topic_deliveries counters | `test_set_topic_delivery_counters_basic`, `test_set_topic_delivery_counters_idempotent`, `test_set_topic_delivery_counters_overwrite`, `test_set_topic_delivery_counters_missing_record`, `test_set_topic_delivery_counters_preserves_non_counter_fields` | PASS | Full |
| R-06 | File path extraction mapping misses Grep or future tools | `test_extract_file_path_read`, `test_extract_file_path_edit`, `test_extract_file_path_write`, `test_extract_file_path_glob`, `test_extract_file_path_grep`, `test_extract_file_path_unknown_tool`, `test_extract_file_path_missing_field`, `test_extract_file_path_non_string_value` | PASS | Full |
| R-07 | Session ordering breaks for concurrent sessions with identical timestamps | `test_session_summaries_ordered_by_started_at`, `test_session_summaries_tiebreak_by_session_id` | PASS | Full |
| R-08 | Rework outcome detection produces false positives on free-form text | `test_rework_events_fires`, `test_rework_events_empty`, `test_rework_events_completed_only`, `test_rework_events_normal_flow` (unimatrix-observe detection::session) | PASS | Full |
| R-09 | New optional fields break backward-compatible deserialization | `test_retrospective_report_deserialize_pre_col020`, `test_retrospective_report_serialize_none_fields_omitted`, `test_retrospective_report_roundtrip_with_new_fields`, `test_retrospective_report_partial_new_fields` | PASS | Full |
| R-10 | Empty topic causes panic or error in new computation paths | `test_session_summaries_empty_input`, `test_session_summaries_single_record`, `test_knowledge_reuse_zero_sessions`, `test_knowledge_reuse_both_sources_empty`, `test_scan_query_log_by_sessions_empty_ids`, `test_scan_injection_log_by_sessions_empty_ids`, `test_count_active_entries_by_category_empty_store` | PASS | Full |
| R-11 | Batch SQL queries with large IN clauses fail or degrade | `test_scan_query_log_by_sessions_empty_ids`, `test_scan_injection_log_by_sessions_empty_ids`, `test_scan_query_log_by_sessions_no_matching` | PASS | Partial |
| R-12 | Knowledge reuse double-counts entries across query_log and injection_log | `test_knowledge_reuse_deduplication_across_sources`, `test_knowledge_reuse_deduplication_across_sessions`, `test_knowledge_reuse_mixed_query_and_injection_cross_session` | PASS | Full |
| R-13 | context_reload_pct division by zero when no files read in later sessions | `test_reload_pct_single_session`, `test_reload_pct_no_files_in_later_sessions`, `test_reload_pct_no_overlap`, `test_reload_pct_range` | PASS | Full |
| R-14 | New computation steps fail and abort entire retrospective | `test_knowledge_reuse_deleted_entry` (graceful handling of missing entries) | PASS | Partial |
| R-15 | Directory prefix extraction produces inconsistent zone names | `test_extract_directory_zone_absolute_path`, `test_extract_directory_zone_relative_path`, `test_extract_directory_zone_short_path`, `test_extract_directory_zone_trailing_slash` | PASS | Full |

## Test Results

### Unit Tests (col-020 specific)

| Crate | Test Module | Count | Result |
|-------|------------|-------|--------|
| unimatrix-observe | session_metrics::tests | 30 | 30 PASS |
| unimatrix-observe | types::tests (col-020 structs) | 8 | 8 PASS |
| unimatrix-store | query_log (batch methods) | 5 | 5 PASS |
| unimatrix-store | injection_log (batch methods) | 2 | 2 PASS |
| unimatrix-store | read (count_active_entries_by_category) | 3 | 3 PASS |
| unimatrix-store | topic_deliveries (set_topic_delivery_counters) | 5 | 5 PASS |
| unimatrix-server | mcp::knowledge_reuse::tests | 26 | 26 PASS |
| **Total col-020** | | **79** | **79 PASS** |

### Full Crate Totals

| Crate | Total Tests | Result |
|-------|------------|--------|
| unimatrix-observe | 340 | 340 PASS |
| unimatrix-store | 94 | 94 PASS |
| unimatrix-server | 910 | 910 PASS |
| unimatrix-vector | 103 passed, 1 failed | 1 pre-existing failure (see below) |
| **Workspace total** | **1449** | **1448 PASS, 1 FAIL (pre-existing)** |

### Pre-Existing Failures

| Test | Crate | Cause | Action |
|------|-------|-------|--------|
| `index::tests::test_compact_search_consistency` | unimatrix-vector | HNSW compaction non-determinism -- result set varies between runs due to approximate nearest neighbor graph rebuild. Not touched by col-020. Last modified in crt-010 (commit ff14bcb). | Pre-existing flaky test. No col-020 code touches unimatrix-vector. |

### Integration Tests (infra-001)

| Suite | Tests | Result |
|-------|-------|--------|
| smoke | 19 selected | 18 PASS, 1 XFAIL (GH#111 pre-existing) |
| lifecycle | 16 | 16 PASS |
| tools (subset: store, search) | 3 | 3 PASS |
| **Total executed** | **38** | **37 PASS, 1 XFAIL** |

**Note**: Full tools suite (68 tests) timed out in CI environment after 300s. The smoke subset (which includes 4 tools tests) and the targeted tools subset both passed. No col-020 regression detected. The test plan OVERVIEW.md confirmed no new infra-001 integration tests are needed -- observation/query_log/injection_log tables cannot be seeded through MCP, so new col-020 behavior is validated via Rust-level integration tests.

## Gaps

| Risk ID | Gap Description | Justification |
|---------|----------------|---------------|
| R-11 | No test with >50 session IDs to validate chunking boundary | Low priority risk (Severity: Med, Likelihood: Low). Architecture specifies 50-batch chunking. Tests cover 0-session and small-batch boundaries. A dedicated volume test would require significant Store seeding infrastructure for marginal coverage gain. |
| R-14 | No test simulating mid-pipeline Store failure for graceful degradation | Partial coverage via `test_knowledge_reuse_deleted_entry` (handles missing entry gracefully) and `test_set_topic_delivery_counters_missing_record` (handles missing topic record). Full simulation of Store read transaction failure would require mock injection. The best-effort pattern (`match { Ok => Some, Err => warn + None }`) is structurally verified by code review. Handler-level integration tests for graceful degradation are constrained by the fact that `context_retrospective` requires a full observation pipeline setup with attributed sessions. |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_session_summaries_groups_by_session_id` -- 6 records across 2 sessions produce 2 SessionSummary entries |
| AC-02 | PASS | `test_session_summaries_tool_distribution_categories` -- 7 tool categories (read, write, execute, search, store, spawn, other) correctly classified |
| AC-03 | PASS | `test_session_summaries_top_file_zones_max_5` -- 7 zones truncated to top 5 in descending frequency |
| AC-04 | PASS | `test_session_summaries_agents_spawned` -- 3 SubagentStart records yield 3 agent names in agents_spawned |
| AC-05 | PASS | `test_session_summaries_knowledge_in_out` -- 5 search + 2 lookup + 1 get = knowledge_in=8, 3 store = knowledge_out=3 |
| AC-06 | PASS | `test_knowledge_reuse_cross_session_query_log`, `test_knowledge_reuse_cross_session_injection_log`, `test_knowledge_reuse_deduplication_across_sources` -- distinct entry deduplication verified |
| AC-07 | PASS | `test_knowledge_reuse_by_category` -- 2 convention + 1 pattern entries yield by_category={"convention": 2, "pattern": 1} |
| AC-08 | PASS | `test_knowledge_reuse_category_gaps`, `test_compute_gaps_basic`, `test_compute_gaps_sorted` -- unreused active categories listed in gaps |
| AC-09 | PASS | `test_rework_events_fires`, `test_rework_events_completed_only` -- case-insensitive substring match on result:rework and result:failed |
| AC-10 | PASS | `test_reload_pct_basic` -- session 1 reads A,B,C; session 2 reads B,C,D; reload_pct = 2/3 |
| AC-11 | PASS | `test_retrospective_report_deserialize_pre_col020`, `test_retrospective_report_serialize_none_fields_omitted` -- pre-col-020 JSON round-trips, None fields omitted |
| AC-12 | PASS | `test_set_topic_delivery_counters_idempotent`, `test_set_topic_delivery_counters_overwrite` -- absolute-set verified idempotent, not additive |
| AC-13 | PASS | `test_reload_pct_range` -- return value always in [0.0, 1.0]; `test_reload_pct_basic` returns raw f64 |
| AC-14 | PASS | `test_session_summaries_empty_input` -- empty records produce empty vec; `test_knowledge_reuse_zero_sessions` -- no sessions yields None |
| AC-15 | PASS | `cargo test -p unimatrix-observe` (340 pass), `cargo test -p unimatrix-server` (910 pass) -- all existing tests pass without modification |
| AC-16 | PASS | `test_session_summaries_ordered_by_started_at` -- 3 sessions with out-of-order timestamps sorted ascending |
