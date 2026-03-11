# Specification: vnc-011 Retrospective ReportFormatter

## Objective

Add a markdown formatter as the default output format for `context_retrospective`, reducing token cost by approximately 80% while preserving all actionable signal. The formatter applies finding collapse, baseline filtering, narrative compression, and recommendation deduplication purely at the formatting layer -- no changes to `RetrospectiveReport`, `build_report()`, or detection rules. JSON output remains available via `format: "json"` and is byte-identical to current behavior (modulo the `evidence_limit` default change).

## Functional Requirements

### FR-01: Format parameter on RetrospectiveParams

Add a `format` field to `RetrospectiveParams` accepting `"markdown"` (default) or `"json"`. When `format` is `"json"`, the handler produces the same JSON output as today (via existing `format_retrospective_report`). When `format` is `"markdown"` or absent, the handler calls the new markdown formatter.

### FR-02: Evidence limit default change

Change the `evidence_limit` default from 3 to 0. When `evidence_limit` is 0, no evidence truncation is applied (all evidence passes through). This is a global default change affecting both formats. The `evidence_limit` parameter remains optional; callers can still pass any non-negative value.

### FR-03: Markdown header

Render a top-level heading: `# Retrospective: {feature_cycle}` followed by a summary line: `{session_count} sessions | {total_records} tool calls | {formatted_total_duration}`. Duration is derived from `metrics.universal.total_duration_secs` and formatted as human-readable (e.g., "1h 54m", "57m", "3h 12m").

### FR-04: Session table

When `report.session_summaries` is `Some` and non-empty, render a `## Sessions` section with a markdown table. Columns: `#` (1-indexed), `Window` (start time formatted as HH:MM, duration appended as e.g., "09:52 (57m)"), `Calls` (sum of `tool_distribution` values), `Knowledge` (formatted as "{served} served, {stored} stored"), `Outcome` (from `outcome` field, or "-" if None). When `session_summaries` is None or empty, omit the entire section.

### FR-05: Attribution quality note

When `report.attribution` is `Some` and `attributed_session_count < total_session_count`, render a blockquote note below the session table (or below the header if no session table): `> Note: {attributed_session_count}/{total_session_count} sessions attributed. Metrics may undercount.` When attribution is None or all sessions are attributed, omit the note.

### FR-06: Baseline outlier filtering

When `report.baseline_comparison` is `Some` and non-empty, render a `## Outliers` section. Include only entries where `status` is `Outlier` or `NewSignal`. Skip entries with status `Normal` or `NoVariance`. Split into two sub-sections: universal outliers (where `phase` is None) rendered as a table with columns `Metric | Value | Mean | sigma`, and phase outliers (where `phase` is Some) rendered as a table with columns `Phase | Metric | Value | Mean | sigma`. If no outliers exist after filtering, omit the entire section. The section heading includes the baseline sample count from the first entry's underlying `BaselineEntry.sample_count` (available via the `mean`/`stddev` fields -- the formatter uses the label `vs {N}-feature baseline` where N is derived from the data).

### FR-07: Finding collapse by rule_name

Group all `HotspotFinding`s by `rule_name`. Each group becomes a single rendered finding. Assign sequential IDs: F-01, F-02, etc., ordered by highest severity in the group (Critical > Warning > Info), then by total event count descending. For each collapsed finding, render: severity tag in brackets (e.g., `[warning]`), a claim line derived from the first finding's `claim` or the `rule_name` humanized, total event count (sum of all `measured` values in the group, rounded to integer), and session count if `session_summaries` is available and cross-referencing is feasible. Below the heading, render a per-entity breakdown line showing tool/entity names with counts (e.g., "Bash(12), Read(6), context_store(5)"), extracted from evidence `tool` fields across the group.

### FR-08: k=3 random evidence examples

For each collapsed finding group, pool all `evidence` records from all findings in the group. Select up to k=3 examples. Render each as a bullet: `- {description} at ts={ts}`. If the pool has fewer than 3 records, render all of them. Selection uses the standard library random facilities available in the Rust runtime; determinism is not required for MVP.

### FR-09: Narrative collapse

