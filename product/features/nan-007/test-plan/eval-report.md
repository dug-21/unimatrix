# Test Plan: `eval/report.rs` (D4)

**Component**: `crates/unimatrix-server/src/eval/report.rs`
**Function under test**: `run_report(results: &Path, scenarios: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>`
**AC coverage**: AC-08 (five sections), AC-09 (zero-regression check), C-07 (exit code 0 always)
**Risk coverage**: R-12 (OR semantics), R-17 (missing headers)

---

## Unit Tests

Location: `crates/unimatrix-server/src/eval/report.rs` (inline `#[cfg(test)]`)

### Test: `test_report_contains_all_five_sections`

**Purpose**: AC-08, R-17 — output Markdown contains all five required section headers.
**Arrange**: Build a minimal in-memory results set (2 scenarios, 2 profiles). Write to a temp `results/` directory.
**Act**: Call `run_report(&results_dir, None, &out_path)`.
**Assert**:
- Returns `Ok(())`.
- `out_path` exists and contains all five of the following headers (exact strings or their equivalents):
  - `## Summary` (or `# Summary`)
  - `## Notable Ranking Changes`
  - `## Latency Distribution`
  - `## Entry-Level Analysis`
  - `## Zero-Regression Check`
- Sections appear in the order specified in FR-27.
**Risk**: R-17 (AC-08)

### Test: `test_zero_regression_check_mrr_regression_only`

**Purpose**: R-12 — OR semantics: scenario with lower MRR but equal P@K appears in the regression list.
**Arrange**: Result set: scenario A with `candidate.mrr = 0.4, baseline.mrr = 0.5, candidate.p_at_k = 0.6, baseline.p_at_k = 0.6` (MRR regression only).
**Act**: `run_report`.
**Assert**: The Zero-Regression Check section contains scenario A's ID. It is NOT in the list only when both MRR and P@K are equal or higher.
**Risk**: R-12 (AC-09)

### Test: `test_zero_regression_check_pak_regression_only`

**Purpose**: R-12 — OR semantics: scenario with lower P@K but equal MRR appears in regression list.
**Arrange**: Scenario B: `candidate.p_at_k = 0.4, baseline.p_at_k = 0.6, candidate.mrr = 0.5, baseline.mrr = 0.5`.
**Act**: `run_report`.
**Assert**: Scenario B appears in the Zero-Regression Check section.
**Risk**: R-12 (AC-09)

### Test: `test_zero_regression_check_both_regression`

**Purpose**: Scenario where both MRR and P@K are lower also appears.
**Arrange**: Scenario C: both metrics lower in candidate.
**Assert**: Scenario C in regression list.
**Risk**: R-12

### Test: `test_zero_regression_check_no_regressions_empty_indicator`

**Purpose**: AC-09 — when no regressions exist, the section contains an explicit empty-list indicator (not just an absent section or empty content).
**Arrange**: Result set where all candidate metrics are equal or higher than baseline.
**Act**: `run_report`.
**Assert**: Zero-Regression Check section is present AND contains text indicating no regressions (e.g., `"No regressions"`, `"None"`, or equivalent non-empty indicator string). The section is not empty or absent.
**Risk**: AC-09

### Test: `test_zero_regression_check_exact_equal_metrics_not_regression`

**Purpose**: Boundary — candidate metric exactly equal to baseline is not a regression.
**Arrange**: Scenario: `candidate.mrr == baseline.mrr` and `candidate.p_at_k == baseline.p_at_k`.
**Assert**: Scenario does NOT appear in the regression list. Empty indicator appears.
**Risk**: R-12 boundary

### Test: `test_report_exit_code_zero_regardless_of_regressions`

**Purpose**: C-07 — `run_report` always returns `Ok(())` regardless of regression count.
**Arrange**: Result set with 5 regressions.
**Act**: `run_report`.
**Assert**: Returns `Ok(())`. The caller (main.rs dispatch arm) exits with code 0.
**Risk**: C-07 (FR-29)

### Test: `test_report_empty_results_dir`

**Purpose**: Edge case — empty results directory produces a report with empty-list indicators in all sections.
**Arrange**: Empty `results/` directory (no JSON files).
**Act**: `run_report(&empty_dir, None, &out_path)`.
**Assert**: Returns `Ok(())`. Report file contains all five section headers. Zero-Regression Check has the empty indicator.
**Risk**: Edge case

