# Component: Report Entry Point

File: `eval/report/mod.rs`

## Purpose

1. Add `PhaseAggregateStats` struct — shared between `aggregate.rs` and `render.rs`.
2. Add `phase: Option<String>` to the local (report-side) `ScenarioResult` copy.
3. Wire `compute_phase_stats` into Step 4 of `run_report`.
4. Pass `phase_stats` as the new parameter to `render_report` in Step 5.
5. Update module docstring to list seven sections.

---

## Module Docstring Update

Current docstring (lines 1-15):
```
//! Markdown report generation for eval results (nan-007 D4).
//!
//! Reads per-scenario JSON result files from a `--results` directory, aggregates
//! across all scenarios, and writes a Markdown report with five required sections:
//!
//! 1. Summary
//! 2. Notable Ranking Changes
//! 3. Latency Distribution
//! 4. Entry-Level Analysis
//! 5. Zero-Regression Check
//! 6. Distribution Analysis
//!
//! This module is entirely synchronous: ...
```

New docstring — update section list:
```
//! Markdown report generation for eval results (nan-007 D4, extended nan-009).
//!
//! Reads per-scenario JSON result files from a `--results` directory, aggregates
//! across all scenarios, and writes a Markdown report with up to seven sections:
//!
//! 1. Summary
//! 2. Notable Ranking Changes
//! 3. Latency Distribution
//! 4. Entry-Level Analysis
//! 5. Zero-Regression Check
//! 6. Phase-Stratified Metrics  (omitted when all phases are None)
//! 7. Distribution Analysis
//!
//! This module is entirely synchronous: ...
```

---

## Import Addition

Add `compute_phase_stats` to the existing `use aggregate::` import:
```
use aggregate::{
    compute_aggregate_stats, compute_cc_at_k_scenario_rows, compute_entry_rank_changes,
    compute_latency_buckets, compute_phase_stats,    // NEW
    find_regressions,
};
```

Update the `use render::` import to use the new `render_report` signature:
```
use render::render_report;    // unchanged — signature change is in render.rs
```

---

## New Struct: `PhaseAggregateStats`

Add after the `AggregateStats` struct definition (after line 153 in current file):

```
/// Aggregate metrics for one phase stratum (nan-009).
///
/// Produced by `compute_phase_stats`. Phase label is the `query_log.phase` value,
/// or `"(unset)"` for the null bucket. Sorted alphabetically with `"(unset)"` last.
#[derive(Debug, Default)]
pub(super) struct PhaseAggregateStats {
    pub phase_label: String,     // "design" | "delivery" | "bugfix" | "(unset)"
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_cc_at_k: f64,
    pub mean_icd: f64,
}
```

Visibility: `pub(super)` — accessible to `aggregate.rs` and `render.rs` (both are sub-modules
of `mod.rs`).

---

## Modified Type: `ScenarioResult` (report-side copy)

Current `ScenarioResult` (lines 112-121):
```
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ScenarioResult {
    pub scenario_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileResult>,
    #[serde(default = "default_comparison")]
    pub comparison: ComparisonMetrics,
}
```

Add `phase` field with `#[serde(default)]` ONLY (ADR-001 — report side is reader-only):
```
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ScenarioResult {
    pub scenario_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileResult>,
    #[serde(default = "default_comparison")]
    pub comparison: ComparisonMetrics,
    #[serde(default)]
    pub phase: Option<String>,    // NEW — tolerates absent key (pre-nan-009) and explicit null
}
```

Note: `#[serde(default)]` handles both:
- Absent key (pre-nan-009 result files) → defaults to `None`
- Explicit `"phase":null` (runner output for null-phase scenarios) → deserializes to `None`
- `"phase":"delivery"` → deserializes to `Some("delivery")`

MUST NOT add `skip_serializing_if` here — the report module deserializes, not serializes.

---

## Modified Function: `run_report`

