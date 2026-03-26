# Agent Report: nan-008-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Feature: nan-008 — Distribution-Aware Metrics (CC@k and ICD)

## Output Files

All files written to `/workspaces/unimatrix-nan-008/product/features/nan-008/test-plan/`:

- `OVERVIEW.md` — overall test strategy, risk-to-test mapping, integration harness plan
- `runner-metrics.md` — unit tests for compute_cc_at_k, compute_icd, compute_comparison
- `runner-output.md` — serialization field presence, compile-time struct literal guard
- `runner-replay.md` — wiring tests, config default tests (R-08, R-09)
- `report-mod.md` — round-trip test, backward compat, serde(default) direct tests
- `report-aggregate.md` — mean accumulation, sort-order tests (R-11, R-12)
- `report-render.md` — section-order position tests, ICD annotation, Summary columns

## Risk Coverage Summary

| Risk ID | Priority | Test Location | Status |
|---------|----------|---------------|--------|
| R-01 | Critical | report/tests.rs round-trip | Covered by mandatory test |
| R-02 | High | report/tests.rs section-order | Covered by position assertion |
| R-03 | High | tests_metrics.rs | Covered by guard test |
| R-04 | High | report/tests.rs | Covered by ln( annotation assertion |
| R-05 | High | tests_metrics.rs | Covered by 4 boundary tests + NaN guard |
| R-06 | High | Manual artifact | Delivery step only (ADR-005) |
| R-07 | High | report/tests.rs | Covered by backward-compat JSON test |
| R-08 | High | report/tests.rs round-trip | Covered (category non-empty assertion) |
| R-09 | Med | Config unit test | Covered by KnowledgeConfig::default test |
| R-10 | Med | tests_metrics.rs | Covered by positive + negative delta tests |
| R-11 | Med | report/tests.rs | Covered by 3-scenario mean assertion |
| R-12 | Med | report/tests.rs | Covered by sort-order test |
| R-13 | Low | Operational check | Run `eval --help` at delivery time |

## Integration Harness Decision

infra-001 is NOT the primary integration vehicle for nan-008. The eval harness
has no MCP interface; all integration risk is reachable via `cargo test`.
Only the smoke suite is required (mandatory compile/binary health gate).

This decision is documented in OVERVIEW.md and stored as pattern #3526.

## Open Questions

1. `compute_comparison` visibility: the test plan assumes `compute_comparison` is
   accessible for direct testing in `tests_metrics.rs`. If it is private, the
   delta-sign tests (R-10) must be exercised through `make_scenario_result` with
   known field values and verified via `compute_aggregate_stats` output instead.

2. `run_single_profile` async complexity: the R-08 integration test (category
   populated from `se.entry.category`) may require a fixture database. If the
   function cannot be called in a unit test context, R-08 coverage falls entirely
   on the round-trip test in `report/tests.rs`, which tests the JSON path but not
   the live `replay.rs` mapping. The delivery agent should assess whether a
   fixture-DB test is feasible; if not, the round-trip test is sufficient per
   ADR-003's intent.

3. `CcAtKScenarioRow` location: architecture says it may live in `report/aggregate.rs`
   or `report/mod.rs`. The test plan references it from `report/tests.rs` via `use
   super::aggregate::...`. The delivery agent should confirm the final location and
   adjust the import accordingly.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "nan-008 architectural decisions" (decision, topic:nan-008)
  — found all 5 ADRs (#3520–#3524), directly applicable.
- Queried: `/uni-knowledge-search` for "eval harness testing patterns edge cases"
  — found #1204 (test plan must cross-reference pseudocode, lesson-learned), #2928
  (string-refactor test patterns), #729 (cross-crate integration tests). Entry #1204
  confirmed the approach of grounding each test in specific pseudocode assertions.
- Queried: `/uni-knowledge-search` for "testing procedures gate verification" (procedure)
  — found #750 (pipeline validation tests), #487 (workspace tests without hanging),
  #3479 (two-site atomicity coupled-test pattern). Entry #3479 (col-027) confirmed
  the round-trip approach for two-site atomicity is an established pattern.
- Stored: entry #3526 "Eval Harness Feature Test Strategy: Round-Trip Over infra-001
  for JSON Schema Boundary Risk" via /uni-context-store (pattern, topic: testing).
  This captures the novel decision to use a round-trip test rather than infra-001 for
  eval-harness features, and the rationale.
