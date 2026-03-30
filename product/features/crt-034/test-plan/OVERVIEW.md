# crt-034 Test Plan Overview

## Feature Summary

crt-034 adds a recurring background tick step (`run_co_access_promotion_tick`) that promotes
qualifying `co_access` pairs (count >= 3) into `GRAPH_EDGES` as `CoAccess`-typed edges and
refreshes the weight of already-promoted edges when normalized weight drift exceeds a
configurable threshold. No schema migration. No new MCP tools.

---

## Test Strategy

### Layers

| Layer | Scope | Tool |
|-------|-------|------|
| Unit | Logic of each new function and constant, in-process SQLite fixtures | `cargo test --workspace` |
| Integration (infra-001 smoke) | Mandatory gate: MCP-level regression check | `pytest -m smoke` |
| Integration (lifecycle suite) | Tick-driven behavior visible at MCP level | `pytest suites/test_lifecycle.py` |

This feature touches no MCP tool surface and adds no schema migration, so `tools`, `security`,
`confidence`, `contradiction`, `volume`, and `protocol` suites are not in scope beyond smoke.

### Unit Test Placement

All unit tests live in `#[cfg(test)]` blocks within the implementation files they test:

- `crates/unimatrix-store/src/read.rs` — constant value tests (AC-07, AC-08)
- `crates/unimatrix-server/src/infra/config.rs` — config field tests (AC-06, AC-10)
- `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` — promotion logic (AC-01..AC-04, AC-09, AC-11..AC-15)

Test infrastructure is cumulative: use existing in-process SQLite fixture patterns from
`nli_detection_tick.rs` tests. Do not create isolated scaffolding.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Component | AC(s) | Test Function(s) |
|---------|----------|---------------|-------|-----------------|
| R-01 | Critical | co_access_promotion_tick | AC-11 | `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` |
| R-02 | High | co_access_promotion_tick | AC-09 | `test_empty_co_access_table_noop`, `test_all_below_threshold_noop` |
| R-03 | High | co_access_promotion_tick | AC-13 | `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` |
| R-04 | High | co_access_promotion_tick | AC-02, AC-03, AC-14 | `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_double_tick_idempotent` |
| R-05 | High | background_tick_insertion | AC-05 | Code review (static) + optional lifecycle integration test |
| R-06 | High | co_access_promotion_tick | AC-09 | `test_early_tick_warn_qualifying_count_zero`, `test_late_tick_no_warn_empty_table`, `test_fully_promoted_table_no_warn` |
| R-07 | High | config_extension | AC-06 | `test_merge_configs_project_overrides_global`, `test_merge_configs_global_only` |
| R-08 | Med | store_constants | AC-07 | `test_co_access_graph_min_count_value` |
| R-09 | Med | co_access_promotion_tick | AC-14, AC-15 | `test_double_tick_idempotent`, `test_sub_threshold_pair_not_gc'd` |
| R-10 | Med | co_access_promotion_tick | AC-12 | `test_inserted_edge_is_one_directional` |
| R-11 | High | co_access_promotion_tick | AC-04 | `test_cap_selects_highest_count_pairs` |
| R-12 | Low | co_access_promotion_tick | — | File size gate: `wc -l` < 500 at Gate 3c |
| R-13 | High | co_access_promotion_tick | AC-12 | `test_inserted_edge_metadata_all_four_fields` |

### Edge Cases from Risk Strategy

| Edge Case | Component | Test Function |
|-----------|-----------|---------------|
| E-01: Single qualifying pair (max=self, weight=1.0) | co_access_promotion_tick | `test_single_qualifying_pair_weight_one` |
| E-02: Tied counts with cap | co_access_promotion_tick | `test_tied_counts_secondary_sort_stable` |
| E-03: Cap exactly equals qualifying count | co_access_promotion_tick | `test_cap_equals_qualifying_count` |
| E-04: cap=1 (minimum valid cap) | co_access_promotion_tick | `test_cap_one_selects_highest_count` |
| E-05: Weight delta exactly at boundary (delta = 0.1 exactly, NOT updated) | co_access_promotion_tick | `test_weight_delta_exactly_at_boundary_no_update` |
| E-06: Self-loop pair (entry_id_a == entry_id_b) | co_access_promotion_tick | `test_self_loop_pair_no_panic` |

