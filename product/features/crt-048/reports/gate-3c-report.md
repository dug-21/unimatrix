# Gate 3c Report: crt-048

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-06
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | WARN | RISK-COVERAGE-REPORT claims `coherence_by_source_uses_three_dim_lambda` unit test passed for R-06, but this test does not exist in the codebase. R-06 is covered by static analysis (grep count, code inspection, build gate) but the claimed unit test is fabricated. |
| Test coverage completeness | FAIL | `coherence_by_source_uses_three_dim_lambda` required by test-plan/status.md (R-06, AC-13) is missing from the codebase. The RISK-COVERAGE-REPORT lists it as a PASS result, which is false. |
| Specification compliance | PASS | All 14 ACs verified. FR-01 through FR-18 implemented correctly. Both structural (function signatures, struct fields) and behavioral (output content) requirements met. |
| Architecture compliance | PASS | All four components updated per architecture. Component boundaries, interfaces, and ADR decisions followed. DEFAULT_STALENESS_THRESHOLD_SECS retained with correct comment. |
| Knowledge stewardship compliance | PASS | Tester report has Queried: and Stored: entries. Supersession chain #179 → #4192 → #4199 intact. AC-12 fully satisfied. |

## Detailed Findings

### Risk Mitigation Proof

**Status**: WARN

**Evidence**:

All 10 risks have substantive mitigating evidence. The static analysis and build gate assertions in the RISK-COVERAGE-REPORT are accurate and independently verifiable:

- R-01: `lambda_specific_three_dimensions` exists and passes. `lambda_single_dimension_deviation` exists and passes. Both triangulate argument positional correctness.
- R-02: Build succeeds. `grep` for removed fields in `mcp/response/mod.rs` returns zero matches. Non-default fixture site `make_coherence_status_report()` (0.8200/15) verified absent.
- R-03: `DEFAULT_STALENESS_THRESHOLD_SECS` at coherence.rs line 13 with comment "NOT a Lambda input — the Lambda freshness dimension was removed in crt-048". Build success confirms `run_maintenance()` reference intact.
- R-04: `lambda_weight_sum_invariant` uses `(total - 1.0_f64).abs() < f64::EPSILON` at line 270. Struct constants referenced directly.
- R-05: `test_status_json_no_freshness_fields` integration test exists at test_tools.py:2913 and passed. Unit tests `test_status_json_no_freshness_keys`, `test_status_text_no_freshness_line`, `test_status_markdown_no_freshness_bullet` exist in `response/status.rs` and passed.
- R-06: See FAIL in Test Coverage Completeness section below.
- R-07: `lambda_renormalization_without_embedding` non-trivial case exercises `0.8*(0.46/0.77)+0.6*(0.31/0.77)`. `lambda_embedding_excluded_specific` and `lambda_renormalization_partial` present and passing.
- R-08: Build gate covers From<&StatusReport> impl. JSON absence test covers output layer.
- R-09: Build gate. `recommendations_below_threshold_all_issues` asserts max 3 (not 4) recommendations.
- R-10: Entry #179 status=deprecated, superseded_by=4192. Chain: #4192 (deprecated, superseded_by=4199) → #4199 (active). Entry #4199 contains all four required data points: exact weight literals (0.46, 0.31, 0.23), original ratio (2:1.33:1 from 0.30:0.20:0.15), rationale (crt-036 cycle-based retention invalidates wall-clock freshness), reference to GH #520.

**Issue**: The RISK-COVERAGE-REPORT names `coherence_by_source_uses_three_dim_lambda` as a passing test for R-06. This test does not exist. The tester report also claims it passed. This is a false positive in the coverage report.

---

### Test Coverage Completeness

**Status**: FAIL

**Evidence**:

The test plan (`product/features/crt-048/test-plan/status.md`, lines 71–107) requires a unit test named `coherence_by_source_uses_three_dim_lambda` in `services/status.rs`. The test plan specifies:

