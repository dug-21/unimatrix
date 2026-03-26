# Specification: nan-008 — Distribution-Aware Metrics (CC@k and ICD) for Eval Harness

## Objective

The eval harness (nan-007) currently measures retrieval quality with P@K and MRR, both of which
compare results against soft ground truth derived from the production baseline. Features that
deliberately shift the result distribution (PPR, Contradicts suppression, phase-conditioned
retrieval) produce false regression signals under this regime. This feature adds two
ground-truth-free metrics — CC@k (Category Coverage at k) and ICD (Intra-query Category
Diversity) — to the eval runner output, eval report rendering, documentation, and baseline log,
so distribution-shifting features can be evaluated on their own terms.

---

## Functional Requirements

### FR-01: Extend `ScoredEntry` with `category`

`ScoredEntry` in `runner/output.rs` must gain a field `category: String` populated from
`se.entry.category` during the result-mapping step in `replay.rs`. The corresponding
deserialization copy of `ScoredEntry` in `report/mod.rs` must gain the same field annotated
`#[serde(default)]`.

Testable: the serialized per-scenario JSON for every profile result must contain a `"category"`
key on each entry object.

### FR-02: Add `cc_at_k` and `icd` to `ProfileResult`

`ProfileResult` in `runner/output.rs` must gain two fields:

```
cc_at_k: f64
icd: f64
```

Both are computed per scenario per profile during `eval run` and written into the JSON output.
The corresponding deserialization copy in `report/mod.rs` must include both fields annotated
`#[serde(default)]`.

Testable: the serialized per-scenario JSON for every profile result object must contain
`"cc_at_k"` and `"icd"` keys with f64 values.

### FR-03: Add `cc_at_k_delta` and `icd_delta` to `ComparisonMetrics`

`ComparisonMetrics` in `runner/output.rs` must gain two fields:

```
cc_at_k_delta: f64    // candidate.cc_at_k - baseline.cc_at_k
icd_delta: f64        // candidate.icd - baseline.icd
```

Both are computed in `compute_comparison` from the baseline and candidate `ProfileResult` values.
The corresponding deserialization copy in `report/mod.rs` must include both fields annotated
`#[serde(default)]`.

Testable: the `"comparison"` object in the per-scenario JSON must contain `"cc_at_k_delta"` and
`"icd_delta"` keys.

### FR-04: Implement `compute_cc_at_k` in `runner/metrics.rs`

Signature:

```rust
pub fn compute_cc_at_k(entries: &[ScoredEntry], configured_categories: &[String]) -> f64
```

Formula:

```
CC@k = |{cat : exists entry in entries with entry.category == cat}| / |configured_categories|
```

Rules:
- Returns `0.0` when `configured_categories` is empty (division-by-zero guard).
- Emits a `tracing::warn!` when `configured_categories` is empty, identifying the call site.
- Uses a `HashSet` (or equivalent) to collect distinct categories from `entries`.
- Is a pure function: no I/O, no async, no database access.
- Range: `[0.0, 1.0]`.

Testable: unit tests in `runner/tests_metrics.rs` verify the formula at boundary values (see
AC-10).

### FR-05: Implement `compute_icd` in `runner/metrics.rs`

Signature:

```rust
pub fn compute_icd(entries: &[ScoredEntry]) -> f64
```

Formula (raw Shannon entropy, natural log):

```
ICD = -sum_cat [ p(cat) * ln(p(cat)) ]
where p(cat) = count(entries with category == cat) / total entries
```

Rules:
- Returns `0.0` for empty `entries`.
- Returns `0.0` when all entries share one category (single-category result).
- Uses `f64::ln` (natural log). Maximum value is `ln(n_unique_categories_in_result)`.
- Is a pure function: no I/O, no async, no database access.
- Does NOT normalize against `configured_categories`; entropy is computed over the actual
  result distribution only.
- Range: `[0.0, ln(n)]` where `n` is the number of distinct categories present in `entries`.

Testable: unit tests in `runner/tests_metrics.rs` verify boundary values (see AC-10).

### FR-06: Wire metric computation in `replay.rs`

`run_single_profile` must accept `configured_categories: &[String]` as a parameter. After
assembling the `entries` vec (with `category` populated per FR-01), it calls:

```rust
let cc_at_k = compute_cc_at_k(&entries, configured_categories);
let icd = compute_icd(&entries);
```

