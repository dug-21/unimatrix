# nan-008: Distribution-Aware Metrics (CC@k and ICD) — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-008/SCOPE.md |
| Architecture | product/features/nan-008/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-008/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-008/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-008/ALIGNMENT-REPORT.md |

## Goal

Add CC@k (Category Coverage at k) and ICD (Intra-query Category Diversity) to the nan-007 eval harness so that distribution-shifting features (PPR, Contradicts suppression, phase-conditioned retrieval) can be evaluated without false regression signals from P@K and MRR. The metrics are ground-truth-free, computed as pure functions from existing result entries, and surfaced in both the runner output JSON and the `eval report` markdown with a new Distribution Analysis section (section 6).

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| runner/metrics.rs | pseudocode/runner-metrics.md | test-plan/runner-metrics.md |
| runner/output.rs | pseudocode/runner-output.md | test-plan/runner-output.md |
| runner/replay.rs | pseudocode/runner-replay.md | test-plan/runner-replay.md |
| report/mod.rs | pseudocode/report-mod.md | test-plan/report-mod.md |
| report/aggregate.rs | pseudocode/report-aggregate.md | test-plan/report-aggregate.md |
| report/render.rs | pseudocode/report-render.md | test-plan/report-render.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Stage 3a complete. All pseudocode and test-plan files produced and paths confirmed.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Category field: inline computation vs. persist on ScoredEntry | Add `category: String` to `ScoredEntry` in both type copies; populate from `se.entry.category` during mapping; enables future metrics without further runner changes | SCOPE.md OQ-1 | product/features/nan-008/architecture/ADR-001-category-field-on-scored-entry.md |
| ICD: normalize to [0,1] vs. raw entropy with label | Raw Shannon entropy with `f64::ln`; ICD column header annotated `ICD (max=ln(n))`; Distribution Analysis section includes comparability guidance | SCOPE.md OQ-2, SR-03 | product/features/nan-008/architecture/ADR-002-icd-raw-entropy-with-report-label.md |
| SR-01 + SR-06: dual type copy divergence + section-order regression guard | Mandatory round-trip integration test `test_report_round_trip_cc_at_k_icd_fields_and_section_6` that asserts non-zero values and section-6-after-section-5 position; single test catches both failure modes | SR-01, SR-06 | product/features/nan-008/architecture/ADR-003-round-trip-integration-test.md |
| Empty `configured_categories` behavior | `tracing::warn!` then return `0.0`; surfaces misconfigured profile TOML to operator without panicking or aborting `eval run` | SR-02 | product/features/nan-008/architecture/ADR-004-warn-on-empty-configured-categories.md |
| Baseline recording: named step vs. implied post-work | Named delivery AC step with 6-step explicit procedure; delivery agent checks for existing snapshot and creates one if absent | SCOPE.md OQ-3, SR-04 | product/features/nan-008/architecture/ADR-005-baseline-recording-procedure.md |

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/eval/runner/metrics.rs` | Modify | Add `compute_cc_at_k` and `compute_icd` pure functions; extend `compute_comparison` with delta fields |
| `crates/unimatrix-server/src/eval/runner/output.rs` | Modify | Add `category: String` to `ScoredEntry`; add `cc_at_k: f64`, `icd: f64` to `ProfileResult`; add `cc_at_k_delta: f64`, `icd_delta: f64` to `ComparisonMetrics` |
| `crates/unimatrix-server/src/eval/runner/replay.rs` | Modify | Extend `run_single_profile` with `configured_categories: &[String]`; populate `category` on `ScoredEntry`; call new metric functions after assembling entries vec |
| `crates/unimatrix-server/src/eval/report/mod.rs` | Modify | Mirror all new fields on local deserialization copies (`ScoredEntry`, `ProfileResult`, `ComparisonMetrics`) with `#[serde(default)]`; extend `AggregateStats`; add `CcAtKScenarioRow` type; update `default_comparison` |
| `crates/unimatrix-server/src/eval/report/aggregate.rs` | Modify | Accumulate `cc_at_k_sum`, `icd_sum`, `cc_at_k_delta_sum`, `icd_delta_sum`; add `compute_cc_at_k_scenario_rows` helper |
| `crates/unimatrix-server/src/eval/report/render.rs` | Modify | Extend Summary table with CC@k and ICD columns; append section 6 Distribution Analysis (per-profile range table, top-5 improvement/degradation) |
| `crates/unimatrix-server/src/eval/runner/tests_metrics.rs` | Modify / Create | Add 6 unit tests for metric boundary values (AC-10) and additional coverage from RISK-TEST-STRATEGY.md |
| `crates/unimatrix-server/src/eval/report/tests.rs` | Modify | Add round-trip integration test `test_report_round_trip_cc_at_k_icd_fields_and_section_6`; extend `test_report_contains_all_five_sections` to six sections; update `make_profile_result` / `make_scenario_result` helpers |
| `docs/testing/eval-harness.md` | Modify | Add CC@k and ICD subsections to "Understanding the metrics"; update result JSON example; update baseline recording example |
| `product/test/eval-baselines/log.jsonl` | Modify | Append one new baseline entry with `cc_at_k`, `icd`, `feature_cycle: "nan-008"` fields after delivery run |
| `product/test/eval-baselines/README.md` | Modify | Add `cc_at_k` and `icd` to field specification table |

