# Gate 3c Report: crt-024

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-21
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 17 risks (R-01–R-16, R-NEW) have named passing tests in RISK-COVERAGE-REPORT.md; evidence independently verified |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; 2 new integration tests added for R-07 and R-09/AC-06 |
| Specification compliance | WARN | AC-16 (D1–D4 eval harness run) is deferred — no `/tmp/eval/crt024-report.json`; spec Constraint 7 requires this before merge |
| Architecture compliance | PASS | Implementation matches ARCHITECTURE.md exactly: six-term formula, ADR-002 (apply_nli_sort removed), ADR-003 (default weights), ADR-004 (pure function, penalty at call site), Step 6c prefetch |
| Knowledge stewardship — tester | PASS | Queried and Stored entries present in agent report; reason given for declining store |

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence verified**:

All 17 risk IDs confirmed covered. Non-negotiable test names grep-verified in source:

- `test_compute_fused_score_six_term_correctness_ac05` — present at search.rs:1842 (AC-05, R-01)
- `test_compute_fused_score_nli_high_beats_coac_high_ac11` — present at search.rs:1870 (AC-11, R-06)
- `test_fusion_weights_effective_zero_denominator_returns_zeros_without_panic` — grep-confirmed in search.rs tests (R-02)
- `test_prov_norm_zero_denominator_returns_zero` — grep-confirmed in search.rs tests (R-03)
- `test_compute_fused_score_ineffective_util_non_negative` — present at search.rs:2252 (R-11)
- `test_compute_fused_score_result_is_finite` — present at search.rs:2194 (R-03, SeR-02)
- `test_inference_config_validate_rejects_sum_exceeding_one` — grep-confirmed in config.rs tests (R-12, AC-02)
- Twelve per-field range tests (`test_inference_config_validate_rejects_w_{field}_{below_zero,above_one}`) — grep-confirmed at config.rs:4040–4188 (AC-03)
- `test_search_coac_signal_reaches_scorer` — present at test_lifecycle.py:1007 (R-07)
- `test_search_nli_absent_uses_renormalized_weights` — present at test_tools.py:1233 (R-09, AC-06)
- `test_eval_service_layer_sim_only_profile_scores_equal_sim` — grep-confirmed in search.rs tests (R-NEW, AC-15)
- `test_eval_service_layer_differential_two_profiles_produce_different_scores` — grep-confirmed in search.rs tests (R-NEW)

R-04 (regression test churn) verified: workspace test count 3197 (all pass, 0 failures), which exceeds the pre-crt-024 baseline of 2169 unit + integration tests.

R-05 (apply_nli_sort removal): confirmed removed — only one reference at search.rs:1632 which is a migration comment. Replacement tests confirmed present.

R-08 (MAX_CO_ACCESS_BOOST constant duplication): confirmed — search.rs line 19 imports from engine crate; `grep "const MAX_CO_ACCESS_BOOST" search.rs` → 0 results.

**Issue**: None.

---

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**:

All 10 minimum test additions from RISK-TEST-STRATEGY.md §Coverage Summary verified:

1. `util_norm` boundary values (-0.05→0.0, 0.0→0.5, +0.05→1.0): `test_util_norm_*` series — confirmed
2. `coac_norm` boundary values: `test_coac_norm_boundary_values` — confirmed
3. `prov_norm` division-by-zero guard: `test_prov_norm_zero_denominator_returns_zero` — confirmed
4. NLI absence re-normalization (five-weight denominator, zero-denominator guard): `test_fusion_weights_effective_*` series — confirmed
5. NLI active path no spurious re-normalization (AC-13): `test_fusion_weights_effective_nli_active_unchanged` — confirmed
6. AC-11 regression test: `test_compute_fused_score_nli_high_beats_coac_high_ac11` — confirmed; Entry A scores 0.540, Entry B scores 0.430
7. ADR-003 Constraint 9 and 10: `test_compute_fused_score_constraint9_nli_disabled_sim_dominant` and `test_compute_fused_score_constraint10_sim_dominant_at_defaults` — confirmed
8. boost_map prefetch integration test: `test_search_coac_signal_reaches_scorer` (test_lifecycle.py) — confirmed, new test
9. `try_nli_rerank` new return type: compile-time gate passes (`cargo build` clean), plus behavioral tests — confirmed
10. Fused score range guarantee: `test_compute_fused_score_result_is_finite` + `test_compute_fused_score_range_guarantee_all_inputs_zero` — confirmed