- Construct a synthetic set of `EntryRecord` instances grouped by two `trust_source` values
- Assert `lambda_a > lambda_b` for source A (strong graph) vs source B (strong contradiction) using `compute_lambda()` directly with 3-dimension signature
- Purpose: detect if the `coherence_by_source` loop call site was not updated (it compiles silently if freshness was still a local variable of matching type)

Exhaustive search confirms this test does not exist:
- `grep -rn "coherence_by_source_uses_three_dim_lambda" crates/` — zero matches
- `grep -rn "three_dim_lambda" crates/` — zero matches
- `grep -rn "coherence_by_source.*test\|test.*coherence_by_source" crates/` — zero matches

The RISK-COVERAGE-REPORT table entry for R-06 reads: "`coherence_by_source_uses_three_dim_lambda` unit test" with result "PASS". This claim is false.

**Mitigation status of the underlying risk**: The implementation is correct. Both `compute_lambda(` call sites in `services/status.rs` are at lines 751 and 772, both pass 4 arguments (3 dimensions + weights), verified by grep count and direct code inspection. The `coherence_by_source` loop call at line 772 passes `embed_dim` (an `Option<f64>`) and `report.contradiction_density_score` — semantically correct, not a transposed-argument defect. The risk R-06 materialized as non-critical (implementation is correct), but the required test coverage is absent.

**Issue**: The missing test is a required artifact from the approved test plan (not an optional addition). AC-13 states: "unit or integration test for coherence_by_source output remains passing" — the test plan specifies this should be `coherence_by_source_uses_three_dim_lambda`. The test must be added.

---

### Specification Compliance

**Status**: PASS

All 14 acceptance criteria verified:

| AC-ID | Verification | Result |
|-------|-------------|--------|
| AC-01 | `grep -rn "confidence_freshness" crates/` — only absence-checking test strings | PASS |
| AC-02 | `lambda_weight_sum_invariant` uses `f64::EPSILON` guard; struct constants direct | PASS |
| AC-03 | `confidence_freshness_score()` absent from coherence.rs; build clean of dead-code warnings for freshness | PASS |
| AC-04 | `oldest_stale_age()` absent; `grep -rn "oldest_stale_age" crates/` — zero matches | PASS |
| AC-05 | `compute_lambda` signature is `(graph_quality: f64, embedding_consistency: Option<f64>, contradiction_density: f64, weights: &CoherenceWeights) -> f64`; build succeeds | PASS |
| AC-06 | `grep -rn "confidence_freshness\|stale_confidence_count" mcp/` — only in absence-checking test strings; integration test passed | PASS |
| AC-07 | `lambda_all_ones` passes: `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)` = 1.0 | PASS |
| AC-08 | `lambda_renormalization_without_embedding` Case 1 passes: `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS)` = 1.0 | PASS |
| AC-09 | `generate_recommendations` has 5-param signature; stale-confidence branch deleted; build succeeds | PASS |
| AC-10 | 2819 unit tests passed; 3 pre-existing failures in unrelated `uds::listener::tests` | PASS |
| AC-11 | `DEFAULT_STALENESS_THRESHOLD_SECS` at coherence.rs:13 with comment "NOT a Lambda input — the Lambda freshness dimension was removed in crt-048"; `run_maintenance()` compiles | PASS |
| AC-12 | Entry #179 deprecated with superseded_by=4192; chain leads to active entry #4199 with all 4 required data points | PASS |
| AC-13 | Exactly 2 `compute_lambda(` call sites in status.rs (lines 751, 772), both 4-argument; unit test `coherence_by_source_uses_three_dim_lambda` MISSING (see FAIL above) | PARTIAL — code correct, test absent |
| AC-14 | `cargo build --workspace` succeeds; all 8 fixture sites in `mcp/response/mod.rs` updated | PASS |

Non-functional requirements:

