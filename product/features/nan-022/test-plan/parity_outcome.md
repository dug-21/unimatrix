# Test Plan: K4 — `harness/parity_outcome.py`

Covers **R-07 (High)**, the roll-up half of **R-02 (Critical defense-in-depth)**, and AC-08.
The four-valued outcome model + the ordered classifier (INFRA→INTRA→PARITY) + the roll-up
exit-code truth table. The classifier ORDER is the structural separation that keeps an infra
hang or an intra-transport flake from masquerading as a cross-transport parity verdict (SR-04).
The most insidious false-GREEN is a real cross-leg divergence being reclassified as
INTRA-NONDET and silently dropped — that negative test is load-bearing.

Surface under test:
- `Outcome` enum: `PARITY_PASS`, `PARITY_FAIL`, `INFRA_ERROR`, `INTRA_TRANSPORT_NONDETERMINISM`
- `DimensionResult(dimension, outcome, diffs, detail)`
- `classify_dimension(dim, cap_uds, cap_https) -> DimensionResult`  (order: INFRA → INTRA → compare)
- `intra_transport_stable(cap_a, cap_b, *, tolerance) -> bool`
- `rollup(results: list[DimensionResult]) -> (verdict, exit_code)`

Tier: **A (off-Docker unit)** — synthetic capture dicts + stub `Dimension`/comparator. File:
`suites/test_parity_outcome.py`.

## Unit Test Expectations

### Classifier order (R-07 scenario 3 — INFRA → INTRA → PARITY)
- `test_classify_dimension_infra_first_before_any_compare`: a capture that is missing/empty/null
  (non-PreCompact) → `INFRA_ERROR` is returned BEFORE the comparator or intra-check runs. Assert
  the comparator is never invoked (e.g. a raising stub comparator does not raise).
- `test_classify_dimension_intra_before_cross_compare`: an `intra_transport_check=True` dimension
  whose one leg's two captures diverge within the stable prefix → `INTRA_TRANSPORT_NONDETERMINISM`;
  assert the cross-leg comparator does NOT run (a stub comparator is not called).
- `test_classify_dimension_order_proven_explicitly`: parametrized — for a capture that is
  simultaneously infra-bad AND intra-unstable AND cross-divergent, the result is `INFRA_ERROR`
  (the earliest applicable class wins). Proves the order is INFRA→INTRA→PARITY, not any other.

### Cross-divergence must NEVER escape to INTRA (R-07 scenario 3 — load-bearing false-GREEN guard)
- `test_classify_dimension_two_intra_stable_legs_cross_divergent_is_parity_fail`: BOTH legs
  intra-stable (each leg's two captures agree within tolerance) but the cross-leg comparator finds
  a non-excluded diff → assert `PARITY_FAIL`, NEVER `INTRA_TRANSPORT_NONDETERMINISM`. **The single
  most important negative test in K4** — a real C0 defect on two stable legs cannot be silently
  dropped into the intra bucket.
- `test_classify_dimension_one_leg_intra_unstable_classed_intra`: one leg intra-stable, the other
  intra-unstable → `INTRA_TRANSPORT_NONDETERMINISM` (not a half-compare); from Edge Cases.

### intra_transport_stable (R-07 scenarios 1–2,4)
- `test_intra_transport_stable_tail_churn_only_is_stable`: two captures differing only in the
  tolerated tail → `True` (proceeds to cross-leg compare).
- `test_intra_transport_stable_in_prefix_divergence_is_unstable`: two captures differing within
  the stable prefix → `False` (the leg is intra-unstable).
- `test_intra_transport_stable_uses_k3_tolerance_single_sourced`: assert the intra-diff uses the
  SAME `ranking_tolerance.ranking_parity` as the cross-leg compare — no second tolerance (SR-03).

### PARITY-PASS / PARITY-FAIL (happy/defect)
- `test_classify_dimension_clean_modulo_excluded_is_parity_pass`: two intra-stable captures, cross
  comparator clean modulo the closed EXCLUDED set → `PARITY_PASS`, empty `diffs`.
- `test_classify_dimension_non_excluded_diff_is_parity_fail`: a non-excluded diff → `PARITY_FAIL`,
  `diffs` carries field + both values + leg; `detail` references the evidence record (AC-10 path).

### Roll-up exit-code truth table (R-02 scenario 3, AC-08 — the gate verdict)
Parametrized truth table — assert `rollup` produces the exact `(verdict, exit_code)`:

| Dimension results | verdict | exit_code |
|-------------------|---------|-----------|
| all `PARITY_PASS` | GREEN | 0 |
| any `PARITY_FAIL` (rest PASS) | RED | non-zero parity-RED code |
| any `INFRA_ERROR` (no PARITY_FAIL) | ERROR | DISTINCT error code (≠ parity-RED, ≠ 0) |
| `INFRA_ERROR` + `PARITY_FAIL` together | ERROR or RED per §4 precedence | assert deterministic, documented |
| any `INTRA_TRANSPORT_NONDETERMINISM` (rest PASS) | GREEN | 0 (does NOT redden) |
| `INTRA_NONDET` + a `PARITY_FAIL` | RED | parity-RED (intra does not mask a real RED) |

- `test_rollup_green_iff_all_parity_pass`
- `test_rollup_any_parity_fail_is_red`
- `test_rollup_infra_error_distinct_exit_not_parity_red`: assert the INFRA exit code is DISTINCT
  from the parity-RED code and from 0 (R-02: INFRA never counted as RED, never green).
- `test_rollup_intra_nondet_does_not_redden`: a lone INTRA-NONDET → GREEN; it is recorded in the
  result detail and routed to a filed bug (GH#746) but does not redden the gate.
- `test_rollup_intra_nondet_does_not_mask_real_red`: INTRA-NONDET alongside a PARITY-FAIL → RED.

### Determinism floor (NFR-6)
- `test_classify_dimension_exact_compare_for_non_ranking_dims`: for a non-D1/D4 dimension
  (behavioral, isolation), assert the comparison is EXACT (no float/ranking tolerance applied) —
  the `intra_transport_check=False` dims never invoke `ranking_parity`.

## Coverage Requirement (from R-07 + R-02)
Classifier order INFRA→INTRA→PARITY proven explicitly; a cross-leg divergence on two
intra-stable legs can NEVER be reclassified as INTRA-NONDET; intra and cross compare use one
K3 tolerance; the roll-up never converts INFRA-ERROR into a parity RED and surfaces the
transport-health detail. The full exit-code truth table is asserted off-Docker before any tag.