---

## Cross-Component Test Dependencies

| Dependency | Detail |
|------------|--------|
| `store_constants` → `co_access_promotion_tick` | Promotion tick uses `CO_ACCESS_GRAPH_MIN_COUNT` and `EDGE_SOURCE_CO_ACCESS` constants; constant tests must pass before promotion tests are meaningful |
| `config_extension` → `co_access_promotion_tick` | `max_co_access_promotion_per_tick` is read by `run_co_access_promotion_tick`; config field tests validate the input side |
| `background_tick_insertion` → all | Static ordering test (AC-05) is a gate prerequisite; only verifiable after code review of `background.rs` |

---

## Integration Harness Plan

### Suite Selection

This feature does not add or modify any MCP tool, does not change security scanning, does
not modify confidence scoring, and does not change the schema. The correct selection is:

| Suite | Rationale |
|-------|-----------|
| `smoke` (-m smoke) | Mandatory gate. Regression baseline: promotion tick must not break any existing tool |
| `lifecycle` | Tick-driven behavior: if a tick integration test (see below) is added, it lives here |

Suites NOT needed: `tools`, `protocol`, `security`, `confidence`, `contradiction`, `volume`,
`edge_cases`, `adaptation`. None of these exercise background tick internals through MCP.

### Existing Suite Coverage Gap Analysis

crt-034 adds a background tick step with no MCP-visible API surface change. The PPR graph
is rebuilt each tick; tick-promoted CoAccess edges affect retrieval re-ranking via PPR, but
this effect is only observable through search results after a tick fires — not through a
deterministic per-call assertion. The existing `lifecycle` suite has `test_tick_liveness`
(under `availability` marker), not a full tick-integration flow test.

**Gap**: No existing infra-001 test validates that tick-promoted CoAccess edges are
reachable after a tick fires (AC-05 ordering, I-03 TypedGraphState inclusion).

### New Integration Test to Add

One new test is warranted in `suites/test_lifecycle.py`:

```python
# test_co_access_promotion_tick_edges_visible_after_tick
# Fixture: server (fresh DB)
# Scenario:
#   1. Store two entries (get their IDs from response)
#   2. Trigger enough co-access events to cross CO_ACCESS_GRAPH_MIN_COUNT threshold
#      (use context_search or context_get calls that co-access-record the two entries)
#   3. Wait for a tick cycle (availability mark or sleep — see existing tick_liveness pattern)
#   4. Call context_status (maintain=false) and confirm server is still alive
#   5. Verify: no error, server still accepts MCP calls
# NOTE: Direct PPR graph inspection is not possible via MCP; this test is
# a liveness + no-crash test after tick, not a graph-content assertion.
# The full correctness check (edges actually promoted) is covered by unit tests.
```

This test validates I-03 (TypedGraphState does not crash on tick-promoted edges) and R-05
(tick ordering does not cause a tick-cycle failure). It does NOT need to assert PPR output
change — that is covered by unit tests.

**Decision**: Because tick synchronization in integration tests requires the `availability`
harness pattern (sleep-based), and the existing `test_tick_liveness` already covers tick
execution liveness, **adding a new integration test is LOW priority**. Stage 3c tester
agent should:

1. Run `pytest -m smoke` (mandatory).
2. Run `pytest suites/test_lifecycle.py` to check for regressions.
3. If neither reveals failures, the integration gate is satisfied.
4. Optionally add the new lifecycle test if implementation agent confirms tick timing is
   testable without `availability`-marker overhead.

---

## Self-Check

- [x] OVERVIEW.md maps all 13 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan with suite selection and gap analysis
- [x] Per-component test plans match architecture component boundaries (4 files)
- [x] Every high-priority risk (R-01, R-02, R-03, R-04, R-05, R-06, R-07, R-11, R-13) has at least one named test
- [x] Integration tests defined for MCP-observable boundaries
- [x] All output files within `product/features/crt-034/test-plan/`
