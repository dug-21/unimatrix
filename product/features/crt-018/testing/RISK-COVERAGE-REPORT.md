# Risk Coverage Report: crt-018

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Classification priority logic misorders categories | E-01 through E-05 (tests_classify: noisy_over_ineffective, ineffective_over_unmatched, unmatched_over_settled, ineffective_boundary, default_effective) | PASS | Full |
| R-02 | NULL/empty topic or NULL feature_cycle causes misclassification | E-06 (empty_topic_mapped_to_unattributed), S-05 (null_feature_cycle_excluded), S-06 (empty_feature_cycle_excluded), S-07 (distinct_feature_cycles), S-08 (null_fc_contributes_to_injection_stats), S-13 (classification_meta_empty_topic_unattributed) | PASS | Full |
| R-03 | COUNT vs COUNT DISTINCT session aggregation | S-01 (count_distinct_session_dedup), S-02 (multiple_distinct_sessions), S-03 (null_outcome_excluded) | PASS | Full |
| R-04 | Calibration bucket boundary handling | E-07 through E-13 (confidence 0.0, 0.1, 0.9, 1.0, 0.09999999, 0.5, empty data), plus negative/above-1 clamp tests | PASS | Full |
| R-05 | Division by zero in utility_score | E-14 (zero_denominator), E-15 (pure_success), E-16 (mixed_outcomes), E-16b (large_values_no_overflow) | PASS | Full |
| R-06 | Query performance at scale | S-18 (performance_at_scale: 500 entries, 200 sessions, 10K injection rows, <500ms) | PASS | Full |
| R-07 | GC race condition during computation | S-11 (code review): Single lock_conn() scope at line 871 of read.rs; all 4 queries execute within one connection lock. No intermediate release. | PASS | Full (code review) |
| R-08 | JSON output compatibility | Code review: skip_serializing_if = "Option::is_none" on effectiveness field (status.rs:678). Integration tests: test_status_empty_db, test_status_with_entries, test_status_all_formats all pass. | PASS | Full |
| R-09 | Settled classification logic error | E-17 (inactive_topic_with_success = Settled), E-18 (settled_requires_success_injection), E-19 (inactive_topic_zero_injections = Effective, not Settled) | PASS | Full |
| R-10 | NOISY_TRUST_SOURCES case sensitivity | E-20 (matching_trust_source = Noisy), E-21 (non_matching_trust_source != Noisy), plus noisy_with_helpful_not_noisy test | PASS | Full |
| R-11 | spawn_blocking failure handling | Code review: Phase 8 at status.rs:527-600 uses match on spawn_blocking result. Ok(Err(e)) -> warn + None, Err(join_err) -> warn + None. No .unwrap() on JoinHandle. | PASS | Full (code review) |
| R-12 | Markdown table injection via entry titles | Not directly tested (entry titles stored verbatim at engine layer; markdown escaping is a formatting concern). Low severity/low priority. | N/A | Partial |
| R-13 | SourceEffectiveness aggregate utility NaN | E-22 (zero_injection_source_utility_zero), E-23 (mixed_trust_sources), E-24 (empty_entries) | PASS | Full |

## Test Results

### Unit Tests
- Total: 2115 (workspace-wide)
- Passed: 2115
- Failed: 0
- Ignored: 18 (unimatrix-embed, pre-existing)

#### crt-018-specific unit tests
- effectiveness-engine (classify + utility_score): 19 tests, all PASS
- effectiveness-engine (aggregate + calibration + report): 14 tests, all PASS
- effectiveness-store (compute_effectiveness_aggregates): 12 tests, all PASS
- effectiveness-store (load_entry_classification_meta): 5 tests, all PASS
- **Subtotal: 50 tests, all PASS**

### Integration Tests (infra-001 harness)

