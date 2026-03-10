# vnc-011: Retrospective ReportFormatter — Architecture

## System Overview

vnc-011 adds a markdown formatter between the `RetrospectiveReport` struct (produced by `unimatrix-observe`) and the MCP tool response (returned by `unimatrix-server`). The formatter is a pure function that reads the report and produces a compact, scannable markdown string optimized for LLM consumption. The report generation pipeline (`build_report()`, detection rules, narrative synthesis) is untouched.

The feature lives entirely in `unimatrix-server`'s response formatting layer. It consumes types from `unimatrix-observe` read-only. A new `format` parameter on `RetrospectiveParams` selects between markdown (default) and JSON (legacy behavior).

## Component Breakdown

### C1: RetrospectiveParams Extension

**Location**: `crates/unimatrix-server/src/mcp/tools.rs`
**Responsibility**: Add `format` field to `RetrospectiveParams`. Change `evidence_limit` default behavior (ADR-001).

Changes:
- Add `format: Option<String>` to `RetrospectiveParams` (values: `"markdown"`, `"json"`)
- `evidence_limit` default changes from 3 to 0 for markdown format, remains 3 for JSON format (ADR-001)

### C2: Retrospective Markdown Formatter

**Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs` (new file)
**Responsibility**: Transform `RetrospectiveReport` into compact markdown. All collapse, filtering, grouping, and deduplication logic lives here.

Sub-responsibilities:
- **Header**: Feature cycle, session count, total records, total duration
- **Session table**: Render from `session_summaries` (omit section if None)
- **Baseline outliers**: Filter `baseline_comparison` to Outlier + NewSignal status only (omit section if no outliers)
- **Finding collapse**: Group `hotspots` by `rule_name`, pick highest severity per group, render per-entity tool breakdown, select k=3 examples (ADR-002)
- **Phase outliers**: Filter phase-level baseline comparisons to Outlier + NewSignal
- **Knowledge reuse**: Render from `feature_knowledge_reuse` (omit section if None)
- **Recommendations**: Deduplicate by `hotspot_type`, render as action list
- **Attribution note**: Render warning when `attributed_session_count < total_session_count`
- **Zero-activity phase suppression**: Skip phases where `tool_call_count <= 1 AND duration_secs == 0`
- **Narrative integration**: Use `narratives` for sequence patterns and cluster counts where available

### C3: Handler Dispatch

**Location**: `crates/unimatrix-server/src/mcp/tools.rs` (context_retrospective handler)
**Responsibility**: Parse format parameter, dispatch to markdown or JSON formatter.

Changes:
- Parse `params.format` to determine output path
- For `"json"` (or explicit JSON request): existing `format_retrospective_report` with clone-and-truncate (col-010b ADR-001)
- For `"markdown"` (default): new `format_retrospective_markdown` — evidence_limit is irrelevant since the formatter controls its own evidence selection
- The clone-and-truncate step for evidence_limit only applies to the JSON path

## Component Interactions

```
RetrospectiveParams { format: "markdown" | "json", ... }
        |
        v
context_retrospective handler (tools.rs)
        |
        |--[build report pipeline unchanged]-->  RetrospectiveReport
        |
        +-- format == "json" --> clone-and-truncate --> format_retrospective_report() --> JSON CallToolResult
        |
        +-- format == "markdown" --> format_retrospective_markdown() --> Markdown CallToolResult
