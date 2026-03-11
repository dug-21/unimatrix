# vnc-011: Retrospective ReportFormatter — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-011/SCOPE.md |
| Architecture | product/features/vnc-011/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-011/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-011/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-011/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| retrospective-formatter | pseudocode/retrospective-formatter.md | test-plan/retrospective-formatter.md |
| params-extension | pseudocode/params-extension.md | test-plan/params-extension.md |
| handler-dispatch | pseudocode/handler-dispatch.md | test-plan/handler-dispatch.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add a markdown formatter as the default output format for `context_retrospective`, reducing token cost by approximately 80% while preserving all actionable signal. The formatter applies finding collapse, baseline filtering, narrative compression, recommendation deduplication, and renders session tables, rework counts, and context reload metrics -- all purely at the formatting layer. The JSON path (`format: "json"`) remains completely unchanged.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Default output format | Markdown is the default; JSON is opt-in via `format: "json"` | Human override | architecture/ADR-001-format-dependent-evidence-limit-default.md |
| evidence_limit for JSON path | NO change -- JSON path keeps its existing `unwrap_or(3)` default. Markdown path ignores evidence_limit entirely (formatter controls its own k=3 selection). | Human override (supersedes SCOPE line 49, Spec FR-02, ADR-001) | architecture/ADR-001-format-dependent-evidence-limit-default.md |
| Evidence example selection | Deterministic, earliest-first by timestamp. Sort combined evidence pool by `ts` ascending, take first 3. | Human override (supersedes Spec FR-08 "random") | architecture/ADR-002-deterministic-example-selection.md |
| rework_session_count and context_reload_pct | IN SCOPE for markdown rendering. Architecture's "not rendered" stance is overridden. Spec FR-13 is accepted. | Human override (supersedes Architecture None-handling table) | architecture/ADR-003-separate-retrospective-module.md |
| Formatter module location | New `retrospective.rs` in `crates/unimatrix-server/src/mcp/response/` | ADR-003 | architecture/ADR-003-separate-retrospective-module.md |
| Actionability tagging | Deferred from this feature. SCOPE excludes it despite PRODUCT-VISION listing it. | SCOPE + Alignment Report | architecture/ADR-003-separate-retrospective-module.md |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-server/src/mcp/response/retrospective.rs` | Markdown formatter: `format_retrospective_markdown()` + all render helpers + `CollapsedFinding` internal type |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-server/src/mcp/response/mod.rs` | Register `retrospective` module behind `#[cfg(feature = "mcp-briefing")]`, add `pub use` for `format_retrospective_markdown` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `format: Option<String>` to `RetrospectiveParams`; add dispatch logic in `context_retrospective` handler to route markdown vs JSON |

## Data Structures

### RetrospectiveReport (consumed read-only, not modified)

```rust
pub struct RetrospectiveReport {
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,          // metrics.universal.total_duration_secs
    pub hotspots: Vec<HotspotFinding>,  // may be empty
    pub is_cached: bool,
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    pub entries_analysis: Option<Vec<EntryAnalysis>>,   // not rendered in markdown
    pub narratives: Option<Vec<HotspotNarrative>>,
    pub recommendations: Vec<Recommendation>,           // may be empty
    pub session_summaries: Option<Vec<SessionSummary>>,
    pub feature_knowledge_reuse: Option<FeatureKnowledgeReuse>,
    pub rework_session_count: Option<u64>,
    pub context_reload_pct: Option<f64>,
    pub attribution: Option<AttributionMetadata>,
}
```

### CollapsedFinding (new, formatter-internal)

```rust
struct CollapsedFinding {
    rule_name: String,
    severity: Severity,                    // highest in group
    claims: Vec<String>,                   // from grouped findings
    total_events: f64,                     // sum of measured across group
    tool_breakdown: Vec<(String, usize)>,  // tool -> count from evidence
    examples: Vec<EvidenceRecord>,         // k=3 earliest by timestamp
    cluster_count: Option<usize>,          // from matched narrative
    sequence_pattern: Option<String>,      // from matched narrative
}
```

### Key Consumed Types

| Type | Key Fields | Source |
|------|-----------|--------|
| `HotspotFinding` | `rule_name`, `severity`, `claim`, `measured`, `evidence: Vec<EvidenceRecord>` | unimatrix-observe |
| `EvidenceRecord` | `description`, `ts: u64`, `tool: Option<String>`, `detail` | unimatrix-observe |
| `BaselineComparison` | `metric_name`, `current_value`, `mean`, `stddev`, `status: BaselineStatus`, `phase: Option<String>` | unimatrix-observe |
| `BaselineStatus` | `Normal`, `Outlier`, `NoVariance`, `NewSignal` | unimatrix-observe |
| `Severity` | `Info`, `Warning`, `Critical` | unimatrix-observe |
| `SessionSummary` | `session_id`, `started_at`, `duration_secs`, `tool_distribution`, `knowledge_served`, `knowledge_stored`, `outcome` | unimatrix-observe |
| `FeatureKnowledgeReuse` | `delivery_count`, `cross_session_count`, `by_category`, `category_gaps` | unimatrix-observe |
| `AttributionMetadata` | `attributed_session_count`, `total_session_count` | unimatrix-observe |
| `HotspotNarrative` | `hotspot_type`, `summary`, `clusters`, `sequence_pattern` | unimatrix-observe |
| `Recommendation` | `hotspot_type`, `action`, `rationale` | unimatrix-observe |

## Function Signatures

### Public (new)

```rust
// crates/unimatrix-server/src/mcp/response/retrospective.rs
pub fn format_retrospective_markdown(
    report: &unimatrix_observe::RetrospectiveReport,
) -> CallToolResult
```

