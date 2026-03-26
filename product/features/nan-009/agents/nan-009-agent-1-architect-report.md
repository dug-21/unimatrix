# Agent Report: nan-009-agent-1-architect

## Status: Complete

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/nan-009/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/nan-009/architecture/ADR-001-serde-null-suppression.md`
- `/workspaces/unimatrix/product/features/nan-009/architecture/ADR-002-dual-type-guard-strategy.md`
- `/workspaces/unimatrix/product/features/nan-009/architecture/ADR-003-phase-vocabulary-governance.md`

### Unimatrix Storage
Attempted to store all three ADRs. Failed with `MCP error -32003: Agent 'anonymous' lacks
Write capability`. ADR content is complete on disk; Unimatrix storage must be performed
by a privileged agent or deferred.

## Key Decisions Made

### ADR-001: Serde Null Suppression (addresses SR-01)

Three distinct annotation rules, not one blanket rule:

| Type | Annotation | Rationale |
|------|-----------|-----------|
| `ScenarioContext.phase` in `types.rs` | `#[serde(default, skip_serializing_if = "Option::is_none")]` | Scenario JSONL: suppress null to preserve wire shape of pre-existing files |
| `ScenarioResult.phase` in `runner/output.rs` | none (always emits) | Result JSON: AC-02/AC-03 require null to be explicit |
| `ScenarioResult.phase` in `report/mod.rs` | `#[serde(default)]` only | Reader side only; tolerate absent key in pre-nan-009 result files |

### ADR-002: Dual-Type Guard (addresses SR-02 and SR-03)

Round-trip integration test `test_report_round_trip_phase_section_7_distribution` in
`report/tests.rs`. Guards both:
- SR-02 (section renumbering): asserts `## 7. Distribution Analysis` and absence of
  `## 6. Distribution Analysis`
- SR-03 (dual-type sync): asserts `"delivery"` appears in rendered section 6 output

No shared type introduced. Module isolation from nan-007 preserved.

Two existing tests must be updated: `test_report_contains_all_five_sections` (now six
sections) and `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (now section 7).

### ADR-003: Phase Vocabulary (addresses SR-04)

Free-form strings at the harness level. No enum validation. Null label is `"(unset)"` —
resolving the SCOPE.md wording conflict between `"(none)"` (Goals §5) and `"(unset)"`
(Constraint 5) in favor of Constraint 5, which is more precise.

Sort order: named phases alphabetically ascending, `"(unset)"` unconditionally last
(requires special-case sort, not alphabetical, because `(` sorts before letters).

## Section Renumbering Impact

| Old | New | Files requiring update |
|-----|-----|----------------------|
| `## 6. Distribution Analysis` | `## 7. Distribution Analysis` | `render.rs`, `mod.rs` docstrings, `tests.rs` (2 tests) |
| (new) | `## 6. Phase-Stratified Metrics` | `render.rs`, `mod.rs` docstring |

## Integration Surface (Key Facts for Implementation Agent)

- New function: `compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>`
  in `aggregate.rs`. Must be synchronous. No async, no DB.
- New struct: `PhaseAggregateStats { phase_label: String, scenario_count: usize,
  mean_p_at_k: f64, mean_mrr: f64, mean_cc_at_k: f64, mean_icd: f64 }` in `mod.rs`.
- `render_report` signature gains `phase_stats: &[PhaseAggregateStats]` parameter.
- `replay_scenario` must set `phase: record.context.phase.clone()` on `ScenarioResult`.
- SQL in `do_scenarios`: add `phase` to SELECT column list.
- `build_scenario_record`: add `row.try_get::<Option<String>, _>("phase")?`.

## Open Questions

1. **SR-06 warning**: Should `run_report` emit `eprintln!("WARN: ...")` when section 6
   is suppressed (all phases None)? Deferred to implementation agent. If added, use
   `eprintln!` style consistent with existing warnings in `run_report`, not `tracing`.

2. **`(phase × profile)` table**: Deferred per SCOPE.md RD-02. `PhaseAggregateStats`
   is deliberately structured without a `profile_name` field to keep it simple.

## Knowledge Stewardship

- Queried: context_search for eval harness serde patterns, nan-009 architectural decisions
- Stored: ADR-001, ADR-002, ADR-003 attempted via context_store — all failed with
  `-32003: Agent 'anonymous' lacks Write capability`. ADR content is complete on disk.
  Delivery Leader must store these via context_store with a Write-capable agent ID.
- Declined: no novel patterns discovered beyond what is already in ADR files
