# Test Plan: Distribution Gate Aggregation (`eval/report/aggregate/distribution.rs`)

Component 4 of 7.

---

## Scope

New submodule `eval/report/aggregate/distribution.rs` (within the pre-split `aggregate/`
module directory). Defines `MetricGateRow`, `DistributionGateResult`, and
`check_distribution_targets`.

`check_distribution_targets` accepts a candidate `AggregateStats` and `DistributionTargets`,
reads `mean_cc_at_k`, `mean_icd`, `mean_mrr` from `AggregateStats`, compares each against
the corresponding target using `>=`, and returns a fully populated `DistributionGateResult`.

All tests are in `eval/report/tests_distribution_gate.rs`.

---

## Pre-Split Prerequisite (R-01)

Before any code is added to this component, the pre-split must be verified:
- `eval/report/aggregate/mod.rs` exists and re-exports all previously public symbols from
  the original `aggregate.rs`.
- `eval/report/aggregate.rs` no longer exists (replaced by `aggregate/mod.rs`).
- `cargo build` passes with zero errors or warnings after the pre-split.
- Line count of `aggregate/mod.rs` is <= 500.

This is a gate-3b static check, not a unit test.

---

## Unit Test Expectations

### `test_check_distribution_targets_all_pass` (AC-13, R-05, R-08)

Primary happy-path test. Validates all three metrics pass and correct field values are returned.

- Arrange:
  ```rust
  let stats = AggregateStats {
      mean_cc_at_k: 0.65,
      mean_icd: 1.35,
      mean_mrr: 0.42,
      // other fields at defaults
  };
  let targets = DistributionTargets {
      cc_at_k_min: 0.60,
      icd_min: 1.20,
      mrr_floor: 0.35,
  };
  ```
- Act: `let result = check_distribution_targets(&stats, &targets);`
- Assert:
  - `result.cc_at_k.target == 0.60`
  - `result.cc_at_k.actual == 0.65`
  - `result.cc_at_k.passed == true`
  - `result.icd.target == 1.20`
  - `result.icd.actual == 1.35`
  - `result.icd.passed == true`
  - `result.mrr_floor.target == 0.35`
  - `result.mrr_floor.actual == 0.42`
  - `result.mrr_floor.passed == true`
  - `result.diversity_passed == true`
  - `result.mrr_floor_passed == true`
  - `result.overall_passed == true`

Explicit field value assertions on `actual` fields validate R-08 (field names correct in
`AggregateStats`).

---

### `test_check_distribution_targets_cc_at_k_fail` (AC-13, R-05)

- Arrange: `stats.mean_cc_at_k = 0.55` (below `cc_at_k_min = 0.60`); ICD and MRR pass.
- Assert:
  - `result.cc_at_k.passed == false`
  - `result.icd.passed == true`
  - `result.mrr_floor.passed == true`
  - `result.diversity_passed == false` (cc_at_k failed)
  - `result.mrr_floor_passed == true`
  - `result.overall_passed == false`

---

### `test_check_distribution_targets_icd_fail` (AC-13, R-05)

- Arrange: `stats.mean_icd = 1.10` (below `icd_min = 1.20`); CC@k and MRR pass.
- Assert:
  - `result.cc_at_k.passed == true`
  - `result.icd.passed == false`
  - `result.diversity_passed == false` (icd failed)
  - `result.mrr_floor_passed == true`
  - `result.overall_passed == false`

---

### `test_check_distribution_targets_mrr_floor_fail` (AC-13, R-05, R-14)

Critical for R-14. Uses values where candidate MRR != baseline MRR so the wrong source
is detectable.

- Arrange:
  - `stats.mean_cc_at_k = 0.65` (pass, above 0.60)
  - `stats.mean_icd = 1.35` (pass, above 1.20)
  - `stats.mean_mrr = 0.40` (candidate MRR — fails `mrr_floor = 0.35`? No, 0.40 >= 0.35 passes)

  To test MRR floor fail: `stats.mean_mrr = 0.30`, `mrr_floor = 0.35`.
  Baseline MRR would be 0.60 — this value must NOT appear in `mrr_floor.actual`.
- Assert:
  - `result.mrr_floor.actual == 0.30` (not 0.60 — R-14 guard)
  - `result.mrr_floor.passed == false`
  - `result.diversity_passed == true`
  - `result.mrr_floor_passed == false`
  - `result.overall_passed == false`

---

## ADR-003 Four-State Coverage (R-05)

`check_distribution_targets` must produce all four distinct states. The four tests above
cover:
1. `diversity_passed=true, mrr_floor_passed=true` → `test_check_distribution_targets_all_pass`
2. `diversity_passed=false, mrr_floor_passed=true` → `test_check_distribution_targets_cc_at_k_fail`
3. `diversity_passed=true, mrr_floor_passed=false` → `test_check_distribution_targets_mrr_floor_fail`
4. `diversity_passed=false, mrr_floor_passed=false` → covered by `test_distribution_gate_distinct_failure_modes`

---

## Boundary Condition Assertions

| Condition | Assertion |
|-----------|-----------|
| `mean_cc_at_k == cc_at_k_min` (equal boundary) | `passed == true` (uses `>=`, not `>`) |
| `mean_icd == icd_min` (equal boundary) | `passed == true` |
| `mean_mrr == mrr_floor` (equal boundary) | `passed == true` |
| `cc_at_k_min = 0.0` | `passed == true` for any non-negative actual |

These boundary assertions may be inline within the main four tests or as post-conditions.

---

## `check_distribution_targets` Signature Invariant

The function signature accepts `stats: &AggregateStats` (candidate stats) only.
`baseline_stats` is not a parameter. This is enforced by the type signature itself — the
compiler rejects any attempt to pass baseline stats as `stats`. The R-14 test with
`mrr_floor.actual == 0.30 != 0.60` is the runtime guard if the value is sourced incorrectly.

---

## Risks Covered

| Risk | Test |
|------|------|
| R-05 (overall_passed conflation) | All four gate-state tests; ADR-003 four-state coverage |
| R-08 (AggregateStats fields renamed) | `test_check_distribution_targets_all_pass` explicit `actual` assertions |
| R-14 (mrr_floor compared against baseline) | `test_check_distribution_targets_mrr_floor_fail` fixture with baseline_mrr != candidate_mrr |
| R-01 (pre-split order) | Gate-3b static: aggregate.rs renamed before distribution.rs created |
