## ADR-004: Phase Narrative as Optional Field on RetrospectiveReport

### Context

`context_cycle_review` needs to surface the explicit phase lifecycle alongside its existing behavioral telemetry. The data involves:

1. The ordered sequence of lifecycle events from `CYCLE_EVENTS`.
2. Per-phase category distribution from `FEATURE_ENTRIES` (this feature only).
3. Cross-cycle comparison: mean distribution per (phase, category) across all prior features that have phase-tagged data.

Three integration options were considered:

**Option A: Extend existing MetricVector / PhaseMetrics with phase signal.**
`PhaseMetrics` in `unimatrix-store` already tracks phase-level metrics. The phase lifecycle data could be folded in as additional fields. Rejected: `PhaseMetrics` is sourced from observation telemetry (session durations, tool counts per phase), not from the lifecycle event log. Mixing audit-trail data with behavioral metrics conflates two distinct signals with different provenance and query paths.

**Option B: Separate top-level field in the MCP response JSON.**
Return cycle events and phase distribution as a separate JSON object at the top level of the MCP response, not inside `RetrospectiveReport`. Rejected: the retrospective response is already assembled in `build_report()` returning `RetrospectiveReport`. Adding a separate JSON stitching layer would break the existing clean builder pattern and require callers to understand a split response structure.

**Option C: Optional field on RetrospectiveReport.**
Add `phase_narrative: Option<PhaseNarrative>` to `RetrospectiveReport` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. `PhaseNarrative` is a new type in `unimatrix-observe/types.rs`. This is the same pattern used for `session_summaries`, `feature_knowledge_reuse`, `narratives`, and `baseline_comparison` — all are optional fields added in later features without breaking the report contract.

### Decision

**Option C**: add `phase_narrative: Option<PhaseNarrative>` to `RetrospectiveReport`.

`PhaseNarrative` contains:
- `phase_sequence: Vec<String>` — ordered phase names derived from `CYCLE_EVENTS`, may repeat (rework).
- `rework_phases: Vec<String>` — phase names appearing more than once in `phase_sequence`.
- `per_phase_categories: HashMap<String, HashMap<String, u64>>` — outer key: phase, inner key: category, value: count of entries stored in that phase/category.
- `cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>` — present when at least one prior feature has phase-tagged data.

`PhaseCategoryComparison` carries `(phase, category, this_feature_count, cross_cycle_mean, sample_features)`.

Phase narrative construction is extracted into a new pure function `build_phase_narrative(events, current_dist, cross_dist)` in a new `unimatrix-observe/src/phase_narrative.rs` module. This keeps the MCP handler thin and makes the narrative logic unit-testable.

The three SQL queries required (cycle events, current distribution, cross-cycle distribution) are executed in the `context_cycle_review` handler before calling `build_report()`, then passed to a new `build_report_with_phase_narrative()` variant — or the field is set on the report after `build_report()` returns.

`phase_narrative` is `None` when `CYCLE_EVENTS` has no rows for the feature (AC-12 backward compatibility).

### Consequences

**Easier**:
- Backward compatible: existing callers that do not consume `phase_narrative` are unaffected; the field is omitted from JSON when `None`.
- Consistent with the established pattern for optional `RetrospectiveReport` fields.
- `build_phase_narrative` is unit-testable independently of the MCP handler.
- The cross-cycle comparison mirrors the existing `compute_baselines` / `compare_to_baseline` pattern already in the codebase.

**Harder**:
- Three new SQL queries execute in the `context_cycle_review` hot path. These are indexed reads (`cycle_id`, `feature_id`) and will be fast, but they are net-new queries on a tool that already does significant database work. Acceptable given the low call frequency of `context_cycle_review`.
- `RetrospectiveReport` gains another optional field, increasing the struct's conceptual surface area.