When `report.narratives` is `Some`, use narrative data to enrich collapsed findings. For each narrative matching a collapsed finding's `rule_name` (via `hotspot_type`): render the `summary` as the finding's description line, append `({cluster_count} clusters)` where `cluster_count` is `clusters.len()`, and if `sequence_pattern` is `Some`, render it inline (e.g., "Escalation pattern: 30s->60s->90s->120s"). Do NOT render individual clusters or `top_files`. When narratives is None, fall back to rendering findings from hotspot data alone.

### FR-10: Recommendation deduplication

Collect all `report.recommendations`. Deduplicate by `hotspot_type` -- keep the first occurrence for each unique `hotspot_type`. Render a `## Recommendations` section as a bulleted list of `action` strings. Omit `rationale`. If no recommendations exist, omit the section.

### FR-11: Zero-activity phase suppression

When rendering phase-level data (phase outliers in FR-06, or any phase breakdown), suppress phases where the corresponding `PhaseMetrics` has `tool_call_count <= 1` AND `duration_secs == 0`. This filtering applies in the formatter only.

### FR-12: Knowledge reuse summary

When `report.feature_knowledge_reuse` is `Some`, render a `## Knowledge Reuse` section as a single summary line: `{delivery_count} entries delivered | {cross_session_count} cross-session | Gaps: {category_gaps joined by comma}`. If `category_gaps` is empty, omit the "Gaps:" segment. When `feature_knowledge_reuse` is None, omit the section.

### FR-13: Rework and context reload

When `report.rework_session_count` is `Some` and > 0, or `report.context_reload_pct` is `Some`, include these in the summary line or a brief metrics note. Render rework as `{N} rework sessions` and context reload as `{pct}% context reload`. These are appended to the knowledge reuse line or rendered as a separate line if knowledge reuse is absent.

### FR-14: Formatter is data-driven

The formatter function takes a `&RetrospectiveReport` and a format enum/string, and produces a `CallToolResult`. All collapse, filtering, grouping, and formatting logic is contained within the formatter module. No changes to report generation, detection rules, or the observe crate.

## Non-Functional Requirements

### NFR-01: Token reduction

Markdown output must achieve at least 80% token reduction compared to JSON output for reports with 5+ hotspot findings and 20+ baseline comparisons. Verification: compare byte sizes of markdown vs JSON output for the same report in tests.

### NFR-02: No new crate dependencies

The formatter must not introduce new crate dependencies. Use `std::fmt::Write` or string building. Random selection for k=3 may use `rand` if already in the dependency tree, otherwise use a simple modular arithmetic approach on evidence timestamps.

### NFR-03: Formatter performance

Formatting must complete in under 5ms for reports with up to 50 hotspot findings and 100 baseline comparisons. The formatter is pure computation on in-memory data with no I/O.

### NFR-04: Backward compatibility