The call site in `replay_scenario` (or equivalent orchestrator) must pass
`&profile.config_overrides.knowledge.categories` for this parameter.

Testable: an integration test that runs `eval run` against a fixture profile and fixture
scenarios must produce non-zero `cc_at_k` and `icd` values in the output JSON when the result
entries span multiple categories.

### FR-07: Extend `AggregateStats` and `compute_aggregate_stats` in `report/`

`AggregateStats` in `report/mod.rs` must gain:

```rust
mean_cc_at_k: f64
mean_icd: f64
cc_at_k_delta: f64    // mean of per-scenario cc_at_k_delta values
icd_delta: f64        // mean of per-scenario icd_delta values
```

`compute_aggregate_stats` in `report/aggregate.rs` must accumulate these values using the same
running-sum pattern used for `mean_p_at_k` and `mean_mrr`.

Testable: rendering a report from a multi-scenario result JSON produces correct aggregate values
that match manual calculation.

### FR-08: Extend the Summary table in `report/render.rs`

The rendered Summary section table must include CC@k and ICD columns adjacent to P@K and MRR.
When two profiles are present, delta columns `CC@k Δ` and `ICD Δ` must appear. Column order
within the table must be: `P@K | MRR | CC@k | ICD` (and deltas in the same order).

Testable: a full rendered-markdown test (see AC-13) asserts all four metric columns appear in
the correct order in the Summary section.

### FR-09: Add Distribution Analysis section (section 6) to `report/render.rs`

A new section 6 titled "Distribution Analysis" must be appended after section 5 (Regression
Detection / Notable Ranking Changes). It must contain:

1. A per-profile sub-table showing CC@k min, max, and mean across all scenarios in that profile.
2. When two or more profiles are present: a sub-table listing the top-5 scenarios by
   `cc_at_k_delta` improvement and the top-5 by `cc_at_k_delta` degradation (baseline vs.
   candidate). When fewer than 5 scenarios qualify in either direction, all qualifying scenarios
   are shown.
3. When only one profile is present, the comparison sub-table is omitted (consistent with the
   existing Notable Ranking Changes section behavior for single-profile runs).

Testable: a full rendered-markdown test (see AC-13) asserts section 6 appears after section 5
and contains the expected sub-tables.

### FR-10: Add ICD max-value annotation to the report ICD column

The ICD column header or a caption in the Distribution Analysis section must label the ICD
maximum as `ln(n_categories)` where `n_categories` is the number of configured categories for
that profile, so consumers can interpret absolute ICD values without silent
cross-profile miscomparisons.

Testable: the rendered Distribution Analysis section contains the string `ln(` in the ICD
annotation.

### FR-11: Document CC@k and ICD in `docs/testing/eval-harness.md`

The "Understanding the metrics" section must gain two subsections — one for CC@k and one for
ICD — each containing:
- The formula as written in FR-04 / FR-05.
- Range information and interpretation guidance.
- For ICD: explicit note that the range is `[0, ln(n_categories)]` and values are not
  comparable across profiles with different `n_categories` without normalization.
- The result JSON field names (`cc_at_k`, `icd`, `cc_at_k_delta`, `icd_delta`).

The result JSON example in the doc must be updated to include the new fields. The "Step 5:
Record the baseline" example must be updated to include `cc_at_k` and `icd` fields.

