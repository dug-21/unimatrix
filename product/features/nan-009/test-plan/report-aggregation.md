# Test Plan: Report Aggregation (`eval/report/aggregate.rs`)

Component files: `aggregate.rs` (new function `compute_phase_stats`), optionally new
`aggregate_phase.rs` if `aggregate.rs` approaches 500 lines.

All tests are sync `#[test]` (report module is entirely synchronous — NFR-03, Constraint 4).

---

## Risk Coverage

| Risk | Tests in this component |
|------|------------------------|
| R-01 (Critical) | `test_compute_phase_stats_null_bucket_label` |
| R-07 (Med) | `test_compute_phase_stats_all_null_returns_empty` |
| R-08 (Med) | `test_compute_phase_stats_null_bucket_sorts_last` |
| EC-01 | `test_compute_phase_stats_empty_results_returns_empty` |

---

## Helper: `make_result_with_phase`

To keep test bodies concise, a minimal helper is needed in `report/tests.rs` alongside
the existing `make_scenario_result` and `make_scenario_result_with_metrics` helpers:

```rust
fn make_result_with_phase(
    id: &str,
    phase: Option<&str>,
    p_at_k: f64,
    mrr: f64,
    cc_at_k: f64,
    icd: f64,
) -> ScenarioResult {
    // ... builds a ScenarioResult with the given phase and metric values
}
```

This helper must construct the report-module's local `ScenarioResult` type (not the
runner type).

---

## Unit Tests

### `test_compute_phase_stats_null_bucket_label` (R-01)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
let results = vec![
    make_result_with_phase("s1", None, 0.7, 0.6, 0.5, 1.0),
];
```

**Act**: `let stats = compute_phase_stats(&results);`

**Assert**:
- `stats.len() == 1`.
- `stats[0].phase_label == "(unset)"` — literal must be exactly `"(unset)"`.
- `stats[0].phase_label != "(none)"` — negative assertion to guard R-01.

**Rationale**: This test is the ground truth for the null-bucket label. Whichever
literal the test asserts is the authoritative canonical value. If the implementation
uses `"(none)"`, this test fails. If the test itself uses `"(none)"`, it contradicts
the resolved decision. The delivery agent must confirm this test uses `"(unset)"`.

---

### `test_compute_phase_stats_empty_results_returns_empty` (EC-01, FM-03)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**: `let results: Vec<ScenarioResult> = vec![];`

**Act**: `let stats = compute_phase_stats(&results);`

**Assert**: `assert!(stats.is_empty(), "empty input must produce empty output, not panic");`

**Rationale**: Guards FM-03 — `compute_phase_stats` must not panic on empty slice.

---

### `test_compute_phase_stats_all_null_returns_empty` (R-07, AC-09 item 4)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
let results = vec![
    make_result_with_phase("s1", None, 0.7, 0.6, 0.5, 1.0),
    make_result_with_phase("s2", None, 0.5, 0.4, 0.3, 0.8),
    make_result_with_phase("s3", None, 0.6, 0.5, 0.4, 0.9),
];
```

**Act**: `let stats = compute_phase_stats(&results);`

**Assert**: `assert!(stats.is_empty(),
    "all-null phases must return empty vec (section 6 omitted per AC-04)");`

**Rationale**: The spec requires section 6 to be omitted when ALL phases are null.
`compute_phase_stats` achieves this by returning an empty vec rather than a vec with
a single `"(unset)"` row. The renderer then skips the section entirely. Both layers
must be tested independently (see `test_render_phase_section_absent_when_stats_empty`
in report-rendering.md for the renderer side).

---

### `test_compute_phase_stats_null_bucket_sorts_last` (R-08, AC-05)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
let results = vec![
    make_result_with_phase("s1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
    make_result_with_phase("s2", Some("design"),   0.7, 0.6, 0.4, 0.9),
    make_result_with_phase("s3", None,             0.5, 0.4, 0.3, 0.7),
    make_result_with_phase("s4", Some("bugfix"),   0.6, 0.5, 0.3, 0.8),
    make_result_with_phase("s5", Some("delivery"), 0.9, 0.8, 0.6, 1.1),
];
```

This gives 3 named phases (bugfix, delivery, design) plus one null bucket.

**Act**: `let stats = compute_phase_stats(&results);`

**Assert**:
- `stats.len() == 4` — one entry per distinct phase (including null bucket).
- `stats.last().unwrap().phase_label == "(unset)"` — null bucket is last.
- Named phases in alphabetical order:
  - `stats[0].phase_label == "bugfix"`.
  - `stats[1].phase_label == "delivery"`.
  - `stats[2].phase_label == "design"`.
- Counts correct:
  - `stats[1].scenario_count == 2` (two "delivery" results).
  - `stats[3].scenario_count == 1` (one null result).
- Mean P@K for "delivery": `(0.8 + 0.9) / 2 = 0.85`:
  `assert!((stats[1].mean_p_at_k - 0.85).abs() < 1e-9)`.

**Rationale**: Three named phases are required to confirm sort stability (R-08).
The `(` character (ASCII 40) sorts before `b` (ASCII 98) in lexicographic order, so
a naive `sort_by_key` would place `"(unset)"` first. The test confirms the null bucket
is last regardless. Mean correctness for one group validates the aggregation arithmetic.

---

### `test_compute_phase_stats_mixed_phases_correct_grouping` (AC-09 item 3)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
```rust
let results = vec![
    make_result_with_phase("s1", Some("delivery"), 1.0, 0.8, 0.6, 1.2),
    make_result_with_phase("s2", Some("delivery"), 0.0, 0.2, 0.4, 0.8),
    make_result_with_phase("s3", Some("design"),   0.6, 0.6, 0.3, 1.0),
];
```

**Act**: `let stats = compute_phase_stats(&results);`

**Assert**:
- `stats.len() == 2` — "delivery" and "design" only (no null bucket).
- "delivery" group: `scenario_count == 2`, `mean_p_at_k == 0.5`, `mean_mrr == 0.5`,
  `mean_cc_at_k == 0.5`, `mean_icd == 1.0`.
- "design" group: `scenario_count == 1`, `mean_p_at_k == 0.6`, `mean_mrr == 0.6`.

**Rationale**: Validates grouping logic and mean arithmetic across two groups with
different sizes. Distinct from `null_bucket_sorts_last` which focuses on ordering.
