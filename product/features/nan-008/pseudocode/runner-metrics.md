# nan-008 Pseudocode: runner/metrics.rs

## Purpose

Pure functions computing retrieval metrics from `ScoredEntry` slices.
No I/O, no async, no database access. All new functions are `pub(super)`,
consumed by `replay.rs`. The existing functions (`compute_p_at_k`, `compute_mrr`,
`compute_tau_safe`, `compute_rank_changes`, `determine_ground_truth`) are unchanged.

## Imports Required

The existing import line:
```
use super::output::{ComparisonMetrics, ProfileResult, RankChange, ScoredEntry};
```
already imports `ScoredEntry`. No new imports are required beyond what is already
present. `std::collections::HashSet` is already imported.

## New Functions

### compute_cc_at_k

```
pub(super) fn compute_cc_at_k(
    entries: &[ScoredEntry],
    configured_categories: &[String],
) -> f64

Algorithm:
  1. Guard: if configured_categories.is_empty()
       tracing::warn!(
           "compute_cc_at_k: configured_categories is empty; \
            returning 0.0. Check [knowledge] categories in the profile TOML."
       )
       return 0.0

  2. Collect distinct categories present in entries that are also in
     configured_categories (intersection semantics — ADR resolution, WARN-2):
       configured_set = HashSet<&String> built from configured_categories
       distinct_covered = HashSet<&str> of entry.category values where
                          configured_set.contains(entry.category)

     Intersection semantics: only count categories that appear in BOTH
     the result entries AND configured_categories. This naturally caps
     CC@k at 1.0. (If a result entry has a category not in configured_categories,
     it is excluded from the numerator.)

  3. numerator = distinct_covered.len() as f64
  4. denominator = configured_categories.len() as f64
  5. return numerator / denominator

Range: [0.0, 1.0]. Denominator is always >= 1 due to guard in step 1.
Numerator cannot exceed denominator because of intersection semantics.
```

Note on intersection semantics: The SCOPE.md formula counts all distinct result
categories regardless of configured list, which can produce CC@k > 1.0. The
ALIGNMENT-REPORT.md flags this as WARN-2 and recommends intersection semantics.
This pseudocode adopts intersection semantics as the safe default. The delivery
agent must implement and test the out-of-configured-list edge case explicitly
(see Key Test Scenarios below).

### compute_icd

```
pub(super) fn compute_icd(entries: &[ScoredEntry]) -> f64

Algorithm:
  1. Guard: if entries.is_empty() return 0.0

  2. total = entries.len() as f64

  3. Build category count map:
       counts: HashMap<&str, usize>
       for entry in entries:
           *counts.entry(entry.category.as_str()).or_insert(0) += 1

  4. Compute raw Shannon entropy:
       entropy = 0.0_f64
       for (_, count) in counts:
           if count == 0: continue   // NaN guard: skip zero-count entries
                                     // (defensive; counts map never has zero values
                                     //  given step 3 construction, but guard is explicit)
           p = count as f64 / total
           entropy -= p * p.ln()     // -= because entropy = -sum(p * ln(p))
                                     // p.ln() is negative for 0 < p < 1
                                     // so -p.ln() is positive; entropy accumulates positive

  5. return entropy

NaN guard rationale: `0.0 * f64::ln(0.0)` = `0.0 * f64::NEG_INFINITY` = NaN.
This must never be evaluated. The HashMap construction in step 3 only inserts
categories that appear at least once, so no zero-count entry exists. The explicit
`if count == 0: continue` guard is defense-in-depth.

Special cases:
  - Single category: all entries have same category, p = 1.0, ln(1.0) = 0.0,
    entropy = -(1.0 * 0.0) = 0.0. Correct.
  - Uniform n categories: each p = 1/n, entropy = -n * (1/n * ln(1/n))
    = -n * (1/n * (-ln(n))) = ln(n). Correct maximum entropy.
  - Two entries, one category each: p = 0.5, entropy = -(2 * 0.5 * ln(0.5))
    = -(2 * 0.5 * (-0.693)) = 0.693 = ln(2). Correct.

Range: [0.0, ln(n)] where n = number of distinct categories in entries.
```