`format: "json"` must produce byte-identical output to the current implementation (same `serde_json::to_string_pretty` path). The only behavioral change for JSON consumers is the `evidence_limit` default (FR-02).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `context_retrospective` returns markdown by default (no `format` param) | Unit test: call with no format, assert output starts with `# Retrospective:` |
| AC-02 | `format: "json"` returns unchanged JSON output | Unit test: compare JSON output with and without format param against known report |
| AC-03 | Baseline comparisons in markdown include only Outlier and NewSignal status entries | Unit test: build report with mix of Normal/Outlier/NewSignal/NoVariance, assert markdown contains only Outlier/NewSignal metric names |
| AC-04 | Hotspot findings collapsed by `rule_name` with per-entity breakdown | Unit test: build report with 4 findings sharing a `rule_name`, assert single F-XX heading with combined counts |
| AC-05 | k=3 random examples per collapsed finding | Unit test: build report with 10 evidence records per finding, assert exactly 3 example bullets rendered |
| AC-06 | Zero-activity phases suppressed in markdown output | Unit test: build report with phase having `tool_call_count=0, duration_secs=0`, assert phase absent from output |
| AC-07 | Recommendations deduplicated by `hotspot_type` | Unit test: build report with duplicate `hotspot_type` recommendations, assert each type appears once |
| AC-08 | `evidence_limit` defaults to 0 | Unit test: deserialize RetrospectiveParams with no `evidence_limit`, assert `unwrap_or(0)` yields 0 |
| AC-09 | Session table rendered from `session_summaries` | Unit test: build report with 2 SessionSummary entries, assert markdown table has 2 data rows |
| AC-10 | Token reduction >= 80% for typical reports | Integration test or manual: generate markdown and JSON for same report, compare byte count |
| AC-11 | All existing tests pass | CI: `cargo test --workspace` green |
| AC-12 | Session table omitted when `session_summaries` is None | Unit test: build report with `session_summaries: None`, assert `## Sessions` absent |
| AC-13 | Attribution quality note rendered when sessions partially attributed | Unit test: build report with `attribution: Some(AttributionMetadata { attributed: 3, total: 5 })`, assert blockquote present |
| AC-14 | Knowledge reuse section rendered from `feature_knowledge_reuse` | Unit test: build report with `FeatureKnowledgeReuse` populated, assert `## Knowledge Reuse` present with correct counts |
| AC-15 | Narrative collapse uses summary + cluster count, preserves sequence_pattern | Unit test: build report with narratives containing sequence_pattern, assert pattern appears in output |
| AC-16 | Formatter handles all-None optional fields gracefully | Unit test: build minimal report (all Optional fields None, empty vecs), assert valid markdown with header only |

## Domain Models

### RetrospectiveReport (consumed, not modified)

The complete analysis output from `build_report()`. Contains 15 fields accumulated across col-002 through col-020b. The formatter consumes this struct read-only. Key fields and their optionality:

| Field | Type | Optional | Source Feature |
|-------|------|----------|---------------|
| `feature_cycle` | `String` | No | col-002 |
| `session_count` | `usize` | No | col-002 |
| `total_records` | `usize` | No | col-002 |
| `metrics` | `MetricVector` | No | col-002 |
| `hotspots` | `Vec<HotspotFinding>` | No (may be empty) | col-002 |
| `is_cached` | `bool` | No | col-002 |
| `baseline_comparison` | `Option<Vec<BaselineComparison>>` | Yes | col-002b |
| `entries_analysis` | `Option<Vec<EntryAnalysis>>` | Yes | col-009 |
| `narratives` | `Option<Vec<HotspotNarrative>>` | Yes | col-010b |
| `recommendations` | `Vec<Recommendation>` | No (may be empty) | col-010b |
| `session_summaries` | `Option<Vec<SessionSummary>>` | Yes | col-020 |
| `feature_knowledge_reuse` | `Option<FeatureKnowledgeReuse>` | Yes | col-020/020b |
| `rework_session_count` | `Option<u64>` | Yes | col-020 |
| `context_reload_pct` | `Option<f64>` | Yes | col-020 |
| `attribution` | `Option<AttributionMetadata>` | Yes | col-020 |

### Collapsed Finding (formatter-internal)

An intermediate grouping structure used only within the formatter:
- `rule_name: String` -- the grouping key
- `severity: Severity` -- highest severity in the group
- `findings: Vec<&HotspotFinding>` -- references to grouped findings
- `total_measured: f64` -- sum of `measured` across the group
- `narrative: Option<&HotspotNarrative>` -- matched narrative if available
- `evidence_pool: Vec<&EvidenceRecord>` -- combined evidence from all findings

### Key Terms

- **Finding collapse**: Grouping multiple `HotspotFinding`s that share the same `rule_name` into a single rendered finding in the markdown output.
- **Narrative collapse**: Replacing full cluster listings with a summary line plus cluster count.
- **Baseline filtering**: Excluding `Normal` and `NoVariance` entries from the markdown baseline section.
- **Zero-activity phase**: A phase where `tool_call_count <= 1` AND `duration_secs == 0`.
- **Evidence pool**: The combined set of `EvidenceRecord`s from all findings within a collapsed group.

## User Workflows

### Primary: Retro agent consumes retrospective