### Private render helpers (all in retrospective.rs)

```rust
fn render_header(report: &RetrospectiveReport) -> String
fn render_sessions(summaries: &[SessionSummary]) -> String
fn render_attribution_note(attr: &AttributionMetadata) -> String
fn render_baseline_outliers(comparisons: &[BaselineComparison]) -> String
fn render_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
) -> String
fn render_phase_outliers(comparisons: &[BaselineComparison]) -> String
fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse) -> String
fn render_rework_reload(
    rework: Option<u64>,
    reload_pct: Option<f64>,
) -> String
fn render_recommendations(recs: &[Recommendation]) -> String
fn collapse_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
) -> Vec<CollapsedFinding>
fn format_duration(secs: u64) -> String
```

### Modified (existing)

```rust
// crates/unimatrix-server/src/mcp/tools.rs — RetrospectiveParams
pub struct RetrospectiveParams {
    pub feature_cycle: String,
    pub agent_id: Option<String>,
    pub evidence_limit: Option<usize>,
    pub format: Option<String>,  // NEW: "markdown" (default) or "json"
}
```

## Constraints

- **C-01: Formatter-only changes.** All collapse, selection, filtering, and formatting logic lives in the formatter. The report generation pipeline (`build_report()`, detection rules, `synthesis.rs`, baseline computation) is untouched.
- **C-02: No struct modifications.** `RetrospectiveReport` and all `unimatrix-observe` types are not modified.
- **C-03: JSON path unchanged.** `format: "json"` produces the exact same output as today. The existing `evidence_limit` default of 3 on the JSON path is NOT changed. The clone-and-truncate logic only applies to the JSON path.
- **C-04: Graceful degradation.** Every Optional field being None produces valid markdown (header + any non-optional hotspot/recommendation data).
- **C-05: No actionability tagging.** Deferred from vision-listed deliverables.
- **C-06: Feature gate.** New `retrospective.rs` module gated behind `#[cfg(feature = "mcp-briefing")]`.
- **C-07: No new crate dependencies.** Use `std::fmt::Write` or string building. No `rand` crate needed since selection is deterministic by timestamp.
- **C-08: Performance.** Formatting completes in under 5ms for reports with up to 50 findings and 100 baseline comparisons.

## Dependencies

| Dependency | Status | Notes |
|-----------|--------|-------|
| `unimatrix-observe` | Existing crate | Types consumed read-only |
| `unimatrix-store` (metrics) | Existing crate | `MetricVector`, `UniversalMetrics`, `PhaseMetrics` |
| `rmcp` | Existing dependency | `CallToolResult`, `Content` for response construction |
| `serde_json` | Existing dependency | JSON path unchanged |
| col-020 | COMPLETE | Provides `SessionSummary`, `AttributionMetadata`, `rework_session_count`, `context_reload_pct` |
| col-020b | In progress | Provides `FeatureKnowledgeReuse`. Formatter handles None gracefully if types change. |

## NOT in Scope

- Changes to `RetrospectiveReport` struct or any `unimatrix-observe` types
- Changes to `build_report()`, detection rules, or `synthesis.rs`
- Changes to baseline computation logic
- Actionability tagging (`[actionable]`/`[expected]`/`[informational]`)
- Drill-down tool for per-finding detail
- Changes to `evidence_limit` default on the JSON path
- `entries_analysis` rendering in markdown (available in JSON only)

## Alignment Status

The Alignment Report identified three variances, all resolved by human decisions:

1. **evidence_limit default (Variance 1)**: Architecture ADR-001 proposed format-dependent defaults. Specification FR-02 proposed global change to 0. **Resolution**: Neither -- the JSON path keeps its existing `unwrap_or(3)` default unchanged. The markdown path ignores `evidence_limit` entirely. No `evidence_limit` parameter change is needed.

2. **Evidence selection (Variance 2)**: Architecture ADR-002 proposed deterministic timestamp ordering. Specification FR-08 proposed random selection. **Resolution**: Deterministic, earliest-first by timestamp. ADR-002 is accepted.

3. **FR-13 rework/context_reload (Variance 3)**: Architecture excluded these from rendering. Specification FR-13 included them. **Resolution**: FR-13 is accepted -- `rework_session_count` and `context_reload_pct` are IN SCOPE for markdown rendering.

The vision-listed actionability tagging deferral is acknowledged and documented (WARN, not a blocking variance).

## Markdown Output Structure

```markdown
# Retrospective: {feature_cycle}
{session_count} sessions | {total_records} tool calls | {formatted_duration}

## Sessions
| # | Window | Calls | Knowledge | Outcome |
|---|--------|-------|-----------|---------|
| 1 | 09:52 (57m) | 312 | 5 served, 2 stored | success |

> Note: 3/5 sessions attributed. Metrics may undercount.

## Outliers (vs {N}-feature baseline)
| Metric | Value | Mean | sigma |
|--------|-------|------|-------|

## Findings ({count})
### F-01 [warning] {claim} -- {total_events} events across {session_count} sessions
{tool_breakdown}
{narrative_summary} ({cluster_count} clusters)
Escalation pattern: {sequence_pattern}
Examples:
- {description} at ts={ts}

## Phase Outliers
| Phase | Metric | Value | Mean | sigma |
|-------|--------|-------|------|-------|

## Knowledge Reuse
{delivery_count} entries delivered | {cross_session_count} cross-session | Gaps: {gaps}
{rework_count} rework sessions | {reload_pct}% context reload

## Recommendations
- {action}
```

Sections are omitted entirely when their source data is None or empty after filtering.
