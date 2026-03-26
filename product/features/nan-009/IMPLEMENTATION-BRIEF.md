# nan-009 Implementation Brief
# Phase-Stratified Eval Scenarios

GH Issue: #400

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-009/SCOPE.md |
| Scope Risk Assessment | product/features/nan-009/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-009/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-009/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/nan-009/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-009/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/nan-009/architecture/ADR-001-serde-null-suppression.md |
| ADR-002 | product/features/nan-009/architecture/ADR-002-dual-type-guard-strategy.md |
| ADR-003 | product/features/nan-009/architecture/ADR-003-phase-vocabulary-governance.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| Scenario Extraction (`eval/scenarios/`) | product/features/nan-009/pseudocode/scenario-extraction.md | product/features/nan-009/test-plan/scenario-extraction.md |
| Result Passthrough (`eval/runner/`) | product/features/nan-009/pseudocode/result-passthrough.md | product/features/nan-009/test-plan/result-passthrough.md |
| Report Aggregation (`eval/report/aggregate.rs`) | product/features/nan-009/pseudocode/report-aggregation.md | product/features/nan-009/test-plan/report-aggregation.md |
| Report Rendering (`eval/report/render.rs`) | product/features/nan-009/pseudocode/report-rendering.md | product/features/nan-009/test-plan/report-rendering.md |
| Report Entry Point (`eval/report/mod.rs`) | product/features/nan-009/pseudocode/report-entrypoint.md | product/features/nan-009/test-plan/report-entrypoint.md |
| Documentation (`docs/testing/eval-harness.md`) | product/features/nan-009/pseudocode/documentation.md | product/features/nan-009/test-plan/documentation.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/nan-009/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/nan-009/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add a `phase` field to `ScenarioContext` and `ScenarioResult`, propagate it through the five-stage eval pipeline (extraction → scenario JSONL → result JSON → aggregation → rendering), and produce a per-phase aggregate section (section 6: Phase-Stratified Metrics) in the eval report. This delivers the primary measurement instrument required by ASS-032 Loop 2 before phase-conditioned retrieval scoring (`w_phase_explicit`) can be activated, without changing retrieval logic, scoring, or baseline recording.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Null-phase serialization in scenario JSONL | `ScenarioContext.phase` uses `#[serde(default, skip_serializing_if = "Option::is_none")]` — null phase is omitted from JSONL, not emitted as `"phase":null`; preserves backward-compatible wire shape for existing scenario files | ADR-001, SPEC NFR-02 | architecture/ADR-001-serde-null-suppression.md |
| Runner-side `ScenarioResult.phase` serde annotation | Runner copy carries `#[serde(default)]` only — no `skip_serializing_if`; result JSON always emits `"phase":null` for null-phase results (AC-03 requires consistent key presence); report copy uses `#[serde(default)]` only for tolerant deserialization | ADR-001, ALIGNMENT V-3 | architecture/ADR-001-serde-null-suppression.md |
| Dual-type guard strategy | Round-trip integration test (`test_report_round_trip_phase_section_7_distribution`) using a non-trivial phase value — preserves compile-time isolation between runner and report modules established in nan-007; module boundary is not changed | ADR-002 | architecture/ADR-002-dual-type-guard-strategy.md |
| Phase vocabulary governance | Free-form `Option<String>`; no enum, no CHECK constraint, no allowlist; known values (`design`, `delivery`, `bugfix`) documented as a snapshot; null bucket renders as `"(unset)"` and sorts last; retroactive relabeling via `UPDATE query_log`, not schema changes | ADR-003, SPEC Constraint 5 | architecture/ADR-003-phase-vocabulary-governance.md |
| Null-phase display label | `"(unset)"` is canonical everywhere — implementation, tests, documentation; `"(none)"` must not appear anywhere; human-confirmed in design session resolving SR-04 | ADR-003, SPEC Constraint 5 | architecture/ADR-003-phase-vocabulary-governance.md |
| Section ordering | Section 6 = Phase-Stratified Metrics; section 7 = Distribution Analysis (shifted from 6); SCOPE.md Goals §5 reference to "section 7" is overridden by SCOPE.md RD-01 and Constraint 5; ARCHITECTURE and SPECIFICATION both implement section 6 | SCOPE.md RD-01, ALIGNMENT V-1 | N/A |
| Table granularity | One row per phase (not per phase × profile) for first iteration; delta columns provide cross-profile signal; cross-product view deferred | SCOPE.md RD-02 | N/A |
| Phase label in section 2 | Read `phase` directly from `ScenarioResult` in renderer; do not extend `NotableEntry` tuple type | SCOPE.md RD-04 | N/A |
| SR-06 warning emission | Deferred to implementation agent; if added, use existing `eprintln!("WARN: ...")` style — do not introduce `tracing` dependency in report module | ARCHITECTURE Open Question 1 | N/A |

