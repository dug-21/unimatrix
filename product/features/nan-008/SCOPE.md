# nan-008: Distribution-Aware Metrics (CC@k and ICD) for Eval Harness

## Problem Statement

The eval harness (nan-007) measures retrieval quality using P@K and MRR, both of
which compare a candidate's results against soft ground truth derived from the
current production system. For features that *deliberately* change the result
distribution — PPR (#398), Contradicts suppression (#395), phase-conditioned
retrieval — this produces false regression signals for correct behavior.

The root cause is access imbalance: high-confidence categories (e.g. `decision`)
dominate soft ground truth because they dominate historical retrieval. A candidate
that surfaces a `lesson-learned` entry the biased baseline never returned is
penalized, not rewarded. P@K and MRR cannot distinguish "results changed for the
worse" from "results changed for the better."

Two complementary metrics that require no ground truth labels and are computable
from any result set are needed to close this gap: CC@k (Category Coverage at k)
and ICD (Intra-query Category Diversity).

## Goals

1. Add `cc_at_k` and `icd` fields to `ProfileResult` in the runner output JSON,
   computed per scenario per profile during `eval run`.
2. Add `cc_at_k_delta` and `icd_delta` to `ComparisonMetrics` in the runner
   output JSON.
3. Surface CC@k and ICD in the `eval report` summary table alongside P@K and MRR,
   with delta columns.
4. Add a Distribution Analysis section (section 6) to the report showing CC@k
   distribution across scenarios and the top improved / degraded scenarios by
   CC@k.
5. Source `configured_categories` from `KnowledgeConfig.categories` (the profile's
   config_overrides) so the metric is domain-agnostic — it adapts to any deployment
   without hardcoding.
6. Document CC@k and ICD in the "Understanding the metrics" section of
   `docs/testing/eval-harness.md`.
7. Record the current baseline CC@k and ICD in
   `product/test/eval-baselines/log.jsonl`.
8. Ship unit tests for the metric functions covering four required cases: CC@k
   with all categories present, CC@k with one category, ICD at maximum entropy,
   ICD single-category.

## Non-Goals

- **NEER (Novel Entry Exposure Rate)** is explicitly deferred. It requires
  session context across multiple queries and is not computable per-scenario.
  Revisit after #397 and session-level eval design.
- **No scenario format changes.** The JSONL scenario format is unchanged. CC@k
  and ICD are computed entirely from the replay result entries; no new scenario
  fields are needed.
- **No dependency on #394 or #397.** These metrics can land immediately without
  graph features or phase-in-scenarios.
- **No automated shipping gate.** The report exits 0 regardless of metric values;
  the CC@k target (>= 0.7 for PPR gate) is a human-reviewed target, not a
  hard CI gate.
- **No changes to the scenario extraction pipeline** (`eval scenarios` command).
- **No changes to the snapshot or live-path clients** (D1, D5, D6).
- **No ICD per-phase breakdown.** The per-phase breakdown of ICD requires #397
  (phase-in-scenarios) and is deferred.
- **No changes to the regression detection logic** (section 5 of the report).
  CC@k and ICD are informational, not gated.

## Background Research

### Existing eval harness structure

The eval harness (nan-007) lives in `crates/unimatrix-server/src/eval/` and has
four sub-modules:

- **runner/** — `eval run` logic; the change surface for metric computation.
  - `metrics.rs` — pure metric functions (`compute_p_at_k`, `compute_mrr`,
    `compute_tau_safe`, `compute_rank_changes`, `compute_comparison`). CC@k and
    ICD add `compute_cc_at_k` and `compute_icd` here.
  - `output.rs` — result type definitions (`ProfileResult`, `ComparisonMetrics`,
    `ScenarioResult`, `ScoredEntry`, `RankChange`). CC@k and ICD fields extend
    `ProfileResult` and `ComparisonMetrics` here.
  - `replay.rs` — orchestrates scenario replay; calls metric functions and
    assembles `ProfileResult`. Will need to pass `configured_categories` to
    `compute_cc_at_k` and `compute_icd`.
  - `mod.rs` — entry point; passes the profile's `config_overrides` to the
    replay loop.
- **report/** — `eval report` logic; the change surface for rendering.
  - `mod.rs` — deserializes result JSON; defines `ProfileResult`,
    `ComparisonMetrics`, `AggregateStats`. Must be updated to deserialize the
    new fields and extend `AggregateStats`.
  - `aggregate.rs` — `compute_aggregate_stats` (averages per profile),
    `find_regressions`. Extend stats accumulation for CC@k and ICD.
  - `render.rs` — `render_report`; builds the markdown. Extend the Summary
    table and add the Distribution Analysis section.
- **scenarios/** — read-only; not modified.
- **profile/** — provides `EvalProfile.config_overrides: UnimatrixConfig`;
  `config_overrides.knowledge.categories` is the denominator for CC@k.

### Type structure

`ProfileResult` (runner/output.rs) currently holds:
`entries`, `latency_ms`, `p_at_k`, `mrr`.

`ComparisonMetrics` (runner/output.rs) currently holds:
`kendall_tau`, `rank_changes`, `mrr_delta`, `p_at_k_delta`, `latency_overhead_ms`.

Both report/mod.rs and runner/output.rs define these types independently (report
module uses local deserialization copies, not compile-time imports from runner).
Both copies must be updated.

### KnowledgeConfig.categories as denominator

`KnowledgeConfig` is defined in `crates/unimatrix-server/src/infra/config.rs`.
Its `categories` field (`Vec<String>`) defaults to the 7 `INITIAL_CATEGORIES`
(`lesson-learned`, `decision`, `convention`, `pattern`, `procedure`,
`lesson-learned` variants, `duty`). A deployment can override this list; CC@k
uses `|configured_categories|` as the denominator so it is automatically correct
for any deployment.

In the runner, `configured_categories` is accessible via
`profile.config_overrides.knowledge.categories` at the point where
`run_single_profile` (replay.rs) assembles the `ProfileResult`. The category list
must be passed in to the computation functions; it is not stored on
`EvalServiceLayer`.

The entries in the replay result carry `se.entry.category` (String), sourced
from `EntryRecord.category` in `unimatrix-store`. This is the value to collect
for coverage.

### ScoredEntry in runner/output.rs does not include category

The runner's `ScoredEntry` struct currently records `id`, `title`, `final_score`,
`similarity`, `confidence`, `status`, `nli_rerank_delta`. The `category` field
from `se.entry.category` is discarded during mapping. To compute CC@k and ICD,
either:
- Compute them inline before constructing `ScoredEntry` (using the full service
  result), or
- Add `category: String` to the runner's `ScoredEntry` so category is
  available in the output JSON and in the report renderer.

Adding `category` to `ScoredEntry` is the cleaner approach: it preserves the
information for future metrics, and the report module's `ScoredEntry` copy can
also be extended, enabling future per-entry category display in the report.

### eval-harness.md location

`docs/testing/eval-harness.md` is the single documentation file for the
harness. The "Understanding the metrics" section (line ~397) currently covers
P@K and MRR. CC@k and ICD subsections must be appended there.

### eval-baselines/log.jsonl format

Entries are JSON objects with fields: `date`, `scenarios`, `p_at_k`, `mrr`,
`avg_latency_ms`, `feature_cycle`, `note` (and optionally `profile`). The
README specifies append-only semantics. New fields `cc_at_k` and `icd` must be
added to the log entry format (additive; existing entries without these fields
remain valid).

### Unimatrix pattern entry #2806

"Eval harness: profile TOML → snapshot → scenario replay → A/B report pattern"
confirms the established pattern: metrics are pure functions over the result
list and comparison is baseline-vs-first-candidate. CC@k and ICD follow this
same pattern.

## Proposed Approach

### Metric formulas

**CC@k (Category Coverage at k):**
```
CC@k = |{cat : exists entry in top-k with entry.category = cat}| / |configured_categories|
```
Range: [0.0, 1.0]. Returns 0.0 if `configured_categories` is empty (guard
against division by zero).

**ICD (Intra-query Category Diversity — Shannon entropy):**
```
ICD = -sum_cat [ p(cat) * log(p(cat)) ]
where p(cat) = count(entries with category=cat) / total entries in result
Maximum = log(|configured_categories|)
```
Range: [0.0, log(n_categories)]. Uses natural log. Returns 0.0 for empty
results. Single-category result → ICD = 0.0. Uniform distribution across
n categories → ICD = log(n).

### Change surface 1: eval run (runner/)

1. **`runner/output.rs`**: Extend `ScoredEntry` with `category: String`.
   Extend `ProfileResult` with `cc_at_k: f64` and `icd: f64`. Extend
   `ComparisonMetrics` with `cc_at_k_delta: f64` and `icd_delta: f64`.

2. **`runner/metrics.rs`**: Add `compute_cc_at_k(entries, configured_categories)`
   and `compute_icd(entries)`. Both are pure functions with no I/O. Add to
   `compute_comparison` the delta fields by reading `cc_at_k` and `icd` from
   baseline and candidate `ProfileResult`.

3. **`runner/replay.rs`**: In `run_single_profile`, receive
   `configured_categories: &[String]` parameter. After building the `entries`
   vec (with `category` now included), call `compute_cc_at_k` and `compute_icd`.
   Pass the list from `profile.config_overrides.knowledge.categories` when calling
   `run_single_profile` from `replay_scenario`.

### Change surface 2: eval report (report/)

4. **`report/mod.rs`**: Extend the local `ScoredEntry`, `ProfileResult`,
   `ComparisonMetrics` deserialization copies with the same new fields (all with
   `#[serde(default)]` for backward compatibility with pre-nan-008 result JSON).
   Extend `AggregateStats` with `mean_cc_at_k: f64`, `mean_icd: f64`,
   `cc_at_k_delta: f64`, `icd_delta: f64`.

5. **`report/aggregate.rs`**: Extend `compute_aggregate_stats` accumulation for
   CC@k and ICD sums and deltas. No new aggregate functions needed.

6. **`report/render.rs`**: Extend the Summary section table header and row
   format to include CC@k and ICD columns. Add section 6 Distribution Analysis:
   show CC@k range (min/max/mean across scenarios) per profile; list top-5
   scenarios by CC@k improvement and top-5 by CC@k degradation (baseline vs.
   candidate).

### Change surface 3: docs and baselines

7. **`docs/testing/eval-harness.md`**: Add CC@k and ICD subsections to
   "Understanding the metrics". Update the result JSON example to show the new
   fields. Update the Step 5 "Record the baseline" example to include `cc_at_k`
   and `icd` fields.

8. **`product/test/eval-baselines/log.jsonl`**: Append a new baseline entry
   after running nan-008 with `cc_at_k` and `icd` fields populated.

9. **`product/test/eval-baselines/README.md`**: Add `cc_at_k` and `icd` to
   the field spec table.

## Acceptance Criteria

- AC-01: `cc_at_k` (f64) is present in each profile's result object in the
  per-scenario JSON produced by `eval run`.
- AC-02: `icd` (f64) is present in each profile's result object in the
  per-scenario JSON produced by `eval run`.
- AC-03: `cc_at_k_delta` and `icd_delta` are present in the `comparison`
  object in the per-scenario JSON produced by `eval run`.
- AC-04: The `eval report` summary table includes CC@k and ICD columns and
  delta columns alongside P@K and MRR.
- AC-05: The `eval report` includes a Distribution Analysis section (section 6)
  showing per-profile CC@k range and the scenarios with the largest CC@k
  improvement and degradation (when two profiles are present).
- AC-06: `configured_categories` is sourced from
  `profile.config_overrides.knowledge.categories` — no hardcoded category
  lists in metric computation code.
- AC-07: Pre-nan-008 result JSON files (without `cc_at_k`/`icd` fields) are
  deserialized by `eval report` without error; missing fields default to 0.0
  via `#[serde(default)]`.
- AC-08: CC@k and ICD are documented in the "Understanding the metrics" section
  of `docs/testing/eval-harness.md`, including formulas and interpretation
  guidance.
- AC-09: The current baseline CC@k and ICD values are recorded in
  `product/test/eval-baselines/log.jsonl` as part of this feature.
- AC-10: Unit tests in `runner/tests_metrics.rs` cover: CC@k = 1.0 when all
  configured categories appear in results; CC@k = 1/n when exactly one category
  appears; ICD = log(n) (maximum entropy) when results are uniformly spread
  across n categories; ICD = 0.0 when all results share one category.
- AC-11: `ScoredEntry` in `runner/output.rs` includes `category: String` so
  category information is preserved in the result JSON for future use.

## Constraints

1. **No scenario format changes.** The `ScenarioRecord` JSONL format must not
   change — this is a purely additive change to the output JSON and report.

2. **Backward-compatible deserialization.** The report module deserializes
   result JSON files without a compile-time dependency on the runner types.
   All new fields in the deserialization copies must use `#[serde(default)]`
   so old result files remain readable.

3. **Pure functions only in metrics.rs.** All metric computations are pure
   functions with no I/O, no async, no database access — consistent with the
   existing pattern in `runner/metrics.rs`.

4. **report/ module is synchronous.** `report/mod.rs` explicitly notes it is
   "entirely synchronous: pure filesystem reads and string formatting." This
   constraint must be maintained — no tokio, no async in any report code path.

5. **Dual type copies.** `runner/output.rs` and `report/mod.rs` maintain
   independent copies of `ScoredEntry`, `ProfileResult`, and
   `ComparisonMetrics`. Both copies must be updated in sync.

6. **Division-by-zero guard.** `compute_cc_at_k` must return 0.0 when
   `configured_categories` is empty rather than panicking.

7. **Natural log for ICD.** Shannon entropy uses `f64::ln()` (natural log),
   consistent with the issue formula. The maximum value documentation must
   match: `log(n)` means natural log.

8. **Two-profile assumption for Distribution Analysis.** The Distribution
   Analysis section only shows baseline-vs-candidate comparison rows when two
   or more profiles are present. Single-profile runs omit the comparison
   sub-table (consistent with the existing Notable Ranking Changes section
   behavior).

9. **`eval run` does not accept a `--categories` flag.** The category list is
   always derived from the profile TOML's `[knowledge] categories` override
   or the compiled default — never from a CLI flag. This keeps the eval
   configuration self-contained in profile TOMLs.

## Open Questions

1. **Category field in result JSON.** Adding `category` to `ScoredEntry` in
   `runner/output.rs` increases per-entry output size. With 1,761 scenarios ×
   5 entries × category string (~15 chars average), the total added bytes are
   negligible (~132 KB across all result files). No concern, but confirm this
   is the intended approach versus computing CC@k and ICD inline without
   persisting category.

2. **ICD denominator: configured_categories vs. actual result entries.** The
   issue formula defines ICD entropy over the category distribution *in the
   result set* (not over all configured categories). A result with 5 entries
   from 2 categories has entropy computed over `p(cat_A)` and `p(cat_B)`. This
   is independent of `|configured_categories|`. CC@k uses
   `configured_categories` as denominator; ICD uses only the actual result
   distribution. This interpretation matches the issue. Confirm no normalization
   against `configured_categories` is expected for ICD.

3. **Baseline recording is manual.** The issue says "current baseline CC@k and
   ICD recorded in log.jsonl." This requires running `eval run` against the
   current snapshot with the new binary. The delivery agent must perform this
   run and append the result. Is the snapshot available, or does the delivery
   agent need to produce one as part of the feature work?

## Tracking

GH Issue: #399
