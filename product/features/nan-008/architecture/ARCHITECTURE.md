# nan-008: Distribution-Aware Metrics — Architecture

## System Overview

nan-008 adds CC@k (Category Coverage at k) and ICD (Intra-query Category
Diversity) to the eval harness introduced in nan-007. The eval harness lives
entirely in `crates/unimatrix-server/src/eval/` and is structured as two
independent pipelines: `runner/` (async `eval run` command) and `report/`
(synchronous `eval report` command). These two pipelines share no compile-time
type dependencies; the report module deserializes runner output from JSON files
on disk.

CC@k and ICD are purely additive metrics. They do not change scenario
extraction (`scenarios/`), profile loading (`profile/`), or the baseline/live
snapshot plumbing. They extend four existing files in place and add no new
modules.

## Component Breakdown

### runner/metrics.rs — Metric computation

Responsibility: Pure functions computing retrieval metrics from `ScoredEntry`
slices. No I/O, no async, no database access.

New functions added in nan-008:
- `compute_cc_at_k(entries: &[ScoredEntry], configured_categories: &[String]) -> f64`
- `compute_icd(entries: &[ScoredEntry]) -> f64`
- `compute_comparison` extended to populate `cc_at_k_delta` and `icd_delta`

### runner/output.rs — Runner result types

Responsibility: Type definitions for all JSON result structures produced by
`eval run`. These types are `Serialize + Deserialize` and written per-scenario.

Changes in nan-008:
- `ScoredEntry`: add `category: String`
- `ProfileResult`: add `cc_at_k: f64`, `icd: f64`
- `ComparisonMetrics`: add `cc_at_k_delta: f64`, `icd_delta: f64`

### runner/replay.rs — Scenario replay orchestration

Responsibility: Loads scenarios from JSONL, calls the service layer per
profile, assembles `ProfileResult` and writes output.

Changes in nan-008:
- `run_single_profile` receives `configured_categories: &[String]` as an
  additional parameter
- After assembling the `entries` vec (now including `category`), calls
  `compute_cc_at_k` and `compute_icd`
- `replay_scenario` passes `&profile.config_overrides.knowledge.categories`
  when calling `run_single_profile`

Ownership trace (SR-07 resolution): `profiles` is borrowed as `&[EvalProfile]`
throughout `run_replay_loop` and `replay_scenario`. `profile.config_overrides`
is never moved — only borrowed. Passing
`&profile.config_overrides.knowledge.categories` as `&[String]` to
`run_single_profile` is a shared borrow within the same async scope. No
lifetime conflict arises.

### report/mod.rs — Report deserialization types

Responsibility: Local deserialization-only copies of the runner types. Must
mirror runner/output.rs field-for-field so that result JSON files are
deserializable without a compile-time dependency on runner.

Changes in nan-008:
- `ScoredEntry`: add `category: String` with `#[serde(default)]`
- `ProfileResult`: add `cc_at_k: f64` and `icd: f64` both with
  `#[serde(default)]`
- `ComparisonMetrics`: add `cc_at_k_delta: f64` and `icd_delta: f64` both
  with `#[serde(default)]`
- `AggregateStats`: add `mean_cc_at_k: f64`, `mean_icd: f64`,
  `cc_at_k_delta: f64`, `icd_delta: f64`
- `default_comparison` updated to include the new delta fields set to 0.0
- New internal type `CcAtKScenarioRow` for Distribution Analysis rendering
  (see report/render.rs)

### report/aggregate.rs — Stats accumulation

Responsibility: Aggregates per-scenario results into per-profile summary
statistics.

Changes in nan-008:
- `compute_aggregate_stats` accumulates `cc_at_k_sum`, `icd_sum`,
  `cc_at_k_delta_sum`, `icd_delta_sum` alongside existing sums
- Divides by `count` to populate `mean_cc_at_k`, `mean_icd` in `AggregateStats`
- For non-baseline profiles: accumulates `cc_at_k_delta` and `icd_delta` from
  `comparison.cc_at_k_delta`

New helper function:
- `compute_cc_at_k_scenario_rows(results: &[ScenarioResult]) -> Vec<CcAtKScenarioRow>`
  Collects per-scenario CC@k values for the Distribution Analysis section.
  Returns `Vec` of `(scenario_id, query, baseline_cc_at_k, candidate_cc_at_k,
  cc_at_k_delta)` sorted by `cc_at_k_delta` descending.

### report/render.rs — Markdown rendering

Responsibility: Renders aggregated data into the final Markdown report.

Changes in nan-008:
- Section 1 (Summary table): header and row format extended with CC@k and ICD
  columns and their delta columns
- Section 6 (Distribution Analysis): new section appended after section 5

Section 6 content:
- Per-profile CC@k range table: min/max/mean across all scenarios
- ICD range table (same structure)
- When two or more profiles present: top-5 scenarios by CC@k improvement
  (largest positive delta) and top-5 by CC@k degradation (largest negative
  delta)
