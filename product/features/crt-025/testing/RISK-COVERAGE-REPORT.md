# Risk Coverage Report: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `current_phase` mutation timing: `context_store` reads stale `None` if `SessionState` mutation is delayed behind any async dispatch | `test_listener_cycle_start_with_next_phase_sets_session_phase`, `test_listener_cycle_phase_end_with_next_phase_updates_phase`, `test_listener_cycle_stop_clears_phase`, `test_listener_cycle_phase_end_without_next_phase_no_change`, `test_listener_cycle_start_without_next_phase_no_phase_change`, `test_listener_cycle_phase_end_missing_feature_cycle_no_phase_change` | PASS | Full |
| R-02 | Phase snapshot skew: analytics drain `FeatureEntry` event uses live `SessionState` at flush time, not snapshot at enqueue | `test_analytics_drain_uses_enqueue_time_phase`, `test_analytics_drain_phase_some_persists_value`, `test_analytics_drain_phase_none_persists_null`, `test_usage_context_current_phase_propagates_to_feature_entry`, `test_usage_context_has_current_phase_field`, `test_usage_context_phase_none_produces_null_phase` | PASS | Full |
| R-03 | `outcome` category removal breaks existing tests and silently rejects callers still using that category | `test_category_allowlist_has_seven_categories`, `test_outcome_category_is_not_in_allowlist`, `test_outcome_category_validate_err`, `test_category_allowlist_poison_recovery`, `test_all_remaining_seven_categories_valid`, `test_cycle_outcome_category_rejected` (infra-001) | PASS | Full |
| R-04 | Cross-cycle comparison SQL query returns wrong baseline when 0 or 1 prior features have phase data | `test_cross_cycle_comparison_none_when_zero_prior_features`, `test_cross_cycle_comparison_none_when_one_prior_feature`, `test_cross_cycle_comparison_some_when_two_prior_features`, `test_cross_cycle_comparison_correct_mean`, `test_cross_cycle_excludes_current_feature_data` | PASS | Full |
| R-05 | Schema migration v14→v15 non-idempotent: second run fails on `ALTER TABLE ADD COLUMN` without `pragma_table_info` pre-check | `test_v14_to_v15_migration_idempotent`, `test_pragma_table_info_guard_prevents_duplicate_column`, `test_v14_to_v15_migration_adds_cycle_events_table`, `test_v14_to_v15_migration_adds_phase_column_to_feature_entries` | PASS | Full |
| R-06 | Phase string normalization inconsistency: mixed-case or underscore variants stored without lowercasing | `test_validate_phase_lowercase_normalization`, `test_validate_phase_uppercase_normalization`, `test_validate_phase_mixed_case_normalization`, `test_validate_next_phase_normalization`, `test_validate_phase_space_rejected`, `test_validate_phase_empty_rejected`, `test_validate_phase_64_char_boundary_accepted`, `test_validate_phase_65_char_rejected`, `test_validate_phase_underscore_accepted`, `test_validate_phase_leading_trailing_space_trimmed_passes`, `test_validate_phase_leading_space_trimmed_internal_space_rejected`, `test_cycle_phase_with_space_rejected` (infra-001) | PASS | Full |
| R-07 | `CYCLE_EVENTS` seq duplication under concurrent cross-session writes produces incorrect phase sequence reconstruction | `test_v15_cycle_events_round_trip`, `test_v15_cycle_events_all_nullable_columns_null` (sequential seq), `test_phase_sequence_follows_timestamp_order` | PASS | Partial (concurrent case: advisory nature documented; unit test verifies ordering) |
| R-08 | `context_cycle_review` phase narrative emitted for pre-WA-1 features that have no `CYCLE_EVENTS` rows | `test_retrospective_report_phase_narrative_none_omitted`, `test_retrospective_report_phase_narrative_backward_compat`, `test_cycle_review_no_phase_narrative_for_old_feature` (infra-001), `test_cycle_review_includes_phase_narrative` (infra-001) | PASS | Full |
| R-09 | Hook path hard-fails on `phase-end` validation error instead of logging warning and falling through | `test_hook_phase_end_invalid_phase_space_falls_through`, `test_hook_phase_end_empty_phase_falls_through`, `test_hook_phase_end_valid_phase_emits_cycle_phase_end`, `test_hook_phase_end_no_phase_field_accepted` | PASS | Full |
| R-10 | `create_tables_if_needed` not updated: `CYCLE_EVENTS` table or `feature_entries.phase` column absent on new installs | `test_fresh_db_creates_schema_v15`, `test_fresh_db_cycle_events_table_schema`, `test_db_schema_version_initialized_to_current_on_fresh_db` | PASS | Full |
| R-11 | `AnalyticsWrite::FeatureEntry` internal match arms not updated for new `phase` field — compilation error or silent field default | `test_analytics_write_feature_entry_has_phase_field`, `test_analytics_write_feature_entry_phase_some_matches_stored`, `test_analytics_write_non_exhaustive_contract_preserved`, `test_analytics_drain_phase_some_persists_value`, `test_analytics_drain_phase_none_persists_null` | PASS | Full |
| R-12 | `context_cycle_review` cross-cycle SQL query includes current feature in cross-cycle mean, inflating baseline | `test_cross_cycle_comparison_correct_mean`, `test_cross_cycle_excludes_current_feature_data`, `test_sample_features_reflects_distinct_feature_count_for_pair` | PASS | Full |
| R-13 | `phase-end` event with no prior `start` for the same `cycle_id` causes panic or query error in phase narrative construction | `test_build_phase_narrative_orphaned_phase_end_no_start`, `test_build_phase_narrative_phase_end_only_sequence`, `test_build_phase_narrative_empty_events_empty_sequence` | PASS | Full |
| R-14 | `record_feature_entries` call sites not updated for new `phase` parameter — compilation breakage or wrong data | `test_record_feature_entries_with_phase_some`, `test_record_feature_entries_with_phase_none`, `test_record_feature_entries_multiple_entries_same_phase`, `test_usage_context_current_phase_propagates_to_feature_entry`, `test_write_ext_record_feature_entries_with_phase_some`, `test_write_ext_record_feature_entries_with_phase_none` | PASS | Full |

