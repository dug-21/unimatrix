# Agent Report: col-031-agent-2-testplan
# Phase: Test Plan Design (Stage 3a)

## Output Files

| File | Path |
|------|------|
| OVERVIEW.md | `/workspaces/unimatrix/product/features/col-031/test-plan/OVERVIEW.md` |
| phase_freq_table.md | `/workspaces/unimatrix/product/features/col-031/test-plan/phase_freq_table.md` |
| query_log_store_method.md | `/workspaces/unimatrix/product/features/col-031/test-plan/query_log_store_method.md` |
| search_scoring.md | `/workspaces/unimatrix/product/features/col-031/test-plan/search_scoring.md` |
| background_tick.md | `/workspaces/unimatrix/product/features/col-031/test-plan/background_tick.md` |
| service_layer.md | `/workspaces/unimatrix/product/features/col-031/test-plan/service_layer.md` |
| inference_config.md | `/workspaces/unimatrix/product/features/col-031/test-plan/inference_config.md` |
| replay_fix.md | `/workspaces/unimatrix/product/features/col-031/test-plan/replay_fix.md` |

---

## Risk Coverage Mapping (Risk ID → Named Test)

| Risk ID | Priority | Named Test(s) | Component File |
|---------|----------|---------------|----------------|
| R-01 | Critical | `test_run_single_tick_propagates_phase_freq_handle`; `cargo build --workspace`; grep audit of 7 sites | `background_tick.md`, `service_layer.md` |
| R-02 | Critical | `test_replay_forwards_current_phase_to_service_search_params`; eval output inspection | `replay_fix.md` |
| R-03 | High | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`; `test_scoring_score_identity_cold_start` | `search_scoring.md` |
| R-04 | High | `test_phase_affinity_score_use_fallback_returns_one`; `test_phase_affinity_score_absent_phase_returns_one`; `test_phase_affinity_score_absent_entry_returns_one` | `phase_freq_table.md` |
| R-05 | High | `test_query_phase_freq_table_returns_correct_entry_id` (TestDb) | `query_log_store_method.md` |
| R-06 | High | `test_scoring_lock_released_before_scoring_loop`; code review | `search_scoring.md` |
| R-07 | High | `test_phase_affinity_score_single_entry_bucket_returns_one`; `test_rebuild_normalization_three_entry_bucket_exact_scores`; `test_rebuild_normalization_last_entry_in_five_bucket` | `phase_freq_table.md` |
| R-08 | Medium | `test_validate_lookback_days_zero_is_error`; `test_validate_lookback_days_3651_is_error`; `test_validate_lookback_days_boundary_values_pass` | `inference_config.md` |
| R-09 | Medium | `test_run_single_tick_retains_state_on_rebuild_error` | `background_tick.md` |
| R-10 | Medium | `test_phase_affinity_score_unknown_phase_returns_one` | `phase_freq_table.md` |
| R-11 | Medium | AC-12 eval gate run with AC-16; sensitivity comparison w=0.0 vs w=0.05 | `replay_fix.md` |
| R-12 | Low | Lock-order code comment at tick site; grep order audit | `background_tick.md` |
| R-13 | Medium | `test_query_phase_freq_table_returns_correct_entry_id` (asserts `freq == 10i64`) | `query_log_store_method.md` |
| R-14 | High | `cargo build --workspace`; grep audit of all 7 ADR-005 sites | `service_layer.md`, `background_tick.md` |

All 14 risks from RISK-TEST-STRATEGY.md have at least one named test or verified code-review check.

---

## Integration Harness Plan Summary

**Applicable suites**: `smoke` (mandatory), `tools`, `lifecycle`, `edge_cases`.

**Suites not applicable**: `confidence`, `contradiction`, `security`, `protocol`, `volume`, `adaptation` — col-031 does not touch these surfaces.

**New integration tests planned** (for Stage 3c addition to `suites/test_lifecycle.py`):

1. `test_search_cold_start_phase_score_identity` — fresh server (cold-start), verify `current_phase="delivery"` produces same results as `current_phase=None`. MCP-accessible, no tick needed. Uses `server` fixture.

2. `test_search_phase_affinity_influences_ranking` — after tick, verify phase-biased ranking is observable for a seeded entry. Requires `shared_server` fixture and either DB seeding or tick triggering. Assess feasibility at Stage 3c; may downgrade to unit-level if tick cannot be triggered synchronously.

**AC-08 note**: `query_phase_freq_table` SQL correctness is internal — no new infra-001 test needed. Covered at store-unit level via `TestDb`.

---

## Key Design Decisions Made During Planning

1. **R-03 test approach**: The `use_fallback` guard test (`test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`) does not require a spy/mock. Asserting `phase_explicit_norm == 0.0` when `current_phase = Some` + `use_fallback = true` is sufficient — if the guard fires too late, `phase_affinity_score` returns `1.0` and leaks `0.05 * 1.0 = 0.05` contribution, which the assertion would catch.

2. **AC-14 test approach**: The normalization test (`test_rebuild_normalization_three_entry_bucket_exact_scores`) is framed to work either via `rebuild` with a mock/TestDb store or via direct construction of `Vec<PhaseFreqRow>` if an internal normalization helper is exposed. Stage 3b should expose a testable normalization function.

3. **R-09 test feasibility note**: Injecting a store error into `run_single_tick` requires a trait mock or deliberate TestDb corruption. If `unimatrix_core::Store` is not object-safe for mocking, this test may need to test the error path via code inspection of `run_single_tick` with a fallback doc-test approach. Flagged for Stage 3b implementer.

---

## Open Questions for Stage 3b

1. **Normalization helper exposure**: Will `PhaseFreqTable` expose an internal `normalize_bucket(rows: Vec<PhaseFreqRow>) -> Vec<(u64, f32)>` helper for unit testing, or does AC-14 test via the full `rebuild()` path with a TestDb?

2. **Store mock feasibility**: Is `unimatrix_core::Store` object-safe enough to construct a failing mock for the R-09 error-retention test? If not, the test must use TestDb with a deliberately empty/null `query_log` and verify the handle was not reset from a pre-populated state.

3. **Tick triggering in infra-001**: Does the MCP server expose any tick-trigger mechanism (e.g., `context_status` with `maintain=true`)? If yes, `test_search_phase_affinity_influences_ranking` can use it to trigger a rebuild before asserting ranking. If no, this test must be unit-level.

---

## Self-Check

- [x] OVERVIEW.md maps all 14 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan — which suites to run, 2 new tests identified
- [x] Per-component test plans match architecture component boundaries (7 files, 7 components)
- [x] Every Critical and High risk has at least one specific named test expectation
- [x] Integration tests defined for component boundaries (tick→search, service_layer→search, replay→scoring)
- [x] All output files written to `product/features/col-031/test-plan/`
- [x] Knowledge Stewardship report block included below

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 5 entries; key: #3688 (ADR-004 AC-16 non-separable from AC-12), #3689 (ADR-005 required handle), #749 (test scenarios pattern). All consistent with RISK-TEST-STRATEGY.md; no contradictions.
- Queried: `mcp__unimatrix__context_search` (col-031 decisions) — returned 5 ADRs (#3682–#3690); confirmed all resolved decisions are accounted for in test plans.
- Queried: `mcp__unimatrix__context_search` (frequency table scoring test patterns) — returned #724 (behavior-based ranking tests) and #707 (status penalty tests). Both confirmed assert-ordering-not-scores pattern; relevant to `test_search_phase_affinity_influences_ranking`.
- Stored: entry #3691 "Two Cold-Start Contracts on One Method: Test Pattern for Guarded vs. Direct Callers" via `/uni-store-pattern` — novel pattern not previously captured; applies to any future feature where a method serves two callers with distinct cold-start semantics.