```

The markdown formatter reads `RetrospectiveReport` immutably. It does not clone or mutate the report. All selection (k=3 examples, outlier filtering, dedup) happens during markdown string construction.

## Technology Decisions

- **ADR-001**: Format-dependent evidence_limit default (SR-03 mitigation)
- **ADR-002**: Deterministic example selection via timestamp ordering (SR-02 mitigation)
- **ADR-003**: Formatter as separate module (not added to briefing.rs)

## Integration Points

- **unimatrix-observe types** (read-only): `RetrospectiveReport`, `HotspotFinding`, `BaselineComparison`, `BaselineStatus`, `SessionSummary`, `FeatureKnowledgeReuse`, `HotspotNarrative`, `Recommendation`, `EvidenceRecord`, `Severity`, `AttributionMetadata`
- **unimatrix-server response module**: New `retrospective.rs` sub-module alongside existing `briefing.rs`, `entries.rs`, `mutations.rs`, `status.rs`
- **unimatrix-server tools.rs**: Handler dispatch modification, `RetrospectiveParams` extension
- **Existing `format_retrospective_report`**: Unchanged, continues to serve JSON path

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `RetrospectiveParams.format` | `pub format: Option<String>` | `crates/unimatrix-server/src/mcp/tools.rs` |
| `RetrospectiveParams.evidence_limit` | `pub evidence_limit: Option<usize>` (existing, default logic changes) | `crates/unimatrix-server/src/mcp/tools.rs` |
| `format_retrospective_markdown` | `pub fn format_retrospective_markdown(report: &unimatrix_observe::RetrospectiveReport) -> CallToolResult` | `crates/unimatrix-server/src/mcp/response/retrospective.rs` (new) |
| `format_retrospective_report` | `pub fn format_retrospective_report(report: &unimatrix_observe::RetrospectiveReport) -> CallToolResult` (unchanged) | `crates/unimatrix-server/src/mcp/response/briefing.rs` |
| `ResponseFormat` enum | existing `Summary`, `Markdown`, `Json` — NOT used for retrospective (retrospective uses its own `"markdown"`/`"json"` string parsing, since it has only two formats and different default) | `crates/unimatrix-server/src/mcp/response/mod.rs` |

### Internal Formatter Functions (private to retrospective.rs)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `render_header` | `fn render_header(report: &RetrospectiveReport) -> String` | Feature cycle + summary stats |
| `render_sessions` | `fn render_sessions(summaries: &[SessionSummary]) -> String` | Session table |
| `render_baseline_outliers` | `fn render_baseline_outliers(comparisons: &[BaselineComparison]) -> String` | Filtered baseline table |
| `render_findings` | `fn render_findings(hotspots: &[HotspotFinding], narratives: Option<&[HotspotNarrative]>) -> String` | Collapsed finding sections |
| `render_phase_outliers` | `fn render_phase_outliers(comparisons: &[BaselineComparison]) -> String` | Phase-level outlier table |
| `render_knowledge_reuse` | `fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse) -> String` | Knowledge delivery summary |
| `render_recommendations` | `fn render_recommendations(recs: &[Recommendation]) -> String` | Deduplicated action list |
| `render_attribution_note` | `fn render_attribution_note(attr: &AttributionMetadata) -> String` | Attribution quality warning |

### Collapsed Finding Internal Type

```rust
struct CollapsedFinding {
    rule_name: String,
    severity: Severity,           // highest in group
    claims: Vec<String>,          // all claims from grouped findings
    total_events: f64,            // sum of measured values
    tool_breakdown: Vec<(String, usize)>, // tool -> count across all evidence
    examples: Vec<EvidenceRecord>, // k=3 selected by timestamp
    cluster_count: Option<usize>, // from narratives if available
    sequence_pattern: Option<String>, // from narratives if available
}
```

This is a formatter-internal struct, not exported.

## None Field Handling (SR-01)

The `RetrospectiveReport` has 8 Optional fields accumulated across 6 features. The formatter must handle every combination gracefully by omitting the corresponding markdown section when the field is None.

| Field | None means | Formatter behavior |
|-------|-----------|-------------------|
| `baseline_comparison` | No historical data for baselines | Omit "Outliers" and "Phase Outliers" sections |
| `entries_analysis` | No flagged signals accumulated | No formatter impact (not rendered in markdown) |
| `narratives` | JSONL fallback path, no structured events | Finding collapse works from `hotspots` directly; no cluster counts or sequence patterns |
| `session_summaries` | Pre-col-020 report or session load failed | Omit "Sessions" section |
| `feature_knowledge_reuse` | Pre-col-020b report or computation failed | Omit "Knowledge Reuse" section |
| `rework_session_count` | Pre-col-020 report | Not rendered (no dedicated section) |
| `context_reload_pct` | Pre-col-020 report | Not rendered (no dedicated section) |
| `attribution` | Pre-col-020 or attribution computation failed | Omit attribution quality note |

The formatter produces valid, complete markdown for ANY combination of None fields. The minimal report (all Optional fields None) renders: header + findings + recommendations.

## Deferred: Actionability Tagging

PRODUCT-VISION.md lists actionability tagging (`[actionable]`/`[expected]`/`[informational]`) as a vnc-011 deliverable. SCOPE.md explicitly excludes it. This architecture acknowledges the deferral. The finding collapse structure can accommodate severity-like tags in a future iteration without structural changes.
