# nan-009: Phase-Stratified Eval Scenarios — Architecture

## System Overview

nan-009 is a pure measurement instrumentation feature. It adds a `phase` dimension to the
eval harness pipeline without changing retrieval logic, scoring, or baseline recording.
The `query_log.phase` column delivered in col-028 (GH #403) is already present in the
database. This feature threads that column through five pipeline stages — extraction,
scenario JSONL, result JSON, report aggregation, and report rendering — and surfaces it as
a new report section (section 6: Phase-Stratified Metrics) while renumbering the existing
Distribution Analysis from section 6 to section 7.

The ASS-032 research context frames this as the measurement instrument for Loop 2 of the
self-learning architecture: phase-conditioned retrieval improvements cannot be measured
without phase-stratified eval output. nan-009 delivers that instrument without activating
the `w_phase_explicit` weight path.

## Component Breakdown

### Component 1: Scenario Extraction (`eval/scenarios/`)

**Responsibility**: Read `query_log` rows and produce JSONL scenario records.

Three files are modified:

- `types.rs` — `ScenarioContext` gains `phase: Option<String>`. Serde attribute decision
  is governed by ADR-001 (suppress null emission via `skip_serializing_if`).
- `output.rs` — The `do_scenarios` SQL SELECT gains the `phase` column.
- `extract.rs` — `build_scenario_record` reads `phase` via
  `row.try_get::<Option<String>, _>("phase")?` and sets `context.phase`.

**Boundary**: `ScenarioRecord` (JSONL) is the output contract. Downstream consumers
(replay, any external tooling) must tolerate absent `phase` keys due to the
`skip_serializing_if` decision.

### Component 2: Result Passthrough (`eval/runner/`)

**Responsibility**: Carry `phase` from the scenario record through to the per-scenario
result JSON without using it for retrieval.

Two files are modified:

- `output.rs` — `ScenarioResult` gains `phase: Option<String>`. No serde annotation
  needed on the writer side (always written). ADR-001 applies to the *report-side* copy.
- `replay.rs` — `replay_scenario` sets `phase: record.context.phase.clone()` on the
  constructed `ScenarioResult`. Phase is NOT injected into `ServiceSearchParams` or
  `AuditContext` (Constraint 3 in SCOPE.md: measurement purity).

**Constraint**: `phase` is metadata only during replay. It must never influence
`ServiceSearchParams.retrieval_mode` or any scoring weight during `eval run`.

### Component 3: Report Aggregation (`eval/report/aggregate.rs`)

**Responsibility**: Group `ScenarioResult`s by phase and compute per-phase mean metrics.

One new function is added:

- `compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>`

  Groups results by `result.phase`. Phase key `None` renders under the label `"(unset)"`.
  Within each group, computes mean P@K, mean MRR, mean CC@k, mean ICD, and scenario count.
  Returns a `Vec<PhaseAggregateStats>` sorted alphabetically by phase name, with `"(unset)"`
  sorted last. Returns an empty Vec when all phase values are None (section 7 is then
  omitted by the renderer).

A new internal struct is added to `report/mod.rs`:

```rust
#[derive(Debug, Default)]
pub(super) struct PhaseAggregateStats {
    pub phase_label: String,   // "design" | "delivery" | "bugfix" | "(unset)"
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_cc_at_k: f64,
    pub mean_icd: f64,
}
```

If `aggregate.rs` approaches 500 lines after adding `compute_phase_stats`, extract the
function to a new `aggregate_phase.rs` sub-module (Constraint 7 in SCOPE.md).

### Component 4: Report Rendering (`eval/report/render.rs`)

**Responsibility**: Render the new section 6 and relabel the existing section 6 to section 7.

Two changes:

1. **New `render_phase_section`** — produces section 6 "Phase-Stratified Metrics" as a
   Markdown table. Only called when at least one scenario has a non-null phase. Omitted
   entirely (not rendered as an empty section) when `phase_stats` is empty.

2. **Section renumbering** — the existing `## 6. Distribution Analysis` heading in
   `render_report` becomes `## 7. Distribution Analysis`. The header string in the module
   docstring is also updated. This is the primary SR-02 risk site.

Phase label in section 2 (Notable Ranking Changes): the `phase` field is read directly
from `ScenarioResult` in the renderer, not by extending the `NotableEntry` tuple type
(RD-04 in SCOPE.md). The per-scenario header line gains `phase` alongside `scenario_id`
and `query` when non-null.

### Component 5: Report Entry Point (`eval/report/mod.rs`)

**Responsibility**: Wire `compute_phase_stats` into the `run_report` pipeline and pass
results to `render_report`.

Changes:
- Add `PhaseAggregateStats` struct.
- Add `phase: Option<String>` to the local `ScenarioResult` copy with
  `#[serde(default)]`. Do NOT add `skip_serializing_if` here — the report side only
  deserializes; it does not re-serialize result JSON (ADR-001 scope is the writer side).
- Wire `compute_phase_stats` call into Step 4 of `run_report`.
- Pass `phase_stats: &[PhaseAggregateStats]` to `render_report`.

## Data Flow

```
query_log table
    └─ phase: Option<String>   (col-028, GH #403)
            │
            ▼
    eval/scenarios/output.rs   [SQL: SELECT ... phase FROM query_log]
            │
            ▼
    eval/scenarios/extract.rs  [build_scenario_record]
            │
            ▼
    ScenarioContext.phase       (JSONL scenario file)
    [serde: skip_serializing_if = "Option::is_none" — ADR-001]
            │
            ▼
    eval/runner/replay.rs      [replay_scenario: passthrough only]
            │
            ▼
    ScenarioResult.phase        (per-scenario .json result file)
    [serde: no annotation on writer side; default on reader copy]
            │
            ▼
    eval/report/mod.rs         [deserialized via local ScenarioResult copy]
            │
            ├─ compute_phase_stats()  → Vec<PhaseAggregateStats>
            │
            ▼
    eval/report/render.rs
            ├─ render_phase_section()   → "## 6. Phase-Stratified Metrics"
            └─ render_distribution_analysis() → "## 7. Distribution Analysis"
```

## Section Renumbering Impact

The existing section 6 "Distribution Analysis" shifts to section 7. Every site that
hard-codes the section number or heading string must be updated:

| File | Current string | New string |
|------|---------------|------------|
| `render.rs` line ~198 | `## 6. Distribution Analysis` | `## 7. Distribution Analysis` |
| `render.rs` module docstring | lists sections 1–6 | must list 1–7 |
| `mod.rs` module docstring | lists sections 1–6 | must list 1–7 |
| `report/tests.rs` | `test_report_contains_all_five_sections` | update assertion for `## 7. Distribution Analysis` and add `## 6. Phase-Stratified Metrics` |
| `report/tests.rs` | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | update section assertions to use `## 7. Distribution Analysis` and add `## 6.` guard |

The golden-output round-trip test (ADR-002) is the primary guard against silent section
renumbering regressions (SR-02).

## Technology Decisions

- **serde null suppression** — `#[serde(default, skip_serializing_if = "Option::is_none")]`
  on `ScenarioContext.phase` in `types.rs`. See ADR-001.
- **Dual-type guard** — Round-trip integration test as the enforcement mechanism. See ADR-002.
- **Phase vocabulary** — Free-form strings, no enum; closed documentation. See ADR-003.
- **Section ordering** — Phase-Stratified Metrics is section 6; Distribution Analysis
  shifts to section 7. Established in SCOPE.md RD-01.
- **Null phase label** — `"(unset)"` (resolved from SR-04: SCOPE.md Constraint 5 takes
  precedence over the Goals text that said `"(none)"`; `"(unset)"` is more precise).

## Integration Points

### Upstream dependency

`crates/unimatrix-store/src/query_log.rs` — `QueryLogRecord.phase: Option<String>` is
fully present post-col-028. The `eval/scenarios/output.rs` SQL query is the only
integration point; no store API changes are needed.

### Internal eval module dependencies

The eval module has a deliberate compile-time isolation: `report/mod.rs` does not import
from `runner/output.rs`. The two `ScenarioResult` types are kept as independent copies
connected only through the JSON file format. This isolation is the reason for the dual-type
maintenance burden and is the architectural constraint that makes the round-trip integration
test mandatory.

### No external service dependencies

`eval report` is synchronous (Constraint 4). `compute_phase_stats` must be a pure
synchronous function operating only on the already-loaded `Vec<ScenarioResult>`. No
database access, no tokio, no async.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ScenarioContext.phase` | `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` | `eval/scenarios/types.rs` |
| `ScenarioResult.phase` (runner) | `Option<String>` (no serde annotation) | `eval/runner/output.rs` |
| `ScenarioResult.phase` (report) | `Option<String>` with `#[serde(default)]` | `eval/report/mod.rs` |
| `PhaseAggregateStats` | `{ phase_label: String, scenario_count: usize, mean_p_at_k: f64, mean_mrr: f64, mean_cc_at_k: f64, mean_icd: f64 }` | `eval/report/mod.rs` (new) |
| `compute_phase_stats` | `fn(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>` | `eval/report/aggregate.rs` (new) |
| `render_phase_section` | `fn(phase_stats: &[PhaseAggregateStats]) -> String` | `eval/report/render.rs` (new) |
| `render_report` (updated signature) | gains `phase_stats: &[PhaseAggregateStats]` parameter | `eval/report/render.rs` |
| `run_report` | unchanged public signature | `eval/report/mod.rs` |
| `replay_scenario` result | `ScenarioResult { ..., phase: record.context.phase.clone() }` | `eval/runner/replay.rs` |
| `build_scenario_record` | reads `row.try_get::<Option<String>, _>("phase")?` | `eval/scenarios/extract.rs` |
| `do_scenarios` SQL | `SELECT ..., phase FROM query_log` | `eval/scenarios/output.rs` |

## Test Requirements

### Golden-output round-trip test (SR-02, SR-03 guard) — ADR-002

A single integration test `test_report_round_trip_phase_section_7_distribution` in
`report/tests.rs` that:

1. Creates a `ScenarioResult` with `phase: Some("delivery".to_string())` and non-trivial
   metric values.
2. Serializes it to a `TempDir` JSON file.
3. Calls `run_report`.
4. Asserts:
   - `content.contains("## 6. Phase-Stratified Metrics")` — new section present.
   - `content.contains("## 7. Distribution Analysis")` — renumbered section present.
   - `content.contains("delivery")` — phase label appears in section 6.
   - Section order: `pos("## 6.")` < `pos("## 7.")`.
   - `!content.contains("## 6. Distribution Analysis")` — old heading absent.
5. Also updates the existing `test_report_contains_all_five_sections` to assert seven
   sections and the existing round-trip test to assert the new section 7 heading.

### Scenario extraction integration test (AC-10)

In `eval/scenarios/tests.rs`: extend `insert_query_log_row` to accept `phase: Option<&str>`.
Add a test that inserts a row with `phase = "delivery"` and asserts the extracted JSONL
contains `"context":{"phase":"delivery",...}`.

### Null-phase omission test (AC-04)

In `report/tests.rs`: a test where all `ScenarioResult`s have `phase: None`, asserts that
`run_report` output does NOT contain `"## 6. Phase-Stratified Metrics"`.

### Phase grouping unit test (AC-05, AC-09)

In `report/tests.rs` or a dedicated unit test: call `compute_phase_stats` with a
`Vec<ScenarioResult>` containing mixed phase values. Assert correct grouping, counts, and
mean values.

## Open Questions

1. **SR-06 warning emission**: Should `render_report` emit a `tracing::warn!` (or
   `eprintln!`) when the phase section is suppressed because all phases are None? The SCOPE
   non-goal says no new CLI flags, but a passive warning to stderr is consistent with the
   existing `WARN: no result JSON files found` pattern. Decision deferred to the
   implementation agent; if added, it should match the existing `eprintln!("WARN: ...")`
   style in `run_report` rather than introducing a `tracing` dependency in the report
   module.

2. **`(phase × profile)` cross-product table**: SCOPE.md RD-02 defers this. The
   `PhaseAggregateStats` struct is designed with only per-phase rows to keep it simple.
   If the cross-product view is added in a future feature, `PhaseAggregateStats` will need
   a `profile_name` dimension. This is not a constraint on the current design but should
   be noted for the future iteration.
