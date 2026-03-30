# Risk Coverage Report: crt-034

Recurring co_access → GRAPH_EDGES Promotion Tick

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 (Critical) | Silent absorption of write failures — no caller-visible signal | `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` | PASS | Full |
| R-02 (High) | Division by zero when max_count=0 / empty table | `test_empty_co_access_table_noop_late_tick`, `test_all_below_threshold_noop_late_tick` | PASS | Full |
| R-03 (High) | Scalar subquery MAX(count) correctness | `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` | PASS | Full |
| R-04 (High) | INSERT OR IGNORE no-op detection via rows_affected | `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_double_tick_idempotent` | PASS | Full |
| R-05 (High) | Tick ordering violation in background.rs | Code review (AC-05) — see evidence below | PASS | Full |
| R-06 (High) | SR-05 early-tick warn! window re-opens on server restart | `test_early_tick_warn_when_qualifying_count_zero`, `test_late_tick_no_warn_empty_table`, `test_fully_promoted_table_no_warn` | PASS | Full |
| R-07 (High) | Config field absent from merge_configs() | `test_merge_configs_project_overrides_global_co_access_cap`, `test_merge_configs_global_only_co_access_cap` | PASS | Full |
| R-08 (Med) | CO_ACCESS_GRAPH_MIN_COUNT diverges from migration constant | `test_co_access_graph_min_count_value`, `test_co_access_constants_colocated_with_nli` | PASS | Full |
| R-09 (Med) | Near-threshold pair re-evaluation overhead / spurious writes | `test_double_tick_idempotent`, `test_sub_threshold_pair_not_gc` | PASS | Full |
| R-10 (Med) | One-directional edge contract violated | `test_inserted_edge_is_one_directional`, `test_basic_promotion_new_qualifying_pair` | PASS | Full |
| R-11 (High) | ORDER BY count DESC omitted from batch query | `test_cap_selects_highest_count_pairs` | PASS | Full |
| R-12 (Low) | File size limit: co_access_promotion_tick.rs exceeds 500 lines | `wc -l` gate check: **288 lines** | PASS | Full |
| R-13 (High) | Inserted edges missing required metadata fields | `test_inserted_edge_metadata_all_four_fields` | PASS | Full |

---

## Test Results

### Unit Tests

- **Total workspace:** 4141 passed, 0 failed, 28 ignored
- **crt-034 specific:** 33 passed, 0 failed
- **Run:** `cargo test --workspace 2>&1 | tail -30`

#### crt-034 Test Breakdown by Component

| Component | Tests | Result |
|-----------|-------|--------|
| `services::co_access_promotion_tick` | 23 | PASS |
| `infra::config` (co_access fields) | 6 | PASS |
| `background` (PROMOTION_EARLY_RUN_WARN_TICKS) | 1 | PASS |
| `read` (store constants) | 3 | PASS |
| **Total** | **33** | **PASS** |

The 28 ignored tests require ONNX NLI models on disk — unrelated to crt-034 and pre-existing.

### Integration Tests

#### Smoke Gate (mandatory)

- **Total:** 22 passed, 0 failed, 232 deselected
- **Run:** `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`
- **Duration:** 191.75s
- **Result:** PASS — gate cleared

#### Lifecycle Suite

- **Total:** 41 passed, 2 xfailed, 1 xpassed, 0 failed
- **Run:** `cd product/test/infra-001 && python -m pytest suites/test_lifecycle.py -v --timeout=60`
- **Duration:** 394.95s (6:34)
- **Result:** PASS — no crt-034-caused failures

##### Lifecycle Suite Details