---

## Files to Create / Modify

### Modified files

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/eval/scenarios/types.rs` | Add `phase: Option<String>` to `ScenarioContext` with `#[serde(default, skip_serializing_if = "Option::is_none")]` |
| `crates/unimatrix-server/src/eval/scenarios/output.rs` | Add `phase` to SELECT column list in `do_scenarios` SQL query |
| `crates/unimatrix-server/src/eval/scenarios/extract.rs` | Add `row.try_get::<Option<String>, _>("phase")?` in `build_scenario_record`; populate `context.phase` |
| `crates/unimatrix-server/src/eval/runner/output.rs` | Add `phase: Option<String>` to `ScenarioResult` with `#[serde(default)]` only |
| `crates/unimatrix-server/src/eval/runner/replay.rs` | Set `phase: record.context.phase.clone()` on constructed `ScenarioResult`; phase must NOT be forwarded to `ServiceSearchParams` or `AuditContext` |
| `crates/unimatrix-server/src/eval/report/mod.rs` | Add `phase: Option<String>` with `#[serde(default)]` to local `ScenarioResult` copy; add `PhaseAggregateStats` struct; wire `compute_phase_stats` into `run_report`; pass `phase_stats` to `render_report` |
| `crates/unimatrix-server/src/eval/report/aggregate.rs` | Add `compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>`; if file approaches 500 lines, extract to `aggregate_phase.rs` |
| `crates/unimatrix-server/src/eval/report/render.rs` | Add `render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String`; add `phase_stats` parameter to `render_report`; rename `## 6. Distribution Analysis` to `## 7. Distribution Analysis`; update module docstring section list |
| `crates/unimatrix-server/src/eval/scenarios/tests.rs` | Extend `insert_query_log_row` to accept `phase: Option<&str>`; add extraction integration tests for non-null and null phase |
| `crates/unimatrix-server/src/eval/report/tests.rs` | Add round-trip integration test; add null-phase omission test; add phase grouping unit tests; update `test_report_contains_all_five_sections` for seven sections; update `test_report_round_trip_cc_at_k_icd_fields_and_section_6` for renumbered heading |
| `docs/testing/eval-harness.md` | Document `context.phase` field; document section 6 Phase-Stratified Metrics; note phase population requires MCP-sourced sessions with `context_cycle`; document migration-based governance |

### Possibly new file (conditional)

| File | Condition |
|------|-----------|
| `crates/unimatrix-server/src/eval/report/aggregate_phase.rs` | Only if `aggregate.rs` approaches 500 lines after adding `compute_phase_stats` |

---

## Data Structures

### `ScenarioContext` (modified — `eval/scenarios/types.rs`)
```rust
pub struct ScenarioContext {
    pub agent_id: ...,
    pub feature_cycle: ...,
    pub session_id: ...,
    pub retrieval_mode: ...,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,  // new field; populated from query_log.phase
}
```

