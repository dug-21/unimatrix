## ADR-002: ICD Uses Raw Shannon Entropy with Max-Value Label in Report

### Context

ICD (Intra-query Category Diversity) is defined as the Shannon entropy of
the category distribution in a query's result set:

```
ICD = -sum_cat [ p(cat) * ln(p(cat)) ]
```

where `p(cat) = count(entries with category=cat) / total entries`.

The scope explicitly declines normalization: ICD is NOT divided by `ln(n)`
to produce a value in [0, 1]. The raw form has range `[0, ln(n_categories)]`
where `n_categories` is the number of distinct categories that actually appear
in the result set (not the configured count).

The risk (SR-03) is that consumers comparing ICD values across profiles or
deployments may misread them as directly comparable, not realizing the maximum
varies with the number of categories.

Two mitigations were considered:
1. Normalize ICD by dividing by `ln(configured_categories_count)`, yielding
   a value in [0, 1] that is always comparable.
2. Keep raw ICD but label the column in the report with its maximum value.

Normalization was rejected because the scope decision is to not normalize,
the issue formula specifies raw entropy, and normalization hides the
information about how many categories were active — which is itself a
diagnostic signal. A deployment with 2 categories and ICD = ln(2) ≈ 0.693
behaves very differently from one with 7 categories and ICD = 0.693.

### Decision

`compute_icd` returns raw Shannon entropy using `f64::ln` (natural log).
Range is `[0.0, ln(n)]` where n is the count of distinct categories in the
result set. Returns 0.0 for empty results and for single-category results.

In `report/render.rs`, the ICD column header in the Summary table is rendered
as `ICD (max=ln(n))` where `n` is replaced with the actual configured
category count sourced from the profile's `AggregateStats` context. If the
configured count is not available in the render context, the header uses the
literal string `ICD (max=ln(n))` as a reminder of the scaling.

In the Distribution Analysis section (section 6), interpretation guidance is
included: "ICD is raw Shannon entropy. Maximum value is ln(n_categories).
Values are comparable across profiles run on the same scenario set with the
same configured categories."

The documentation in `docs/testing/eval-harness.md` must include a formula
block and the natural log base clarification.

### Consequences

- ICD values are not normalized; two runs with different `configured_categories`
  produce ICD values that are not directly comparable.
- The column label and section 6 guidance surface this constraint to report
  readers, addressing SR-03 without changing the metric definition.
- The metric implementation remains a pure function with no dependency on
  `configured_categories` — it only observes the result distribution.
- Future normalization remains possible by dividing the output of `compute_icd`
  by `(configured_categories.len() as f64).ln()` at the call site; this does
  not require changing `compute_icd`.