## Data Structures

### `ScoredEntry` (runner/output.rs — canonical; report/mod.rs — deserialization copy)

```rust
// runner/output.rs (new field added)
pub struct ScoredEntry {
    pub id: String,
    pub title: String,
    pub category: String,          // NEW — from se.entry.category
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    pub nli_rerank_delta: Option<f64>,
}

// report/mod.rs mirror (all new fields with #[serde(default)])
struct ScoredEntry {
    id: String,
    title: String,
    #[serde(default)]
    category: String,
    final_score: f64,
    similarity: f64,
    confidence: f64,
    status: String,
    nli_rerank_delta: Option<f64>,
}
```

### `ProfileResult` (both copies)

```rust
// New fields added:
pub cc_at_k: f64,    // report copy: #[serde(default)]
pub icd: f64,        // report copy: #[serde(default)]
```

### `ComparisonMetrics` (both copies)

```rust
// New fields added:
pub cc_at_k_delta: f64,    // candidate.cc_at_k - baseline.cc_at_k; report copy: #[serde(default)]
pub icd_delta: f64,        // candidate.icd - baseline.icd; report copy: #[serde(default)]
```

### `AggregateStats` (report/mod.rs)

```rust
// New fields added:
pub mean_cc_at_k: f64,
pub mean_icd: f64,
pub cc_at_k_delta: f64,    // mean of per-scenario cc_at_k_delta values
pub icd_delta: f64,        // mean of per-scenario icd_delta values
```

### `CcAtKScenarioRow` (report/aggregate.rs or report/mod.rs)

```rust
pub struct CcAtKScenarioRow {
    pub scenario_id: String,
    pub query: String,
    pub baseline_cc_at_k: f64,
    pub candidate_cc_at_k: f64,
    pub cc_at_k_delta: f64,
}
```

### `eval-baselines/log.jsonl` entry (appended)

```json
{
  "date": "<ISO-8601>",
  "scenarios": <count>,
  "p_at_k": <f64>,
  "mrr": <f64>,
  "avg_latency_ms": <f64>,
  "cc_at_k": <f64>,
  "icd": <f64>,
  "feature_cycle": "nan-008",
  "note": "initial CC@k and ICD baseline"
}
```

## Function Signatures

```rust
// runner/metrics.rs

/// Fraction of configured_categories represented by at least one entry in the result.
/// Returns 0.0 and emits tracing::warn! when configured_categories is empty.
/// Range: [0.0, 1.0]. Numerator counts distinct categories present in entries;
/// delivery agent must resolve intersection-vs-union and guard against CC@k > 1.0.
pub fn compute_cc_at_k(entries: &[ScoredEntry], configured_categories: &[String]) -> f64;

/// Raw Shannon entropy (natural log) over the category distribution in entries.
/// Returns 0.0 for empty entries or single-category results.
/// Must skip zero-count categories to avoid 0.0 * ln(0.0) = NaN.
/// Range: [0.0, ln(n_distinct_categories_in_result)].
pub fn compute_icd(entries: &[ScoredEntry]) -> f64;

// compute_comparison — extended signature (internal change):
// adds cc_at_k_delta and icd_delta from baseline and candidate ProfileResult values.

// runner/replay.rs

// run_single_profile gains one parameter:
async fn run_single_profile(
    ...,
    configured_categories: &[String],  // NEW — from profile.config_overrides.knowledge.categories
) -> Result<ProfileResult, EvalError>;

// report/aggregate.rs

/// Collect per-scenario CC@k rows sorted by cc_at_k_delta descending.
pub fn compute_cc_at_k_scenario_rows(results: &[ScenarioResult]) -> Vec<CcAtKScenarioRow>;

// report/render.rs

// render_report gains one parameter:
pub fn render_report(
    ...,
    cc_at_k_rows: &[CcAtKScenarioRow],  // NEW — for section 6 Distribution Analysis
) -> String;
```

## Constraints

