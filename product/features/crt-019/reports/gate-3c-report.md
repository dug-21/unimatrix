# Gate 3c Report: crt-019

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-14
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 17 risks + 3 integration risks have passing tests per RISK-COVERAGE-REPORT |
| Test coverage completeness | PASS | All 28 risk-to-scenario mappings exercised; 4 new integration tests present |
| Specification compliance | PASS | All 12 ACs verified PASS; all FRs implemented; NFRs confirmed |
| Architecture compliance | PASS | Components 1-7 implemented; ConfidenceStateHandle wired correctly through ServiceLayer |
| Integration smoke gate | PASS | 18 smoke passed, 1 xfailed (pre-existing GH#111) |
| xfail audit | PASS | 6 pre-existing xfails all reference GH Issues; none masking crt-019 regressions |
| No integration test deletions | PASS | Verified via grep; no tests removed or commented out |
| Build | PASS | `cargo build --workspace` succeeds with 0 errors, 7 warnings (all pre-existing) |
| Unit test count | PASS | 2401 passed / 0 failed — matches reported count |
| Knowledge stewardship | WARN | Tester queried server (unavailable); fallback documented; no novel patterns to store |

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS

Every risk in RISK-TEST-STRATEGY.md has a test scenario mapped in RISK-COVERAGE-REPORT.md.

Critical risk R-01 (compute_confidence bare function pointer) is covered by:
- `test_empirical_prior_flows_to_stored_confidence` (integration, test_lifecycle.py) — end-to-end proof that Bayesian formula is active
- `test_context_lookup_access_weight_2_increments_by_2` — store-layer unit confirming closure path

R-11 (store.record_usage_with_confidence internal ID deduplication): Resolved by the `flat_map repeat` approach, verified in `test_context_lookup_access_weight_2_increments_by_2`. The comment at usage.rs line 144-147 explicitly documents the verification reasoning: the store's update loop uses `all_ids` for iteration and `access_ids` for set-membership checks, making duplicate IDs in `all_ids` produce `access_count += 2`.

R-05 (threshold discrepancy): SPEC was updated to ≥10 (consistent with ARCH ADR-002). MINIMUM_VOTED_POPULATION = 10 is confirmed at status.rs line 31. Boundary tests at 9 and 10 entries pass.

R-06 (ConfidenceState initial value): `ConfidenceState::default()` initializes `observed_spread = 0.1471`, `confidence_weight = 0.18375`. Tests `test_confidence_state_initial_observed_spread` and `test_confidence_state_initial_weight` verify this explicitly.

### Test Coverage Completeness

**Status**: PASS

**Unit test suites** (all PASS, cargo test --workspace):

| Test Group | File | Key Tests | Verified |
|-----------|------|-----------|----------|
| Weight constants + sum | confidence.rs | `weight_sum_invariant_f64`, `weight_constants_values` | PASS |
| base_score 2-param | confidence.rs | 7 tests incl. `auto_proposed_base_score_unchanged` | PASS |
| Bayesian helpfulness | confidence.rs | 7 tests incl. exact AC-02 assertions | PASS |
| rerank_score 3-param | confidence.rs | 7 tests incl. `rerank_score_adaptive_differs_from_fixed` | PASS |
| adaptive_confidence_weight | confidence.rs | 6 tests incl. floor/cap/initial-spread | PASS |
| ConfidenceState | services/confidence.rs | 7 tests incl. R-06 initial value guard | PASS |
| Empirical prior threshold | services/status.rs | 4 tests at n=9/10 boundary | PASS |
| Zero-variance degeneracy | services/status.rs | 3 tests (all-helpful, all-unhelpful, mixed) | PASS |
| UsageContext access_weight | services/usage.rs | `test_context_lookup_access_weight_2_increments_by_2`, dedup-before-multiply | PASS |
| Batch constant | infra/coherence.rs | `test_max_confidence_refresh_batch_is_500` | PASS |
| Pipeline calibration | tests/pipeline_calibration.rs | T-ABL-01..06, T-CAL-04 (tau>0.6), AC-12, T-CAL-SPREAD-01 | PASS |
| Pipeline regression | tests/pipeline_regression.rs | T-REG-01 (ordering), T-REG-02 (new constants) | PASS |
| Pipeline retrieval | tests/pipeline_retrieval.rs | T-RET-01 uses 3-param rerank_score (no old constant) | PASS |

**Integration test suites** (all PASS):

| Suite | Tests | Passed | Xfailed | Status |
|-------|-------|--------|---------|--------|
| test_confidence.py | 14 | 14 | 0 | PASS |
| test_tools.py | 71 | 69 | 4 (pre-existing) | PASS |
| test_lifecycle.py | 17 | 16 | 1 (pre-existing) | PASS |

**New crt-019 integration tests** (all four required by spawn prompt are present):

| Test | Suite | Risk | Status |
|------|-------|------|--------|
| `test_empirical_prior_flows_to_stored_confidence` | test_lifecycle.py | R-01 critical | PASS |
| `test_context_get_implicit_helpful_vote` | test_tools.py | AC-08a | PASS |
| `test_context_lookup_doubled_access_count` | test_tools.py | AC-08b, R-07, R-11 | PASS |
| `test_search_uses_adaptive_confidence_weight` | test_confidence.py | R-02, AC-06 | PASS |

### Specification Compliance

**Status**: PASS

All 12 Acceptance Criteria verified against ACCEPTANCE-MAP.md:

**AC-01** (confidence spread ≥ 0.20): `test_cal_spread_synthetic_population` uses a 50-entry population (10 low-signal auto, 30 moderate-signal agent, 10 high-signal human) and asserts `p95 - p5 >= 0.20`. PASS.

**AC-02** (Bayesian posterior exact assertions): All four exact assertions confirmed in confidence.rs unit tests:
- `helpfulness_score(0, 0, 3.0, 3.0) == 0.5` (cold-start neutral)
- `helpfulness_score(0, 2, 3.0, 3.0) == 0.375` (two unhelpful votes)
- `helpfulness_score(2, 2, 3.0, 3.0) == 0.5` (balanced, R-14 corrected assertion)
- `helpfulness_score(2, 0, 3.0, 3.0) > 0.5` (two helpful votes)

**AC-03** (weight sum): `assert_eq!(stored_sum, 0.92_f64)` in `weight_sum_invariant_f64`. PASS.

**AC-04** (ablation tests): T-ABL-01..06 all pass with new weight vector. PASS.

**AC-05** (trust-source differentiated base_score): Confirmed in confidence.rs — Active/auto returns 0.35, all other Active sources return 0.5. `auto_proposed_base_score_unchanged` confirms Proposed/auto = 0.5 (ADR-003, C-03). PASS.

**AC-06** (adaptive blend): `SEARCH_SIMILARITY_WEIGHT` confirmed absent from codebase (`grep` returns zero results). `rerank_score` uses 3-param signature at all 4 call sites in search.rs (lines 295, 296, 347, 348, 390). `confidence_weight` snapshotted before search loop via `ConfidenceStateHandle.read()`. PASS.

**AC-07** (batch size + duration guard): `MAX_CONFIDENCE_REFRESH_BATCH = 500` at coherence.rs line 20. Duration guard `if loop_start.elapsed() > wall_budget { break; }` at status.rs line 837, confirmed pre-iteration (before `store_for_refresh.update_confidence(id, new_conf)` at line 844). PASS.

**AC-08a** (context_get implicit helpful vote): tools.rs line 619 passes `helpful: params.helpful.or(Some(true))`. Zero new `spawn_blocking` calls in the handler (C-04 compliant). PASS.

**AC-08b** (context_lookup doubled access): tools.rs line 470 sets `access_weight: 2`. usage.rs `record_mcp_usage` applies `flat_map repeat` after dedup (C-05 compliant). PASS.

**AC-09** (skill files): Search skill line 34 has `helpful: true` in primary example; lines 54-55 provide `helpful: false` guidance. Lookup skill line 37 has `helpful: true`; lines 62-63 have guidance. PASS.

**AC-10** (calibration + regression): T-REG-01 ordering (`expert > good > auto > stale > quarantined`) preserved. T-REG-02 updated to new constants `{W_BASE: 0.16, W_USAGE: 0.16, W_HELP: 0.12, W_TRUST: 0.16}`. PASS.

**AC-11** (weight sum exact): `assert_eq!` (not tolerance). PASS.

**AC-12** (auto vs. agent spread): `auto_vs_agent_spread` tests three signal levels (zero, mid, high) with Active-only entries; at all three levels `conf(Active,"agent") > conf(Active,"auto")`. PASS.

**NFR compliance**:
- NFR-01 (spread ≥ 0.20): AC-01 covers this.
- NFR-02 (calibration stability): T-ABL-01..06, T-CAL-04 (tau > 0.6) pass.
- NFR-03 (200ms guard): Duration guard present pre-iteration.
- NFR-04 (fire-and-forget < 50ms): `test_record_access_fire_and_forget_returns_quickly` present and passing.
- NFR-05 (no schema change): Schema version remains 12 — confirmed.
- NFR-06 (no new lock_conn calls in async context): No new direct `lock_conn()` in async context — `spawn_blocking` wraps all store operations.
- NFR-07 (T-REG-01 ordering): Preserved.

### Architecture Compliance

**Status**: PASS

**Component 1** (confidence.rs engine): All function signatures match architecture spec — `helpfulness_score(helpful, unhelpful, alpha0, beta0)`, `base_score(status, trust_source)`, `compute_confidence(entry, now, alpha0, beta0)`, `rerank_score(similarity, confidence, confidence_weight)`. Old constants (`MINIMUM_SAMPLE_SIZE`, `WILSON_Z`, `SEARCH_SIMILARITY_WEIGHT`) absent. New constants (`COLD_START_ALPHA`, `COLD_START_BETA`) present as documentation constants.

**Component 2** (ConfidenceState): `ConfidenceState` struct with four fields (`alpha0`, `beta0`, `observed_spread`, `confidence_weight`). `ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>`. `Default` implementation uses pre-crt-019 measured values (R-06). All lock acquisitions use `unwrap_or_else(|e| e.into_inner())` (FM-03). PASS.

**Component 3** (empirical prior): `compute_empirical_prior` and `compute_observed_spread` in status.rs. `MINIMUM_VOTED_POPULATION = 10` (ADR-002). Zero-variance degeneracy guard returns cold-start. Clamp [0.5, 50.0] applied. PASS.

**Component 4** (refresh batch): `MAX_CONFIDENCE_REFRESH_BATCH = 500` in coherence.rs. Wall-clock duration guard in spawn_blocking loop. `alpha0`/`beta0` snapshotted once before the loop (IR-02). PASS.

**Component 5** (deliberate retrieval signals): `context_get` uses `params.helpful.or(Some(true))` — no new spawn_blocking. `context_lookup` sets `access_weight: 2`. All other UsageContext construction uses `access_weight: 1`. PASS.

**Component 6** (skill files): Both skill files updated with `helpful: true` in primary examples and `helpful: false` guidance. PASS.

**Component 7** (test infrastructure): All pipeline tests updated to new signatures. `auto_vs_agent_spread` scenario present. Duplicate `adaptive_confidence_weight_local` in status.rs noted (see WARN below). PASS.

**ServiceLayer wiring** (IR-01): `ConfidenceService::state_handle()` produces a cloned `Arc` shared to `SearchService` (reader), `StatusService` (writer), and `UsageService` (reader) via `ServiceLayer::new`. Confirmed at mod.rs lines 280, 289, 316, 322. PASS.

### Integration Smoke Tests

**Status**: PASS

Smoke gate result: 18 passed, 1 xfailed (GH#111 volume test). No failures.

### xfail Audit

**Status**: PASS

| Test | File | Marker Reason | Pre-existing? |
|------|------|---------------|--------------|
| `test_store_restricted_agent_rejected` | test_tools.py | GH#233 | Yes |
| `test_correct_requires_write` | test_tools.py | GH#233 | Yes |
| `test_deprecate_requires_write` | test_tools.py | GH#233 | Yes |
| `test_status_includes_observation_fields` | test_tools.py | GH#187 | Yes |
| `test_multi_agent_interaction` | test_lifecycle.py | GH#238 | Yes |
| `TestVolume1K::test_store_1000_entries` | (smoke) | GH#111 | Yes |

All xfail markers reference pre-existing GitHub Issues. None of these tests exercise crt-019 functionality. The four new crt-019 integration tests (`test_empirical_prior_flows_to_stored_confidence`, `test_context_get_implicit_helpful_vote`, `test_context_lookup_doubled_access_count`, `test_search_uses_adaptive_confidence_weight`) carry no xfail markers — they are expected to pass and do pass.

### Knowledge Stewardship Compliance

**Status**: WARN

The tester's RISK-COVERAGE-REPORT includes a `## Knowledge Stewardship` section. The report documents that `/uni-query-patterns` (queried as `/uni-knowledge-search`) was attempted but the server was unavailable. The fallback and reasoning are documented. The report concludes nothing novel to store, with rationale ("specific to a limitation of the current MCP response schema"). The section is present and the reason is provided. The unavailability of the server is an infrastructure constraint, not a procedural failure.

## Warnings (Non-Blocking)

### WARN: adaptive_confidence_weight_local Duplicates Engine Formula

`status.rs` contains `fn adaptive_confidence_weight_local(observed_spread: f64) -> f64` (lines 148-150) which is a private copy of `unimatrix_engine::confidence::adaptive_confidence_weight`. A code comment at line 145-147 documents this as intentional with a note to consolidate. Independent tests in status.rs verify the local formula. No functional divergence at time of review, but the duplication could diverge in a future change. Recommend filing a follow-up to consolidate to a single call site.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — this gate run found no recurring cross-feature validation failures; all issues were resolved in implementation. The threshold discrepancy pattern (SPEC/ARCH disagreement on voted-entry count) was specific to this feature and already documented in RISK-TEST-STRATEGY.md R-05.
