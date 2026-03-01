# Test Plan: baseline

## Component: Baseline computation and comparison in `baseline.rs`

## Test Module: `#[cfg(test)] mod tests` within `baseline.rs`

### Helper

```rust
fn make_mv(tool_calls: u64, duration: u64, phases: &[(&str, u64, u64)]) -> MetricVector
```

Creates a MetricVector with specified universal metrics and phases.

### compute_baselines Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_baselines_minimum_three` | Pass 3 MVs with total_tool_calls [10, 20, 30] | Some(baselines), mean=20.0, verify stddev |
| `test_baselines_two_returns_none` | Pass 2 MVs | None |
| `test_baselines_one_returns_none` | Pass 1 MV | None |
| `test_baselines_zero_returns_none` | Pass empty slice | None |
| `test_baselines_identical_values` | 3 MVs all with tool_calls=50 | mean=50.0, stddev=0.0 |
| `test_baselines_all_zeros` | 3 MVs with all zero metrics | mean=0.0, stddev=0.0 |
| `test_baselines_phase_specific` | MVs with phases "3a" (durations: 100, 200, 300) and "3b" (durations: 50, 60, 70) | Separate baselines per phase |
| `test_baselines_phase_insufficient` | Phase "3a" appears in only 2 of 4 MVs | No phase baseline for "3a" (< 3 samples) |
| `test_baselines_universal_metrics_complete` | 3 MVs with various universal fields | All universal metric names present in baselines |
| `test_baselines_no_nan_inf` | 3 MVs with edge case values (0, MAX, etc.) | No NaN/Inf in any BaselineEntry |

Risk coverage: R-02 (NaN/Inf), AC-08, AC-09, AC-11

### compare_to_baseline Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_comparison_outlier` | BaselineEntry(mean=100, stddev=20), current=140 | is_outlier=true, status=Outlier (140 > 100+30=130) |
| `test_comparison_normal` | BaselineEntry(mean=100, stddev=20), current=120 | is_outlier=false, status=Normal (120 < 130) |
| `test_comparison_no_variance` | BaselineEntry(mean=50, stddev=0), current=50 | is_outlier=false, status=NoVariance |
| `test_comparison_no_variance_different` | BaselineEntry(mean=50, stddev=0), current=60 | is_outlier=false, status=NoVariance |
| `test_comparison_new_signal` | BaselineEntry(mean=0, stddev=0), current=5 | is_outlier=false, status=NewSignal |
| `test_comparison_zero_all` | BaselineEntry(mean=0, stddev=0), current=0 | is_outlier=false, status=Normal |
| `test_comparison_phase_specific` | Phase "3a" with baseline, current has matching phase | Comparison includes phase="3a" |
| `test_comparison_no_matching_phase` | Current has phase "new-phase" not in baselines | No comparison for that phase |
| `test_comparison_no_nan_inf` | Various edge cases | Assert all f64 fields are finite |
| `test_comparison_boundary` | current = mean + 1.5*stddev exactly | is_outlier=false (> not >=) |

Risk coverage: R-02 (NaN/Inf guards), R-03 (phase name matching), AC-10

### Integration-style Tests (within unit test module)

| Test | Scenario | Expected |
|------|----------|----------|
| `test_full_pipeline` | 5 MVs as history, 1 current MV, compute + compare | Valid comparisons for all metrics |
| `test_phase_duration_outlier_via_baseline` | History with phase "3a" mean=100s, current phase "3a"=300s | Flagged as outlier via baseline comparison |

Risk coverage: AC-13 (phase duration outlier via baseline)
