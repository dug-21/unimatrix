# nan-008 Pseudocode: report/mod.rs

## Purpose

Local deserialization-only copies of the runner types, plus the `run_report` public
entry point. Must mirror `runner/output.rs` field-for-field so result JSON files are
deserializable without a compile-time dependency on runner. All new fields carry
`#[serde(default)]` for backward compatibility with pre-nan-008 result files.

## New/Modified Types

### ScoredEntry — add `category` field with serde(default)

```
struct ScoredEntry {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub category: String,              // NEW — defaults to "" for pre-nan-008 JSON
    #[serde(default)]
    pub final_score: f64,
    #[serde(default)]
    pub similarity: f64,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub nli_rerank_delta: Option<f64>,
}
visibility: pub(crate)
derive: Debug, Deserialize, Serialize
```

Note: In the existing code, `id` and `title` do NOT carry `#[serde(default)]`
(they are required fields). The new `category` field must carry `#[serde(default)]`
because pre-nan-008 JSON files lack it and must deserialize to `""` without error.

### ProfileResult — add `cc_at_k` and `icd` with serde(default)

```
struct ProfileResult {
    #[serde(default)]
    pub entries: Vec<ScoredEntry>,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub p_at_k: f64,
    #[serde(default)]
    pub mrr: f64,
    #[serde(default)]
    pub cc_at_k: f64,                  // NEW
    #[serde(default)]
    pub icd: f64,                      // NEW
}
visibility: pub(crate)
derive: Debug, Deserialize, Serialize
```

### ComparisonMetrics — add `cc_at_k_delta` and `icd_delta` with serde(default)

```
struct ComparisonMetrics {
    #[serde(default)]
    pub kendall_tau: f64,
    #[serde(default)]
    pub rank_changes: Vec<RankChange>,
    #[serde(default)]
    pub mrr_delta: f64,
    #[serde(default)]
    pub p_at_k_delta: f64,
    #[serde(default)]
    pub latency_overhead_ms: i64,
    #[serde(default)]
    pub cc_at_k_delta: f64,            // NEW
    #[serde(default)]
    pub icd_delta: f64,                // NEW
}
visibility: pub(crate)
derive: Debug, Deserialize, Serialize
```

### default_comparison — update to include new fields

```
pub(crate) fn default_comparison() -> ComparisonMetrics {
    ComparisonMetrics {
        kendall_tau: 1.0,
        rank_changes: Vec::new(),
        mrr_delta: 0.0,
        p_at_k_delta: 0.0,
        latency_overhead_ms: 0,
        cc_at_k_delta: 0.0,            // NEW
        icd_delta: 0.0,                // NEW
    }
}
```

### AggregateStats — add four new fields

```
struct AggregateStats {
    pub profile_name: String,
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_latency_ms: f64,
    pub p_at_k_delta: f64,
    pub mrr_delta: f64,
    pub latency_delta_ms: f64,
    pub mean_cc_at_k: f64,             // NEW
    pub mean_icd: f64,                 // NEW
    pub cc_at_k_delta: f64,            // NEW — mean of per-scenario cc_at_k_delta values
    pub icd_delta: f64,                // NEW — mean of per-scenario icd_delta values
}
visibility: pub(super)
derive: Debug
```

Not serialized. Used only within the report pipeline.

### CcAtKScenarioRow — new internal type

```
pub(super) struct CcAtKScenarioRow {
    pub scenario_id: String,
    pub query: String,
    pub baseline_cc_at_k: f64,
    pub candidate_cc_at_k: f64,
    pub cc_at_k_delta: f64,
}
derive: Debug
```

Produced by `compute_cc_at_k_scenario_rows` in `report/aggregate.rs`.
Consumed by `render_report` in `report/render.rs` for section 6.
Defined in `report/mod.rs` to be in scope for both submodules.

## Modified: run_report — call site changes

```
pub fn run_report(
    results: &Path,
    scenarios: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

Changes to existing steps:

Step 4 — Aggregate (extend):
    BEFORE: compute_aggregate_stats, find_regressions, compute_latency_buckets,
            compute_entry_rank_changes
    AFTER:  add:
        let cc_at_k_rows = compute_cc_at_k_scenario_rows(&scenario_results);

    Import: add `compute_cc_at_k_scenario_rows` to the use statement:
        use aggregate::{
            compute_aggregate_stats, compute_entry_rank_changes,
            compute_latency_buckets, compute_cc_at_k_scenario_rows,
            find_regressions,
        };

Step 5 — Render (extend):
    BEFORE: render_report(stats, results, regressions, latency_buckets,
                          entry_rank_changes, query_map)
    AFTER:  render_report(stats, results, regressions, latency_buckets,
                          entry_rank_changes, query_map,
                          &cc_at_k_rows)               // NEW argument

All other steps unchanged.
```

## Module-Level Doc Comment — update

Extend the existing module doc comment to list section 6:

```
//! 1. Summary
//! 2. Notable Ranking Changes
//! 3. Latency Distribution
//! 4. Entry-Level Analysis
//! 5. Zero-Regression Check
//! 6. Distribution Analysis    <-- NEW
```

## Error Handling

No new error paths in `run_report`. `compute_cc_at_k_scenario_rows` is a pure
function that returns `Vec<CcAtKScenarioRow>` — no `Result` wrapper. The new
`render_report` parameter is a slice, so no allocation failure is possible at
the call site.

## Integration Points

- `report/aggregate.rs` imports and produces `AggregateStats` and `CcAtKScenarioRow`.
- `report/render.rs` consumes `AggregateStats` and `CcAtKScenarioRow` (via the new
  `cc_at_k_rows` parameter on `render_report`).
- The `ScenarioResult`, `ProfileResult`, `ComparisonMetrics`, and `ScoredEntry`
  types here are deserialization copies — they share the JSON schema with their
  counterparts in `runner/output.rs` but have no Rust compile-time dependency.

## Key Test Scenarios

Tests live in `report/tests.rs`.

1. `test_report_backward_compat_pre_nan008_json` (R-07)
   - Construct a JSON string that omits `cc_at_k`, `icd`, `cc_at_k_delta`,
     `icd_delta`, and `category` on every entry.
   - Deserialize as `ScenarioResult`.
   - Assert deserialization succeeds (no error).
   - Assert `profile_result.cc_at_k == 0.0`.
   - Assert `profile_result.icd == 0.0`.
   - Assert `comparison.cc_at_k_delta == 0.0`.
   - Assert `comparison.icd_delta == 0.0`.
   - Assert `entry.category == ""`.

2. `test_default_comparison_includes_new_fields`
   - Call `default_comparison()`.
   - Assert the struct has `cc_at_k_delta: 0.0` and `icd_delta: 0.0`.

3. `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (ADR-003 primary test)
   - Create `ScenarioResult` with `cc_at_k: 0.857`, `icd: 1.234`,
     `cc_at_k_delta: 0.143`, `icd_delta: 0.211` (non-zero, non-trivially-round).
   - Serialize to JSON and write to TempDir.
   - Call `run_report(tempdir, None, &out_path)`.
   - Read the output Markdown.
   - Assert `content.contains("0.857")` (CC@k value appears in report).
   - Assert `content.contains("1.234")` (ICD value appears in report).
   - Assert `content.contains("## 6. Distribution Analysis")` or equivalent.
   - Assert section position order:
     `pos("## 1.") < pos("## 2.") < pos("## 3.") < pos("## 4.") < pos("## 5.") < pos("## 6.")`.