| Test | Result | Notes |
|------|--------|-------|
| `test_store_search_find_flow` | PASS | |
| `test_correction_chain_integrity` | PASS | |
| `test_isolation_no_state_leakage` | PASS | |
| `test_concurrent_search_stability` | PASS | |
| `test_briefing_reflects_stored_knowledge` | PASS | |
| `test_status_reflects_lifecycle_changes` | PASS | |
| `test_deprecate_then_correct_errors` | PASS | |
| `test_multi_step_correction_chain` | PASS | |
| `test_full_pipeline_10_entries` | PASS | |
| `test_effectiveness_search_ordering_after_cold_start` | PASS | |
| `test_briefing_effectiveness_tiebreaker` | PASS | |
| `test_context_status_does_not_advance_consecutive_counters` | PASS | |
| `test_auto_quarantine_disabled_when_env_zero` | PASS | |
| `test_auto_quarantine_after_consecutive_bad_ticks` | XFAIL | Pre-existing: GH#406 (tick interval env var needed) |
| `test_empirical_prior_flows_to_stored_confidence` | PASS | |
| `test_search_multihop_injects_terminal_active` | XPASS | Pre-existing xfail GH#406 — test now passes; not caused by crt-034 |
| `test_search_deprecated_entry_visible_with_topology_penalty` | PASS | |
| `test_concurrent_search_stability` | PASS | |
| `test_search_nli_absent_returns_cosine_results` | PASS | |
| `test_post_store_nli_edge_written` | PASS | |
| `test_search_coac_signal_reaches_scorer` | PASS | |
| `test_bootstrap_promotion_restart_noop` | PASS | |
| `test_phase_tag_store_cycle_review_flow` | PASS | |
| `test_session_histogram_boosts_category_match` | PASS | |
| `test_cold_start_session_search_no_regression` | PASS | |
| `test_duplicate_store_histogram_no_inflation` | PASS | |
| `test_briefing_flat_index_format_no_section_headers` | PASS | |
| `test_briefing_session_id_applies_wa2_boost` | PASS | |
| `test_dead_knowledge_entries_deprecated_by_tick` | XFAIL | Pre-existing: tick interval test limitation |
| `test_cycle_start_with_goal_persists_across_restart` | PASS | |
| `test_cycle_goal_drives_briefing_query` | PASS | |
| `test_cycle_review_knowledge_reuse_cross_feature_split` | PASS | |
| `test_briefing_then_get_does_not_consume_dedup_slot` | PASS | |
| `test_context_search_writes_query_log_row` | PASS | |
| `test_search_cold_start_phase_score_identity` | PASS | |
| `test_search_current_phase_none_succeeds` | PASS | |
| `test_cycle_review_persists_across_restart` | PASS | |

##### XPASS Note: test_search_multihop_injects_terminal_active

This test is marked `@pytest.mark.xfail(reason="Pre-existing: GH#406 ...")` and produced an XPASS result. crt-034 does not touch the multi-hop search injection path. This XPASS is pre-existing and unrelated to this feature. Because `xfail_strict` is not set in pytest.ini, XPASS is a warning not a failure — the suite result is still a pass. The xfail marker and GH#406 should be reviewed separately.

---

## AC-05 Code Review Evidence

**Requirement:** `run_co_access_promotion_tick` call must appear AFTER orphaned-edge compaction and BEFORE `TypedGraphState::rebuild()`, with ORDERING INVARIANT anchor comment.

**Evidence from `crates/unimatrix-server/src/background.rs` lines 550–556:**

```rust
// ── ORDERING INVARIANT (crt-034, ADR-005) ─────────────────────────────────────
// co_access promotion MUST run:
//   AFTER  step 2 (orphaned-edge compaction) — so dangling entries are removed first
//   BEFORE step 3 (TypedGraphState::rebuild) — so PPR sees promoted edges this tick
// Do NOT insert new tick steps between here and TypedGraphState::rebuild() below.
// ─────────────────────────────────────────────────────────────────────────────
run_co_access_promotion_tick(store, inference_config, current_tick).await;
```

Line 570 (immediately after): `tokio::spawn(async move { TypedGraphState::rebuild(&store_clone).await })`

**Additional structural checks:**
- No `nli_enabled` or other conditional guard wraps the call (unconditional as required by FR-07)
- `PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5` constant defined in `background.rs` (not in tick module)
- `services/mod.rs` line 28: `pub(crate) mod co_access_promotion_tick;` — registered

AC-05 is **VERIFIED**.

---

## File Size Gate (R-12)

