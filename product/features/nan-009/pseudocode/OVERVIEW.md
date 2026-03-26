# nan-009 Pseudocode Overview — Phase-Stratified Eval Scenarios

## Components Involved

| Component | Files Modified | Why |
|-----------|---------------|-----|
| Scenario Extraction | `eval/scenarios/types.rs`, `output.rs`, `extract.rs` | Read `phase` from `query_log`, populate `ScenarioContext.phase`, emit in JSONL |
| Result Passthrough | `eval/runner/output.rs`, `replay.rs` | Carry `phase` from scenario context into per-result JSON; never forward to search params |
| Report Aggregation | `eval/report/aggregate.rs` | New `compute_phase_stats` groups results by phase, computes mean metrics per stratum |
| Report Rendering | `eval/report/render.rs` | New `render_phase_section`; add `phase_stats` param to `render_report`; renumber §6→§7 |
| Report Entry Point | `eval/report/mod.rs` | New `PhaseAggregateStats` struct; new `ScenarioResult.phase` field; wire aggregation + render |
| Documentation | `docs/testing/eval-harness.md` | Document `context.phase`, section 6, phase vocabulary, population requirement |

## Data Flow

```
query_log.phase (col-028)
    |
    v
eval/scenarios/output.rs           -- SQL adds `phase` to SELECT list
    |
    v
eval/scenarios/extract.rs          -- build_scenario_record reads row.try_get("phase")?
    |
    v
ScenarioContext.phase               -- JSONL scenario file
  serde: skip_serializing_if = "Option::is_none"   (ADR-001)
  null phase => key absent from JSONL
    |
    v
eval/runner/replay.rs              -- replay_scenario copies context.phase to ScenarioResult
  MUST NOT forward phase to ServiceSearchParams or AuditContext
    |
    v
ScenarioResult.phase (runner copy) -- per-scenario .json result file
  serde: no annotation; always emits "phase":null or "phase":"delivery"
    |
    v [JSON file boundary — dual-type isolation]
eval/report/mod.rs                 -- local ScenarioResult.phase: serde(default)
    |
    |-- compute_phase_stats(results) --> Vec<PhaseAggregateStats>
    |     returns empty vec when ALL phases are None
    |
    v
eval/report/render.rs
    |-- render_phase_section(phase_stats)  --> "## 6. Phase-Stratified Metrics" (or "")
    |-- render_report (updated signature) --> renumbers §6 Distribution Analysis to §7
```

## Shared Types

### New type: `PhaseAggregateStats` (eval/report/mod.rs)

```
PhaseAggregateStats {
    phase_label: String,     // "design" | "delivery" | "bugfix" | "(unset)"
    scenario_count: usize,
    mean_p_at_k: f64,
    mean_mrr: f64,
    mean_cc_at_k: f64,
    mean_icd: f64,
}
visibility: pub(super) -- shared between aggregate.rs and render.rs
```

### Modified types

`ScenarioContext` (eval/scenarios/types.rs):
- Add: `phase: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`

`ScenarioResult` runner copy (eval/runner/output.rs):
- Add: `phase: Option<String>` with `#[serde(default)]` ONLY — no `skip_serializing_if`

`ScenarioResult` report copy (eval/report/mod.rs):
- Add: `phase: Option<String>` with `#[serde(default)]` ONLY

## New Functions

| Function | Location | Signature |
|----------|----------|-----------|
| `compute_phase_stats` | `eval/report/aggregate.rs` | `fn(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>` |
| `render_phase_section` | `eval/report/render.rs` | `fn(phase_stats: &[PhaseAggregateStats]) -> String` |

## Modified Function Signatures

`render_report` gains one new parameter:
```
pub(super) fn render_report(
    stats: &[AggregateStats],
    phase_stats: &[PhaseAggregateStats],    // NEW
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],
) -> String
```

All other public signatures are unchanged.

## Section Renumbering — Five Sites

| Site | Old | New |
|------|-----|-----|
| `render.rs` ~line 198 (heading in render_report body) | `## 6. Distribution Analysis` | `## 7. Distribution Analysis` |
| `render.rs` module docstring (section list) | lists 1-6 | lists 1-7 |
| `mod.rs` module docstring (section list) | lists 1-6 | lists 1-7 |
| `report/tests.rs` `test_report_contains_all_five_sections` | asserts 5 sections | asserts 7 sections with `## 6. Phase-Stratified Metrics` and `## 7. Distribution Analysis` |
| `report/tests.rs` `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | asserts `## 6. Distribution Analysis` | asserts `## 7. Distribution Analysis` |

## Sequencing Constraints

1. `PhaseAggregateStats` must be defined in `mod.rs` before `aggregate.rs` or `render.rs` use it.
2. `ScenarioContext.phase` (extraction side) is independent of the report-side changes — both can be implemented in parallel.
3. The two `ScenarioResult` copies (`runner/output.rs` and `report/mod.rs`) must both be updated before the round-trip test is written — a partial update causes the round-trip test to fail as designed.
4. `render_report` signature change in `render.rs` must be coordinated with the call site in `mod.rs` — compiler enforces this.

## Null-Phase Canonical Label

`"(unset)"` is the canonical string everywhere (ADR-003, SPEC Constraint 5). It must appear
in exactly one place in the implementation — the label assignment inside `compute_phase_stats`.
Both the sort logic and the render logic use the same string. `"(none)"` must not appear
anywhere in code, tests, or documentation.

## Key Pattern References

- Pattern #3526: Round-trip integration test strategy for dual-type and schema boundary risk
- Pattern #3550: Dual-type constraint — independent ScenarioResult copies in runner and report
- Pattern #3555: Eval harness phase gap — SQL omits query_log.phase
- ADR-001: Serde null suppression (skip_serializing_if on writer side only)
- ADR-002: Round-trip integration test as dual-type guard
- ADR-003: Phase vocabulary governance — free-form, "(unset)" label, sorts last