---

## Test Results

### Unit Tests (cargo test --workspace)

- **Total passed**: 3,284
- **Failed**: 0
- **Ignored**: 27 (NLI model tests requiring disk-resident model — pre-existing)
- **Feature-specific tests identified and passing**: 107

Notable crt-025 unit test modules:
- `phase_narrative::tests` — 22 tests (R-04, R-08, R-12, R-13, AC-12, AC-13, AC-14)
- `infra::validation::tests` — 23 tests including phase normalization (R-06, AC-02, AC-03)
- `infra::categories::tests` — `test_category_allowlist_has_seven_categories`, `test_outcome_category_is_not_in_allowlist`, `test_outcome_category_validate_err`, `test_category_allowlist_poison_recovery`, `test_all_remaining_seven_categories_valid` (R-03, AC-15)
- `uds::hook::tests` — 8 tests (R-09, AC-16)
- `uds::listener::tests` — 7 tests (R-01, AC-05, AC-06, AC-07)
- `services::usage::usage_tests` — 3 tests (R-02, R-14)
- `analytics::tests` — 3 drain integration tests (R-02, R-11)
- `write_ext::tests` — 3 tests (R-14, AC-09)
- `migration_v14_to_v15` integration tests (with `--features test-support`) — 13 tests (R-05, R-10, AC-10, AC-11)
- `mcp::tools::tests` — `test_cycle_params_deserialize_phase_end`, `test_cycle_params_deserialize_phase_end_with_outcome` (AC-01)
- `mcp::response::retrospective::tests` — 9 tests (R-08, AC-12, AC-13)

### Integration Tests (infra-001)

#### Smoke (mandatory gate)
- **Passed**: 20
- **Failed**: 0
- **Duration**: 174s

