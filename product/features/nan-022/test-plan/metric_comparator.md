# Test Plan: MC — `harness/metric_comparator.py` (consumed verbatim)

Covers **R-14 (Low)** — the analytics comparator is CONSUMED, not re-authored (AC-04 forbids
re-proving/re-authoring the proven nan-021 comparator). The only net-new test surface is the
ADAPTER (`MetricVectorComparator` in `parity_comparator.py`) — it must wrap
`compare_metric_vectors`/`EXCLUDED` WITHOUT altering the logic. The consumed surface itself
(`compare_metric_vectors`, `EXCLUDED`, `field_by_field_record`) keeps its existing nan-021
tests in `test_parity_workload.py` unchanged.

Surface NOT under new test (consumed verbatim — must not change):
- `compare_metric_vectors(mv_https, mv_uds) -> list[diff]`
- `EXCLUDED`, `EXCLUSION_JUSTIFICATIONS`, `UNIVERSAL_FIELDS`, `AT_RISK_FIELDS`
- `field_by_field_record(...)`, `ParityMismatch`

Surface under new test (the adapter, defined in `parity_comparator.py`):
- `MetricVectorComparator(DimensionComparator)` wrapping the consumed logic

Tier: **A (off-Docker unit)** — golden synthetic MetricVector pairs (the `test_parity_workload.py`
`_mv`/`_universal` helpers). File: assertions in `suites/test_parity_comparator.py`.

## Unit Test Expectations

### Adapter is behavior-identical (R-14 scenario 1, AC-04)
- `test_metric_vector_comparator_golden_diff_identical_to_consumed`: a golden nan-021 MetricVector
  pair (one clean, one with a known non-excluded diff) produces the IDENTICAL diff list through
  `MetricVectorComparator.compare(...)` as a direct call to `compare_metric_vectors(...)`. **The
  load-bearing AC-04 guard** — the adapter delegates, it does not re-implement.
- `test_metric_vector_comparator_excluded_is_consumed_excluded`: `MetricVectorComparator.EXCLUDED`
  IS the consumed `metric_comparator.EXCLUDED` object (identity/equality), not a re-declared copy —
  a re-declaration would re-introduce the #5302 drift the framework exists to prevent.
- `test_metric_vector_comparator_raises_consumed_parity_mismatch`: a non-excluded diff raises the
  CONSUMED `ParityMismatch` (same type, same field+values+leg payload) through the adapter.

### Consumed surface regression (no alteration)
- The existing nan-021 `test_parity_workload.py` tests over `compare_metric_vectors`/`EXCLUDED`/
  `field_by_field_record` MUST still pass unchanged — assert the consumed logic was not edited
  by the K2 framework introduction.

### Informs edges + phase signal (R-11 — net-new analytics sub-surface, off-Docker shape)
- `test_analytics_excluded_edge_wall_clock_justified`: any wall-clock/ordering edge field (e.g.
  edge creation timestamp) is a JUSTIFIED `EXCLUDED` entry; the edge-ID SET itself is NOT excluded
  (R-11 scenario 3). The live barrier-gated edge-set + phase compare is in `test_https_uds_parity.md`.

## Coverage Requirement (from R-14)
The adapter is behavior-identical to the consumed nan-021 comparator on golden inputs (same diff
list, same EXCLUDED object, same ParityMismatch); the consumed `compare_metric_vectors` logic is
NOT altered (existing tests unchanged); net-new Informs-edge wall-clock fields are justified
exclusions while edge IDs are compared exactly (R-11).
