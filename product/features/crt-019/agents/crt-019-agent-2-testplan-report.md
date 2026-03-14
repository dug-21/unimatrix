# Agent Report: crt-019-agent-2-testplan
## Phase: Test Plan Design (Stage 3a)
## Status: COMPLETE

---

## Deliverables

All 8 test plan files produced:

| File | Status |
|------|--------|
| `product/features/crt-019/test-plan/OVERVIEW.md` | Complete |
| `product/features/crt-019/test-plan/confidence-formula-engine.md` | Complete |
| `product/features/crt-019/test-plan/confidence-state.md` | Complete |
| `product/features/crt-019/test-plan/empirical-prior-computation.md` | Complete |
| `product/features/crt-019/test-plan/confidence-refresh-batch.md` | Complete |
| `product/features/crt-019/test-plan/deliberate-retrieval-signal.md` | Complete |
| `product/features/crt-019/test-plan/query-skills.md` | Complete |
| `product/features/crt-019/test-plan/test-infrastructure.md` | Complete |

---

## Risk Coverage Mapping

| Risk ID | Priority | Test Location | Coverage |
|---------|----------|---------------|---------|
| R-01 | P1/Critical | `test_lifecycle.py::test_empirical_prior_flows_to_stored_confidence` + `services/usage.rs` unit | Full — integration test required (documented in both OVERVIEW and empirical-prior-computation) |
| R-02 | P2/High | `confidence.rs` unit `rerank_score_adaptive_differs_from_fixed`, `pipeline_retrieval.rs` T-RET-01 | Full |
| R-03 | P2/High | `confidence.rs` `weight_sum_invariant_f64` + `weight_constants_values` | Full |
| R-04 | P2/High | Implementation ordering guard documented in OVERVIEW.md and test-infrastructure.md | Process guard — flagged as first-commit requirement |
| R-05 | P2/High | `test_empirical_prior_below_threshold_returns_cold_start` (9 entries), `test_empirical_prior_at_threshold_uses_population` (10 entries) | Full — boundary both sides |
| R-06 | P2/High | `test_confidence_state_initial_observed_spread`, `test_confidence_state_initial_weight` | Full |
| R-07 | P2/High | `test_context_lookup_doubled_access_second_call_same_agent_zero` | Full — scenario 2 is the dedup-before-multiply test |
| R-08 | P3/High | Code review + `test_record_access_fire_and_forget_returns_quickly` existing test | Process + behavioral |
| R-09 | P4/Med | Code review — RwLock poison recovery pattern | Code review |
| R-10 | P2/High | `auto_proposed_base_score_unchanged` unit test | Full |
| R-11 | P2/High | `test_store_record_usage_duplicate_ids_increments_twice`, `test_tools.py::test_context_lookup_doubled_access_count` | Full — store-layer + integration |
| R-12 | P2/High | `test_prior_zero_variance_all_helpful_clamped`, `test_prior_zero_variance_all_unhelpful_clamped`, `bayesian_helpfulness_nan_inputs_clamped` | Full |
| R-13 | P3/Med | Code review + loop guard placement assertion | Process |
| R-14 | P3/Med | `bayesian_helpfulness_balanced_votes_exact_half` (`== 0.5` not `> 0.5`) | Full |
| R-15 | P3/Med | Integration IR-01 test (ServiceLayer wiring visible via adaptive weight behavior) | Integration |
| R-16 | P3/Med | `test_cal_spread_synthetic_population` (T-CAL-SPREAD-01) | Full |
| R-17 | P4/Low | Grep verification: removed constants absent from codebase | Verification command documented |

---

## Integration Suite Plan

| Suite | Reason | Tests Added |
|-------|--------|-------------|
| `smoke` | Mandatory gate | None new — existing smoke tests must pass |
| `confidence` | Formula changes | `test_search_uses_adaptive_confidence_weight` |
| `tools` | context_get vote injection, context_lookup doubled access | `test_context_get_implicit_helpful_vote`, `test_context_lookup_doubled_access_count` |
| `lifecycle` | R-01 empirical prior end-to-end | `test_empirical_prior_flows_to_stored_confidence` |
| `edge_cases` | Empty population for spread computation | No new test needed (EC-01 covered by unit test) |