Integration xfail markers audited:
- `test_lifecycle.py`: 1 xfail — `test_auto_quarantine_after_consecutive_bad_ticks` (GH#291, pre-existing, tick-interval limitation)
- `test_tools.py`: 1 xfail — `test_retrospective_baseline_present` (GH#305, pre-existing, baseline_comparison null with synthetic features)
- `test_edge_cases.py`: 1 xfail — `test_100_rapid_sequential_stores` (GH#111, pre-existing, rate limit timing)

All three xfails reference existing GitHub issues and are genuinely unrelated to crt-024 (scoring formula, config weights, or pipeline ordering).

Smoke suite: 20/20 PASS (mandatory integration gate).

**Issue**: None.

---

### 3. Specification Compliance

**Status**: WARN (AC-16 deferred)

**Evidence**:

AC-01 through AC-15 verified as PASS by the tester agent with named test evidence. Independently confirmed via grep for key test names and direct code inspection.

SPECIFICATION.md Constraint 7 states: "Eval harness gate required before merge (supersedes prior 'no eval gate' statement): D1–D4 eval harness run on the pre-crt024 snapshot is required before the PR is merged (AC-16)."

SPECIFICATION.md AC-16 states: "Eval report file exists at `/tmp/eval/crt024-report.json`. Baseline log updated. PR description includes report review summary."

**Gap**: `/tmp/eval/crt024-report.json` does not exist. The pre-crt024 snapshot (`/tmp/eval/pre-crt024-snap.db`) and scenarios (`/tmp/eval/pre-crt024-scenarios.jsonl`) ARE present. The eval run itself has not been performed.

The RISK-COVERAGE-REPORT.md and tester agent correctly flag AC-16 as DEFERRED pending human sign-off, and the IMPLEMENTATION-BRIEF.md §Eval Harness Steps explicitly describes the procedure. However, the IMPLEMENTATION-BRIEF.md NOT-IN-SCOPE section contains a contradictory statement: "Eval harness changes — no eval gate; formula-deterministic feature." This is incorrect: the SPECIFICATION.md and SCOPE constraints both require the eval gate. The IMPLEMENTATION-BRIEF.md misstatement does not override the binding spec.

**Classification**: WARN rather than FAIL because:
- The spec designates this as a human-gate pre-merge step, not an automated test
- All preconditions for running the eval are met (snapshot and scenarios are present)
- No automated coverage gap exists; this is a procedural gate requiring human action
- The tester correctly deferred rather than skipping

The PR must not be merged until AC-16 is completed. This is not a code defect — it is an outstanding human-gate step.

**Issue (WARN)**: AC-16 eval harness not yet run. Required before PR merge. Snapshot ready at `/tmp/eval/pre-crt024-snap.db`.

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

**Formula implementation** (ARCHITECTURE.md §Canonical Fused Scoring Formula): `compute_fused_score` at search.rs:180–187 implements exactly:
```
w_sim * similarity + w_nli * nli_entailment + w_conf * confidence + w_coac * coac_norm + w_util * util_norm + w_prov * prov_norm
```
Status penalty applied at call site (search.rs:800): `fused * penalty` — not inside the function. ADR-004 invariant holds.

**NLI re-normalization** (ARCHITECTURE.md §NLI Absence Re-normalization): `FusionWeights::effective()` at search.rs:124–162 implements the five-weight denominator (`w_sim + w_conf + w_coac + w_util + w_prov`) with zero-denominator guard. Correct.

**Step 6c prefetch** (ARCHITECTURE.md §Data Flow): boost_map computed at search.rs:677–723 (Step 6c), fully `.await`-ed, result bound before Step 7 NLI scoring begins at search.rs:732. No interleaved await points between boost_map resolution and the scoring loop at search.rs:755. SR-07 correctly resolved.

**ADR-002 compliance**: `apply_nli_sort` removed from production code. Only migration comment at search.rs:1632. `try_nli_rerank` returns `Option<Vec<NliScores>>` (raw scores, no sort). NLI entailment consumed inline in scoring loop.

**ADR-003 compliance**: Default weights confirmed: `w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05` at config.rs:381–386. Sum = 0.95, 0.05 headroom for WA-2.

**ADR-004 compliance**: `compute_fused_score` is a pure `pub(crate) fn` with no I/O, async, locks, or side effects. Status penalty applied by caller.

**Scope constraint (NFR-04)**: Only `search.rs` and `config.rs` modified. Engine crates unchanged. Schema version not incremented.

**BriefingService isolation** (FR-13, AC-14): `git diff` confirms no changes to briefing.rs. `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` unchanged.

**EvalServiceLayer wiring** (FR-14, AC-15): `FusionWeights::from_config(&inference_config)` confirmed at services/mod.rs:394 via tester code audit.

**Issue**: None.

---

### 5. Knowledge Stewardship — Tester Agent

**Status**: PASS

**Evidence**:

`product/features/crt-024/agents/crt-024-agent-5-tester-report.md` contains a `## Knowledge Stewardship` section:
- `Queried:` — entries #487 and #750 (testing procedures)
- `Stored:` — "nothing novel to store — scoring formula integration test patterns are first/second observations; will store if pattern recurs in a third feature"

Reason given for declining store is substantive (threshold reasoning, not boilerplate). PASS.

---

## Rework Required

No code rework required. One outstanding human-gate item:

| Item | Type | What to Do |
|------|------|------------|
| AC-16: D1–D4 eval harness not run | Human gate | Run `eval-harness run --db /tmp/eval/pre-crt024-snap.db --scenarios /tmp/eval/pre-crt024-scenarios.jsonl --profiles old-behavior.toml crt024-weights.toml --out /tmp/eval/crt024-report.json`; human reviews report; update PR description with outcome summary. Snapshot is present. |

**This is classified as REWORKABLE FAIL because the spec (Constraint 7 + AC-16) makes the eval gate run a hard requirement for merge readiness**, and the gate cannot be satisfied by the tester agent alone — it requires a human reviewer. The code is correct; the procedure is incomplete.

If the human has already reviewed the eval output in a separate channel and accepted the results, they may override this gate and treat it as PASS. The code and all automated tests are correct.

---

## Knowledge Stewardship

- Queried: `context_search` for "gate 3c validation failure patterns eval harness human gate deferred" (category: lesson-learned) — found entry #2758 ("Gate 3c: always grep non-negotiable test names before accepting RISK-COVERAGE-REPORT PASS claims"). Applied: independently grep-verified all named tests in the coverage report against source files.
- Stored: nothing novel to store — the "human eval gate deferred" pattern is feature-specific; the procedure for handling human-required pre-merge steps is documented in spec and implementation brief, not a systemic gap worth a lesson entry.