#### Smoke suite (mandatory gate)
- Total: 19
- Passed: 18
- xfail: 1 (test_store_1000_entries: pre-existing GH#111)

#### Tools suite
- Total: 71
- Passed: 70
- xfail: 1 (test_status_includes_observation_fields: pre-existing)

#### Lifecycle suite
- Total: 16
- Passed: 16

#### Integration totals
- Total: 106 (across 3 suites)
- Passed: 104
- xfail: 2 (both pre-existing, not caused by crt-018)
- Failed: 0

## Gaps

| Risk ID | Gap Description | Justification |
|---------|----------------|---------------|
| R-12 | No dedicated test for markdown table injection via pipe character in entry titles | Low severity (Low) / Low priority. Engine stores titles verbatim; markdown escaping is a server formatting concern. Existing integration tests (test_status_all_formats) pass, indicating no breakage in current format output. |
| I-01 through I-11 | Server-level Rust integration tests for effectiveness (full pipeline, format output, graceful degradation) not implemented as separate test functions | Covered by: (a) the 50 unit tests across engine + store layers validate the full data contract; (b) existing infra-001 tests for status tool verify non-regression; (c) Phase 8 error handling verified by code review. The test plan OVERVIEW.md noted that effectiveness scenarios requiring injection_log population are not feasible through MCP alone, so these become code-review-verified items rather than harness tests. |

## Code Review Verifications

| Item | Status | Evidence |
|------|--------|----------|
| AC-11: spawn_blocking wraps Phase 8 | VERIFIED | status.rs:527 `tokio::task::spawn_blocking(move \|\| { ... })` |
| AC-17: Named constants for outcome weights | VERIFIED | effectiveness/mod.rs:25-31 `OUTCOME_WEIGHT_SUCCESS=1.0`, `OUTCOME_WEIGHT_REWORK=0.5`, `OUTCOME_WEIGHT_ABANDONED=0.0` |
| R-07: Single lock_conn scope | VERIFIED | read.rs:871 `let conn = self.lock_conn();` — all 4 queries within this scope, no intermediate release |
| R-11: No unwrap on spawn_blocking | VERIFIED | status.rs:527-600 uses `match` with `Ok(Ok(...))`, `Ok(Err(e))`, and `Err(join_err)` arms |
| R-08: skip_serializing_if on JSON | VERIFIED | status.rs:678 `#[serde(skip_serializing_if = "Option::is_none")]` on effectiveness field |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | E-01 through E-05: all five categories tested with priority chain; classify_entry assigns exactly one category per entry |
| AC-02 | PASS | E-03 (Unmatched for active topic + zero injections), E-17/E-19 (Settled logic), S-01 through S-04 (injection_log + sessions join verified) |
| AC-03 | PASS | E-17 (inactive topic + success = Settled), E-18 (no success = NOT Settled), E-19 (zero injections = NOT Settled) |
| AC-04 | PASS | E-04 boundary tests: 3 injections + 33% = NOT Ineffective; 10 injections + 25% = Ineffective |
| AC-05 | PASS | E-01 (auto + 0 helpful + injections = Noisy), E-20/E-21 (trust source matching), test_noisy_with_helpful_not_noisy |
| AC-06 | PASS | E-22 (zero-injection utility = 0.0), E-23 (mixed sources counted), E-24 (empty = empty Vec) |
| AC-07 | PASS | E-07 through E-13 (all bucket boundaries), E-13 (empty = 10 empty buckets) |
| AC-08 | PASS | Code review: summary format "Effectiveness: N effective, N settled, ..." at status.rs:242-246; "no injection data" at status.rs:249 |
| AC-09 | PASS | Code review: markdown format includes "### Effectiveness Analysis" section with category, source, calibration tables |
| AC-10 | PASS | Code review: JSON format includes EffectivenessReportJson with skip_serializing_if; infra-001 test_status_all_formats passes |
| AC-11 | PASS | Code review: Phase 8 wrapped in tokio::task::spawn_blocking (status.rs:527) |
| AC-12 | PASS | E-25 (top 10 ineffective cap), E-26 (all noisy, no cap), E-27 (top 10 unmatched cap) |
| AC-13 | PASS | Code review: no writes in effectiveness path; all queries are SELECT-only; classifications computed fresh per call |
| AC-14 | PASS | E-28 (empty data = valid report with zero counts); E-14 (utility_score zero = 0.0); all boundary tests pass |
| AC-15 | PASS | Store tests S-01 through S-18 + engine tests E-01 through E-28 provide end-to-end coverage from SQL through classification to report assembly |
| AC-16 | PASS | E-06 (empty topic -> "(unattributed)"), S-13 (store layer), S-05/S-08 (NULL feature_cycle handling) |
| AC-17 | PASS | Code review: OUTCOME_WEIGHT_SUCCESS (1.0), OUTCOME_WEIGHT_REWORK (0.5), OUTCOME_WEIGHT_ABANDONED (0.0) defined as pub const in effectiveness/mod.rs |
