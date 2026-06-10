# Test Plan — Report extensions (`find_regressions` trust + cost)

**Component**: `eval/report/aggregate/mod.rs` (`find_regressions` extended), `eval/runner/output.rs` (`ProfileResult` gains `cost_tokens`, `trust`), `eval/runner/{replay,metrics}.rs` (call evaluators), report rendering.
**Wave**: 1. **Primary risks**: R-12 (trust not OR-folded, Med), R-17 (exit-semantics regression, Low).

## Unit / integration test expectations

### R-12 — trust regression OR-folded correctly — AC-02/03
- `test_trust_flip_registers_regression`: baseline satisfies a forbidden-set/rank-below assertion, candidate **violates** it ⇒ appears in the **Section 5 regression list** with the existing fail-in-body-only semantics.
- `test_trust_no_flip_no_regression`: both baseline and candidate satisfy ⇒ **no** regression recorded.
- `test_trust_or_composition_with_relevance`: a candidate that **holds trust but regresses MRR** is still flagged (trust pass does not mask a relevance regression); and the inverse — a candidate that holds MRR but flips trust is flagged. Proves OR-extension, not AND-masking, mirroring the existing `mrr < baseline.mrr || p_at_k < baseline.p_at_k` semantics.

### Cost in regression block (advisory, ε=0.0) — AC-09
- `test_cost_growth_reported_advisory`: a candidate whose `cost_tokens` exceeds baseline by **any** amount (delta > 0.0, ε=0.0 LOCKED) is **listed** in the human-reviewed regression block.
- `test_cost_growth_blocks_nothing`: cost growth is advisory — it does **not** change the gate verdict / exit code (FR-12a).

### R-17 — exit-code invariance (the #3524/#2610 lineage) — AC-09
- `test_eval_report_exit_code_unchanged_with_trust_regression`: with a trust regression present, `eval report` exit code is **unchanged** (failures reported in body, not via exit code).
- `test_eval_report_exit_code_unchanged_with_cost_growth`: same for cost growth.
- `test_existing_report_tests_green`: the full existing report test suite passes unchanged (NFR-02).

## Serde / dual-direction (#3557) — ProfileResult new fields
- Producer side (`runner/output.rs`): `cost_tokens` and `trust` serialized; round-trip with **non-trivial** values (cost != 0, a populated `TrustOutcome` with a violation).
- Consumer side (report deserialize): new fields deserialize; backward-compat — a pre-nan-018 `ProfileResult` JSON missing the fields deserializes via `#[serde(default)]` (the #3526 dual-type-copy boundary, the #3548 named-backward-compat-test lesson).
- `test_profile_result_cost_trust_roundtrip_nontrivial` and `test_report_backward_compat_pre_nan018_json` (a **named** missing-field deserialization test — #3548 requires the named test, not just structural `#[serde(default)]`).

## AC-04 — correlated surfacing
- `test_report_surfaces_trust_alongside_p_at_k`: one report contains trust outcomes AND P@5/MRR for the **same scenarios in one section** (feeds AC-14 condition 1).
- `test_report_surfaces_cost_and_k`: cost (primary) and k (secondary) both surfaced per profile.

## Boundary note
**trust/cost <-> report aggregation** is the R-12/R-17 seam: `TrustOutcome` + `cost_tokens` flow from `run_single_profile` into `find_regressions`; OR-composition + exit-code convention are the fragile points. **Baseline (first-profile) selection must sort keys** (#2610 lineage) — `test_baseline_selection_sorts_profile_keys`: the correlated report must not depend on HashMap iteration order.
