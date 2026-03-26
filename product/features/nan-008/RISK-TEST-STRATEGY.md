# Risk-Based Test Strategy: nan-008

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Dual type copy divergence: `runner/output.rs` updated but `report/mod.rs` not (or vice versa); `serde(default)` silently zeros the missing field in report output | High | High | Critical |
| R-02 | Section-order regression in `render.rs`: section 6 (Distribution Analysis) inserted at wrong position or duplicated; report is syntactically valid but semantically wrong | High | Med | High |
| R-03 | `compute_cc_at_k` called with empty `configured_categories`; returns 0.0 silently, producing a baseline log entry that looks valid but is meaningless | Med | Med | High |
| R-04 | ICD cross-profile miscomparison: report consumers read ICD values as directly comparable across profiles with different category counts without noticing the unbounded scale | Med | Med | High |
| R-05 | Float precision error in `compute_icd`: natural-log edge cases (p near 0 causing `ln(0)`) produce `-inf` or `NaN` that propagate silently into JSON and aggregate stats | High | Low | High |
| R-06 | Baseline recording skipped or uses stale snapshot; AC-09 `log.jsonl` entry absent or contains prior-feature values for `cc_at_k`/`icd` | Med | Med | High |
| R-07 | Pre-nan-008 result JSON files cause `eval report` to error when the new binary encounters them; backward-compat break despite `#[serde(default)]` intent | High | Low | High |
| R-08 | `ScoredEntry.category` missing from runner output JSON; metric functions receive empty-string categories; CC@k and ICD compute correctly on garbage input and pass tests without revealing the mapping bug | High | Low | High |
| R-09 | `KnowledgeConfig.categories` path produces an empty `Vec` for a profile that omits `[knowledge]` section entirely, causing CC@k = 0.0 for all scenarios without a clear error signal | Med | Low | Med |
| R-10 | `compute_comparison` delta fields (`cc_at_k_delta`, `icd_delta`) computed in wrong order (candidate − baseline inverted); produces plausible but sign-flipped deltas in the report | Med | Low | Med |
| R-11 | `compute_aggregate_stats` accumulation error: CC@k/ICD values divided by wrong count (e.g., total entries vs. total scenarios), producing aggregates that are off by factor of k | Med | Low | Med |
| R-12 | Distribution Analysis section 6 top-5 list uses wrong sort direction (ascending instead of descending delta) so "top improved" and "top degraded" are swapped | Med | Low | Med |
| R-13 | `eval snapshot` subcommand does not exist; delivery agent cannot complete AC-09 baseline recording without discovering the correct snapshot command at delivery time | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Dual type copy divergence
**Severity**: High
**Likelihood**: High
**Impact**: `eval report` silently outputs CC@k = 0.0 or ICD = 0.0 even when the runner correctly
computed non-zero values. Baseline log records wrong values. Distribution Analysis section shows
all zeros. No compile error surfaces the problem.
**Evidence**: Pattern #3512 names this as a confirmed recurring failure for the eval harness.
Pattern #3472 identifies the same atomic-update requirement in a different feature (col-027).

**Test Scenarios**:
1. `test_report_round_trip_cc_at_k_icd_fields_and_section_6` — write a `ScenarioResult` JSON
   with `cc_at_k: 0.857`, `icd: 1.234`, `cc_at_k_delta: 0.143`, `icd_delta: 0.211`; call
   `run_report`; assert all four non-zero values appear in the rendered markdown (AC-12 / ADR-003).
2. Verify the `report/mod.rs` `ProfileResult` deserialization copy has `cc_at_k` and `icd`
   annotated `#[serde(default)]` by attempting to deserialize a JSON object that omits them
   and asserting the resulting struct has `0.0` (not a deserialization error).

**Coverage Requirement**: The round-trip test (ADR-003) with non-zero, non-trivially-round
values is mandatory. The test must fail if either copy is missing a field.

---

### R-02: Section-order regression in render.rs
**Severity**: High
**Likelihood**: Med
**Impact**: Report is syntactically valid Markdown but section 6 appears before section 5, or
section 5 is duplicated, or section 6 is omitted. Human reviewers may not notice the ordering
is wrong. The PPR gate review would reference an incorrectly structured report.
**Evidence**: Pattern #3426 documents that formatter overhaul features consistently
underestimate section-order regression risk; golden-output test is required.

**Test Scenarios**:
1. `test_report_contains_all_six_sections` — extend the existing five-section order test to
   assert `pos("## 1.") < pos("## 2.") < ... < pos("## 5.") < pos("## 6.")` (AC-13 / ADR-003).