Testable: the file contains subsection headings for "CC@k" and "ICD" (or "Intra-query Category
Diversity") within the metrics section.

### FR-12: Record baseline entry in `product/test/eval-baselines/log.jsonl`

A new baseline log entry must be appended to `product/test/eval-baselines/log.jsonl` after
running `eval run` on the current snapshot with the nan-008 binary. The entry must include
`cc_at_k` and `icd` fields alongside the existing fields (`date`, `scenarios`, `p_at_k`, `mrr`,
`avg_latency_ms`, `feature_cycle`, `note`).

`product/test/eval-baselines/README.md` must be updated to add `cc_at_k` and `icd` to the
field specification table.

Testable: `log.jsonl` contains a line with `"feature_cycle"` matching `"nan-008"` and non-null
`"cc_at_k"` and `"icd"` values.

---

## Non-Functional Requirements

### NFR-01: Backward-compatible deserialization

All new fields added to deserialization copies in `report/mod.rs` must carry
`#[serde(default)]`. Pre-nan-008 result JSON files (which lack `cc_at_k`, `icd`,
`cc_at_k_delta`, `icd_delta`, and `category` on entries) must deserialize without error and
produce `0.0` / empty-string defaults. No schema migration is required.

### NFR-02: Pure metric functions

`compute_cc_at_k` and `compute_icd` must be pure functions: no I/O, no async, no database
access, no global mutable state. This is consistent with the existing functions in
`runner/metrics.rs` (`compute_p_at_k`, `compute_mrr`, `compute_tau_safe`).

### NFR-03: Synchronous report module

No tokio, no async, no `spawn_blocking` may be introduced in any file under `report/`. The
module comment "entirely synchronous: pure filesystem reads and string formatting" must remain
accurate.

### NFR-04: No hardcoded category lists

No metric computation code may contain a literal list of category strings. The denominator for
CC@k is always `configured_categories.len()`, sourced from
`profile.config_overrides.knowledge.categories` at the call site.

### NFR-05: No scenario format changes

`ScenarioRecord` (the JSONL scenario format) must not gain any new fields. CC@k and ICD are
computed entirely from the replay result entries.

### NFR-06: No CLI flag for category override

`eval run` must not accept a `--categories` flag. The category list is always derived from the
profile TOML `[knowledge] categories` field or the compiled defaults — never from the command
line.

### NFR-07: Output size acceptability

Adding `category: String` to `ScoredEntry` in the result JSON is acceptable. The estimated
overhead (~132 KB across all result files for 1,761 scenarios at k=5 with ~15-char average
category names) is well within storage constraints. The delivery agent must verify the actual
scenario count before the baseline run; if it has grown significantly, note the actual overhead
in the baseline log entry's `note` field. No design change is needed regardless.

### NFR-08: Dual-copy atomicity

`runner/output.rs` and `report/mod.rs` maintain independent copies of `ScoredEntry`,
`ProfileResult`, and `ComparisonMetrics`. Both copies must be updated in the same commit.
Partial updates (one copy updated, the other not) must not be submitted.

---

## Acceptance Criteria

### AC-01 — `cc_at_k` in runner output JSON

**Verification:** Run `eval run` against any profile + scenario set. Parse the output JSON.
Every per-scenario result object for every profile must contain a `"cc_at_k"` key with a
numeric value.

### AC-02 — `icd` in runner output JSON

**Verification:** Same run as AC-01. Every per-scenario result object for every profile must
contain an `"icd"` key with a numeric value.

### AC-03 — `cc_at_k_delta` and `icd_delta` in comparison object

**Verification:** Same run as AC-01 (with two profiles). The `"comparison"` object in each
per-scenario JSON must contain `"cc_at_k_delta"` and `"icd_delta"` keys with numeric values.

### AC-04 — Summary table includes CC@k and ICD columns

**Verification:** Run `eval report` on any result JSON. The rendered markdown Summary section
table must contain column headers `CC@k` and `ICD` (and delta columns when two profiles are
present) alongside `P@K` and `MRR`.

### AC-05 — Distribution Analysis section present

**Verification:** Run `eval report` on a multi-profile result JSON. The rendered markdown must
include a section 6 titled "Distribution Analysis" containing a per-profile CC@k range
(min/max/mean) sub-table and top-5 improvement / top-5 degradation scenario rows.

### AC-06 — No hardcoded category lists

**Verification:** Search `runner/metrics.rs`, `runner/replay.rs` for any literal string
matching a known category name (`"decision"`, `"convention"`, `"lesson-learned"`, etc.). None
must appear in metric computation or wiring code. The only source of the category list is
`profile.config_overrides.knowledge.categories`.

### AC-07 — Backward-compatible deserialization of pre-nan-008 JSON

**Verification:** Pass a pre-nan-008 result JSON file (one that lacks `cc_at_k`, `icd`,
`cc_at_k_delta`, `icd_delta`, and `category` on entries) to `eval report`. The command must
exit 0 and produce a report with those fields defaulted to `0.0` / empty string. No
deserialization error.

### AC-08 — Documentation updated

**Verification:** `docs/testing/eval-harness.md` contains subsection headings for CC@k and ICD
within the "Understanding the metrics" section. The result JSON example includes `cc_at_k` and
`icd`. The baseline recording example includes `cc_at_k` and `icd`.

### AC-09 — Baseline entry recorded

**Verification:** `product/test/eval-baselines/log.jsonl` contains a line whose
`"feature_cycle"` value is `"nan-008"` with non-null, non-zero `"cc_at_k"` and `"icd"` fields.
`product/test/eval-baselines/README.md` lists `cc_at_k` and `icd` in its field specification
table.

### AC-10 — Unit tests for metric boundary values

**Verification:** `runner/tests_metrics.rs` (or the equivalent test module) contains passing
tests for all four required cases:

1. `compute_cc_at_k` returns `1.0` when result entries include at least one entry from every
   configured category.
2. `compute_cc_at_k` returns `1/n` (where `n = configured_categories.len()`) when exactly one
   category appears across all result entries.
3. `compute_icd` returns `ln(n)` (within float tolerance) when result entries are uniformly
   distributed across `n` distinct categories.
4. `compute_icd` returns `0.0` when all result entries share one category.

### AC-11 — `ScoredEntry.category` present in runner output

**Verification:** Run `eval run`. Parse the per-scenario JSON. Every entry object within a
profile result's `"entries"` array must contain a `"category"` key with a non-empty string
value (assuming the snapshot contains entries with categories).

### AC-12 — Round-trip test: runner output deserializes correctly in report module (addresses SR-01)

**Verification:** An integration or end-to-end test must:
1. Run `eval run` (or call the runner logic) producing a result JSON with `cc_at_k`, `icd`,
   `cc_at_k_delta`, `icd_delta`, and per-entry `category` fields populated with non-zero /
   non-empty values.
2. Pass that result JSON to the report deserialization path (`report/mod.rs` types).
3. Assert that `profile_result.cc_at_k`, `profile_result.icd`,
   `comparison.cc_at_k_delta`, `comparison.icd_delta`, and `entry.category` are all
   non-zero / non-empty after deserialization.

This test catches the class of failure where one copy of a dual-type is updated and the other
is not.

### AC-13 — Full rendered-markdown test for section ordering (addresses SR-06)

**Verification:** A test in `report/` must render a complete report from a synthetic
multi-profile result fixture and assert on the rendered string that:
1. Section 5 (regression detection / notable ranking changes) appears before section 6.
2. Section 6 heading "Distribution Analysis" is present.
3. The Summary table contains `CC@k` and `ICD` columns.
4. No section appears out of order or duplicated.

The test must use `assert!(rendered.contains("## 6.") || rendered.contains("## Distribution"))` or
an equivalent substring check; exact formatting may vary, but section identity must be asserted.

### AC-14 — ICD column annotated with maximum value (addresses SR-03)

**Verification:** The rendered Distribution Analysis section (or ICD column header in the
Summary table) contains an annotation of the form `ln(N)` where N is the number of configured
categories, so consumers can interpret absolute ICD values. The test in AC-13 must also assert
this string appears in the Distribution Analysis section.

---

## Domain Models

### CC@k (Category Coverage at k)

A retrieval quality metric in the range `[0.0, 1.0]`. Measures the fraction of
`configured_categories` that are represented by at least one entry in the top-k result set.

```
CC@k = |distinct_categories(top_k_results)| / |configured_categories|
```

- `k` is the count of entries in the scenario's result set (determined by the profile's `top_k`
  setting, not a separate parameter to the function).
