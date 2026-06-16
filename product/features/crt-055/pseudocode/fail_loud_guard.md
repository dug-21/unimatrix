# Component 7 — Fail-loud presentation guard (per-metric availability)

**Crate**: `unimatrix-observe` (report shape) + presentation/formatter (`tools.rs` render path)
**Files**: `unimatrix-observe/src/types.rs` (RetrospectiveReport presentation fields), the formatter that renders the retrospective
**ADRs**: ADR-003 (#5046), ADR-004 (#5039) | **Risks**: R-06 (believable-zero, Critical), R-17 (ratio) | **Wave**: 1 (sequenced FIRST — de-risks before any column lands)

## Purpose

Replace believable-zero rendering with explicit "unavailable" per metric, and mark regex-derived behavioral signals coarse/directional. Presentation-only: the flags do NOT gate the single writer (Component 2) and do NOT change which source a metric reads. Two honesty axes — (1) available vs unavailable, (2) exact vs coarse/directional.

## Constraints honored

- Per-metric presence, not one cycle-wide flag — sources differ (cycle_events, SessionRecord.outcome, query∪injection, fold, compaction_events) (ADR-003).
- Ratios rendered from stored num/den pairs — `0 of 0` → "unavailable", `0 of N` → measured rate (R-17).
- Behavioral signals (`transcript_error_count`/`refusal_count`/`signal_class_counts_json`) ALWAYS coarse/directional — a constant rule, not a flag (Constraint 6 / ADR-003).
- Presentation-only: does not touch persistence; existing `raw_signals_available` (cycle-wide, `cycle_review_index.rs:80`) stays as-is; this adds per-metric granularity on the presentation layer.

## 7a. MetricAvailability (presentation-layer, NOT persisted on CycleReviewRecord)

```
struct MetricAvailability {
    phase_metrics_available: bool       // cycle_events non-empty for the cycle
    rework_ratio_available: bool        // total_session_count > 0
    knowledge_reuse_available: bool     // query_log ∪ injection_log non-empty
    transcript_fold_available: bool     // ≥1 declared session produced a fold (Component 6)
    compaction_available: bool          // ≥1 attributed compaction_events row (Component 5)
    context_reload_available: bool      // ≥2 sessions in the cycle (cross-session window exists)
}
```

Carried on the `RetrospectiveReport` presentation layer (a new `#[serde(default, skip_serializing_if = ...)]` field, OR computed at render time from the report's own counts). Either way it is NOT a `CycleReviewRecord` DB column (no schema impact, no leak surface).

```
fn compute_availability(agg: &CycleAggregates, ctx: &CycleContext) -> MetricAvailability:
    MetricAvailability {
        phase_metrics_available:  ctx.cycle_events_count > 0,
        rework_ratio_available:   agg.total_session_count > 0,
        knowledge_reuse_available:ctx.knowledge_log_nonempty,   // query∪injection had ≥1 served row
        transcript_fold_available:ctx.any_declared_fold,        // from Component 6 landing.available
        compaction_available:     agg.compaction_count > 0 OR ctx.any_compaction_boundary,
        context_reload_available: ctx.session_count >= 2,
    }
```

Each flag is INDEPENDENT (R-06): one empty source does not flip another's flag; one present source does not mask another's emptiness.

## 7b. Formatter branching (two axes)

```
fn render_metric(label, value, available, coarse) -> String:
    if not available:
        return f"{label}: unavailable ({terse_reason(label)})"   // never "0"
    if coarse:
        return f"{label}: ~{value} (directional)"                // coarse/directional qualifier
    return f"{label}: {value}"                                   // exact, bare

// Exactly-counted aggregates (bare when available):
render_metric("Phases",            agg.phase_count,          avail.phase_metrics_available, coarse=false)
render_metric("Phase transitions", agg.phase_transition_count, avail.phase_metrics_available, false)
render_metric("Rework loops",      agg.phase_rework_count,   avail.phase_metrics_available, false)
render_metric("Never-closed phases", agg.phase_unclosed_count, avail.phase_metrics_available, false)  // #556 hotspot
render_metric("Compactions",       agg.compaction_count,     avail.compaction_available, false)
render_metric("Compaction re-reads", agg.compaction_reread_count, avail.compaction_available, false)

// Ratios from num/den PAIRS (R-17): 0 of 0 → unavailable; 0 of N → measured:
render_ratio("Rework rate", agg.rework_session_count, agg.total_session_count, avail.rework_ratio_available)
render_ratio("Knowledge reuse", agg.knowledge_reuse_served_count, ctx.reuse_denominator, avail.knowledge_reuse_available)

// context_reload: stored bps → percent at presentation (divide by 100):
if avail.context_reload_available: f"Context reload: {agg.context_reload_pct / 100.0:.1}%"
else: "Context reload: unavailable (single-session cycle)"

// Behavioral signals — ALWAYS coarse (constant rule), still subject to availability:
render_metric("Errors (signal)",   agg.transcript_error_count,   avail.transcript_fold_available, coarse=true)
render_metric("Refusals (signal)", agg.transcript_refusal_count, avail.transcript_fold_available, coarse=true)
// signal_class_counts_json rendered with a directional header; every entry directional by construction.
```

`render_ratio(label, num, den, available)`: if not available OR den == 0 → "unavailable"; else `f"{num} of {den}"` (+ optional `{num/den:.0%}`). This is how `0 of 0` (unavailable) is distinguished from `0 of N` (measured 0%).

## Data flow

- IN: `CycleAggregates` + `CycleContext` (counts/flags gathered in the pipeline).
- OUT: `MetricAvailability` + the rendered retrospective strings (presentation only).

## Error handling

- None new — pure presentation. A missing source already resolves to `available=false` upstream; the formatter never divides by zero (ratio guard) and never emits a bare `0` for an unavailable metric.

## Key test scenarios

- Per source class, synthesize an empty cycle (zero cycle_events; zero compaction_events; empty fold; zero served-knowledge) → each renders "unavailable", never literal "0" (AC-01, R-06).
- Per-metric independence: one empty source does not flip another metric's flag (R-06).
- Ratio: `0 of 0` → "unavailable"; `0 of N` → measured rate (R-17).
- **Behavioral-signal honesty (AC-21, R-06)**: `transcript_error_count`/`refusal_count` render with the coarse/directional qualifier (`~`/"directional"), visibly distinct from an exactly-counted aggregate (compaction_count, rework ratio) which renders bare — the two presentations are distinguishable.
- `context_reload_pct` stored bps renders as a percentage (3750 → "37.5%"); single-session cycle → "unavailable", not "0%".