Suites NOT required beyond smoke: `protocol`, `security`, `contradiction`, `volume`.

---

## AC Verification Mapping

| AC-ID | Test Plan Coverage |
|-------|-------------------|
| AC-01 | T-CAL-SPREAD-01 in `pipeline_calibration.rs` + manual DB export |
| AC-02 | `bayesian_helpfulness_*` unit tests in `confidence.rs` (4 exact assertions) |
| AC-03 | `weight_sum_invariant_f64` (exact f64 equality) |
| AC-04 | T-ABL-01..06 ablation tests (updated ablation_pair for new signatures) |
| AC-05 | `base_score_active_auto`, `base_score_active_agent`, `auto_proposed_base_score_unchanged` |
| AC-06 | `adaptive_confidence_weight_*` unit tests (6 assertions), integration `test_search_uses_adaptive_confidence_weight` |
| AC-07 | `test_max_confidence_refresh_batch_is_500` + code review (grep + guard placement) |
| AC-08a | `test_context_get_implicit_helpful_vote` unit + integration |
| AC-08b | `test_context_lookup_doubled_access_*` (3 scenarios) unit + integration |
| AC-09 | Manual review checklist in `query-skills.md` |
| AC-10 | `pipeline_regression.rs` T-REG-01 + T-REG-02 + `pipeline_calibration.rs` T-ABL + T-CAL-04 |
| AC-11 | `weight_sum_invariant_f64` with `assert_eq!` (not tolerance) |
| AC-12 | `auto_vs_agent_spread` test in `pipeline_calibration.rs` (3 signal levels) |

---

## Open Questions

1. **R-11 is a go/no-go gate**: The `test_store_record_usage_duplicate_ids_increments_twice`
   test must pass before the `flat_map` repeat approach is committed. If the store
   deduplicates IDs, the implementing agent must document the fallback strategy
   (explicit `(id, increment)` pairs) and update the `deliberate-retrieval-signal`
   implementation accordingly. This affects Stage 3b ordering — the store verification
   test should be the second implementation step (per Implementation Brief ordering).

2. **Integration test timing**: `test_context_get_implicit_helpful_vote` and
   `test_context_lookup_doubled_access_count` rely on `spawn_blocking` completion.
   The test may need a small sleep (0.1s) or a polling loop to read back updated values.
   The harness's fixture teardown handles cleanup; the timing of async completion
   may require adjustment per the actual harness patterns in `test_tools.py`.

3. **T-CAL-SPREAD-01 signal population**: The synthetic 50-entry population in
   `test_cal_spread_synthetic_population` is designed to produce spread >= 0.20.
   The actual spread depends on the formula output for the chosen entry profiles.
   If the spread falls below 0.20 after implementation, the population must be adjusted
   to more extreme values (or the issue escalates to an AC-01 violation requiring
   formula review).

4. **`compute_empirical_prior` function visibility**: The test plan assumes this is a
   standalone function with a testable signature
   `fn compute_empirical_prior(voted_entries: &[(u32, u32)]) -> (f64, f64)`. If the
   implementation inlines this logic directly into `run_maintenance`, the unit tests
   become harder to write. The implementing agent should expose this as a
   `pub(crate)` function for testability.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures — server unavailable; proceeded
  without Unimatrix results per non-blocking guidance.
- Stored: nothing novel to store — the test patterns used here (boundary tests, NaN
  propagation tests, dedup-before-multiply scenario structure) are standard Rust testing
  conventions. The R-11 blocking prerequisite pattern (test store behavior before committing
  implementation approach) is feature-specific risk mitigation, not a reusable procedure.
  No new fixture patterns were invented; no test helper utilities beyond what already exists
  in the pipeline test infrastructure.