- `configured_categories` is the denominator; a result that covers all categories scores 1.0
  regardless of result count.
- Returns `0.0` if `configured_categories` is empty.

### ICD (Intra-query Category Diversity)

A raw Shannon entropy score over the category distribution within a single query's result set.
Uses natural log (`f64::ln`).

```
ICD = -sum_cat [ p(cat) * ln(p(cat)) ]
p(cat) = count(results with category=cat) / total results
```

- Range: `[0.0, ln(n_distinct_categories_in_result)]`.
- Maximum entropy `ln(n)` is achieved when results are uniformly distributed across `n`
  categories.
- ICD = 0.0 for empty results or single-category results.
- ICD values are NOT comparable across profiles with different `configured_categories` counts
  without normalization. This is by design; the report must surface the maximum value context
  (see FR-10, AC-14).

### `configured_categories`

The `Vec<String>` sourced from `profile.config_overrides.knowledge.categories` at the time
`run_single_profile` is called. Corresponds to `KnowledgeConfig.categories` in
`crates/unimatrix-server/src/infra/config.rs`. Defaults to the 7 `INITIAL_CATEGORIES`
(`lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duty`, and any configured
variants) when no `[knowledge]` override is present in the profile TOML.

This is the sole source of category denominator truth. It is passed as a parameter to
`compute_cc_at_k`; it is never read from a global or hardcoded.