#### Tools suite
- **Passed**: 82
- **Failed**: 0
- **xfailed**: 1 (pre-existing: GH#305)
- **Duration**: 688s
- **New crt-025 tests added**: 7 (`test_cycle_phase_end_type_accepted`, `test_cycle_phase_end_stores_row`, `test_cycle_invalid_type_rejected`, `test_cycle_phase_with_space_rejected`, `test_cycle_outcome_category_rejected`, `test_cycle_review_includes_phase_narrative`, `test_cycle_review_no_phase_narrative_for_old_feature`)

#### Lifecycle suite
- **Passed**: 29
- **Failed**: 0
- **xfailed**: 1 (pre-existing: tick interval test)
- **Duration**: 262s
- **New crt-025 tests added**: 1 (`test_phase_tag_store_cycle_review_flow`)

#### Edge cases suite
- **Passed**: 23
- **Failed**: 0
- **xfailed**: 1 (pre-existing: GH#111 rate limit)
- **Duration**: 207s
- **Changes**: Updated `test_concurrent_store_operations` — replaced `"outcome"` category (now retired) with `"procedure"` (crt-025 category retirement, ADR-005)

#### Adaptation suite
- **Passed**: 9
- **Failed**: 0
- **xfailed**: 1 (pre-existing: GH#111 rate limit)
- **Duration**: 95s
- **Changes**: None required — no `"outcome"` category usage in adaptation suite

### Integration Test Total Summary

| Suite | Passed | Failed | xFailed | New Tests |
|-------|--------|--------|---------|-----------|
| smoke | 20 | 0 | 0 | 0 |
| tools | 82 | 0 | 1 (pre-existing) | 7 |
| lifecycle | 29 | 0 | 1 (pre-existing) | 1 |
| edge_cases | 23 | 0 | 1 (pre-existing) | 0 (updated 1) |
| adaptation | 9 | 0 | 1 (pre-existing) | 0 |
| **Total** | **163** | **0** | **4** | **8** |

---

## Harness Client Update

`product/test/infra-001/harness/client.py` — `context_cycle()` method extended with `phase`, `outcome`, `next_phase` parameters to match the updated `CycleParams` wire schema (crt-025). The `keywords` parameter was retained for backward compatibility (silently discarded by server per AC-01).

---

## Gaps

No uncovered risks.

The concurrent-session seq advisory behavior (R-07 medium-priority, concurrent aspect) is not exercised through the MCP interface — this is documented as intentional (OVERVIEW.md: "Tests NOT needed in infra-001 — seq monotonicity under concurrent sessions: internal storage detail, not MCP-visible"). Sequential seq behavior is verified via the store-layer and migration round-trip tests.

CYCLE_EVENTS write path via the MCP `context_cycle` tool: the tool itself does NOT write to `cycle_events` directly — it only acknowledges and routes through the UDS hook path. CYCLE_EVENTS writes only happen via `uds/listener.rs`. The infra-001 harness does not have an active UDS connection, so `context_cycle` calls in integration tests cannot produce CYCLE_EVENTS rows. Integration tests that verify `phase_narrative` presence use direct SQL seeding of `cycle_events` (documented in test docstrings with reference to ADR-003 and the UDS-only write path). This is a valid test approach since the unit tests and migration tests already verify the insert path end-to-end.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_cycle_params_deserialize_phase_end`, `test_cycle_params_deserialize_phase_end_with_outcome` — `keywords` silently discarded; no `deny_unknown_fields` |
| AC-02 | PASS | `test_validate_cycle_params_type_phase_end_accepted`, `test_validate_cycle_params_type_start`, `test_validate_cycle_params_type_stop`, `test_validate_cycle_params_type_invalid_pause`; infra-001: `test_cycle_phase_end_type_accepted`, `test_cycle_invalid_type_rejected` |
| AC-03 | PASS | `test_validate_phase_space_rejected`, `test_validate_phase_65_char_rejected`, `test_validate_phase_lowercase_normalization`; infra-001: `test_cycle_phase_with_space_rejected` |
| AC-04 | PASS | `test_v15_cycle_events_round_trip`, `test_v15_cycle_events_all_nullable_columns_null` — NULL phase on `cycle_start` with no phase parameter |
| AC-05 | PASS | `test_listener_cycle_start_with_next_phase_sets_session_phase` — verifies synchronous mutation before DB spawn |
| AC-06 | PASS | `test_listener_cycle_phase_end_with_next_phase_updates_phase`, `test_listener_cycle_phase_end_without_next_phase_no_change` |
| AC-07 | PASS | `test_listener_cycle_stop_clears_phase` |
| AC-08 | PASS | `test_v15_cycle_events_round_trip` (sequential insert via `insert_cycle_event`); infra-001: `test_cycle_phase_end_stores_row` (three calls succeed) |
| AC-09 | PASS | `test_record_feature_entries_with_phase_some`, `test_record_feature_entries_with_phase_none`, `test_analytics_drain_uses_enqueue_time_phase`, `test_analytics_drain_phase_some_persists_value` |
| AC-10 | PASS | `test_v14_to_v15_migration_adds_cycle_events_table`, `test_v14_to_v15_migration_adds_phase_column_to_feature_entries`, `test_schema_version_is_15_after_migration`, `test_v14_to_v15_migration_idempotent` |
| AC-11 | PASS | `test_fresh_db_creates_schema_v15`, `test_fresh_db_cycle_events_table_schema` |
| AC-12 | PASS | `test_retrospective_report_phase_narrative_some_serialized`, `test_phase_narrative_types_defined`; infra-001: `test_cycle_review_includes_phase_narrative` (SQL-seeded CYCLE_EVENTS), `test_phase_tag_store_cycle_review_flow` |
| AC-13 | PASS | `test_retrospective_report_phase_narrative_none_omitted`, `test_retrospective_report_phase_narrative_backward_compat`; infra-001: `test_cycle_review_no_phase_narrative_for_old_feature` |
| AC-14 | PASS | `test_cross_cycle_comparison_none_when_zero_prior_features`, `test_cross_cycle_comparison_none_when_one_prior_feature`, `test_cross_cycle_comparison_some_when_two_prior_features`, `test_cross_cycle_excludes_current_feature_data` |
| AC-15 | PASS | `test_category_allowlist_has_seven_categories`, `test_outcome_category_is_not_in_allowlist`, `test_outcome_category_validate_err`; infra-001: `test_cycle_outcome_category_rejected` |
| AC-16 | PASS | `test_hook_phase_end_invalid_phase_space_falls_through`, `test_hook_phase_end_empty_phase_falls_through`, `test_hook_phase_end_valid_phase_emits_cycle_phase_end` |
| AC-17 | PASS | `test_v15_cycle_events_round_trip` — all three event types (`cycle_start`, `cycle_phase_end`, `cycle_stop`) insertable; `test_v15_cycle_events_all_nullable_columns_null` |

---

## GH Issues Filed for Pre-Existing Failures

None required — all xfail markers in the executed suites pre-date crt-025 and carry existing GH Issue references (GH#305, GH#111). No new xfail markers were added.

---

## Harness Changes Summary

| File | Change | Reason |
|------|--------|--------|
| `suites/test_tools.py` | Added 7 new crt-025 tests; added `_seed_cycle_events_sql` helper | New MCP behavior: `phase-end` type, phase rejection, `outcome` retirement, `phase_narrative` |
| `suites/test_lifecycle.py` | Added `test_phase_tag_store_cycle_review_flow` with SQL seeding helpers | Phase-tag lifecycle end-to-end flow (AC-12) |
| `suites/test_edge_cases.py` | Updated `test_concurrent_store_operations`: replaced `"outcome"` with `"procedure"` | Category retirement (R-03, ADR-005) — crt-025-caused change |
| `harness/client.py` | Extended `context_cycle()` with `phase`, `outcome`, `next_phase` parameters | Match updated `CycleParams` wire schema |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures — found #487 (workspace test procedure), #750 (pipeline validation tests), #553 (worktree validation), #729 (intelligence pipeline testing pattern), #129 (concrete assertions convention). Pattern #729 confirmed that the analytics drain path (R-02) is best tested at the store-crate level.
- Queried: `/uni-query-patterns` — not available (search performed via `context_search` instead).
- Stored: entry via `/uni-store-pattern` — the CYCLE_EVENTS-via-UDS architectural constraint (CYCLE_EVENTS are only written through the hook path, not the MCP tool path) is a new pattern relevant to future test authors. Filing below.