### Test: `test_report_skips_malformed_result_json`

**Purpose**: `eval report` skips unreadable result files and continues with valid ones.
**Arrange**: Results directory with 2 valid JSON files and 1 malformed JSON file.
**Act**: `run_report`.
**Assert**: Returns `Ok(())`. Report is generated based on the 2 valid files. A warning about the malformed file is emitted (to stderr or as a note in the report).
**Risk**: Failure mode table in RISK-TEST-STRATEGY.md

### Test: `test_report_summary_table_has_per_profile_rows`

**Purpose**: FR-27 — summary table has one row per profile with aggregate P@K, MRR, average latency, rank change rate, and delta vs. baseline.
**Arrange**: 3 scenarios × 2 profiles (baseline + candidate). Known metric values.
**Assert**: The Summary section in the report contains a Markdown table with columns for each profile and a delta column.
**Risk**: FR-27

### Test: `test_report_latency_distribution_present`

**Purpose**: FR-27 — latency distribution section is non-empty.
**Arrange**: Results with known `latency_ms` values.
**Assert**: The Latency Distribution section contains a table or histogram with per-profile data.
**Risk**: FR-27

### Test: `test_report_entry_level_analysis_promotion_demotion`

**Purpose**: FR-27 — entry-level analysis section identifies promoted and demoted entries.
**Arrange**: Results where entry 42 moved from rank 5 → rank 1 (promoted) and entry 99 moved rank 1 → rank 5 (demoted).
**Assert**: The Entry-Level Analysis section mentions entry 42 as promoted and entry 99 as demoted.
**Risk**: FR-27

---

## Integration Tests (Python Subprocess)

Location: `product/test/infra-001/tests/test_eval_offline.py`

### Test: `test_eval_report_five_sections`

**Purpose**: AC-08 — subprocess invocation produces Markdown with all five headers.
**Act**: `unimatrix eval report --results <prepared_dir> --out <report.md>`.
**Assert**:
- Exit code 0.
- `report.md` contains all five section headers.

### Test: `test_eval_report_zero_regression_mrr_only`

**Purpose**: AC-09 + R-12 — scenario with MRR regression but equal P@K appears in list.
**Arrange**: Pre-built results directory with one known MRR-only regression.
**Assert**: Exit 0. Report zero-regression section contains that scenario's ID.

### Test: `test_eval_report_zero_regression_pak_only`

**Purpose**: AC-09 + R-12 — scenario with P@K regression but equal MRR appears in list.
**Assert**: Exit 0. Regression section contains the scenario.

### Test: `test_eval_report_empty_regression_indicator`

**Purpose**: AC-09 — when no regressions, empty indicator is present.
**Arrange**: Results where all candidate metrics >= baseline.
**Assert**: Exit 0. Zero-Regression Check section contains an explicit non-empty indicator string (e.g., `"No regressions"` or `"None"`).

### Test: `test_eval_report_exit_zero_with_regressions`

**Purpose**: C-07 — exit code 0 even when regressions exist.
**Arrange**: Results directory with known regressions.
**Assert**: Exit code 0.

### Test: `test_eval_report_empty_results_dir`

**Purpose**: Edge case via subprocess.
**Assert**: Exit 0. Report file has all five headers and empty indicator.

---

## Edge Cases from Risk Strategy

- OR semantics for regression check: a scenario where MRR drops but P@K is unchanged must appear in the regression list. A scenario where P@K drops but MRR is unchanged must also appear. Both must be tested independently (R-12).
- Empty indicator exactness: the Zero-Regression Check section must contain an explicit non-empty indicator string, not simply be absent or contain only the section header.
- Empty results directory: all sections must be present even with no data.
- Malformed JSON files: skip and continue, not abort.
- `--scenarios` optional path: when supplied, queries in the report are annotated with their text from the JSONL file. When absent, scenario IDs are used instead.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #157 (Test infrastructure is cumulative), #128 (Risk drives testing)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-004 (report module in unimatrix-server src/eval/report.rs), ADR-001 (exit-0 always, FR-29/C-07 from RISK-TEST-STRATEGY)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #129 (Concrete assertions), #748 (TestHarness Server Integration Pattern)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
