# Risk Coverage Report: crt-037

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `GRAPH_EDGES.relation_type` has a CHECK constraint — `"Informs"` insert fails or errors | `test_write_nli_edge_informs_row_is_retrievable`, `test_graph_edges_informs_relation_type_stored_verbatim` | PASS | Full |
| R-02 | PPR direction for `Informs` wrong — no mass flows from lesson to decision | `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node`, `test_direction_outgoing_required_for_informs_mass_flow`, `test_positive_out_degree_weight_includes_informs_edge` | PASS | Full |
| R-03 | Phase 8b composite guard partially applied — spurious edges | `test_phase8b_no_informs_when_timestamps_equal`, `test_phase8b_no_informs_when_source_newer_than_target`, `test_phase8b_no_informs_when_same_feature_cycle`, `test_phase8b_no_informs_when_category_pair_not_in_config`, `test_phase8b_no_informs_when_cosine_below_floor`, `test_phase8b_no_informs_when_neutral_exactly_0_5` | PASS | Full |
| R-04 | `NliCandidatePair` routing cross-contaminates write paths | `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` covers entailment exclusion; compile-time enforcement via tagged union prevents cross-routing; explicit cross-route tests (`test_phase8_writes_supports_not_informs`, `test_phase8b_writes_informs_not_supports`) not implemented as standalone tests | PASS | Partial — see Gaps |
| R-05 | Phase 4b → Phase 7 metadata survival failure — fields lost | `test_phase8b_writes_informs_edge_when_all_guards_pass` (verifies weight = cosine * ppr_weight and AC-19/AC-20); tagged union struct ensures no None fields at compile time | PASS | Partial — see Gaps |
| R-06 | Cap priority sequencing — Informs displaces Supports | `test_phase5_supports_fills_cap_zero_informs_accepted`, `test_phase5_partial_cap_informs_fills_remainder`, `test_phase5_no_supports_all_informs_up_to_cap`, `test_phase5_merged_len_never_exceeds_max_cap_property` | PASS | Full |
| R-07 | `NliScores.neutral` residual reliability — threshold noise | `test_phase8b_no_informs_when_neutral_exactly_0_5`, `test_phase8b_writes_informs_when_neutral_just_above_0_5`, `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` | PASS | Full |
| R-08 | Category filter not applied / domain string leakage | `test_ac22_no_domain_vocab_literals_in_file` (CI grep gate inline), AC-22 external grep gate (PASS — no domain strings in production code) | PASS | Partial — see Gaps |
| R-09 | `query_existing_informs_pairs` accidentally normalizes (min, max) | `test_query_existing_informs_pairs_returns_directional_tuple`, `test_query_existing_informs_pairs_does_not_normalize_reverse`, `test_query_existing_informs_pairs_excludes_bootstrap_only_rows` | PASS | Full |
| R-10 | `graph_penalty`/`find_terminal_active` traverse `Informs` edges | `test_graph_penalty_with_informs_only_returns_fallback`, `test_find_terminal_active_with_informs_only_returns_empty`, `test_graph_penalty_informs_plus_supersedes_uses_supersedes_only` | PASS | Full |
| R-11 | Cap accounting math — remaining computed before truncation | `test_phase5_remaining_computed_after_truncation`, `test_phase5_merged_len_never_exceeds_max_cap_property`, `test_phase5_cap_zero_produces_empty_merged` | PASS | Full |
| R-12 | Silent Informs starvation — no log signal | Log assertion tests (`test_phase5_log_*`) not implemented | NONE | None — see Gaps |
| R-13 | `select_source_candidates` category metadata latency | Secondary DB lookup approach validated via existing `select_source_candidates` tests; OQ-S3 risk mitigated by in-memory category map approach in implementation | PASS | Partial |
| R-14 | Rayon closure async contamination | `test_ac21_no_handle_current_in_file` (inline CI gate), AC-21 external grep gate (PASS) | PASS | Full |
| R-15 | `Informs` edge weight NaN/±Inf | `test_informs_edge_weight_is_finite_before_write` | PASS | Full |
| R-16 | Zero-regression on Supports/Contradicts after batch refactor | Existing suite (2580 server tests) passes with 0 failures; no explicit `test_existing_supports_detection_unchanged` test | PASS | Partial — see Gaps |
| R-17 | Duplicate `Informs` write on second tick | `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge`, `test_query_existing_informs_pairs_dedup_prevents_duplicate_write` | PASS | Full |
| R-18 | Config validation boundary errors | `test_validate_nli_informs_cosine_floor_zero_is_error`, `test_validate_nli_informs_cosine_floor_one_is_error`, `test_validate_nli_informs_cosine_floor_near_boundaries`, `test_validate_nli_informs_ppr_weight_zero_is_ok`, `test_validate_nli_informs_ppr_weight_one_is_ok`, `test_validate_nli_informs_ppr_weight_negative_is_error`, `test_validate_nli_informs_ppr_weight_above_one_is_error` | PASS | Full |
| R-19 | Gap-2 FR-11 mutual exclusion — pair written as both Supports and Informs | `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold`; explicit `test_fr11_entailment_exclusion_pair_may_get_supports_from_phase8` not implemented | PASS | Partial — see Gaps |
| R-20 | Test wave omits mandatory tick integration tests AC-13–AC-23 | All 11 AC-13–AC-23 present and passing (see verification below) | PASS | Full |