1. **No scenario format changes.** `ScenarioRecord` JSONL format is unchanged.
2. **Dual-copy atomicity.** `runner/output.rs` and `report/mod.rs` type copies must be updated in the same commit. Never submit a partial update.
3. **`#[serde(default)]` on all new report deserialization fields.** Required for backward compatibility.
4. **Pure functions in `runner/metrics.rs`.** No I/O, no async, no side effects.
5. **Synchronous `report/` module.** No tokio or async introduced anywhere in `report/`.
6. **No hardcoded category lists in metric code.** Category denominator always comes from `configured_categories` parameter.
7. **Division-by-zero guard.** `compute_cc_at_k` returns `0.0` and emits `tracing::warn!` when `configured_categories` is empty.
8. **Natural log.** `compute_icd` uses `f64::ln`. ICD range is `[0.0, ln(n)]` — all docs and annotations must use natural log.
9. **Two-profile assumption for Distribution Analysis comparison rows.** Improvement/degradation sub-table omitted for single-profile runs.
10. **No `--categories` CLI flag.** Category list derives from profile TOML only.
11. **Baseline recording is a named delivery step.** Must complete AC-09 before PR is marked ready. Procedure: check for existing snapshot; create one if absent (`eval snapshot --db <live> --out snap-nan-008.db`); run `eval run`; extract means; append to `log.jsonl`; update `README.md`.
12. **CC@k > 1.0 guard.** The ALIGNMENT-REPORT.md WARN flags an ambiguity in the numerator formula (intersection vs. union semantics). Delivery agent must resolve and add a guard if union semantics are chosen. Intersection semantics (only categories in `configured_categories` counted) naturally caps CC@k at 1.0.
13. **ICD NaN guard.** `compute_icd` must iterate only over categories with non-zero count. `0.0 * f64::ln(0.0)` must never be evaluated.

## Dependencies

### Internal crates (read-only, no changes needed)

- `crates/unimatrix-server/src/infra/config.rs` — `KnowledgeConfig`, `UnimatrixConfig`, `INITIAL_CATEGORIES`
- `crates/unimatrix-store` — source of `EntryRecord.category`

### External crates (no new dependencies)

- `std::collections::HashSet` — used in `compute_cc_at_k` for distinct category collection
- `f64::ln` — standard library, natural log for `compute_icd`
- `tracing` — already in `unimatrix-server`; used for `tracing::warn!` in `compute_cc_at_k`
- `serde` — already in scope; `#[serde(default)]` on new report deserialization fields

## NOT in Scope

- **NEER (Novel Entry Exposure Rate)** — deferred; requires session context across queries.
- **Per-phase ICD breakdown** — deferred; requires #397 (phase-in-scenarios).
- **Automated CC@k shipping gate** — the `>= 0.7` PPR target is human-reviewed; `eval report` exits 0 regardless.
- **`eval scenarios` command changes** — scenario extraction pipeline unchanged.
- **Snapshot or live-path client changes** — D1, D5, D6 clients unchanged.
- **Section 5 regression detection logic changes** — CC@k and ICD are informational only.
- **ICD normalization** — raw entropy by design; normalization is a future enhancement.
- **NLI model or scoring pipeline changes** — metrics-only feature.
- **Changes to GH issues #394 or #397** — no dependency on those features.
- **`context_search` or store changes** — no knowledge engine modifications.

## Alignment Status

**Overall: PASS with two WARNs. No VARIANCEs or FAILs. Cleared for delivery.**

| WARN | Description | Delivery Action Required |
|------|-------------|--------------------------|
| WARN-1: Baseline recording execution dependency | AC-09 / FR-12 baseline recording is satisfiable only at delivery runtime — it cannot be pre-verified in unit tests. The procedure is fully specified in ADR-005 but remains an execution dependency. | Delivery agent must complete all 6 steps of ADR-005 before marking PR ready. The `log.jsonl` entry with `feature_cycle: "nan-008"` is evidence of completion. |
| WARN-2: CC@k formula intersection-vs-union ambiguity | SCOPE.md formula counts all distinct result categories; if a result contains a category absent from `configured_categories`, CC@k can exceed 1.0. RISK-TEST-STRATEGY.md flags this but does not assign a test or resolve the ambiguity. Delivery agent must choose semantics and guard accordingly. | Prefer intersection semantics (`cat ∈ results AND cat ∈ configured_categories`) as numerator — this caps CC@k at 1.0 naturally and is the defensible default. Add a test case for the out-of-configured-list category scenario. |

All 7 scope risks (SR-01 through SR-07) are fully traced to architecture decisions, specification constraints, and test coverage. No VARIANCEs exist.
