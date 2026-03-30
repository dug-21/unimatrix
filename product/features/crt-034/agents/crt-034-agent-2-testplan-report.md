# Agent Report: crt-034-agent-2-testplan

**Phase:** Test Plan Design (Stage 3a)
**Agent ID:** crt-034-agent-2-testplan

---

## Deliverables

All test plan files written to `product/features/crt-034/test-plan/`:

| File | Lines | Scope |
|------|-------|-------|
| `OVERVIEW.md` | ~110 | Overall strategy, risk-to-test mapping, integration harness plan |
| `store_constants.md` | ~55 | AC-07, AC-08, R-08 |
| `config_extension.md` | ~100 | AC-06, AC-10, R-07 |
| `co_access_promotion_tick.md` | ~350 | AC-01..AC-04, AC-09, AC-11..AC-15, E-01..E-06, R-01..R-04, R-06, R-09..R-11, R-13 |
| `background_tick_insertion.md` | ~100 | AC-05, R-05, module registration |

---

## Risk Coverage Mapping

| Risk ID | Priority | Test Component | Primary Tests |
|---------|----------|---------------|---------------|
| R-01 (Critical) | Critical | co_access_promotion_tick | `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` |
| R-02 (High) | High | co_access_promotion_tick | `test_empty_co_access_table_noop_late_tick`, `test_all_below_threshold_noop_late_tick` |
| R-03 (High) | High | co_access_promotion_tick | `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` |
| R-04 (High) | High | co_access_promotion_tick | `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_double_tick_idempotent` |
| R-05 (High) | High | background_tick_insertion | Code review (AC-05) + `test_promotion_early_run_warn_ticks_constant_value` |
| R-06 (High) | High | co_access_promotion_tick | `test_early_tick_warn_when_qualifying_count_zero`, `test_late_tick_no_warn_empty_table`, `test_fully_promoted_table_no_warn` |
| R-07 (High) | High | config_extension | `test_merge_configs_project_overrides_global_co_access_cap` |
| R-08 (Med) | Med | store_constants | `test_co_access_graph_min_count_value` |
| R-09 (Med) | Med | co_access_promotion_tick | `test_double_tick_idempotent`, `test_sub_threshold_pair_not_gcd` |
| R-10 (Med) | Med | co_access_promotion_tick | `test_inserted_edge_is_one_directional` |
| R-11 (High) | High | co_access_promotion_tick | `test_cap_selects_highest_count_pairs` |
| R-12 (Low) | Low | Gate gate check | `wc -l` < 500 at Gate 3c |
| R-13 (High) | High | co_access_promotion_tick | `test_inserted_edge_metadata_all_four_fields` |

All 13 risks from RISK-TEST-STRATEGY.md have test coverage.

---

## Integration Harness Plan Summary

**Suites to run at Stage 3c:**
- `pytest -m smoke` — mandatory gate
- `pytest suites/test_lifecycle.py` — regression check on tick loop

**New integration test:** Optional one test in `suites/test_lifecycle.py` —
`test_co_access_promotion_tick_no_crash_after_tick`. Purpose: liveness check after
tick fires with co-access state. Not required for gate, but recommended if tick timing
is testable without `availability` marker overhead.

**Suites not needed:** `tools`, `protocol`, `security`, `confidence`, `contradiction`,
`volume`, `edge_cases`, `adaptation`. This feature has no MCP tool surface changes.

---

## Key Design Decisions Applied

1. **AC-09 two-branch structure** — SR-05 warn is only emitted when `qualifying_count==0 AND current_tick < 5`. Outside that window, empty table = clean no-op. Tests cover all four quadrants.

2. **AC-13 as query-shape test** — Because `ORDER BY count DESC` always puts the global max in the batch, AC-13 cannot distinguish global vs batch normalization by output value. The test documents the SQL shape requirement for future-proofing correctness.

3. **E-05 boundary test (delta = 0.1 exactly)** — Strictly greater than, not >=. Test uses exact-f64 counts (5/10, 6/10) to avoid floating-point ambiguity. ADR-003 f64 type prevents f32 cast noise.

4. **E-02 tied counts** — Secondary sort behavior is an open question for the implementation agent. Test asserts count=N without specifying which N pairs, unless the implementation adds `entry_id_a ASC` secondary sort.

5. **R-01 write failure injection** — The exact injection mechanism is implementation-dependent (mock Store or constraint conflict). Test plan specifies the behavioral requirement; implementation agent chooses the technique.

6. **AC-05 as code review** — Tick ordering is not testable without a full tick integration harness. Code review of `background.rs` is the primary gate. The optional lifecycle integration test supplements but does not replace it.

---

## Open Questions for Stage 3b

1. **Write failure injection pattern** — Does the existing test infrastructure for `nli_detection_tick.rs` have a mock write path? If not, the implementation agent needs to either (a) use a constraint-conflict-based fake failure, or (b) trait-inject a mock pool. Recommend checking `nli_detection_tick.rs` tests before deciding.

2. **warn! capture in tests** — The SR-05 tests require asserting that `warn!` IS or IS NOT emitted. The test plan assumes a tracing subscriber pattern exists in the codebase. Implementation agent should reuse the pattern from existing background tick tests if one exists, or use `tracing_test` crate.

3. **E-02 secondary sort** — If `ORDER BY count DESC` produces non-deterministic tie-breaking in SQLite, the implementation agent should add `ORDER BY count DESC, entry_id_a ASC` to make the selection deterministic. The test for E-02 should then be updated to assert which specific pairs were selected.

4. **PROMOTION_EARLY_RUN_WARN_TICKS location** — ADR-005 places this constant in `background.rs`. If the implementation agent places it in `co_access_promotion_tick.rs` instead, the `background_tick_insertion.md` test needs to be updated accordingly.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found entries #3826 (ADR-004 InferenceConfig cap), #3822 (near-threshold idempotency pattern), #3827 (ADR-005 tick insertion and SR-05), #3806 (Gate 3b handler integration tests absent lesson). Applied: AC-09 two-branch structure, E-05 delta boundary test, integration test lesson to ensure promotion tick tests are implementation-wave deliverables, not deferred to tester.
- Queried: `context_search('crt-034 architectural decisions', category: 'decision', topic: 'crt-034')` — found ADR-003 (#3829 f64 delta constant), ADR-005 (#3827 tick insertion), ADR-004 (#3826 InferenceConfig), ADR-006 (#3830 edge directionality). All four ADRs incorporated into test assertions.
- Queried: `context_search('background tick graph edges testing patterns')` — found #3822 (promotion tick idempotency), #3656 (ADR-001 crt-029 new module pattern), #3675 (tick candidate bound and shuffle pattern). Applied to co_access_promotion_tick.md fixture design.
- Stored: nothing novel — existing patterns #3822 (near-threshold idempotency) and #3806 (Gate 3b handler test omission lesson) already cover the primary new patterns relevant to this test plan. The AC-13 query-shape framing (future-proofing test under ORDER BY DESC invariant) is feature-specific and not yet observed across 2+ features.