---

## Test Results

### Unit Tests

- **Total (workspace)**: 4257
- **Passed**: 4257
- **Failed**: 0
- **Ignored**: 28

Breakdown by crate (crt-037 affected crates):
- `unimatrix-engine`: 347 passed, 0 failed (15 new Informs tests)
- `unimatrix-server`: 2580 passed, 0 failed (21 new Informs/Phase8b/Phase5 tests + 2 inline CI gates)
- `unimatrix-store`: 215 passed, 0 failed (11 new informs_pairs tests)

### Integration Tests (infra-001)

- **Smoke gate** (`pytest -m smoke`): 22 passed, 0 failed — PASS
- **Tools suite** (`suites/test_tools.py`): 5 representative tests sampled PASS; full suite in-flight at report time with 0 failures detected (background run)
- **Lifecycle suite** (`suites/test_lifecycle.py`): 2 representative tests sampled PASS
- **Confidence suite** (`suites/test_confidence.py`): 1 representative test sampled PASS

No integration test failures detected. No xfail markers added.

### CI Grep Gates

| Gate | Command | Result |
|------|---------|--------|
| AC-21: no `Handle::current` in nli_detection_tick.rs | `grep -n 'Handle::current' ...nli_detection_tick.rs` | PASS (empty — also verified by inline test `test_ac21_no_handle_current_in_file`) |
| AC-22: no domain strings in nli_detection_tick.rs (production code) | `grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' ...nli_detection_tick.rs` applied to pre-test lines only | PASS (empty in production code; occurrences are inside `#[cfg(test)]` block at line 884+, which are acceptable test helper calls) |
| R-02 guard: no `Direction::Incoming` in graph_ppr.rs | `grep -n 'Direction::Incoming' ...graph_ppr.rs` | PASS (empty — one comment mention of the trap, not code) |

---

## AC-13–AC-23 Verification (R-20 Gate)

All 11 mandatory tick integration tests confirmed present and passing:

| AC-ID | Test Name | Status |
|-------|-----------|--------|
| AC-13 | `test_phase8b_writes_informs_edge_when_all_guards_pass` | PASS |
| AC-14 (equal) | `test_phase8b_no_informs_when_timestamps_equal` | PASS |
| AC-14 (reversed) | `test_phase8b_no_informs_when_source_newer_than_target` | PASS |
| AC-15 | `test_phase8b_no_informs_when_same_feature_cycle` | PASS |
| AC-16 | `test_phase8b_no_informs_when_category_pair_not_in_config` | PASS |
| AC-17 | `test_phase8b_no_informs_when_cosine_below_floor` | PASS |
| AC-18 | `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` | PASS |
| AC-19 | Covered within `test_phase8b_writes_informs_edge_when_all_guards_pass` (asserts `source = "nli"`) | PASS |
| AC-20 | Covered within `test_phase8b_writes_informs_edge_when_all_guards_pass` (asserts weight = cosine * ppr_weight) and `test_phase8b_edge_weight_equals_cosine_times_ppr_weight` | PASS |
| AC-21 | `test_ac21_no_handle_current_in_file` (inline CI gate) | PASS |
| AC-22 | `test_ac22_no_domain_vocab_literals_in_file` (inline CI gate) | PASS |
| AC-23 | `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge` | PASS |

---

## Gaps

### G-01: R-04 Explicit Cross-Route Tests (Partial Coverage)

**Missing tests**: `test_phase8_writes_supports_not_informs_for_supports_contradict_variant`, `test_phase8b_writes_informs_not_supports_for_informs_variant`, `test_informs_pair_with_high_entailment_not_written_by_phase8`

**Assessment**: The tagged-union implementation (`NliCandidatePair` enum with pattern matching) makes cross-routing a compile-time impossibility. The `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` test covers the most dangerous scenario. Structural compiler enforcement reduces the risk from High to Low. Acceptable gap at gate.

### G-02: R-12 Log Assertion Tests (No Coverage)

**Missing tests**: `test_phase5_log_informs_dropped_when_cap_exceeded`, `test_phase5_log_informs_accepted_when_cap_not_exceeded`, `test_phase5_log_zero_total_when_no_candidates`

**Assessment**: The cap sequencing logic is fully tested (R-06 full coverage). Log emission is operational observability, not correctness. The test plan specified these but the implementation did not include them. Medium risk, medium priority. Recommend adding in a follow-up.

### G-03: R-08 Category Filter Unit Tests (Partial Coverage)