### `ScoredEntry.category`

A `String` field added to `ScoredEntry` in both `runner/output.rs` and `report/mod.rs`.
Populated from `se.entry.category` (the `EntryRecord.category` field from `unimatrix-store`)
during result mapping in `replay.rs`. Preserved in the output JSON for future per-entry
category display and for any downstream metric that requires per-entry category access.

### `ProfileResult`

A per-scenario, per-profile aggregate that now includes `cc_at_k: f64` and `icd: f64` in
addition to the existing `entries`, `latency_ms`, `p_at_k`, `mrr` fields.

### `ComparisonMetrics`

A per-scenario baseline-vs-candidate differential that now includes `cc_at_k_delta: f64` and
`icd_delta: f64` in addition to the existing `kendall_tau`, `rank_changes`, `mrr_delta`,
`p_at_k_delta`, `latency_overhead_ms` fields.

### `AggregateStats`

A per-profile summary across all scenarios in the report that now includes `mean_cc_at_k: f64`,
`mean_icd: f64`, `cc_at_k_delta: f64`, and `icd_delta: f64`.

---

## User Workflows

### Workflow 1: Run eval with distribution metrics

1. User runs `eval run --profiles baseline.toml candidate.toml --output results/`.
2. For each scenario, `replay.rs` maps result entries to `ScoredEntry` (now including
   `category`), calls `compute_cc_at_k(&entries, &profile.config_overrides.knowledge.categories)`
   and `compute_icd(&entries)`, and stores results in `ProfileResult`.
3. `compute_comparison` computes `cc_at_k_delta` and `icd_delta` from the two profiles'
   `ProfileResult` values.
4. Output JSON for each scenario contains `cc_at_k`, `icd` on each profile result and
   `cc_at_k_delta`, `icd_delta` on the comparison object.

### Workflow 2: Generate report with Distribution Analysis

1. User runs `eval report --results results/ --output report.md`.
2. `report/mod.rs` deserializes result JSON files; new fields default to `0.0` via
   `#[serde(default)]` for pre-nan-008 files.
3. `aggregate.rs` accumulates `mean_cc_at_k`, `mean_icd`, and delta averages.
4. `render.rs` produces a Summary table with CC@k and ICD columns, then appends section 6
   Distribution Analysis with per-profile CC@k range and top-5 improvement/degradation
   scenario lists.

### Workflow 3: Evaluate a distribution-shifting feature (PPR gate)

1. Engineer runs `eval run` with baseline and PPR candidate profiles.
2. Reviews the Distribution Analysis section to check whether the candidate CC@k >= 0.7
   (human-reviewed target, not a hard CI gate).
3. If CC@k delta is negative, reviews the top-5 degraded scenarios to assess whether
   degradation is intentional (e.g., PPR correctly suppressing a biased category).

### Workflow 4: Record baseline after nan-008 delivery

1. Delivery agent runs `eval run` against the current snapshot using the nan-008 binary.
2. Reads `cc_at_k` and `icd` from the aggregate stats in the runner output or report.
3. Appends a new line to `product/test/eval-baselines/log.jsonl` with all required fields
   including `cc_at_k` and `icd`, and `"feature_cycle": "nan-008"`.

---

## Constraints

1. **No scenario format changes.** `ScenarioRecord` JSONL format is unchanged.

2. **Dual-copy atomicity.** `runner/output.rs` and `report/mod.rs` type copies must be updated
   in the same commit (addresses SR-01).

3. **`#[serde(default)]` on all new report deserialization fields.** Required for
   backward compatibility with pre-nan-008 result JSON files (NFR-01).

4. **Pure functions in `runner/metrics.rs`.** No I/O, no async, no side effects (NFR-02).

5. **Synchronous `report/` module.** No tokio or async introduced anywhere in `report/`
   (NFR-03).

6. **No hardcoded category lists in metric code.** Category denominator always comes from
   `configured_categories` parameter (NFR-04).

7. **Division-by-zero guard.** `compute_cc_at_k` returns `0.0` and emits `tracing::warn!`
   when `configured_categories` is empty (FR-04, addresses SR-02).

8. **Natural log.** `compute_icd` uses `f64::ln`. Maximum is `ln(n)`, not `log2(n)`. All
   documentation and report annotations must use natural log notation (FR-05).