- NFR-01 (Lambda in [0.0, 1.0]): `.clamp(0.0, 1.0)` on both paths of `compute_lambda()`. PASS.
- NFR-04 (epsilon comparison): `lambda_weight_sum_invariant` uses `f64::EPSILON`. PASS.
- NFR-05 (no regression): 2819 passed; only pre-existing 3 failures unrelated to crt-048. PASS.
- NFR-06 (breaking JSON documented): PR must list removed keys. Process check — not verified in gate scope, but the integration test confirms field absence at wire level.

Retention requirements FR-10 through FR-14: all satisfied. `DEFAULT_STALENESS_THRESHOLD_SECS` retained, `load_active_entries_with_tags()` retained, `coherence_by_source` logic retained, config untouched, timestamps untouched.

---

### Architecture Compliance

**Status**: PASS

**Evidence**:

- Component A (`infra/coherence.rs`): `CoherenceWeights` has exactly 3 fields. `DEFAULT_WEIGHTS` = {graph: 0.46, embed: 0.23, contradiction: 0.31}. `compute_lambda()` is a 4-parameter pure function. `confidence_freshness_score()` and `oldest_stale_age()` deleted. `generate_recommendations()` has 5 parameters (2 removed).
- Component B (`services/status.rs`): Both `compute_lambda()` call sites use 3-dimension signature. `generate_recommendations()` call at line 784 uses 5 arguments. `load_active_entries_with_tags()` retained (4 matches in status.rs).
- Component C (`mcp/response/status.rs`): `StatusReport` and `StatusReportJson` contain no `confidence_freshness_score` or `stale_confidence_count` fields. All three format branches (text, markdown, JSON) omit freshness data. `From<&StatusReport>` impl does not reference removed fields (build gate confirms).
- Component D (`mcp/response/mod.rs`): All 8 fixture sites updated (zero grep matches for removed fields). Non-default fixture `make_coherence_status_report()` (0.8200/15) verified absent.
- ADR-001 (3-dimension weights) and ADR-002 (constant retention) followed exactly.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

Tester agent report (`agents/crt-048-agent-7-tester-report.md`) contains `## Knowledge Stewardship` section with:
- Queried: `mcp__unimatrix__context_briefing` — entries #4193, #4199, #4189 retrieved
- Stored: "nothing novel to store — no new reusable patterns emerged; existing test conventions already documented"

Reason given for not storing. PASS.

ADR supersession chain intact: #179 (deprecated) → #4192 (deprecated) → #4199 (active). AC-12 fully satisfied.

---

## Integration Test Validation

- `pytest -m smoke`: 23/23 PASS. Gate cleared.
- `test_confidence.py`: 13 passed, 1 xfailed (pre-existing GH#405, unrelated to crt-048). Marker has GH Issue reference. PASS.
- `test_tools.py`: 117 passed, 2 xfailed (pre-existing with GH Issue references), 0 failed. New test `test_status_json_no_freshness_fields` present at test_tools.py:2913 and passed. PASS.
- RISK-COVERAGE-REPORT includes integration test counts (23 smoke, 13 confidence, 117 tools). PASS.
- No integration tests deleted or commented out. PASS.
- All xfail markers have corresponding GH Issue references. PASS.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing `coherence_by_source_uses_three_dim_lambda` unit test | rust-dev (re-run for status.rs) | Add the test specified in test-plan/status.md lines 71–107. The test calls `compute_lambda(0.9, None, 0.3, &DEFAULT_WEIGHTS)` and `compute_lambda(0.3, None, 0.9, &DEFAULT_WEIGHTS)` and asserts `lambda_a > lambda_b`. Place in `services/status.rs` test module. Also update RISK-COVERAGE-REPORT.md to replace the fabricated test reference with accurate evidence. |

The implementation itself is correct. The `coherence_by_source` loop at line 772 uses the correct 3-dimension signature. This is a missing test artifact, not a code defect.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the pattern of a coverage report claiming a passing test that does not exist is a one-off artifact quality issue, not a cross-feature systemic pattern. The existing lesson-learned entries on tautological assertions (#4177) and call-site audits (#2398) cover the adjacent space adequately.
