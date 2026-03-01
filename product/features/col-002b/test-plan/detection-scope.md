# Test Plan: detection-scope

## Component: 5 scope hotspot rules in `detection/scope.rs`

## Test Module: `#[cfg(test)] mod tests` within `scope.rs`

### SourceFileCountRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_source_file_count_fires` | Write 7 distinct .rs files | Finding with measured=7 |
| `test_source_file_count_below` | Write 5 .rs files | No finding |
| `test_source_file_count_dedup` | Write same .rs file 3 times | Counted as 1 |
| `test_source_file_count_non_rs` | Write 10 .md files | No finding (only .rs counts) |
| `test_source_file_count_empty` | Empty input | No findings |
| `test_source_file_count_mixed` | 4 .rs + 20 .md | No finding (only 4 .rs) |

Risk coverage: R-01, R-12 (file_path extraction from Write input)

### DesignArtifactCountRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_design_artifact_fires` | Write/Edit 26 files under `product/features/` | Finding |
| `test_design_artifact_below` | 20 files under product/features/ | No finding |
| `test_design_artifact_outside` | 30 files NOT under product/features/ | No finding |
| `test_design_artifact_dedup` | Same artifact path edited 5 times | Counted once |
| `test_design_artifact_empty` | Empty input | No findings |

Risk coverage: R-01, R-12

### AdrCountRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_adr_count_fires` | Write 4 ADR-*.md files | Finding |
| `test_adr_count_below` | Write 2 ADR files | No finding |
| `test_adr_count_non_adr` | Write files not matching ADR-* | No finding |
| `test_adr_count_empty` | Empty input | No findings |

Risk coverage: R-01, R-12

### PostDeliveryIssuesRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_post_delivery_fires` | TaskUpdate "completed" at ts=5000. `gh issue create` at ts=6000 | Finding |
| `test_post_delivery_before_completion` | `gh issue create` at ts=4000, completion at ts=5000 | No finding (before boundary) |
| `test_post_delivery_no_boundary` | `gh issue create` but no TaskUpdate | No finding (no boundary) |
| `test_post_delivery_empty` | Empty input | No findings |
| `test_post_delivery_multiple` | 3 `gh issue create` after completion | Finding with measured=3 |

Risk coverage: R-01, R-09 (completion boundary), R-12

### PhaseDurationOutlierRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_phase_outlier_no_history` | PhaseDurationOutlierRule::new(None) | detect() returns empty |
| `test_phase_outlier_insufficient_history` | History with 2 vectors (< 3) | detect() returns empty |
| `test_phase_outlier_with_baselines` | History with 3+ vectors for phase "3a" | Rule constructed with baselines |
| `test_phase_outlier_empty_records` | Empty input | No findings |

Note: The actual phase duration comparison against baselines is tested in the baseline component. The PhaseDurationOutlierRule's detect(records) may return empty since it cannot access the current MetricVector through the trait interface. Phase duration outlier detection is covered by `compare_to_baseline()` tests in the baseline component.

Risk coverage: R-03 (mismatched phase names), R-01
