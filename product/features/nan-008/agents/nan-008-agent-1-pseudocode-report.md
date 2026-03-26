# Agent Report: nan-008-agent-1-pseudocode

## Summary

Produced pseudocode for all 6 components of nan-008 (Distribution-Aware Metrics).
All files written to `/workspaces/unimatrix-nan-008/product/features/nan-008/pseudocode/`.

## Files Produced

- `pseudocode/OVERVIEW.md` — component interaction, data flow, shared type definitions,
  sequencing constraints, and critical invariants.
- `pseudocode/runner-output.md` — struct field additions with types, field positions,
  dual-copy atomicity checklist, and key test scenarios.
- `pseudocode/runner-metrics.md` — full algorithm for `compute_cc_at_k` and `compute_icd`,
  extension of `compute_comparison`, NaN guard rationale, intersection semantics
  decision, and 10 key test scenarios covering all AC-10 boundary values and R-05/R-10 risks.
- `pseudocode/runner-replay.md` — parameter extension for `run_single_profile`, category
  field population step, metric call wiring, borrow analysis (SR-07), and integration
  test scenarios.
- `pseudocode/report-mod.md` — all mirror type changes with `#[serde(default)]`, new
  `CcAtKScenarioRow` type, `default_comparison` update, `run_report` call site changes,
  and round-trip test specification (ADR-003).
- `pseudocode/report-aggregate.md` — accumulation extension for `compute_aggregate_stats`,
  full `compute_cc_at_k_scenario_rows` algorithm with descending sort, and 6 key test
  scenarios covering R-11 (wrong count) and R-12 (wrong sort direction).
- `pseudocode/report-render.md` — Section 1 table extension, Section 6 algorithm with
  `render_distribution_analysis` helper, data availability analysis (Option A: pass
  `results` to helper for min/max computation), and 6 key test scenarios.

## Design Decisions Made

### Intersection Semantics for CC@k (WARN-2 resolution)

The ALIGNMENT-REPORT.md flags an ambiguity: if entries contain categories absent from
`configured_categories`, CC@k can exceed 1.0 under the SCOPE.md formula. This
pseudocode adopts intersection semantics (numerator = categories present in BOTH
entries AND `configured_categories`). This caps CC@k at 1.0 naturally and avoids
requiring a post-computation clamp. The delivery agent must implement a test case for
the out-of-configured-list edge case to confirm the guard works.

### render_distribution_analysis Receives ScenarioResult Slice

`AggregateStats` carries only `mean_cc_at_k` — no min/max. To render the per-profile
CC@k/ICD range table, either `AggregateStats` must gain `min_cc_at_k`/`max_cc_at_k`
fields, or `render_distribution_analysis` must receive the raw `results` slice for
direct min/max computation. This pseudocode chooses Option A (pass `results` to the
helper) over adding more fields to `AggregateStats`. The delivery agent should
confirm this is acceptable; if `AggregateStats` size is a concern, Option B
(min/max in aggregate.rs) is equally valid.

### Degradation Row Sort Logic

`compute_cc_at_k_scenario_rows` returns rows sorted descending by `cc_at_k_delta`.
The degradation sub-table in section 6 uses `filter(cc_at_k_delta < 0.0).rev().take(5)`.
This avoids a second sort pass. The delivery agent may prefer collecting and sorting
a separate degradation list; either approach is acceptable as long as the test at
R-12 passes.

## Open Questions for Delivery Agent

1. **`render_distribution_analysis` signature**: This pseudocode passes `results:
   &[ScenarioResult]` to the helper to compute min/max. If the delivery agent prefers
   to add `min_cc_at_k` and `max_cc_at_k` to `AggregateStats` instead (keeping all
   computation in `aggregate.rs`), that is an acceptable deviation. It requires
   extending `report-aggregate.md` instructions to track running min/max in the
   accumulation loop.

2. **ICD range table**: The pseudocode specifies a per-profile ICD range table in
   section 6 alongside the CC@k range table. The specification (FR-09) only explicitly
   requires CC@k range but section 6 framing (ARCHITECTURE.md) mentions both. The
   delivery agent should confirm that rendering an ICD range table is in scope.
   If not, the ICD table can be omitted and the `ln(n)` annotation on the column
   header alone satisfies AC-14.

3. **`tests_metrics.rs` file existence**: The specification (OQ-03) asks whether
   this is an existing file or new. If the file does not exist, the delivery agent
   must create it as a new test module file and register it in the enclosing
   `mod.rs` or via `#[path]`.

## Knowledge Stewardship

- Queried: /uni-query-patterns for eval harness metrics implementation — knowledge
  package from delivery leader provided entries #3512, #2806, #3522, #3520 covering
  dual-type constraint, A/B pattern, round-trip test guard, and category field ADR.
  No additional Unimatrix queries were needed; the knowledge package was complete.
- Deviations from established patterns:
  - Dual-type atomicity (pattern #3512): followed exactly — both type copies updated
    in the same pseudocode wave with explicit atomicity checklist.
  - A/B evaluation pattern (pattern #2806): followed exactly — profile TOML to
    snapshot to replay to report flow is preserved.
  - Round-trip integration test (entry #3522): followed exactly — test specification
    included in report-mod.md.