2. Single-profile run test: assert section 6 is present but omits the improvement/degradation
   comparison sub-tables (two-profile assumption, NFR constraint 9).

**Coverage Requirement**: Position assertion on the full rendered string with all six sections.
A `contains` check alone is insufficient — the ordering must be asserted.

---

### R-03: Empty `configured_categories` produces silent CC@k = 0.0
**Severity**: Med
**Likelihood**: Med
**Impact**: A profile TOML with `categories = []` produces CC@k = 0.0 for every scenario.
The baseline log entry records CC@k = 0.0, which looks like "no category coverage" but
means "denominator was zero — data is meaningless." Future features using this baseline
as a reference will regress against a garbage anchor.

**Test Scenarios**:
1. `test_cc_at_k_empty_configured_categories_returns_zero` — call `compute_cc_at_k` with an
   empty slice; assert return is `0.0` and no panic (AC-10 guard test).
2. Verify `tracing::warn!` path is reachable: confirm by code inspection that the early-return
   branch emits the warning before returning 0.0 (ADR-004).

**Coverage Requirement**: Guard test verifying 0.0 return and no panic. Warning emission does
not need to be asserted in tests (it is a diagnostic, not a contract).

---

### R-04: ICD cross-profile miscomparison
**Severity**: Med
**Likelihood**: Med
**Impact**: A reviewer compares ICD = 1.4 (7 categories, max = ln(7) ≈ 1.95) against
ICD = 1.4 (2 categories, max = ln(2) ≈ 0.69) and concludes they are equal, missing that the
second profile has saturated ICD. The PPR evaluation conclusion is incorrect.

**Test Scenarios**:
1. `test_report_icd_column_annotated_with_ln_n` — render a report and assert the rendered
   output contains `ln(` in the ICD column header or Distribution Analysis section (AC-14).
2. Check documentation: `docs/testing/eval-harness.md` ICD subsection contains the string
   "not comparable" or equivalent normalization caveat (AC-08 / FR-11).

**Coverage Requirement**: Rendered output annotation test. Documentation review for
comparability caveat in the ICD subsection.

---

### R-05: Float precision in `compute_icd` with near-zero probabilities
**Severity**: High
**Likelihood**: Low
**Impact**: If any `p(cat)` is exactly 0.0 (a category with zero entries appearing in the
iteration), `0.0 * f64::ln(0.0)` evaluates to `0.0 * -inf = NaN`. NaN propagates through
aggregation into `AggregateStats.mean_icd`, then into the rendered report. JSON serialization
of NaN may produce `null` or `nan` depending on the serializer, potentially causing
`eval report` to error or produce malformed output.

**Test Scenarios**:
1. `test_icd_single_category` — all entries share one category; assert ICD = 0.0 exactly (AC-10).
2. `test_icd_maximum_entropy` — n uniform categories; assert ICD ≈ ln(n) within f64 tolerance
   (AC-10). Float comparison must use an epsilon, not `==`.
3. Additional: `test_icd_two_entries_one_category_each` — two entries, two categories; assert
   ICD = ln(2) ≈ 0.693 within tolerance. Exercises the p = 0.5 path.
4. Verify that the implementation skips zero-count categories explicitly (categories with no
   entries must be excluded from the entropy sum, not included as `0 * ln(0)`).

**Coverage Requirement**: The zero-probability path must be explicitly guarded in the
implementation. Unit tests at k=1 (single entry, single category) and uniform-k cases cover
the boundary.

---