```
288 /workspaces/unimatrix/crates/unimatrix-server/src/services/co_access_promotion_tick.rs
```

288 lines — well under the 500-line limit. PASS.

---

## Gaps

None. All 13 risks from RISK-TEST-STRATEGY.md have test coverage:

- R-01 (Critical): 2 tests covering write-failure paths
- R-02 through R-13: all covered by named test functions matching the test plan

Integration risks I-01 through I-04 are deployment/ordering concerns verified by code review and the lifecycle suite regression run. No integration test failures were found.

Edge cases E-01 through E-06 are covered by named unit tests in `co_access_promotion_tick.rs`:
- E-01: `test_single_qualifying_pair_weight_one`
- E-02: `test_tied_counts_secondary_sort_stable`
- E-03: `test_cap_equals_qualifying_count`
- E-04: `test_cap_one_selects_highest_count`
- E-05: `test_weight_delta_exactly_at_boundary_no_update`
- E-06: `test_self_loop_pair_no_panic`

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_basic_promotion_new_qualifying_pair` — row inserted with `relation_type = 'CoAccess'` |
| AC-02 | PASS | `test_existing_edge_stale_weight_updated` — weight updated when delta > 0.1 |
| AC-03 | PASS | `test_existing_edge_current_weight_no_update` — no update when delta <= 0.1 |
| AC-04 | PASS | `test_cap_selects_highest_count_pairs` — only top-3 by count promoted; pairs with count=3 absent |
| AC-05 | PASS | Code review of `background.rs` — ORDERING INVARIANT comment at lines 550–555, call at line 556, `TypedGraphState::rebuild()` at line 570 |
| AC-06 | PASS | `test_max_co_access_promotion_per_tick_default` (200), `test_max_co_access_promotion_per_tick_validation_zero` (error), `test_max_co_access_promotion_per_tick_validation_over_limit` (error), `test_merge_configs_project_overrides_global_co_access_cap` (50 wins) |
| AC-07 | PASS | `test_co_access_graph_min_count_value` — `CO_ACCESS_GRAPH_MIN_COUNT == 3i64`, accessible at crate root |
| AC-08 | PASS | `test_edge_source_co_access_value` — `EDGE_SOURCE_CO_ACCESS == "co_access"`, accessible at crate root |
| AC-09 | PASS | `test_empty_co_access_table_noop_late_tick` (no panic, no warn at tick>=5), `test_early_tick_warn_when_qualifying_count_zero` (warn emitted at tick<5), `test_all_below_threshold_noop_late_tick` (sub-threshold no-op) |
| AC-10 | PASS | `test_max_co_access_promotion_per_tick_validation_zero` — error message contains "max_co_access_promotion_per_tick" |
| AC-11 | PASS | `test_write_failure_mid_batch_warn_and_continue` — returns (), warn emitted, remaining pairs attempted |
| AC-12 | PASS | `test_inserted_edge_metadata_all_four_fields` — `bootstrap_only=0`, `source="co_access"`, `created_by="tick"`, `relation_type="CoAccess"` all verified |
| AC-13 | PASS | `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` — normalization anchor is global MAX over all qualifying pairs |
| AC-14 | PASS | `test_double_tick_idempotent` — exactly 1 row after 2 ticks, weight unchanged |
| AC-15 | PASS | `test_sub_threshold_pair_not_gc` — GRAPH_EDGES row persists after count drops below threshold |

---

## Integration Risks Triage

No integration tests failed due to crt-034. No GH Issues filed for this feature.

The pre-existing XPASS (`test_search_multihop_injects_terminal_active` / GH#406) is outside crt-034 scope and requires no action from this feature's PR.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3822, #3826, #3821 (background tick patterns, co_access promotion ADR), #2800 (testing lesson), #3621 (SQL bugs caught late). Applied to verify test coverage against known patterns.
- Stored: nothing novel to store — the crt-034 test suite follows established patterns from `nli_detection_tick.rs` and the infra-001 USAGE-PROTOCOL. No new fixture patterns or test infrastructure was invented; all tests used the existing in-process SQLite fixture approach.