### `ScenarioResult` — runner copy (modified — `eval/runner/output.rs`)
```rust
pub struct ScenarioResult {
    // existing fields ...
    #[serde(default)]
    pub phase: Option<String>,  // no skip_serializing_if; always emits "phase":null or "phase":"delivery"
}
```

### `ScenarioResult` — report copy (modified — `eval/report/mod.rs`)
```rust
struct ScenarioResult {
    // existing fields ...
    #[serde(default)]
    phase: Option<String>,  // tolerates both absent key (pre-nan-009) and explicit null
}
```

### `PhaseAggregateStats` (new — `eval/report/mod.rs`)
```rust
#[derive(Debug, Default)]
pub(super) struct PhaseAggregateStats {
    pub phase_label: String,    // "design" | "delivery" | "bugfix" | "(unset)"
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_cc_at_k: f64,
    pub mean_icd: f64,
}
```

---

## Function Signatures

```rust
// eval/scenarios/extract.rs
// Unchanged public signature; internal change: reads row.try_get::<Option<String>, _>("phase")?
fn build_scenario_record(row: &Row) -> Result<ScenarioRecord>;

// eval/runner/replay.rs
// Unchanged public signature; sets phase: record.context.phase.clone() on result
fn replay_scenario(record: &ScenarioRecord, ...) -> ScenarioResult;

// eval/report/aggregate.rs (new function)
// Returns empty Vec when all phases are None (caller must not render section 6 in that case)
// Sorted: alphabetical ascending for named phases, "(unset)" unconditionally last
pub(super) fn compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>;

// eval/report/render.rs (new function)
// Returns empty String when phase_stats is empty; caller skips the section
pub(super) fn render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String;

// eval/report/render.rs (modified signature)
pub fn render_report(
    aggregate_stats: &[AggregateStats],
    phase_stats: &[PhaseAggregateStats],   // new parameter
    results: &[ScenarioResult],
    // ... existing params
) -> String;

// eval/report/mod.rs (unchanged public signature)
pub fn run_report(result_dir: &Path, output_path: &Path) -> Result<()>;
```

---

## Section Renumbering — Affected Sites