- ICD column header includes max-value annotation: `ICD (max=ln(n))`
- Single-profile runs: omit the improvement/degradation sub-tables (consistent
  with Section 2 Notable Ranking Changes behavior)

### runner/tests_metrics.rs — Unit tests for new metric functions

New tests added (AC-10):
1. `test_cc_at_k_all_categories_present` — CC@k = 1.0 when all configured
   categories appear in the top-k results
2. `test_cc_at_k_one_category_present` — CC@k = 1/n when exactly one of n
   configured categories appears
3. `test_icd_maximum_entropy` — ICD = ln(n) when results are uniformly
   distributed across n categories
4. `test_icd_single_category` — ICD = 0.0 when all results share one category
5. `test_cc_at_k_empty_configured_categories_returns_zero` — guard: no panic
6. `test_icd_empty_entries_returns_zero` — guard: no panic

### report/tests.rs — Round-trip integration test (SR-01 + SR-06)

New test added:
- `test_report_round_trip_cc_at_k_icd_fields_and_section_6` — writes a
  `ScenarioResult` JSON that includes `cc_at_k`, `icd`, `cc_at_k_delta`,
  `icd_delta` fields, then calls `run_report` and asserts:
  1. `cc_at_k` and `icd` columns appear in section 1
  2. Section 6 exists and appears after section 5
  3. Section order is strictly 1 < 2 < 3 < 4 < 5 < 6

This single test guards against both SR-01 (missing field in report copy) and
SR-06 (section-order regression).

## Component Interactions

```
eval run command
      |
      v
runner/mod.rs (run_eval → run_eval_async)
      |
      +-- for each profile/scenario:
      |       runner/replay.rs
      |             |
      |             +-- run_single_profile(&profile.config_overrides.knowledge.categories)
      |             |         |
      |             |         +-- maps search results → ScoredEntry (with category)
      |             |         +-- compute_cc_at_k(entries, configured_categories) → f64
      |             |         +-- compute_icd(entries) → f64
      |             |         +-- builds ProfileResult {entries, p_at_k, mrr, cc_at_k, icd}
      |             |
      |             +-- compute_comparison(profile_results, baseline_name) → ComparisonMetrics
      |             |         (now includes cc_at_k_delta, icd_delta)
      |             |
      |             +-- write_scenario_result (ScenarioResult JSON to disk)
      |
eval report command
      |
      v
report/mod.rs (run_report)
      |
      +-- deserialize ScenarioResult JSON files (with new fields via serde(default))
      +-- compute_aggregate_stats → AggregateStats (with mean_cc_at_k, mean_icd, deltas)
      +-- compute_cc_at_k_scenario_rows → Vec<CcAtKScenarioRow>
      +-- render_report → Markdown (sections 1–6)
      +-- write report.md
```

## Technology Decisions

See individual ADR files. Summary:

- **ADR-001**: `category: String` added to `ScoredEntry` in both type copies
  rather than computed inline and discarded. Preserves information for future
  metrics; accepted output-size cost is negligible (~132 KB across 1761
  scenarios).
- **ADR-002**: ICD uses raw Shannon entropy (`f64::ln`, natural log) without
  normalization. The ICD column in the report is labeled with its maximum value
  `ln(n_categories)` to prevent misinterpretation.
- **ADR-003**: Round-trip integration test required to guard both SR-01 (dual
  type copy divergence) and SR-06 (section-order regression) simultaneously.
- **ADR-004**: `tracing::warn!` emitted from `compute_cc_at_k` when
  `configured_categories` is empty to surface profile misconfiguration without
  panicking.
- **ADR-005**: Baseline recording is a named acceptance criterion step executed
  by the delivery agent after building, with an explicit command; the delivery
  agent checks for an existing snapshot first and creates one if absent.

## Integration Points