### R-06: Baseline recording skipped or stale
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-09 is not satisfied. `log.jsonl` has no nan-008 entry. Future baseline
comparisons for features like PPR (GH#398) have no distribution-aware anchor. The feature
ships without evidence that the binary computes metrics end-to-end.

**Test Scenarios**:
1. Post-delivery verification: parse `product/test/eval-baselines/log.jsonl` and assert a line
   exists where `feature_cycle == "nan-008"` and both `cc_at_k` and `icd` are non-null
   non-zero numbers (AC-09).
2. Verify `product/test/eval-baselines/README.md` field spec table contains `cc_at_k` and
   `icd` entries (AC-09 / FR-12).

**Coverage Requirement**: Manual verification step in the delivery checklist (ADR-005). Not
automated in unit tests — it is an artifact existence check.

---

### R-07: Backward-compat break when reading pre-nan-008 result JSON
**Severity**: High
**Likelihood**: Low
**Impact**: `eval report` errors on any pre-nan-008 result file that lacks the new fields.
Engineers using stored result files from previous eval runs cannot regenerate historical
reports. The `#[serde(default)]` annotation may have been omitted on one of the new fields.

**Test Scenarios**:
1. `test_report_backward_compat_pre_nan008_json` — construct a `ScenarioResult` JSON that
   omits `cc_at_k`, `icd`, `cc_at_k_delta`, `icd_delta`, and `category` entirely; pass it
   to `run_report`; assert it exits successfully and produces a report with those values
   defaulted to `0.0` / empty string (AC-07).

**Coverage Requirement**: One test exercising deserialization of a JSON file that lacks every
new field. Must assert `0.0` defaults, not just absence of error.

---

### R-08: `ScoredEntry.category` empty string in output due to mapping gap
**Severity**: High
**Likelihood**: Low
**Impact**: `se.entry.category` is never assigned in the mapping step in `replay.rs`, so
`ScoredEntry.category` is always `""`. `compute_cc_at_k` then collects only `{""}` as the
distinct category set. CC@k = 1/n regardless of actual diversity. ICD = 0.0 always (one
category). Both metrics pass unit tests against synthetic entries but are wrong for live data.
The baseline log entry is silently incorrect.

**Test Scenarios**:
1. Integration test: run `eval run` against a fixture scenario set with entries spanning at
   least two distinct categories (e.g., `"decision"` and `"lesson-learned"`). Parse the
   output JSON; assert per-entry `"category"` field is non-empty and matches the expected
   categories (AC-11).
2. Assert `cc_at_k > 0` and `icd > 0` in the same integration test output (AC-01, AC-02),
   confirming metrics are computed over real category data.

**Coverage Requirement**: At least one integration test using a real (or fixture) snapshot
that contains entries from multiple categories.

---

### R-09: Empty `Vec` for `configured_categories` from omitted TOML `[knowledge]` section
**Severity**: Med
**Likelihood**: Low
**Impact**: `KnowledgeConfig::default()` should populate the 7 `INITIAL_CATEGORIES`, but if
the TOML deserialization path for a profile that omits `[knowledge]` does not invoke the
default, the categories list is empty and the tracing::warn! path fires for every scenario.
Less severe than R-03 because the warning fires, but the baseline recording would still
produce CC@k = 0.0.

**Test Scenarios**:
1. Load a profile TOML fixture that omits the `[knowledge]` section entirely; assert that
   `profile.config_overrides.knowledge.categories` is non-empty (contains the 7 defaults).
2. Confirm `KnowledgeConfig::default()` initializes categories to `INITIAL_CATEGORIES` by
   unit test on the config type.

**Coverage Requirement**: One config unit test asserting default population; separate from
the empty-slice guard test in R-03.

---

### R-10: Delta field computation order inverted in `compute_comparison`
**Severity**: Med
**Likelihood**: Low
**Impact**: `cc_at_k_delta` is computed as `baseline.cc_at_k - candidate.cc_at_k` instead
of `candidate - baseline`. A candidate that improves CC@k from 0.4 to 0.7 shows a negative
delta of −0.3 in the report. The PPR gate review interprets it as degradation when it is
improvement.

**Test Scenarios**:
1. `test_compute_comparison_delta_signs` — create two `ProfileResult` values where candidate
   has higher CC@k and ICD than baseline; assert `cc_at_k_delta > 0` and `icd_delta > 0`.
2. Symmetric: create candidate with lower values; assert both deltas are negative.

**Coverage Requirement**: Two unit tests (positive and negative delta) on `compute_comparison`
for the new fields.

---

### R-11: Aggregate accumulation divides by wrong count
**Severity**: Med
**Likelihood**: Low
**Impact**: `mean_cc_at_k` is computed as `cc_at_k_sum / entries_count` instead of
`cc_at_k_sum / scenario_count`. With k=5 entries per scenario, the mean is off by a factor
of 5. Report shows mean CC@k = 0.17 when it should be 0.85.

**Test Scenarios**:
1. `test_aggregate_stats_cc_at_k_mean` — build 3 `ScenarioResult` values with known CC@k
   values (0.2, 0.4, 0.6); assert `mean_cc_at_k ≈ 0.4` (tolerance 1e-9).
2. Same pattern for `mean_icd`.

**Coverage Requirement**: Explicit mean-value assertion for both new aggregate fields, using
a multi-scenario fixture where the correct mean is manually verifiable.

---

### R-12: Distribution Analysis top-5 sort direction inverted
**Severity**: Med
**Likelihood**: Low
**Impact**: The "top-5 improved scenarios" list shows the five scenarios with the largest
*negative* CC@k delta (degradations), and vice versa. A reviewer assessing the PPR candidate
draws the opposite conclusion from the correct one.

**Test Scenarios**:
1. `test_cc_at_k_scenario_rows_sort_order` — build a `Vec<CcAtKScenarioRow>` with mixed
   positive and negative deltas; assert the first row has the largest positive delta and
   the last row (in the degradation list) has the most negative delta.

**Coverage Requirement**: One sort-order unit test on `compute_cc_at_k_scenario_rows`.

---

### R-13: `eval snapshot` subcommand absent at delivery time
**Severity**: Low
**Likelihood**: Med
**Impact**: Delivery agent cannot complete AC-09 baseline recording because the snapshot
creation command documented in ADR-005 does not exist. Feature ships without a baseline entry.

**Test Scenarios**:
1. Pre-delivery check: run `eval --help` and confirm the snapshot subcommand exists. If
   absent, document the actual creation command in the PR description and use it in place
   of the ADR-005 procedure (ADR-005 consequence noted).

**Coverage Requirement**: Operational check at delivery time; not a code test.

---

## Integration Risks

**runner/replay.rs → runner/metrics.rs**: `run_single_profile` passes `configured_categories`
as `&[String]` to `compute_cc_at_k`. If the categories slice is empty due to R-09, the metric
silently returns 0.0 with a warn. The warn must be observable in `eval run` stderr.

**runner/output.rs ↔ report/mod.rs** (JSON schema boundary): The two type copies share no
compile-time link. Any field added to one but not the other is a schema gap detectable only
at runtime via the round-trip test (ADR-003). This is the highest-priority integration risk
(R-01).

**report/aggregate.rs → report/render.rs**: `CcAtKScenarioRow` is a new internal type passed
from `compute_cc_at_k_scenario_rows` to `render_report`. The `render_report` signature gains
a `cc_at_k_rows: &[CcAtKScenarioRow]` parameter. A call site that does not pass this argument
(or passes an empty slice by mistake) silently omits the top-5 table without a compile error
if the parameter has a default or the function handles empty input gracefully.

**product/test/eval-baselines/log.jsonl** (append-only): A write to this file during baseline
recording that does not produce a valid JSON object per line (e.g., a trailing comma, partial
write) silently corrupts the log for all future readers. The delivery agent must verify the
appended line is valid JSON before submitting.

---

## Edge Cases

- **k=0 result set**: `eval run` with a profile whose `top_k = 0` produces an empty entries
  slice. `compute_cc_at_k` returns 0.0 (empty numerator); `compute_icd` returns 0.0
  (explicit empty-input guard). Both are correct; verified by `test_icd_empty_entries_returns_zero`
  and `test_cc_at_k_empty_configured_categories_returns_zero`.

- **All results from one category**: CC@k = 1/n (one category covered); ICD = 0.0. Tests
  `test_cc_at_k_one_category_present` and `test_icd_single_category` cover this (AC-10).

- **Result categories not in `configured_categories`**: An entry whose `category` is a value
  not listed in `configured_categories` (e.g., a legacy category string) still counts toward
  the numerator if it appears in the result. The formula counts distinct categories *in
  results* that are also in *configured_categories* — the intersection. If the formula
  counts all distinct result categories regardless, CC@k could exceed 1.0. The specification
  (FR-04) must be confirmed: the numerator is `|{cat ∈ results : cat ∈ configured_categories}|`
  or `|{cat ∈ results}|`? The SCOPE.md formula uses "exists entry in top-k with category = cat",
  which counts all distinct result categories without filtering against `configured_categories`.
  This means CC@k can exceed 1.0 if results contain categories absent from the configured list.
  A test case for this edge case is required.

- **Single scenario, single profile**: `compute_cc_at_k_scenario_rows` returns one row.
  The top-5 improvement list shows 0 or 1 rows (no baseline comparison possible in
  single-profile mode). Section 6 omits the comparison sub-tables (FR-09).

- **ICD with ln(0) guard**: If the formula iterates over *all* configured categories including
  those with zero entries in the result, `p = 0/total = 0.0` and `0.0 * ln(0.0)` = NaN.
  The implementation must iterate only over categories that appear in results (non-zero count).

- **Float aggregation over 1761 scenarios**: Summing 1761 `f64` CC@k values (each in [0,1])
  with naive running sum may accumulate floating-point error. Not a correctness risk at this
  scale, but mean should be computed as `sum / count` not incrementally re-averaged.

---

## Security Risks

**Untrusted input surface**: nan-008 adds no new network-facing inputs. The eval harness
operates on local filesystem files only. The relevant untrusted-input surfaces are:

- **Scenario JSONL files**: Read by `runner/scenarios/`. nan-008 does not change the scenario
  format (NFR-05). No new deserialization attack surface.
- **Result JSON files**: Written by `eval run`, read by `eval report`. Pre-nan-008 result
  files with missing fields are handled via `serde(default)` — no panic. A crafted result
  JSON with a string `"category"` field of extreme length (e.g., 1 MB) would be stored in
  `ScoredEntry.category` and passed to `compute_cc_at_k`. The `HashSet` deduplication is
  O(n entries) in memory; no meaningful blast radius beyond memory use in a local tool.
- **Profile TOML files**: `KnowledgeConfig.categories` is a `Vec<String>`. A profile with
  10,000 configured categories would cause `compute_cc_at_k` to allocate a `HashSet` of up
  to 10,000 strings — negligible for a local CLI tool. No injection risk.
- **`eval-baselines/log.jsonl`**: Append-only write by the delivery agent. Not read by
  production code paths; only by future delivery agents. No security risk beyond data
  integrity (see Integration Risks above).

**Blast radius**: The eval harness is a local development tool, not a production server path.
Compromise of any eval artifact affects only development decisions, not the running Unimatrix
server.

---

## Failure Modes

| Failure | Expected System Behavior |
|---------|--------------------------|
| `configured_categories` empty | `compute_cc_at_k` emits `tracing::warn!` and returns 0.0; `eval run` continues; report shows CC@k = 0.0 across all scenarios |
| Pre-nan-008 result JSON passed to `eval report` | `serde(default)` produces 0.0/empty-string for missing fields; `eval report` exits 0; Distribution Analysis section shows zero values |
| `eval snapshot` subcommand absent | `eval run` cannot produce baseline; delivery agent must use alternative snapshot command; AC-09 remains blockers until baseline recorded |
| NaN from `ln(0)` in `compute_icd` | Must not occur — implementation must skip zero-count categories; if it does occur, JSON serialization may produce `null`/`nan` and `eval report` may error on aggregate computation |
| Section 6 accidentally omitted from `render_report` | Round-trip test (ADR-003) catches this; `content.contains("## 6.")` assertion fails |
| `cc_at_k_delta` sign inverted | Produces plausible-looking but wrong report; caught only by R-10 delta-sign unit test |
| Report type copy missing a new field | `serde(default)` returns 0.0 silently; caught by round-trip test asserting non-zero values |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Dual type copy divergence | R-01 | Mitigated by mandatory round-trip integration test (ADR-003). Both copies must be updated atomically per NFR-08. |
| SR-02: Empty `configured_categories` silent 0.0 | R-03, R-09 | Mitigated by `tracing::warn!` in `compute_cc_at_k` (ADR-004). R-09 covers the specific TOML-omission path. |
| SR-03: ICD unbounded range causes cross-profile misread | R-04 | Mitigated by `ln(n)` annotation in ICD column header and Distribution Analysis interpretation guidance (ADR-002, FR-10, AC-14). |
| SR-04: Baseline recording requires snapshot that may not exist | R-06, R-13 | Mitigated by named delivery step with explicit procedure (ADR-005). Delivery agent approved to create snapshot if absent. |
| SR-05: Output size estimate may be stale | — | Accepted. NFR-07 requires delivery agent to note actual overhead in log.jsonl `note` field. No design change needed. |
| SR-06: Section-order regression in render.rs | R-02 | Mitigated by full section-order position assertion in round-trip test (ADR-003, AC-13). Pattern #3426 evidence elevated this to mandatory. |
| SR-07: Borrow lifetime conflict in replay.rs | — | Resolved by architecture. Ownership trace in ARCHITECTURE.md confirms `profile` is borrowed as `&[EvalProfile]` throughout; no lifetime conflict arises. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 1 round-trip integration test with non-zero field assertions |
| High | 7 (R-02–R-08) | Section-order position test; ICD annotation test; empty-slice guard; backward-compat deserialization test; float-precision boundary tests; delta-sign tests; integration test with real category data |
| Medium | 5 (R-09–R-13) | Config default test; delta-order unit tests; aggregate mean unit tests; sort-direction unit test; operational snapshot check |
| Low | 1 (R-13) | Pre-delivery operational check only |