## Modified Functions

### compute_comparison — extend to populate cc_at_k_delta and icd_delta

```
pub(super) fn compute_comparison(
    profile_results: &HashMap<String, ProfileResult>,
    baseline_name: &str,
) -> Result<ComparisonMetrics, Box<dyn std::error::Error>>

Changes (additions only — existing logic unchanged):
  After computing mrr_delta and p_at_k_delta, add:
      cc_at_k_delta = candidate.cc_at_k - baseline.cc_at_k
      icd_delta = candidate.icd - baseline.icd

  For single-profile self-comparison (candidate == baseline):
      cc_at_k_delta = 0.0   (self - self = 0)
      icd_delta = 0.0

  Return ComparisonMetrics {
      kendall_tau,
      rank_changes,
      mrr_delta,
      p_at_k_delta,
      latency_overhead_ms,
      cc_at_k_delta,        // NEW
      icd_delta,            // NEW
  }

Sign convention: positive = candidate improved relative to baseline.
cc_at_k_delta = candidate.cc_at_k - baseline.cc_at_k (same order as mrr_delta).
```

## Error Handling

- `compute_cc_at_k`: returns 0.0 + warn on empty configured_categories. No panics.
- `compute_icd`: returns 0.0 on empty entries. No panics. No NaN possible.
- `compute_comparison`: existing error path (baseline not found) is unchanged.
  New fields are simple subtraction of f64 values — no new error paths.

## Key Test Scenarios

The following map to AC-10 in the specification. Tests live in `runner/tests_metrics.rs`.

1. `test_cc_at_k_all_categories_present`
   Input: entries with categories ["decision", "convention", "pattern"],
          configured_categories = ["decision", "convention", "pattern"]
   Expected: 3/3 = 1.0

2. `test_cc_at_k_one_category_present`
   Input: entries all with category "decision",
          configured_categories = ["decision", "convention", "pattern"]
   Expected: 1/3 ≈ 0.3333... (assert with f64 tolerance 1e-9)

3. `test_cc_at_k_empty_configured_categories_returns_zero`
   Input: any non-empty entries, configured_categories = []
   Expected: 0.0, no panic

4. `test_cc_at_k_out_of_configured_list_category`
   Input: entries = [category: "legacy-category", category: "decision"],
          configured_categories = ["decision", "convention"]
   Expected: 1/2 = 0.5 (only "decision" counts; "legacy-category" is excluded)
   This verifies intersection semantics and confirms CC@k cannot exceed 1.0.

5. `test_icd_maximum_entropy`
   Input: 3 entries with categories ["a", "b", "c"] (one each, uniform)
   Expected: ln(3) ≈ 1.0986... (assert within 1e-9 tolerance — never use ==)

6. `test_icd_single_category`
   Input: 5 entries all with category "decision"
   Expected: 0.0 (assert exactly equal — ln(1.0) = 0.0 is exact)

7. `test_icd_empty_entries_returns_zero`
   Input: entries = []
   Expected: 0.0, no panic

8. `test_icd_two_entries_one_category_each`
   Input: 2 entries, categories ["a", "b"]
   Expected: ln(2) ≈ 0.6931... (within 1e-9 tolerance)
   This exercises the p = 0.5 path explicitly (R-05 coverage).

9. `test_compute_comparison_delta_signs_positive`
   Input: baseline ProfileResult with cc_at_k=0.4, icd=0.8
          candidate ProfileResult with cc_at_k=0.7, icd=1.2
   Expected: cc_at_k_delta = +0.3, icd_delta = +0.4

10. `test_compute_comparison_delta_signs_negative`
    Input: baseline ProfileResult with cc_at_k=0.7, icd=1.2
           candidate ProfileResult with cc_at_k=0.4, icd=0.8
    Expected: cc_at_k_delta = -0.3, icd_delta = -0.4