| Component | Dependency | Nature |
|-----------|------------|--------|
| `runner/replay.rs` | `profile.config_overrides.knowledge.categories` | Read-only borrow of `Vec<String>` from `EvalProfile` |
| `runner/replay.rs` | `se.entry.category` from `ScoredResult` | String field on `EntryRecord` from unimatrix-store |
| `runner/metrics.rs` | `runner/output.rs ScoredEntry` | Reads `category` field |
| `report/mod.rs` | `runner/output.rs` schema | JSON schema compatibility only (no compile-time import) |
| `report/aggregate.rs` | `report/mod.rs AggregateStats` | Adds four new f64 fields |
| `report/render.rs` | `report/aggregate.rs CcAtKScenarioRow` | New type passed to section 6 renderer |
| `docs/testing/eval-harness.md` | none | Documentation update only |
| `product/test/eval-baselines/log.jsonl` | none | Append-only baseline record |

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `ScoredEntry.category` | `String` | `runner/output.rs` and `report/mod.rs` (both copies) |
| `ProfileResult.cc_at_k` | `f64` | `runner/output.rs` and `report/mod.rs` (both copies) |
| `ProfileResult.icd` | `f64` | `runner/output.rs` and `report/mod.rs` (both copies) |
| `ComparisonMetrics.cc_at_k_delta` | `f64` | `runner/output.rs` and `report/mod.rs` (both copies) |
| `ComparisonMetrics.icd_delta` | `f64` | `runner/output.rs` and `report/mod.rs` (both copies) |
| `AggregateStats.mean_cc_at_k` | `f64` | `report/mod.rs` |
| `AggregateStats.mean_icd` | `f64` | `report/mod.rs` |
| `AggregateStats.cc_at_k_delta` | `f64` | `report/mod.rs` |
| `AggregateStats.icd_delta` | `f64` | `report/mod.rs` |
| `compute_cc_at_k` | `fn(entries: &[ScoredEntry], configured_categories: &[String]) -> f64` | `runner/metrics.rs` |
| `compute_icd` | `fn(entries: &[ScoredEntry]) -> f64` | `runner/metrics.rs` |
| `CcAtKScenarioRow` | `(scenario_id: String, query: String, baseline_cc_at_k: f64, candidate_cc_at_k: f64, cc_at_k_delta: f64)` | `report/aggregate.rs` or `report/mod.rs` |
| `run_single_profile` | adds `configured_categories: &[String]` parameter | `runner/replay.rs` (internal) |
| `compute_aggregate_stats` signature | unchanged externally; internal accumulation extended | `report/aggregate.rs` |
| `render_report` | adds `cc_at_k_rows: &[CcAtKScenarioRow]` parameter | `report/render.rs` |
| `KnowledgeConfig.categories` | `Vec<String>` | `crates/unimatrix-server/src/infra/config.rs` |
| `se.entry.category` (search result) | `String` from `EntryRecord` | `unimatrix-store` EntryRecord |

## Dual Type Copy Synchronization (SR-01)

The most significant implementation risk is the independent `ScoredEntry`,
`ProfileResult`, and `ComparisonMetrics` type copies in `runner/output.rs`
and `report/mod.rs`. A missing field in either copy produces silent
zero-valued metrics with no compile error.

The mitigation strategy is a mandatory round-trip integration test
(see ADR-003 and report/tests.rs). The test writes a result JSON containing
all new fields with non-zero values, runs `run_report`, and asserts their
presence in the rendered output. If the report copy is missing a field,
`serde(default)` silently zeroes it and the assertion catches it.

Delivery agents must update both copies in the same commit. The checklist:

1. `runner/output.rs ScoredEntry` — add `category: String`
2. `runner/output.rs ProfileResult` — add `cc_at_k: f64`, `icd: f64`
3. `runner/output.rs ComparisonMetrics` — add `cc_at_k_delta: f64`, `icd_delta: f64`
4. `report/mod.rs ScoredEntry` — add `category: String` with `#[serde(default)]`
5. `report/mod.rs ProfileResult` — add `cc_at_k: f64`, `icd: f64` with `#[serde(default)]`
6. `report/mod.rs ComparisonMetrics` — add `cc_at_k_delta: f64`, `icd_delta: f64` with `#[serde(default)]`
7. Run the round-trip test to verify both copies are in sync

## Baseline Recording Procedure (SR-04)

The delivery agent must complete baseline recording as a named step:

1. Check whether `product/test/eval-baselines/` contains a current snapshot
   file. A snapshot is a `.db` file not equal to the live database path.
2. If no snapshot exists: run `eval snapshot --db <live-db> --out <snapshot-path>`
   to create one before proceeding.
3. Run `eval run --db <snapshot-path> --scenarios <scenarios-path> --profiles <baseline-profile> --out <results-dir>`
   using the newly built binary.
4. Extract `cc_at_k` and `icd` mean values from the result directory.
5. Append a new entry to `product/test/eval-baselines/log.jsonl` with fields:
   `date`, `scenarios`, `p_at_k`, `mrr`, `avg_latency_ms`, `cc_at_k`, `icd`,
   `feature_cycle: "nan-008"`, `note`.
6. Update `product/test/eval-baselines/README.md` field spec table to include
   `cc_at_k` and `icd`.

## Open Questions

None unresolved. Prior open questions from SCOPE.md are answered by scope
decisions recorded in ADRs:

- OQ-1 (category in ScoredEntry vs. inline): resolved by ADR-001 — add to struct.
- OQ-2 (ICD denominator): resolved — ICD uses only the actual result distribution,
  not configured_categories. This matches the issue formula.
- OQ-3 (baseline snapshot availability): resolved by ADR-005 — delivery agent
  checks first and creates snapshot if absent.
