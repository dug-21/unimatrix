# Test Plan: report/render.rs

## Component Responsibility

Renders aggregated data into the final Markdown report. Two changes:
1. Section 1 Summary table gains CC@k and ICD columns (and delta columns for two-profile runs).
2. Section 6 "Distribution Analysis" is appended after section 5, containing per-profile
   CC@k/ICD range tables and (for two-profile runs) top-5 improvement/degradation scenario rows.

New parameter: `render_report` gains `cc_at_k_rows: &[CcAtKScenarioRow]`.

## Risks Covered

R-02 (section-order regression), R-04 (ICD annotation absent), with indirect coverage
of R-01 via the round-trip test that calls `render_report`.

---

## Tests in `report/tests.rs`

### `test_report_summary_table_has_cc_at_k_and_icd_columns` (FR-08, AC-04)

```
Arrange:
  results with two profiles, cc_at_k and icd populated with non-zero values

Act:
  run_report(results_dir.path(), None, &out_path)
  content = read(out_path)

Assert:
  // Column headers present in Summary section
  summary_section = content[content.find("## 1. Summary").unwrap()..]
  summary_section.contains("CC@k")
  summary_section.contains("ICD")

  // Delta columns present (two-profile run)
  summary_section.contains("CC@k") && (
      summary_section.contains("Delta") || summary_section.contains("Δ") || summary_section.contains("delta")
  )
```

### `test_report_section_6_distribution_analysis_present` (FR-09, AC-05)

```
Arrange: two-profile result with cc_at_k values: baseline=[0.4, 0.6, 0.5], candidate=[0.6, 0.8, 0.7]
         (min/max/mean are manually verifiable)

Act: run_report(...)

Assert:
  content.contains("## 6.")
  content.contains("Distribution Analysis")

  // Per-profile range table present
  section_6 = content[content.find("## 6.").unwrap()..]
  section_6.contains("min") || section_6.contains("Min")
  section_6.contains("max") || section_6.contains("Max")
  section_6.contains("mean") || section_6.contains("Mean")

  // Top-5 improvement and degradation present for two-profile run
  section_6.contains("Improved") || section_6.contains("improved") || section_6.contains("improvement")
  section_6.contains("Degraded") || section_6.contains("degraded") || section_6.contains("degradation")
```

### `test_report_section_6_icd_annotation_contains_ln` (R-04, AC-14)

```
Arrange: two-profile result set

Act: run_report(...)

Assert:
  section_6 = content[content.find("## 6.").unwrap()..]
  section_6.contains("ln(")
```

The ICD column header or caption in section 6 must contain `ln(` to indicate the
maximum value context.

### `test_report_section_order_1_through_6` (R-02, AC-13)

```
Arrange: standard two-profile result

Act: run_report(...)

Assert:
  pos1 = content.find("## 1.").unwrap()
  pos2 = content.find("## 2.").unwrap()
  pos3 = content.find("## 3.").unwrap()
  pos4 = content.find("## 4.").unwrap()
  pos5 = content.find("## 5.").unwrap()
  pos6 = content.find("## 6.").unwrap()

  pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5 && pos5 < pos6
```

Position assertion — `contains` alone is insufficient for R-02.

### `test_report_single_profile_section_6_no_comparison_table` (FR-09 / NFR-01)

```
Arrange: one profile only

Act: run_report(...)

Assert:
  // Section 6 present
  content.contains("## 6.")

  // No improvement/degradation sub-tables (single-profile run)
  section_6 = content[content.find("## 6.").unwrap()..]
  !section_6.contains("Improved") && !section_6.contains("Degraded")
```

### `test_report_section_5_not_duplicated` (R-02 anti-regression)

```
Arrange: standard two-profile result

Act: run_report(...)

Assert:
  // Section 5 heading appears exactly once
  count = content.matches("## 5.").count()
  count == 1

  // Section 6 heading appears exactly once
  count6 = content.matches("## 6.").count()
  count6 == 1
```

### `test_report_top_5_improvement_rows_capped` (FR-09 boundary)

```
Arrange: two-profile result with only 3 scenarios
         all with positive cc_at_k_delta

Act: run_report(...)

Assert:
  section_6 = content[content.find("## 6.").unwrap()..]
  // Section shows 3 rows, not 5 (fewer than 5 qualify)
  // Verify by counting table rows in the improvement sub-section
  // Exact assertion depends on rendering format
  // Minimum: section renders without panic and no "top-5" row count is exceeded
```

---

## NFR Checks (code review)

- `render_report` signature gains `cc_at_k_rows: &[CcAtKScenarioRow]` parameter
- No tokio or async in render.rs
- Section 6 is appended after section 5 — check that `render_report` builds sections
  in a sequence and does not insert 6 between 4 and 5
- Column order in Summary table: `P@K | MRR | CC@k | ICD` (and deltas in same order)
- ICD column header uses `ln(n)` notation, not `log2(n)` or `log(n)`
