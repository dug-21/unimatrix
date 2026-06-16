# Agent Report — crt-055 Component 7: Fail-loud presentation guard (Wave 1)

**Agent**: crt-055-agent-3-fail_loud_guard
**Component**: 7 — Fail-loud presentation guard (per-metric availability)
**Wave**: 1 (sequenced FIRST to de-risk believable-zero before any column lands)
**ADRs honored**: ADR-003 (#5046), ADR-004 (#5039) | **Risks**: R-06 (believable-zero, Critical), R-17 (ratio)

## Summary

Implemented the presentation-only fail-loud guard as a dedicated `unimatrix-observe`
module. Introduced `MetricAvailability` on the presentation layer (NOT a `CycleReviewRecord`
column — zero schema impact, zero leak surface), plus pure render primitives that branch on
two orthogonal honesty axes:

1. **available vs unavailable** — a metric whose source class is empty renders the literal
   `"unavailable"` (with a terse reason), never a bare `0`.
2. **exact vs coarse/directional** — behavioral signals (`transcript_error_count`,
   `transcript_refusal_count`, `signal_class_counts_json`) ALWAYS render with a
   directional qualifier (`~N` / "directional") by a constant rule, visually distinct from
   exactly-counted aggregates which render bare.

Per-metric (not one cycle-wide flag); each flag independent (R-06). Ratios render from
num/den PAIRS so `0 of 0`→unavailable stays distinguishable from `0 of N`→measured 0% (R-17).
`context_reload` basis-points (0–10000) render via integer division — no float reaches the
formatter.

### Scoping decisions

- The server-side markdown formatter `unimatrix-server/src/mcp/response/retrospective.rs` is
  already **4326 lines** and `unimatrix-observe/src/types.rs` is **2010 lines**. Neither was
  extended. Wave 1's deliverable — the reusable, unit-testable `MetricAvailability` type and
  render primitives — lives in a new self-contained module `fail_loud_guard.rs` (~470 lines,
  under the 500-line limit). Wiring these primitives into the server render path is a Wave 3 /
  Component 9 (pipeline) concern, consistent with the wave order in OVERVIEW.md.
- `CycleAggregates` / `CycleContext` are presentation-layer input views (mirroring the
  forthcoming v5 columns) so the formatter has typed inputs to branch on in Wave 1, before any
  DB column exists. They are not DB rows.

## Files modified

1. `/workspaces/unimatrix/crates/unimatrix-observe/src/fail_loud_guard.rs` (new) — module +
   14 unit tests.
2. `/workspaces/unimatrix/crates/unimatrix-observe/src/lib.rs` — `pub mod fail_loud_guard;` +
   public re-exports (`MetricAvailability`, `CycleAggregates`, `CycleContext`,
   `compute_availability`, `render_metric`, `render_ratio`, `render_context_reload`,
   `render_metrics_block`).

## Tests

**14 new unit tests, all passing.** Full `unimatrix-observe` lib suite: **530 passed, 0 failed**
(516 pre-existing + 14 new). No integration tests run or modified (Stage 3c).

Test-plan coverage:
- AC-01 / R-06: `test_empty_source_renders_unavailable_per_metric`,
  `test_per_metric_flags_independent`, `test_measured_zero_distinct_from_unavailable`,
  `test_zero_declared_sessions_all_unavailable`, `test_mixed_present_and_empty_sources`.
- AC-01 / R-17: `test_ratio_zero_of_zero_unavailable`, `test_ratio_zero_of_n_measured`,
  `test_ratio_nonzero_rounds_from_pair`.
- AC-21 / R-06: `test_behavioral_signals_carry_directional_qualifier`,
  `test_exact_aggregates_do_not_carry_qualifier`, `test_behavioral_signal_zero_still_directional`.
- reload bps: `test_context_reload_bps_renders_percent`,
  `test_context_reload_single_session_unavailable`, `test_context_reload_clamps_basis_points`.

`cargo fmt` clean; `cargo clippy` clean on the new module (the two remaining observe-crate
warnings are pre-existing in `detection/`, `metrics.rs`, `report.rs` — untouched).

## Issues / blockers

None. The two integration tests in the test plan
(`test_cycle_review_empty_source_renders_unavailable`,
`test_cycle_review_behavioral_signals_directional_qualifier`) are deferred to Stage 3c and
require the Component 9 pipeline wiring (Wave 3) to exercise the real rendered output.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern) + context_get(#5046,
  #3420) — surfaced ADR-003 #5046 (binding coarse/directional clause), pattern #3420
  (Option<bool> for event-derived status — informed keeping per-metric flags independent), and
  lesson #4234 (multi-signal render-guard wrong-condition trap — reinforced per-source-class
  flags over a single condition).
- Stored: entry #5062 "Fail-loud presentation: per-metric availability + always-directional
  behavioral signals are two orthogonal axes" via /uni-store-pattern.