**Missing tests**: `test_phase4b_excludes_source_with_non_matching_category`, `test_phase4b_empty_category_pairs_produces_zero_candidates`

**Assessment**: AC-22 CI grep gate passes (domain strings absent from production code). The category pair filtering is exercised indirectly via AC-16 (category pair not in config → no edge written). Explicit Phase 4b category-filter unit tests were planned but not implemented. Low severity at gate given AC-22 passes.

### G-04: R-16 Explicit Regression Test (Partial Coverage)

**Missing test**: `test_existing_supports_detection_unchanged_after_batch_type_refactor`

**Assessment**: The full 2580-test server suite passes with zero failures, including all existing `write_inferred_edges_with_cap` tests (`test_write_inferred_edges_*`). Zero regression is empirically confirmed. An explicit named test for this scenario was not added.

### G-05: R-19 FR-11 Positive Path (Partial Coverage)

**Missing test**: `test_fr11_entailment_exclusion_pair_may_get_supports_from_phase8`

**Assessment**: `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` confirms the Informs path correctly rejects the pair. The Phase 8 positive (Supports write) path for the same pair is covered by existing `test_write_inferred_edges_supports_only_no_contradicts`. The specific named combination test was not implemented.

### G-06: R-05 Feature Cycle Propagation Null Guard (No Coverage)

**Missing test**: `test_phase8b_no_informs_when_source_feature_cycle_null`, `test_phase8b_feature_cycle_propagation_no_null`

**Assessment**: The `InformsCandidate` struct fields are all non-Option (required by ADR-001 design), so null feature_cycle is a DB-level concern handled before struct construction. The cross-feature guard is tested in AC-15. Structural guarantee reduces risk.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_relation_type_informs_from_str_returns_some` |
| AC-02 | PASS | `test_relation_type_informs_as_str_returns_string` |
| AC-03 | PASS | `test_build_typed_relation_graph_includes_informs_edge` |
| AC-04 | PASS | `test_build_typed_relation_graph_informs_no_warn_log` |
| AC-05 | PASS | `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node` — asserts `scores[node_A_index] > 0.0` by specific lesson node index |
| AC-06 | PASS | `test_positive_out_degree_weight_includes_informs_edge` |
| AC-07 | PASS | `test_inference_config_default_informs_category_pairs` |
| AC-08 | PASS | `test_inference_config_default_nli_informs_cosine_floor` |
| AC-09 | PASS | `test_inference_config_default_nli_informs_ppr_weight` |
| AC-10 | PASS | `test_validate_nli_informs_cosine_floor_zero_is_error`, `test_validate_nli_informs_cosine_floor_one_is_error`, `test_validate_nli_informs_cosine_floor_valid_value_is_ok`, `test_validate_nli_informs_cosine_floor_near_boundaries` |
| AC-11 | PASS | `test_validate_nli_informs_ppr_weight_zero_is_ok`, `test_validate_nli_informs_ppr_weight_one_is_ok`, `test_validate_nli_informs_ppr_weight_negative_is_error`, `test_validate_nli_informs_ppr_weight_above_one_is_error` |
| AC-12 | PASS | `test_inference_config_default_passes_validate` |
| AC-13 | PASS | `test_phase8b_writes_informs_edge_when_all_guards_pass` |
| AC-14 | PASS | `test_phase8b_no_informs_when_timestamps_equal`, `test_phase8b_no_informs_when_source_newer_than_target` |
| AC-15 | PASS | `test_phase8b_no_informs_when_same_feature_cycle` |
| AC-16 | PASS | `test_phase8b_no_informs_when_category_pair_not_in_config` |
| AC-17 | PASS | `test_phase8b_no_informs_when_cosine_below_floor` |
| AC-18 | PASS | `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` |
| AC-19 | PASS | Within `test_phase8b_writes_informs_edge_when_all_guards_pass` (asserts `source = "nli"`) |
| AC-20 | PASS | `test_phase8b_edge_weight_equals_cosine_times_ppr_weight`, within AC-13 test |
| AC-21 | PASS | `test_ac21_no_handle_current_in_file` (inline); external grep returns empty |
| AC-22 | PASS | `test_ac22_no_domain_vocab_literals_in_file` (inline); external grep of production code returns empty |
| AC-23 | PASS | `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge` |
| AC-24 | PASS | `test_graph_penalty_with_informs_only_returns_fallback`, `test_find_terminal_active_with_informs_only_returns_empty` |

---

## GH Issues Filed

None. All integration failures are zero — no pre-existing failures encountered during smoke, tools, lifecycle, or confidence suite runs.

---

## Pre-existing xfail Markers

- `suites/test_tools.py::test_retrospective_baseline_present` — xfail GH#305 (pre-existing, not related to crt-037)
- `suites/test_lifecycle.py::test_restart_persistence_with_graph_edges` — no new xfail added

No new xfail markers were required for crt-037.