Current Step 4 (lines 244-248):
```
// Step 4: Aggregate.
let aggregate_stats = compute_aggregate_stats(&scenario_results);
let regressions = find_regressions(&scenario_results, &query_map);
let latency_buckets = compute_latency_buckets(&scenario_results);
let entry_rank_changes = compute_entry_rank_changes(&scenario_results);
let cc_at_k_rows = compute_cc_at_k_scenario_rows(&scenario_results);
```

New Step 4 — add `phase_stats` computation:
```
// Step 4: Aggregate.
let aggregate_stats = compute_aggregate_stats(&scenario_results);
let regressions = find_regressions(&scenario_results, &query_map);
let latency_buckets = compute_latency_buckets(&scenario_results);
let entry_rank_changes = compute_entry_rank_changes(&scenario_results);
let cc_at_k_rows = compute_cc_at_k_scenario_rows(&scenario_results);
let phase_stats = compute_phase_stats(&scenario_results);    // NEW
// Returns empty vec when all phases are None; render_report omits section 6 in that case.
```

Current Step 5 (lines 251-259):
```
// Step 5: Render.
let md = render_report(
    &aggregate_stats,
    &scenario_results,
    &regressions,
    &latency_buckets,
    &entry_rank_changes,
    &query_map,
    &cc_at_k_rows,
);
```

New Step 5 — add `&phase_stats` as second argument (matching new render_report signature):
```
// Step 5: Render.
let md = render_report(
    &aggregate_stats,
    &phase_stats,           // NEW second argument
    &scenario_results,
    &regressions,
    &latency_buckets,
    &entry_rank_changes,
    &query_map,
    &cc_at_k_rows,
);
```

Public signature of `run_report` is UNCHANGED. The change is internal wiring only.

---

## Error Handling

`compute_phase_stats` is infallible (returns `Vec<PhaseAggregateStats>`). No new error
paths introduced in `run_report`.

---

## Key Test Scenarios

Tests live in `eval/report/tests.rs`.

**T1: `test_scenario_result_phase_round_trip_serde`** (R-03, AC-06)
- Serialize `ScenarioResult { phase: Some("design"), ... }` to JSON using the report-side type
- Deserialize back
- Assert `phase == Some("design")`
- This catches a type mismatch (wrong annotation) without a full report run

**T2: `test_scenario_result_legacy_no_phase_key`** (AC-06, NFR-01)
- Deserialize a JSON string without any `"phase"` key into the report-side `ScenarioResult`
- Assert no error
- Assert `phase == None`

**T3: `test_scenario_result_phase_explicit_null`** (EC-06)
- Deserialize a JSON string with `"phase":null` explicitly
- Assert no error
- Assert `phase == None`

**T4: `test_report_round_trip_phase_section_7_distribution`** (ADR-002 — mandatory)
See report-rendering.md T4 for the full spec. This test exercises the wiring in `run_report`
as well as the rendering in `render_report`.

**T5: `test_report_contains_all_sections`** (updated)
- Requires at least one result with non-null phase
- Assert seven section headings present
- Assert `## 6. Phase-Stratified Metrics` present
- Assert `## 7. Distribution Analysis` present

**T6: Update `make_scenario_result` helper** (compiler requirement)
The `make_scenario_result` helper function in `tests.rs` constructs `ScenarioResult` struct
literals. After adding the `phase` field, all struct literal constructions must include
`phase`. Update the helper to include `phase: None` (or a value) so the existing tests
compile. Tests that specifically need a non-null phase should call the helper with an
explicit `phase` parameter — or create a separate `make_scenario_result_with_phase` helper.

---

## Notes

- `run_report`'s public signature is NOT changed — this is internal wiring.
- The `tests` sub-module (`#[cfg(test)] mod tests;`) structure is unchanged.
- The `load_scenario_query_map` helper function is unchanged.
- The comment block ordering in `run_report` grows by one line (Step 4 gains one binding).
  The step numbering in comments (Step 4, Step 5, Step 6, Step 7, Step 8) is NOT renumbered
  — only the `phase_stats` binding and `render_report` call site are updated.