1. Agent calls `context_retrospective(feature_cycle: "nxs-010")` (no format param).
2. Server builds report via existing pipeline (unchanged).
3. Server passes report to markdown formatter.
4. Formatter groups findings, filters baselines, compresses narratives, deduplicates recommendations.
5. Agent receives compact markdown -- can reason about findings directly without parsing JSON.

### Secondary: Automation consumes JSON

1. Script calls `context_retrospective(feature_cycle: "nxs-010", format: "json")`.
2. Server builds report via existing pipeline.
3. Server serializes report as JSON (existing `format_retrospective_report` path).
4. Script receives structured JSON for programmatic processing.

### Tertiary: Human reviews via /retro skill

1. Human runs `/retro nxs-010`.
2. Skill invokes `context_retrospective` (markdown default).
3. Human reads scannable markdown in terminal.

## Constraints

### C-01: Formatter-only changes

All collapse, selection, filtering, and formatting logic lives in the formatter. The report generation pipeline (`build_report()`, detection rules, `synthesis.rs`, baseline computation) is untouched. This is a hard constraint from SCOPE.

### C-02: No struct modifications

`RetrospectiveReport` and all types in `unimatrix-observe::types` are not modified. The formatter consumes them as-is.

### C-03: evidence_limit applies globally (SR-03 mitigation)

The `evidence_limit` default change from 3 to 0 applies to both formats. This is an intentional behavioral change. JSON consumers that previously relied on the implicit 3-record truncation will now receive all evidence by default. The SCOPE explicitly states this change. Callers who want truncation can pass `evidence_limit: 3` explicitly.

### C-04: Graceful degradation for all None combinations (SR-01 mitigation)

The formatter must handle every combination of Optional fields being None. For each None optional section, the formatter omits that section entirely. A report with all Optional fields as None must produce valid markdown containing at minimum the header (FR-03) and any non-optional hotspot/recommendation data. This is tested explicitly in AC-16.

### C-05: No actionability tagging

SCOPE explicitly excludes `[actionable]`/`[expected]`/`[informational]` tagging. The PRODUCT-VISION.md lists this as a vnc-011 deliverable, but SCOPE defers it. This specification follows SCOPE. The deferral is acknowledged; no work is scoped here.

### C-06: Formatter location

The formatter function lives in `crates/unimatrix-server/src/mcp/response/` alongside the existing `briefing.rs`. It consumes types from `unimatrix-observe` (re-exported via `unimatrix-store`). No cross-crate changes.

## Dependencies

| Dependency | Status | Impact |
|-----------|--------|--------|
| col-020 (multi-session retrospective) | COMPLETE, merged | Provides `SessionSummary`, `AttributionMetadata`, `rework_session_count`, `context_reload_pct` |
| col-020b (knowledge reuse refinement) | In progress | Provides `FeatureKnowledgeReuse` with `delivery_count`, `cross_session_count`. Formatter handles None gracefully if col-020b types change (SR-07). |
| `unimatrix-observe` types | Consumed read-only | `RetrospectiveReport`, `HotspotFinding`, `BaselineComparison`, `HotspotNarrative`, `Recommendation`, `SessionSummary`, `FeatureKnowledgeReuse`, `AttributionMetadata`, `Severity`, `BaselineStatus`, `EvidenceRecord`, `EvidenceCluster` |
| `unimatrix-store` metrics | Consumed read-only | `MetricVector`, `UniversalMetrics`, `PhaseMetrics` |
| `rmcp` | Existing dependency | `CallToolResult`, `Content` for response construction |
| `serde_json` | Existing dependency | JSON serialization path (unchanged) |

## NOT in Scope

- Changes to `RetrospectiveReport` struct or any `unimatrix-observe` types
- Changes to `build_report()`, detection rules, or `synthesis.rs`
- Changes to baseline computation logic
- Actionability tagging (`[actionable]`/`[expected]`/`[informational]`) -- deferred from PRODUCT-VISION vnc-011 description
- Drill-down tool for per-finding detail
- Deterministic evidence selection (SR-02 accepted for MVP)
- Format-dependent `evidence_limit` defaults (SR-03: global change is intentional per SCOPE)
- Entries analysis rendering in markdown (the `entries_analysis` field is not surfaced in the markdown target structure; it remains available in JSON)
