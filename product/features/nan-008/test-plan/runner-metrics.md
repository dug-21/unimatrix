# Test Plan: runner/metrics.rs

## Component Responsibility

Pure functions computing CC@k and ICD from `ScoredEntry` slices. No I/O, no async.
Extended `compute_comparison` adds `cc_at_k_delta` and `icd_delta` fields.

## Risks Covered

R-03 (empty categories silent 0.0), R-05 (ICD float precision / NaN), R-10 (delta
sign inversion), with partial coverage of R-01 (dual copy) and R-08 (category mapping).

---

## Tests in `runner/tests_metrics.rs`

All tests call the functions directly. The existing `make_entries` helper must be
extended to accept an optional category parameter so tests can populate
`ScoredEntry.category` with real values.

### Helper: `make_entries_with_categories`

```rust
fn make_entries_with_categories(pairs: &[(u64, &str)]) -> Vec<ScoredEntry> {
    pairs.iter()
        .map(|&(id, cat)| ScoredEntry {
            id,
            title: format!("Entry {id}"),
            category: cat.to_string(),  // NEW field
            final_score: 0.9,
            similarity: 0.85,
            confidence: 0.7,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        })
        .collect()
}
```

The existing `make_entries` helper must also be updated to add a default `category`
value (empty string or `"decision"`) so existing tests compile after the struct gains
the new field.

---

## AC-10 Boundary Tests (required — all four must pass)

### `test_cc_at_k_all_categories_present`

```
Arrange: entries = [{cat: "a"}, {cat: "b"}, {cat: "c"}]
         configured = ["a", "b", "c"]
Act:     result = compute_cc_at_k(&entries, &configured)
Assert:  result == 1.0
```

All configured categories covered. CC@k = 3/3 = 1.0.

### `test_cc_at_k_one_category_present`

```
Arrange: entries = [{cat: "a"}, {cat: "a"}, {cat: "a"}]
         configured = ["a", "b", "c"]  // n = 3
Act:     result = compute_cc_at_k(&entries, &configured)
Assert:  (result - (1.0 / 3.0)).abs() < 1e-9
```

Only 1 of 3 configured categories appears. CC@k = 1/3.

### `test_icd_maximum_entropy`

```
Arrange: entries = [{cat: "a"}, {cat: "b"}, {cat: "c"}, {cat: "d"}]
         // uniform distribution, n = 4
Act:     result = compute_icd(&entries)
Assert:  (result - f64::ln(4.0)).abs() < 1e-9
```

Uniform distribution across n=4 categories. ICD = ln(4) ≈ 1.386.

### `test_icd_single_category`

```
Arrange: entries = [{cat: "a"}, {cat: "a"}, {cat: "a"}]
Act:     result = compute_icd(&entries)
Assert:  result == 0.0
```

Zero entropy when all entries share one category.

---

## Guard Tests (R-03 and R-05)

### `test_cc_at_k_empty_configured_categories_returns_zero`

```
Arrange: entries = [{cat: "a"}, {cat: "b"}]
         configured = []
Act:     result = compute_cc_at_k(&entries, &[])
Assert:  result == 0.0
         // and no panic — the function must not divide by zero
```

Division-by-zero guard. tracing::warn! fires but is not asserted in tests.

### `test_icd_empty_entries_returns_zero`

```
Arrange: entries = []
Act:     result = compute_icd(&[])
Assert:  result == 0.0
```

Empty result set: ICD = 0.0 without panic.

### `test_icd_two_entries_one_category_each`

```
Arrange: entries = [{cat: "a"}, {cat: "b"}]  // p(a) = 0.5, p(b) = 0.5
Act:     result = compute_icd(&entries)
Assert:  (result - f64::ln(2.0)).abs() < 1e-9
         // ln(2) ≈ 0.693
```

Exercises the p = 0.5 path. Confirms no NaN from ln(0.5).

### `test_icd_no_nan_propagation` (R-05 NaN guard)