9. **Two-profile assumption for Distribution Analysis comparison rows.** The top-5
   improvement / top-5 degradation sub-table is omitted for single-profile runs (FR-09).

10. **No `--categories` CLI flag.** Category list derives from profile TOML only (NFR-06).

11. **Baseline recording is a named delivery step.** The delivery agent must execute
    `eval run` against the current snapshot and append the result to
    `product/test/eval-baselines/log.jsonl` as an explicit named step — not an implied
    post-run action (addresses SR-04). If no snapshot exists at delivery time, the delivery
    agent is approved to create one as part of this feature; creation steps are the same as
    the existing snapshot procedure documented in `docs/testing/eval-harness.md`.

---

## Dependencies

### Internal crates

- `crates/unimatrix-server/src/eval/runner/` — primary change surface
- `crates/unimatrix-server/src/eval/report/` — secondary change surface
- `crates/unimatrix-server/src/infra/config.rs` — `KnowledgeConfig`, `UnimatrixConfig`
  (read-only; no changes needed here)
- `crates/unimatrix-store` — source of `EntryRecord.category` (read-only; no changes needed)

### External crates

No new crate dependencies are introduced. All metric computation uses `std` only (`HashSet`,
`f64::ln`, iterators).

### Existing components

- `runner/metrics.rs` — extended with two new pure functions
- `runner/output.rs` — three types extended: `ScoredEntry`, `ProfileResult`,
  `ComparisonMetrics`
- `runner/replay.rs` — `run_single_profile` signature extended with `configured_categories`
  parameter
- `report/mod.rs` — three local deserialization types extended; `AggregateStats` extended
- `report/aggregate.rs` — accumulation logic extended
- `report/render.rs` — Summary table extended; section 6 added
- `docs/testing/eval-harness.md` — metrics documentation extended
- `product/test/eval-baselines/log.jsonl` — baseline entry appended
- `product/test/eval-baselines/README.md` — field spec table updated

### GH issue

\#399

---

## NOT in Scope

- **NEER (Novel Entry Exposure Rate)** — deferred; requires session context across queries.
- **Per-phase ICD breakdown** — deferred; requires #397 (phase-in-scenarios).
- **Automated CC@k shipping gate** — the `>= 0.7` PPR target is human-reviewed only; `eval
  report` exits 0 regardless of metric values.
- **Scenario extraction pipeline changes** — `eval scenarios` command is unchanged.
- **Snapshot or live-path client changes** — D1, D5, D6 clients are unchanged.
- **Regression detection logic changes** — section 5 of the report is unchanged; CC@k and ICD
  are informational only.
- **ICD normalization** — ICD is raw entropy, not normalized against `configured_categories`.
  Normalization is a future enhancement.
- **NLI model or scoring pipeline changes** — this feature is metrics-only.
- **Changes to GH issues #394 or #397** — nan-008 has no dependency on those features.
- **`context_search` or store changes** — this feature does not modify the knowledge engine.

---

## Open Questions

### OQ-01: Snapshot availability at delivery time (SR-04)

The baseline recording step (AC-09, FR-12) requires running `eval run` against the current
snapshot. If no snapshot exists at the time of delivery, the delivery agent must create one
using the standard snapshot procedure. This is approved per Constraint 11. The architect should
confirm the snapshot procedure is documented and accessible before delivery begins.

### OQ-02: Ownership chain in `replay.rs` (SR-07)

`run_single_profile` will receive `configured_categories: &[String]`. The call site must pass
a borrow of `profile.config_overrides.knowledge.categories` without consuming or moving the
profile. The architect should trace whether `profile` is moved into any closure or async block
before this call site; if so, the categories must be cloned before the move. This is a
compilation concern, not a design concern, but should be confirmed during architecture.

### OQ-03: Test module location for metric unit tests

SCOPE.md references `runner/tests_metrics.rs`. The architect should confirm whether this is an
existing file (extending existing tests) or a new file to be created. AC-10 references it by
name; the delivery agent must use the confirmed location.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for eval harness metrics distribution -- found pattern #2806
  (eval harness profile→snapshot→replay→report pattern, confirms CC@k/ICD follow established
  pattern) and pattern #3512 (dual-type constraint, directly applicable to SR-01 mitigation).
  Convention results (#307, #235, #238) were general server conventions, not directly applicable
  to this feature. No contradictions or stale entries found.
