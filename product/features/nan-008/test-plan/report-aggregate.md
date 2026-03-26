# Test Plan: report/aggregate.rs

## Component Responsibility

Accumulates per-scenario results into per-profile summary statistics.
`compute_aggregate_stats` gains four new accumulators and divides by scenario count.
New helper `compute_cc_at_k_scenario_rows` produces a sorted `Vec<CcAtKScenarioRow>`
for the Distribution Analysis section.

## Risks Covered

R-11 (aggregate divides by wrong count), R-12 (sort direction inverted).

---

## Tests in `report/tests.rs`

### `test_aggregate_stats_cc_at_k_mean` (R-11)

```
Arrange:
  r1 = make_scenario_result_with_metrics("s1", "q1",
       baseline_cc: 0.4, candidate_cc: 0.6,
       baseline_icd: 0.5, candidate_icd: 0.8)
  r2 = make_scenario_result_with_metrics("s2", "q2",
       baseline_cc: 0.6, candidate_cc: 0.8,
       baseline_icd: 0.7, candidate_icd: 1.0)
  r3 = make_scenario_result_with_metrics("s3", "q3",
       baseline_cc: 0.2, candidate_cc: 0.4,
       baseline_icd: 0.3, candidate_icd: 0.6)

  // baseline mean_cc_at_k = (0.4 + 0.6 + 0.2) / 3 = 0.4
  // candidate mean_cc_at_k = (0.6 + 0.8 + 0.4) / 3 ≈ 0.6

Act:
  stats = compute_aggregate_stats(&[r1, r2, r3])

Assert:
  baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap()
  (baseline.mean_cc_at_k - 0.4).abs() < 1e-9

  candidate = stats.iter().find(|s| s.profile_name == "candidate").unwrap()
  (candidate.mean_cc_at_k - 0.6).abs() < 1e-9
```

This test catches the R-11 failure: if `mean_cc_at_k` were divided by entry count
(k=5 entries) instead of scenario count (3), the values would be off by a factor of 5.

### `test_aggregate_stats_icd_mean` (R-11 symmetric)

```
Arrange: same three scenarios as above

Assert:
  baseline.mean_icd ≈ (0.5 + 0.7 + 0.3) / 3 ≈ 0.5
  candidate.mean_icd ≈ (0.8 + 1.0 + 0.6) / 3 ≈ 0.8
```

### `test_aggregate_stats_cc_at_k_delta_mean` (R-11 for delta accumulation)

```
Arrange:
  r1 = scenario with cc_at_k_delta = 0.2 (from comparison)
  r2 = scenario with cc_at_k_delta = 0.4
  r3 = scenario with cc_at_k_delta = 0.0

Act:  stats = compute_aggregate_stats(&[r1, r2, r3])

Assert:
  candidate = stats.iter().find(|s| s.profile_name == "candidate").unwrap()
  (candidate.cc_at_k_delta - (0.2 + 0.4 + 0.0) / 3.0).abs() < 1e-9
```

### `test_aggregate_stats_baseline_has_zero_cc_at_k_delta`

```
Arrange: any multi-scenario result set

Act: stats = compute_aggregate_stats(&results)

Assert:
  baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap()
  baseline.cc_at_k_delta == 0.0
  baseline.icd_delta == 0.0
```

The baseline profile has no comparison delta by definition.

---

## `compute_cc_at_k_scenario_rows` Tests

### `test_cc_at_k_scenario_rows_sort_order` (R-12)

```
Arrange:
  rows = [
      CcAtKScenarioRow { scenario_id: "s1", cc_at_k_delta:  0.1, ... },
      CcAtKScenarioRow { scenario_id: "s2", cc_at_k_delta: -0.3, ... },
      CcAtKScenarioRow { scenario_id: "s3", cc_at_k_delta:  0.5, ... },
      CcAtKScenarioRow { scenario_id: "s4", cc_at_k_delta: -0.1, ... },
      CcAtKScenarioRow { scenario_id: "s5", cc_at_k_delta:  0.2, ... },
  ]

Act:
  sorted = compute_cc_at_k_scenario_rows(&scenarios)
  // where scenarios have the cc_at_k values above

Assert:
  // Descending order by cc_at_k_delta
  sorted[0].scenario_id == "s3"   // delta = 0.5, largest positive
  sorted[1].scenario_id == "s5"   // delta = 0.2
  sorted[2].scenario_id == "s1"   // delta = 0.1
  sorted[3].scenario_id == "s4"   // delta = -0.1
  sorted[4].scenario_id == "s2"   // delta = -0.3, most negative last
```

This test directly catches R-12: if sort direction is ascending, `s2` would be first.

### `test_cc_at_k_scenario_rows_single_scenario`

```
Arrange: one scenario with cc_at_k_delta = 0.3

Act: rows = compute_cc_at_k_scenario_rows(&[scenario])

Assert:
  rows.len() == 1
  rows[0].cc_at_k_delta ≈ 0.3
```

### `test_cc_at_k_scenario_rows_empty`

```
Arrange: no scenarios

Act: rows = compute_cc_at_k_scenario_rows(&[])

Assert:
  rows.is_empty()
```

---

## Helper for Tests

The existing `make_scenario_result` helper in `report/tests.rs` must be extended
or supplemented with a version that sets `cc_at_k` and `icd` on both profiles,
and `cc_at_k_delta` / `icd_delta` on the comparison. Example signature:

```rust
fn make_scenario_result_with_metrics(
    id: &str,
    query: &str,
    baseline_p: f64, baseline_mrr: f64,
    baseline_cc: f64, baseline_icd: f64,
    candidate_p: f64, candidate_mrr: f64,
    candidate_cc: f64, candidate_icd: f64,
) -> ScenarioResult
```

This allows all aggregate tests to use concrete, manually verifiable input values.

---

## NFR Checks (code review)

- `compute_aggregate_stats` divides `cc_at_k_sum` and `icd_sum` by `count` (scenario count),
  not by entry count
- `compute_cc_at_k_scenario_rows` sorts by `cc_at_k_delta` descending
  (largest positive first, most negative last)
- No async or tokio in aggregate.rs
- `CcAtKScenarioRow` fields match the architecture spec:
  `scenario_id: String`, `query: String`, `baseline_cc_at_k: f64`,
  `candidate_cc_at_k: f64`, `cc_at_k_delta: f64`
