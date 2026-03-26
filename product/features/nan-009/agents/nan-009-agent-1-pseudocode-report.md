# Agent Report: nan-009-agent-1-pseudocode

Agent ID: nan-009-agent-1-pseudocode
Feature: nan-009 — Phase-Stratified Eval Scenarios
Stage: Session 2 Stage 3a (Pseudocode)

---

## Output

Seven pseudocode files produced:

- `product/features/nan-009/pseudocode/OVERVIEW.md`
- `product/features/nan-009/pseudocode/scenario-extraction.md`
- `product/features/nan-009/pseudocode/result-passthrough.md`
- `product/features/nan-009/pseudocode/report-aggregation.md`
- `product/features/nan-009/pseudocode/report-rendering.md`
- `product/features/nan-009/pseudocode/report-entrypoint.md`
- `product/features/nan-009/pseudocode/documentation.md`

---

## Components Covered

1. Scenario Extraction (`eval/scenarios/types.rs`, `output.rs`, `extract.rs`)
2. Result Passthrough (`eval/runner/output.rs`, `replay.rs`)
3. Report Aggregation (`eval/report/aggregate.rs`)
4. Report Rendering (`eval/report/render.rs`)
5. Report Entry Point (`eval/report/mod.rs`)
6. Documentation (`docs/testing/eval-harness.md`)

---

## Source Inspection Findings

### `aggregate.rs` line count
Current: ~395 lines. Adding `compute_phase_stats` (~65-80 lines) brings it to ~460-475 lines.
Within the 500-line limit. Conditional split to `aggregate_phase.rs` deferred to implementation
agent if the final count exceeds 490.

### `render.rs` — `render_report` parameter position
New `phase_stats` parameter placed second (after `stats`, before `results`). This matches
the pattern #3529 reference in the IMPLEMENTATION-BRIEF.

### Section 2 phase label implementation
`find_notable_ranking_changes` returns `NotableEntry<'a>` tuples. RD-04 states "do not extend
the tuple." Pseudocode uses `results.iter().find(|r| r.scenario_id == scenario_id)` to look
up phase from the `results` slice. O(n) per notable entry — at most 10 entries × ~few thousand
results; acceptable.

### `do_scenarios` SQL
`phase` appended to the SELECT list after `similarity_scores`. Column reads in `extract.rs`
use `try_get` by name (not position) — column order is safe to change but append-at-end
is conventional.

### Dual-type ScenarioResult
Both copies confirmed to need `phase: Option<String>`:
- Runner copy (`runner/output.rs`): `#[serde(default)]` only
- Report copy (`report/mod.rs`): `#[serde(default)]` only
- Neither has `skip_serializing_if` (ADR-001: only `ScenarioContext.phase` in `types.rs` suppresses null)

### Test helpers in `runner/output.rs`
The `make_scored_entry`, `make_profile_result`, and `make_comparison_metrics` helpers in the
existing `tests` module do NOT construct `ScenarioResult`. The `ScenarioResult` struct literal
appears only in `replay.rs`. No test helpers in `output.rs` require updating for `phase`.
(The `make_scenario_result` helper mentioned in `report/tests.rs` is a separate test-only
function — not visible in the source read; implementation agent must update it.)

---

## Open Questions

None. All ambiguities resolved in IMPLEMENTATION-BRIEF, ARCHITECTURE, and ADRs.

Specific confirmations:
- `"(unset)"` is canonical null label — ADR-003 confirmed
- `phase_stats` is the second parameter to `render_report` — IMPLEMENTATION-BRIEF confirmed
- Section 6 = Phase-Stratified Metrics; section 7 = Distribution Analysis — all five
  renumbering sites identified and documented
- Runner `ScenarioResult.phase` uses `#[serde(default)]` only (no `skip_serializing_if`) — V-3 resolved
- SR-06 (warning when section 6 suppressed) deferred to implementation agent — use
  `eprintln!("WARN: ...")` style if added; do not introduce `tracing` in report module

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for eval harness serde phase extraction patterns
  — found #3555 (eval harness phase gap), #3526 (round-trip dual-type test strategy),
  #2806 (eval harness general pattern), #3550 (dual-type constraint)
- Queried: `/uni-query-patterns` for nan-009 architectural decisions (category: decision,
  topic: nan-009) — no results (ADRs are in files, not yet in Unimatrix KB for nan-009)
- ADR files read directly: ADR-001, ADR-002, ADR-003

- Deviations from established patterns: none
  - Null suppression via `skip_serializing_if` follows pattern #3255
  - Round-trip integration test follows ADR-003 nan-008 / pattern #3526
  - Dual-type constraint follows pattern #3550
  - `"(unset)"` sort-last override follows ADR-003 (documented deviation from plain lexicographic sort)
