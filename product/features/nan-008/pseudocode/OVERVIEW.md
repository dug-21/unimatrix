# nan-008 Pseudocode Overview: Distribution-Aware Metrics (CC@k and ICD)

## Components Involved

Six files are modified in two independent pipelines that share no compile-time
dependency. The boundary between them is the per-scenario JSON file on disk.

| Component | Pipeline | Change Type |
|-----------|----------|-------------|
| runner/output.rs | runner (async) | Struct field additions |
| runner/metrics.rs | runner (async) | Two new pure functions, one extended function |
| runner/replay.rs | runner (async) | Signature extension, new field population, metric calls |
| report/mod.rs | report (sync) | Mirror field additions + new types |
| report/aggregate.rs | report (sync) | Accumulation extension + new helper function |
| report/render.rs | report (sync) | Table extension + new section 6 |

## Data Flow

```
RUNNER PIPELINE (async, eval run command)
==========================================

EvalProfile.config_overrides.knowledge.categories  (Vec<String>)
                    |
                    | passed as &[String] to
                    v
replay.rs::run_single_profile(record, layer, k, configured_categories)
        |
        +-- maps search results to ScoredEntry {
        |       id, title, category,   <-- NEW: se.entry.category
        |       final_score, similarity, confidence, status, nli_rerank_delta
        |   }
        |
        +-- calls compute_cc_at_k(&entries, configured_categories) -> f64
        +-- calls compute_icd(&entries) -> f64
        |
        +-- builds ProfileResult {
                entries, latency_ms, p_at_k, mrr,
                cc_at_k,  <-- NEW
                icd,      <-- NEW
            }

replay.rs::compute_comparison(profile_results, baseline_name)
        +-- reads candidate.cc_at_k - baseline.cc_at_k -> cc_at_k_delta  <-- NEW
        +-- reads candidate.icd - baseline.icd -> icd_delta              <-- NEW
        +-- builds ComparisonMetrics { ..existing.., cc_at_k_delta, icd_delta }

write_scenario_result(ScenarioResult { profiles, comparison, .. }) -> JSON file

REPORT PIPELINE (sync, eval report command)
============================================

JSON files on disk (including new fields)
        |
        | serde(default) on all new fields preserves backward compat
        v
report/mod.rs: deserialize ScenarioResult
        profiles: HashMap<String, ProfileResult { .., cc_at_k, icd }>
        comparison: ComparisonMetrics { .., cc_at_k_delta, icd_delta }

report/aggregate.rs::compute_aggregate_stats(results)
        +-- accumulates cc_at_k_sum, icd_sum per profile
        +-- accumulates cc_at_k_delta_sum, icd_delta_sum for non-baseline profiles
        +-- divides by count to populate mean_cc_at_k, mean_icd, cc_at_k_delta, icd_delta

report/aggregate.rs::compute_cc_at_k_scenario_rows(results)
        +-- collects per-scenario (scenario_id, query, baseline_cc_at_k,
            candidate_cc_at_k, cc_at_k_delta) tuples
        +-- sorts by cc_at_k_delta descending
        -> Vec<CcAtKScenarioRow>

report/render.rs::render_report(stats, results, regressions, latency_buckets,
                                entry_rank_changes, query_map, cc_at_k_rows)
        +-- extends section 1 table with CC@k, ICD, CC@k Delta, ICD Delta columns
        +-- appends section 6 Distribution Analysis after section 5
        -> String (Markdown)
```

## Shared Types Introduced or Modified

All new types and field additions — defined in full in per-component files.

### runner/output.rs (canonical runner types)

```
ScoredEntry:
  + category: String                        (from se.entry.category)

ProfileResult:
  + cc_at_k: f64
  + icd: f64

ComparisonMetrics:
  + cc_at_k_delta: f64                      (candidate.cc_at_k - baseline.cc_at_k)
  + icd_delta: f64                          (candidate.icd - baseline.icd)
```

### report/mod.rs (deserialization copies — mirror of runner types)

```
ScoredEntry:
  + category: String   [serde(default)]

ProfileResult:
  + cc_at_k: f64       [serde(default)]
  + icd: f64           [serde(default)]

ComparisonMetrics:
  + cc_at_k_delta: f64 [serde(default)]
  + icd_delta: f64     [serde(default)]

AggregateStats (internal, not serialized):
  + mean_cc_at_k: f64
  + mean_icd: f64
  + cc_at_k_delta: f64                      (mean of per-scenario cc_at_k_delta values)
  + icd_delta: f64                          (mean of per-scenario icd_delta values)

CcAtKScenarioRow (new internal type):
  scenario_id: String
  query: String
  baseline_cc_at_k: f64
  candidate_cc_at_k: f64
  cc_at_k_delta: f64
```

## Sequencing Constraints

1. `runner/output.rs` must be modified before `runner/metrics.rs` and
   `runner/replay.rs` — both import `ScoredEntry` from `super::output`.

2. `runner/metrics.rs` must have `compute_cc_at_k` and `compute_icd` before
   `runner/replay.rs` calls them.

3. `report/mod.rs` must define `CcAtKScenarioRow` and the updated `AggregateStats`
   before `report/aggregate.rs` produces them and `report/render.rs` consumes them.

4. Both type-copy files (`runner/output.rs` and `report/mod.rs`) must be updated
   in the same commit (dual-copy atomicity, NFR-08, ADR-003).

5. The round-trip integration test (`report/tests.rs`) depends on all six components
   being complete. It must not be written until all components compile.

## Critical Invariants

- `compute_icd` MUST skip zero-count categories. It must never evaluate
  `0.0 * f64::ln(0.0)` which produces NaN.
- CC@k uses intersection semantics: numerator counts only categories that are
  both in `entries` AND in `configured_categories`. This caps CC@k at 1.0.
- `serde(default)` is MANDATORY on every new field in `report/mod.rs` deserialization
  types. A missing annotation causes `eval report` to fail on pre-nan-008 JSON files.
- `render_report` in `report/render.rs` gains the `cc_at_k_rows` parameter. The
  call site in `report/mod.rs::run_report` must pass it.
