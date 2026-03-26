# nan-008 Researcher Agent Report

## Summary

SCOPE.md written to `product/features/nan-008/SCOPE.md`.

## Key Findings

### Eval harness structure

The eval harness has two distinct change surfaces:

**eval run (runner/)** — where metrics are computed and written to JSON:
- `runner/metrics.rs` — pure metric functions. CC@k and ICD add two new functions here.
- `runner/output.rs` — result type definitions. `ProfileResult` and `ComparisonMetrics` extend here; `ScoredEntry` must also gain a `category: String` field.
- `runner/replay.rs` — orchestrates replay; calls metric functions with the entries list. Must receive `configured_categories` from the profile and pass it to the new metric functions.
- `runner/mod.rs` — already has access to `EvalProfile.config_overrides.knowledge.categories`.

**eval report (report/)** — where metrics are aggregated and rendered:
- `report/mod.rs` — independent deserialization copies of all result types (does not import from runner). All new fields need `#[serde(default)]` here.
- `report/aggregate.rs` — `compute_aggregate_stats` needs to accumulate CC@k and ICD sums; `AggregateStats` struct needs four new fields.
- `report/render.rs` — Summary table header/rows extend; new section 6 (Distribution Analysis) added.

### Critical constraint: dual-type copies

`runner/output.rs` and `report/mod.rs` each define their own copy of `ScoredEntry`, `ProfileResult`, `ComparisonMetrics`, and `ScenarioResult`. The report module deliberately does not import from the runner. This means every new field must be added to both copies. The report copy needs `#[serde(default)]` on all new fields for backward compatibility with pre-nan-008 result JSON files.

### KnowledgeConfig.categories is the correct denominator

`KnowledgeConfig` lives in `crates/unimatrix-server/src/infra/config.rs`. Its `categories` field defaults to `INITIAL_CATEGORIES` (7 categories). Profile TOMLs can override this via `[knowledge] categories = [...]`. The runner accesses this as `profile.config_overrides.knowledge.categories` — this is available at the point `replay_scenario` is called in `replay.rs`.

### category is not currently preserved in ScoredEntry

The runner's `run_single_profile` maps `se.entry.category` but does not include it in `ScoredEntry`. CC@k and ICD can be computed inline during the mapping (before discarding category), but adding `category: String` to `ScoredEntry` is cleaner and preserves the information in the output JSON.

### ScoredEntry in services is distinct from eval ScoredEntry

`services/search.rs` defines its own `ScoredEntry { entry: EntryRecord, final_score, similarity }`. The eval runner maps from this to its own `eval/runner/output.rs::ScoredEntry`. `EntryRecord.category` (from `unimatrix-store/src/schema.rs`) is the source field.

### eval-harness.md is the sole documentation file

Located at `docs/testing/eval-harness.md`. The "Understanding the metrics" section starts at line ~397. Two new subsections for CC@k and ICD are additive. The result JSON examples and the Step 5 baseline recording example also need updating.

### Baselines log format

`product/test/eval-baselines/log.jsonl` is append-only. Existing entries have: `date`, `scenarios`, `p_at_k`, `mrr`, `avg_latency_ms`, `feature_cycle`, `note` (and optionally `profile`). New `cc_at_k` and `icd` fields are additive. The README specifies the field schema and must be updated. Baseline recording requires an actual eval run with the nan-008 binary — this is a delivery-time task, not a design-time task.

### No changes to scenarios, profiles, or snapshot modules

The `scenarios/`, `profile/`, and snapshot modules are untouched. This feature is purely additive to the runner output and report rendering.

### Existing test structure

Unit tests for metric functions live in `runner/tests_metrics.rs` (not in `runner/tests.rs`, which handles integration). New CC@k and ICD unit tests belong in `runner/tests_metrics.rs`, following the existing pattern of pure-function tests with no DB or filesystem access.

## Files That Need Changes

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/eval/runner/output.rs` | Add `category: String` to `ScoredEntry`; add `cc_at_k`, `icd` to `ProfileResult`; add `cc_at_k_delta`, `icd_delta` to `ComparisonMetrics` |
| `crates/unimatrix-server/src/eval/runner/metrics.rs` | Add `compute_cc_at_k`, `compute_icd`; extend `compute_comparison` for new deltas |
| `crates/unimatrix-server/src/eval/runner/replay.rs` | Pass `configured_categories` to metric functions; include `category` in `ScoredEntry` construction |
| `crates/unimatrix-server/src/eval/report/mod.rs` | Extend `ScoredEntry`, `ProfileResult`, `ComparisonMetrics` copies with new fields (`#[serde(default)]`); extend `AggregateStats` |
| `crates/unimatrix-server/src/eval/report/aggregate.rs` | Extend `compute_aggregate_stats` for CC@k and ICD accumulation |
| `crates/unimatrix-server/src/eval/report/render.rs` | Extend summary table; add Distribution Analysis section |
| `crates/unimatrix-server/src/eval/runner/tests_metrics.rs` | Add 4 required unit tests for CC@k and ICD |
| `docs/testing/eval-harness.md` | Add CC@k and ICD to "Understanding the metrics"; update result JSON example; update baseline recording example |
| `product/test/eval-baselines/log.jsonl` | Append baseline entry with `cc_at_k` and `icd` (delivery-time, requires actual run) |
| `product/test/eval-baselines/README.md` | Add `cc_at_k` and `icd` to field spec |

## Open Questions for Human

1. **Category in ScoredEntry output JSON.** Adding `category: String` to the runner's `ScoredEntry` makes it available in the result JSON (useful for future per-entry analysis). The alternative is computing CC@k/ICD inline without persisting `category`. Preference?

2. **ICD normalization.** ICD is Shannon entropy over the *actual result distribution* (not normalized against `|configured_categories|`). For a result set with all entries from one category, ICD = 0.0 regardless of how many categories are configured. This is the correct interpretation per the issue formula. Confirm this is expected — in particular, should ICD be displayed as a raw nats value or normalized to [0, 1] by dividing by `log(|configured_categories|)`?

3. **Baseline recording prerequisite.** AC-09 requires running `eval run` with the nan-008 binary to produce actual CC@k and ICD values. This requires a current snapshot. Can the delivery agent assume a snapshot is available at `/tmp/eval/snap.db`, or does it need to document a procedure for producing one as part of the feature?

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness metrics" — found entry #2806 confirming the established eval pattern; no CC@k/ICD patterns existed.
- Queried: `/uni-query-patterns` for "KnowledgeConfig categories" — found entry #2312 (boosted_categories validation gotcha) and #1886 (domain-neutral attribution fields); no prior patterns for category-as-eval-denominator.
- Stored: entry #3512 "Eval harness dual-type constraint: runner/output.rs and report/mod.rs maintain independent copies of result types" via `/uni-store-pattern`