```
Arrange: entries = [{cat: "a"}, {cat: "a"}, {cat: "b"}]
         // p(a) = 2/3, p(b) = 1/3
Act:     result = compute_icd(&entries)
Assert:  !result.is_nan()
         !result.is_infinite()
         result > 0.0
```

Verifies that only categories present in `entries` are iterated (zero-count
categories from `configured_categories` must never enter the entropy sum).

---

## Delta Tests (R-10)

These tests require importing `compute_comparison` or exercising via an internal
helper. If `compute_comparison` is not directly testable, mock two `ProfileResult`
values and verify the resulting `ComparisonMetrics`.

### `test_compute_comparison_delta_positive`

```
Arrange: baseline = ProfileResult { cc_at_k: 0.4, icd: 0.6, ... }
         candidate = ProfileResult { cc_at_k: 0.7, icd: 1.1, ... }
Act:     result = compute_comparison(profiles, baseline_name)
Assert:  result.cc_at_k_delta > 0.0    // 0.7 - 0.4 = 0.3
         result.icd_delta > 0.0         // 1.1 - 0.6 = 0.5
         (result.cc_at_k_delta - 0.3).abs() < 1e-9
         (result.icd_delta - 0.5).abs() < 1e-9
```

Candidate improves over baseline — both deltas must be positive.

### `test_compute_comparison_delta_negative`

```
Arrange: baseline = ProfileResult { cc_at_k: 0.8, icd: 1.2, ... }
         candidate = ProfileResult { cc_at_k: 0.5, icd: 0.9, ... }
Act:     result = compute_comparison(profiles, baseline_name)
Assert:  result.cc_at_k_delta < 0.0    // 0.5 - 0.8 = -0.3
         result.icd_delta < 0.0         // 0.9 - 1.2 = -0.3
```

Candidate degrades — both deltas must be negative.

---

## Additional Edge Cases

### `test_cc_at_k_result_category_not_in_configured` (WARN-2 resolution)

```
Arrange: entries = [{cat: "legacy-cat"}, {cat: "decision"}]
         configured = ["decision", "convention", "pattern"]
Act:     result = compute_cc_at_k(&entries, &configured)
Assert:  result is in [0.0, 1.0]
         // "legacy-cat" is NOT in configured; only "decision" counts
         // If intersection semantics: result ≈ 1/3 (one of three covered)
         // If union semantics: result > 1.0 is possible — assertion must catch it
```

This test guards WARN-2 from IMPLEMENTATION-BRIEF.md. The expected value depends
on the semantics chosen by the delivery agent (intersection preferred). The assertion
`result <= 1.0` is unconditional; the exact expected value is determined at
implementation time.

### `test_cc_at_k_entries_empty_configured_non_empty`

```
Arrange: entries = []
         configured = ["a", "b", "c"]
Act:     result = compute_cc_at_k(&entries, &configured)
Assert:  result == 0.0
```

No entries returned: zero coverage, regardless of configured categories.

---

## Reproducibility

### `test_cc_at_k_deterministic`

```
Arrange: entries, configured (any non-trivial inputs)
Act:     r1 = compute_cc_at_k(&entries, &configured)
         r2 = compute_cc_at_k(&entries, &configured)
Assert:  r1 == r2
```

### `test_icd_deterministic`

```
Arrange: entries (any non-trivial inputs)
Act:     r1 = compute_icd(&entries)
         r2 = compute_icd(&entries)
Assert:  r1 == r2
```

---

## NFR Checks (code review, not test assertions)

- `compute_cc_at_k` and `compute_icd` are `pub fn` (not `async fn`, not `pub async fn`)
- No `tokio`, `spawn_blocking`, or any future primitives present
- No hardcoded category strings (`"decision"`, `"convention"`, etc.) in the function body
- `HashSet` used for distinct category collection in `compute_cc_at_k`
- `f64::ln` used (not `f64::log2`) in `compute_icd`