All five sites must be updated to avoid the SR-02 regression (pattern #3426):

| File | Old string | New string |
|------|-----------|-----------|
| `render.rs` line ~198 | `## 6. Distribution Analysis` | `## 7. Distribution Analysis` |
| `render.rs` module docstring | sections 1–6 listed | sections 1–7 listed |
| `mod.rs` module docstring | sections 1–6 listed | sections 1–7 listed |
| `report/tests.rs` — `test_report_contains_all_five_sections` | 5 sections asserted | 7 sections asserted; `## 7. Distribution Analysis`; `## 6. Phase-Stratified Metrics` |
| `report/tests.rs` — `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | asserts `## 6. Distribution Analysis` | asserts `## 7. Distribution Analysis` |

---

## Constraints

1. **Backward-compatible scenario format.** `ScenarioContext.phase` is `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. No `"phase":null` key is emitted for null-phase records. Existing JSONL files deserialize cleanly.

2. **Dual-type constraint.** `runner/output.rs` and `report/mod.rs` maintain independent copies of `ScenarioResult`. Both must gain `phase: Option<String>` in sync. The mandatory round-trip integration test (AC-11, ADR-002) is the enforcement mechanism.

3. **Phase is metadata only during replay.** Phase must NOT be injected into `ServiceSearchParams` or `AuditContext` during `eval run`. Replay reproduces the query as originally issued.

4. **Report path is synchronous.** `eval/report/` contains no tokio, no async functions, no database access. `compute_phase_stats` and `render_phase_section` must be pure synchronous functions.

5. **Null-phase label is `"(unset)"` everywhere.** `"(none)"` must not appear in implementation, tests, or documentation. Null bucket sorts last in the phase table (explicit override of lexicographic order — `(` ASCII 40 precedes `a-z`).

6. **No phase filter on `eval scenarios` CLI.** No `--phase` flag is added.

7. **Max 500 lines per file.** If `aggregate.rs` approaches the limit, extract `compute_phase_stats` to a new `aggregate_phase.rs`.

8. **No phase enum validation.** Free-form strings flow through the pipeline unchanged.

---

## Dependencies

| Dependency | Version / Source | Notes |
|------------|-----------------|-------|
| `query_log.phase` column | col-028, GH #403 | Already present in schema and `QueryLogRecord`; dependency satisfied |
| `eval/scenarios/` module | nan-007 | Provides `ScenarioContext`, `ScenarioRecord`, `build_scenario_record`, `insert_query_log_row` test helper |
| `eval/runner/` module | nan-007 | Provides runner-side `ScenarioResult`, `replay_scenario` |
| `eval/report/` module | nan-007/nan-008 | Provides `compute_aggregate_stats`, `render_report`, report-side `ScenarioResult` |
| `serde` / `serde_json` | Existing workspace deps | `#[serde(default)]`, `#[serde(skip_serializing_if)]` used on new fields |
| Pattern #3255 | Unimatrix KB | `serde(default)` alone does not suppress null serialization |
| Pattern #3426 | Unimatrix KB | Golden-output test required for section-order regression guard |
| Pattern #3550 | Unimatrix KB | Dual-type constraint: independent ScenarioResult copies in runner and report |
| Pattern #3526 | Unimatrix KB | Round-trip integration test strategy for dual-type risk |
| Lesson #3543 | Unimatrix KB | Test helper must bind phase column; col-028 precedent |
| Lesson #885 | Unimatrix KB | Serde annotation gate failure; test both serialization directions |

---

## NOT in Scope

- Phase-conditioned retrieval scoring (`w_phase_explicit` activation, phase-affinity matrix)
- `--phase` filter flag on `eval scenarios` or `eval run` CLI
- Changes to regression detection logic (report section 5)
- Per-phase delta columns (requires paired baseline/candidate per stratum; deferred)
- NEER metric (session-level tracking; deferred from nan-008)
- Changes to `eval run` replay execution logic beyond metadata passthrough
- Phase enum validation or allowlist enforcement
- Changes to `eval-baselines/log.jsonl` baseline recording procedure
- Per-phase × profile cross-product table (deferred to later iteration)

---

## Alignment Status

Vision alignment: **PASS** — Feature directly serves W1-3 (Evaluation Harness) and ASS-032 Loop 2 measurement instrument. Milestone discipline maintained; `w_phase_explicit` weight path remains inactive.

**Three variances from the alignment report — all resolved before delivery:**

**V-1 (WARN — resolved):** SCOPE.md Goals §5 references "section 7" but SCOPE.md RD-01 and Constraint 5 establish section 6. ARCHITECTURE.md and SPECIFICATION.md both implement section 6. **Authoritative answer: section 6 is correct.** Delivery agents must not read Goals §5 in isolation.

**V-2 (WARN — resolved):** RISK-TEST-STRATEGY.md BLOCKER heading for the `"(none)"` vs `"(unset)"` conflict is stale — the conflict is resolved by SPECIFICATION.md Constraint 5 and human-confirmed in the design session. **Canonical label is `"(unset)"`**; the BLOCKER does not block delivery.

**V-3 (WARN — resolved):** SPECIFICATION.md FR-04 originally specified `#[serde(default, skip_serializing_if = "Option::is_none")]` on runner-side `ScenarioResult`. ARCHITECTURE.md Component 2 is correct: runner copy carries `#[serde(default)]` only — no `skip_serializing_if`. Runner always emits `"phase":null` for null-phase results. **Delivery agents must follow the architecture position, not FR-04 as originally written.** The spawn prompt explicitly records this: FR-04 uses `#[serde(default)]` only.

Open Question 1 from ARCHITECTURE.md (warning emission when phase section is suppressed): deferred to implementation agent. If a warning is added, use `eprintln!("WARN: ...")` style matching existing patterns; do not introduce `tracing` in the report module.
