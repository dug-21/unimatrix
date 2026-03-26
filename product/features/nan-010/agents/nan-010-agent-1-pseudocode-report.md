# Agent Report: nan-010-agent-1-pseudocode

## Task

Produce per-component pseudocode for all 7 components of nan-010 (Distribution Change
Profile Flag for Eval Harness).

## Output Files

All files in `/workspaces/unimatrix/product/features/nan-010/pseudocode/`:

| File | Lines | Component |
|------|-------|-----------|
| `OVERVIEW.md` | ~100 | Data flow, shared types, pre-split steps, sequencing |
| `profile-types.md` | ~70 | Component 1: DistributionTargets + EvalProfile extension |
| `profile-validation.md` | ~130 | Component 2: parse_profile_toml extension |
| `runner-profile-meta.md` | ~160 | Component 3: ProfileMetaFile + write_profile_meta + mod.rs wiring |
| `aggregate-distribution.md` | ~140 | Component 4: MetricGateRow + DistributionGateResult + check_distribution_targets |
| `render-distribution-gate.md` | ~180 | Component 5: render_distribution_gate_section |
| `section5-dispatch.md` | ~170 | Component 6: Section 5 dispatch loop in render_report |
| `report-sidecar-load.md` | ~180 | Component 7: load_profile_meta + run_report wiring |

## Components Covered

1. Profile Types (`eval/profile/types.rs`)
2. Profile Validation (`eval/profile/validation.rs`)
3. Runner Profile Meta Sidecar (`eval/runner/profile_meta.rs` + `eval/runner/mod.rs`)
4. Distribution Gate Aggregation (`eval/report/aggregate/distribution.rs`)
5. Distribution Gate Renderer (`eval/report/render_distribution_gate.rs`)
6. Section 5 Dispatch (`eval/report/render.rs`)
7. Report Sidecar Load (`eval/report/mod.rs`)

## Open Questions Found

### OQ-1 (Component 7): ADR-002 corrupt sidecar language is stale

ADR-002 as written says corrupt sidecar → WARN + fallback to empty map. ARCHITECTURE.md
Component 7, IMPLEMENTATION-BRIEF.md, and RISK-TEST-STRATEGY R-07 all say corrupt sidecar
→ abort + non-zero exit. The pseudocode follows the architecture (abort). The ADR file
text has a stale description in its Consequences section. No action required beyond noting
the discrepancy — the implementation agent should implement abort.

### OQ-2 (Component 6): `render.rs` line budget after Section 5 replacement

`render.rs` is at 499 lines. The Section 5 replacement adds a loop (~50+ lines) while
removing only the old flat Section 5 block (~30 lines). Net: likely +20 lines → 519 lines,
exceeding the 500-line limit. The implementation agent must measure and either:
(a) extract the zero-regression render block into a private helper within `render.rs`, or
(b) create a `render_zero_regression.rs` sibling module.

Recommendation: option (b) for consistency with the `render_phase.rs` and
`render_distribution_gate.rs` pattern, if option (a) is insufficient.

### OQ-3 (Component 6): `distribution_gates` map parameter

The pseudocode introduces `distribution_gates: &HashMap<String, DistributionGateResult>`
as a second new parameter to `render_report`. This is not in the IMPLEMENTATION-BRIEF.md
`render_report` signature (which only adds `profile_meta`). However, passing pre-computed
gate results keeps `render_report` pure and testable.

The alternative — computing gate results inside `render_report` — would require importing
`DistributionTargets` and `check_distribution_targets` into the render layer, mixing
aggregation and rendering concerns.

Recommended resolution: keep `distribution_gates` as a separate parameter computed in
`run_report` at Step 4.5. The architecture does not prohibit this addition.

### OQ-4 (OVERVIEW.md): `DistributionTargets` re-export from `profile/mod.rs`

`DistributionTargets` must be added to the `pub use` list in `eval/profile/mod.rs` so that
`runner/profile_meta.rs` can import it. Currently `profile/mod.rs` only re-exports
`AnalyticsMode`, `EvalProfile`, and `EvalServiceLayer`. The implementation agent must add
`DistributionTargets` to this re-export list.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for eval harness profile sidecar patterns — found #3585
  (atomic write pattern), #3582 (sidecar metadata pattern). Both applied.
- Queried: `/uni-query-patterns` for nan-010 ADR decisions — found #3586 (ADR-001 pre-split),
  #3587 (ADR-002 sidecar), #3588 (ADR-003 mrr-floor veto), #3589 (ADR-004 atomic write),
  #3590 (ADR-005 per-profile Section 5).
- All ADR files read in full.
- Existing eval codebase inspected: `types.rs`, `validation.rs`, `runner/mod.rs`,
  `report/mod.rs`, `report/render.rs`, `report/render_phase.rs`, `report/aggregate.rs`,
  `profile/error.rs`, `profile/mod.rs`.

## Deviations from Established Patterns

- **`distribution_gates` map parameter (OQ-3)**: Added to `render_report` beyond what
  IMPLEMENTATION-BRIEF.md specifies. This is a deviation justified by separation of
  concerns (aggregation vs. rendering). Flagged for architect review.

- **`render.rs` module declaration location**: Clarified in Component 7 that
  `mod render_distribution_gate;` should follow the `render_phase` pattern and live in
  `report/mod.rs`, not `render.rs`. This contradicts IMPLEMENTATION-BRIEF.md which says
  to add `mod render_distribution_gate;` to `render.rs`. The correct location is
  `mod.rs` based on the existing `render_phase` pattern. Flagged — implementation agent
  must verify against the actual codebase pattern.

  UPDATE: On inspection, `render.rs` uses `use super::render_phase::render_phase_section`
  which means `render_phase.rs` is declared in `mod.rs`. The same pattern must apply to
  `render_distribution_gate.rs`. The implementation brief's instruction to add the `mod`
  declaration to `render.rs` appears to be incorrect — it should go in `mod.rs`.
