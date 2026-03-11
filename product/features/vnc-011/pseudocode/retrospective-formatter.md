# Component: retrospective-formatter

## Purpose

New module `retrospective.rs` containing the markdown formatter for `RetrospectiveReport`. Transforms the full report struct into compact, scannable markdown optimized for LLM consumption. All collapse, filtering, grouping, deduplication, and rendering logic lives here.

## Location

`crates/unimatrix-server/src/mcp/response/retrospective.rs` (new file)

Feature-gated: `#[cfg(feature = "mcp-briefing")]` (applied at module registration in `mod.rs`).

## Dependencies (imports)

```
use rmcp::model::{CallToolResult, Content}
use unimatrix_observe::{
    RetrospectiveReport, HotspotFinding, HotspotNarrative, EvidenceRecord,
    BaselineComparison, BaselineStatus, Severity,
    SessionSummary, FeatureKnowledgeReuse, AttributionMetadata, Recommendation,
}
use std::fmt::Write  // for write!() into String
use std::collections::HashMap
```

No new external crate dependencies.

---

## Internal Type: CollapsedFinding

```
struct CollapsedFinding {
    rule_name: String,
    severity: Severity,               // highest in group
    claims: Vec<String>,              // all claims from grouped findings
    total_events: f64,                // sum of measured across group
    tool_breakdown: Vec<(String, usize)>,  // tool name -> count, sorted descending by count
    examples: Vec<EvidenceRecord>,    // k=3 earliest by ts
    narrative_summary: Option<String>, // from matched narrative.summary (FR-09)
    cluster_count: Option<usize>,     // from matched narrative.clusters.len()
    sequence_pattern: Option<String>, // from matched narrative
}
```

---

## Public Function: format_retrospective_markdown

```
pub fn format_retrospective_markdown(
    report: &RetrospectiveReport,
) -> CallToolResult

BODY:
    let mut output = String::with_capacity(4096)

    // 1. Header (always rendered -- uses non-optional fields)
    output.push_str(&render_header(report))

    // 2. Sessions table (only if session_summaries is Some and non-empty)
    if let Some(summaries) = &report.session_summaries {
        if !summaries.is_empty() {
            output.push_str(&render_sessions(summaries))
        }
    }

    // 3. Attribution note (only if partial attribution)
    if let Some(attr) = &report.attribution {
        if attr.attributed_session_count < attr.total_session_count {
            output.push_str(&render_attribution_note(attr))
        }
    }

    // 4. Baseline outliers -- universal (only Outlier/NewSignal, omit section if none pass filter)
    if let Some(comparisons) = &report.baseline_comparison {
        let universal_outliers: Vec<&BaselineComparison> = comparisons.iter()
            .filter(|c| c.phase.is_none())
            .filter(|c| matches!(c.status, BaselineStatus::Outlier | BaselineStatus::NewSignal))
            .collect()
        if !universal_outliers.is_empty() {
            output.push_str(&render_baseline_outliers(&universal_outliers))
        }
    }

    // 5. Findings (group by rule_name, collapse, render)
    if !report.hotspots.is_empty() {
        output.push_str(&render_findings(&report.hotspots, report.narratives.as_deref()))
    }

    // 6. Phase outliers (only Outlier/NewSignal with phase != None, after zero-activity suppression)
    if let Some(comparisons) = &report.baseline_comparison {
        let phase_outliers: Vec<&BaselineComparison> = comparisons.iter()
            .filter(|c| c.phase.is_some())
            .filter(|c| matches!(c.status, BaselineStatus::Outlier | BaselineStatus::NewSignal))
            .filter(|c| !is_zero_activity_phase(c.phase.as_deref().unwrap(), &report.metrics.phases))
            .collect()
        if !phase_outliers.is_empty() {
            output.push_str(&render_phase_outliers(&phase_outliers))
        }
    }

    // 7. Knowledge reuse (only if present)
    if let Some(reuse) = &report.feature_knowledge_reuse {
        output.push_str(&render_knowledge_reuse(reuse))
    }

    // 8. Rework & context reload (FR-13: render if either is present and meaningful)
    //    Appended after knowledge reuse, or standalone if knowledge reuse is None
    let has_rework = report.rework_session_count.map_or(false, |n| n > 0)
    let has_reload = report.context_reload_pct.is_some()
    if has_rework || has_reload {
        output.push_str(&render_rework_reload(report.rework_session_count, report.context_reload_pct))
    }

    // 9. Recommendations (deduplicated by hotspot_type)
    if !report.recommendations.is_empty() {
        output.push_str(&render_recommendations(&report.recommendations))
    }

    CallToolResult::success(vec![Content::text(output)])
```

---

## Private Function: render_header

```
fn render_header(report: &RetrospectiveReport) -> String

BODY:
    let duration = format_duration(report.metrics.universal.total_duration_secs)
    format!(
        "# Retrospective: {}\n{} sessions | {} tool calls | {}\n\n",
        report.feature_cycle,
        report.session_count,
        report.total_records,
        duration,
    )
```

---

## Private Function: render_sessions

```
fn render_sessions(summaries: &[SessionSummary]) -> String

BODY:
    let mut out = String::new()
    out.push_str("## Sessions\n")
    out.push_str("| # | Window | Calls | Knowledge | Outcome |\n")
    out.push_str("|---|--------|-------|-----------|--------|\n")

    for (i, s) in summaries.iter().enumerate() {
        let idx = i + 1

        // Window: HH:MM (Xm) from started_at epoch millis and duration_secs
        // started_at is epoch millis, convert to HH:MM
        let start_secs = s.started_at / 1000
        let hours = (start_secs % 86400) / 3600
        let minutes = (start_secs % 3600) / 60
        let dur_min = s.duration_secs / 60
        let window = format!("{:02}:{:02} ({}m)", hours, minutes, dur_min)

        // Calls: sum of tool_distribution values
        let calls: u64 = s.tool_distribution.values().sum()

        // Knowledge: "N served, M stored"
        let knowledge = format!("{} served, {} stored", s.knowledge_served, s.knowledge_stored)

        // Outcome: from field or "-"
        let outcome = s.outcome.as_deref().unwrap_or("-")

        writeln!(out, "| {} | {} | {} | {} | {} |", idx, window, calls, knowledge, outcome)
    }

    out.push('\n')
    out
```

---

## Private Function: render_attribution_note

```
fn render_attribution_note(attr: &AttributionMetadata) -> String

BODY:
    format!(
        "> Note: {}/{} sessions attributed. Metrics may undercount.\n\n",
        attr.attributed_session_count,
        attr.total_session_count,
    )
```

---

## Private Function: render_baseline_outliers

```
fn render_baseline_outliers(outliers: &[&BaselineComparison]) -> String

BODY:
    let mut out = String::new()
    out.push_str("## Outliers\n")
    out.push_str("| Metric | Value | Mean | sigma |\n")
    out.push_str("|--------|-------|------|-------|\n")

    for c in outliers {
        // sigma = (current - mean) / stddev, or "new" if NewSignal
        let sigma_str = match c.status {
            BaselineStatus::NewSignal => "new".to_string(),
            _ => {
                if c.stddev > 0.0 {
                    format!("{:.1}", (c.current_value - c.mean) / c.stddev)
                } else {
                    "inf".to_string()  // NoVariance with outlier shouldn't happen, but handle
                }
            }
        }
        writeln!(out, "| {} | {:.1} | {:.1} | {} |",
            c.metric_name, c.current_value, c.mean, sigma_str)
    }

    out.push('\n')
    out
```

---

## Private Function: render_findings

```
fn render_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
) -> String

BODY:
    let collapsed = collapse_findings(hotspots, narratives)

    if collapsed.is_empty() {
        return String::new()  // omit section if no findings
    }

    let mut out = String::new()
    writeln!(out, "## Findings ({})", collapsed.len())

    for (i, finding) in collapsed.iter().enumerate() {
        let id = format!("F-{:02}", i + 1)
        let severity_tag = match finding.severity {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
        let total_events = finding.total_events.round() as u64

        // Heading line: use narrative summary as description when available (FR-09),
        // otherwise fall back to claims[0]
        let description = match &finding.narrative_summary {
            Some(summary) => summary.as_str(),
            None => finding.claims[0].as_str(),
        }
        writeln!(out, "### {} [{}] {} -- {} events",
            id, severity_tag, description, total_events)

        // Tool breakdown line
        if !finding.tool_breakdown.is_empty() {
            let breakdown: Vec<String> = finding.tool_breakdown.iter()
                .map(|(tool, count)| format!("{}({})", tool, count))
                .collect()
            writeln!(out, "{}", breakdown.join(", "))
        }

        // Narrative enrichment (cluster count, sequence pattern)
        if let Some(count) = finding.cluster_count {
            writeln!(out, "({} clusters)", count)
        }
        if let Some(pattern) = &finding.sequence_pattern {
            writeln!(out, "Escalation pattern: {}", pattern)
        }

        // k=3 examples
        if !finding.examples.is_empty() {
            out.push_str("Examples:\n")
            for ex in &finding.examples {
                writeln!(out, "- {} at ts={}", ex.description, ex.ts)
            }
        }

        out.push('\n')
    }

    out
```

---

## Private Function: collapse_findings

```
fn collapse_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
) -> Vec<CollapsedFinding>

BODY:
    // 1. Group findings by rule_name (preserve insertion order via Vec of seen names)
    let mut groups: HashMap<&str, Vec<&HotspotFinding>> = HashMap::new()
    let mut order: Vec<&str> = Vec::new()
    for h in hotspots {
        let key = h.rule_name.as_str()
        if !groups.contains_key(key) {
            order.push(key)
        }
        groups.entry(key).or_default().push(h)
    }

    // 2. Build narrative lookup: hotspot_type -> &HotspotNarrative
    let narrative_map: HashMap<&str, &HotspotNarrative> = match narratives {
        Some(narrs) => narrs.iter().map(|n| (n.hotspot_type.as_str(), n)).collect(),
        None => HashMap::new(),
    }

    // 3. Build CollapsedFinding for each group
    let mut collapsed: Vec<CollapsedFinding> = Vec::new()

    for rule_name in &order {
        let findings = &groups[rule_name]

        // Highest severity
        let severity = findings.iter()
            .map(|f| &f.severity)
            .max_by_key(|s| severity_rank(s))
            .cloned()
            .unwrap()  // group is non-empty by construction

        // Collect all claims
        let claims: Vec<String> = findings.iter().map(|f| f.claim.clone()).collect()

        // Sum measured values
        let total_events: f64 = findings.iter().map(|f| f.measured).sum()

        // Pool all evidence, sort by ts ascending, take first 3 (ADR-002)
        let mut evidence_pool: Vec<&EvidenceRecord> = findings.iter()
            .flat_map(|f| f.evidence.iter())
            .collect()
        evidence_pool.sort_by_key(|e| e.ts)
        let examples: Vec<EvidenceRecord> = evidence_pool.iter()
            .take(3)
            .map(|e| (*e).clone())
            .collect()

        // Build tool breakdown from ALL evidence (not just selected examples)
        let mut tool_counts: HashMap<String, usize> = HashMap::new()
        for ev in &evidence_pool {
            if let Some(tool) = &ev.tool {
                *tool_counts.entry(tool.clone()).or_insert(0) += 1
            }
        }
        let mut tool_breakdown: Vec<(String, usize)> = tool_counts.into_iter().collect()
        tool_breakdown.sort_by(|a, b| b.1.cmp(&a.1))  // descending by count

        // Narrative enrichment (FR-09: capture summary for description line)
        let narrative = narrative_map.get(rule_name.as_ref())
        let narrative_summary = narrative.map(|n| n.summary.clone())
        let cluster_count = narrative.map(|n| n.clusters.len())
        let sequence_pattern = narrative.and_then(|n| n.sequence_pattern.clone())

        collapsed.push(CollapsedFinding {
            rule_name: rule_name.to_string(),
            severity,
            claims,
            total_events,
            tool_breakdown,
            examples,
            narrative_summary,
            cluster_count,
            sequence_pattern,
        })
    }

    // 4. Sort collapsed findings: Critical first, then Warning, then Info.
    //    Within same severity: descending by total_events.
    collapsed.sort_by(|a, b| {
        severity_rank(&b.severity).cmp(&severity_rank(&a.severity))
            .then_with(|| b.total_events.partial_cmp(&a.total_events).unwrap_or(std::cmp::Ordering::Equal))
    })

    collapsed
```

---

## Private Function: severity_rank

```
fn severity_rank(s: &Severity) -> u8

BODY:
    match s {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Critical => 2,
    }
```

---

## Private Function: render_phase_outliers

```
fn render_phase_outliers(outliers: &[&BaselineComparison]) -> String

BODY:
    let mut out = String::new()
    out.push_str("## Phase Outliers\n")
    out.push_str("| Phase | Metric | Value | Mean | sigma |\n")
    out.push_str("|-------|--------|-------|------|-------|\n")

    for c in outliers {
        let phase = c.phase.as_deref().unwrap_or("unknown")
        let sigma_str = match c.status {
            BaselineStatus::NewSignal => "new".to_string(),
            _ => {
                if c.stddev > 0.0 {
                    format!("{:.1}", (c.current_value - c.mean) / c.stddev)
                } else {
                    "inf".to_string()
                }
            }
        }
        writeln!(out, "| {} | {} | {:.1} | {:.1} | {} |",
            phase, c.metric_name, c.current_value, c.mean, sigma_str)
    }

    out.push('\n')
    out
```

---

## Private Function: is_zero_activity_phase

```
fn is_zero_activity_phase(phase_name: &str, phases: &BTreeMap<String, PhaseMetrics>) -> bool

BODY:
    match phases.get(phase_name) {
        Some(pm) => pm.tool_call_count <= 1 && pm.duration_secs == 0,
        None => false,  // unknown phase, don't suppress
    }
```

Note: imports `std::collections::BTreeMap` and `unimatrix_store::PhaseMetrics`.

---

## Private Function: render_knowledge_reuse

```
fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse) -> String

BODY:
    let mut out = String::new()
    out.push_str("## Knowledge Reuse\n")

    let mut parts: Vec<String> = vec![
        format!("{} entries delivered", reuse.delivery_count),
        format!("{} cross-session", reuse.cross_session_count),
    ]

    if !reuse.category_gaps.is_empty() {
        parts.push(format!("Gaps: {}", reuse.category_gaps.join(", ")))
    }

    writeln!(out, "{}", parts.join(" | "))
    out.push('\n')
    out
```

---

## Private Function: render_rework_reload

```
fn render_rework_reload(
    rework: Option<u64>,
    reload_pct: Option<f64>,
) -> String

BODY:
    let mut parts: Vec<String> = Vec::new()

    if let Some(n) = rework {
        if n > 0 {
            parts.push(format!("{} rework sessions", n))
        }
    }

    if let Some(pct) = reload_pct {
        parts.push(format!("{:.0}% context reload", pct * 100.0))
    }

    if parts.is_empty() {
        return String::new()
    }

    format!("{}\n\n", parts.join(" | "))
```

Note: This renders as a line below Knowledge Reuse (if present) or standalone. The caller in `format_retrospective_markdown` places it after the knowledge reuse section.

---

## Private Function: render_recommendations

```
fn render_recommendations(recs: &[Recommendation]) -> String

BODY:
    // Deduplicate by hotspot_type, first occurrence wins
    let mut seen: Vec<&str> = Vec::new()
    let mut deduped: Vec<&Recommendation> = Vec::new()

    for r in recs {
        if !seen.contains(&r.hotspot_type.as_str()) {
            seen.push(&r.hotspot_type)
            deduped.push(r)
        }
    }

    if deduped.is_empty() {
        return String::new()
    }

    let mut out = String::new()
    out.push_str("## Recommendations\n")

    for r in &deduped {
        writeln!(out, "- {}", r.action)
    }

    out.push('\n')
    out
```

---

## Private Function: format_duration

```
fn format_duration(secs: u64) -> String

BODY:
    if secs == 0 {
        return "0m".to_string()
    }

    let hours = secs / 3600
    let minutes = (secs % 3600) / 60

    if hours > 0 && minutes > 0 {
        format!("{}h {}m", hours, minutes)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", minutes)
    }
```

---

## Key Test Scenarios

### Minimal report (all Optional fields None, empty Vecs) -- R-03
Build report with: empty hotspots, empty recommendations, all Optional fields None.
Assert: output contains `# Retrospective:` header, no other sections. Valid markdown.

### Finding collapse by rule_name -- R-01
Build 4 findings: 2x "retry_storms" (Info + Critical), 2x "sleep_workarounds" (Warning + Warning).
Assert: 2 collapsed findings. First is "retry_storms" with severity Critical. Second is "sleep_workarounds" with severity Warning. F-01 before F-02.

### Evidence selection k=3 deterministic -- R-05, ADR-002
Build collapsed group with 10 evidence records having distinct timestamps.
Assert: exactly 3 examples rendered, they are the 3 with lowest ts values.

### Evidence pool empty -- R-05
Build finding with 0 evidence records.
Assert: no "Examples:" line rendered. No panic.

### Narrative matching -- R-04, FR-09
Build report with narratives where `hotspot_type == rule_name`.
Assert: finding heading uses narrative `summary` as description (not claims[0]).
Assert: cluster count and sequence_pattern appear in finding output.

### Narrative mismatch -- R-04
Build report with narratives where `hotspot_type` does not match any finding's `rule_name`.
Assert: findings render using claims[0] as description (no narrative summary), no panic, no cluster info.

### Baseline outlier filtering -- R-07
Build baseline_comparison with mix of Normal, Outlier, NewSignal, NoVariance.
Assert: only Outlier and NewSignal appear in Outliers table. Normal and NoVariance excluded.

### All baselines Normal -- R-07
Build baseline_comparison with all Normal entries.
Assert: `## Outliers` section completely absent from output.

### Recommendation dedup -- R-08
Build 3 recommendations: two with same hotspot_type but different actions.
Assert: only first action for the duplicate type appears. Third (unique type) also appears.

### Session table -- R-06
Build 2 SessionSummary entries with known values.
Assert: `## Sessions` table has 2 data rows with correct column values.

### Session table omitted when None -- AC-12
Build report with session_summaries: None.
Assert: `## Sessions` not in output.

### Attribution note -- AC-13
Build report with attribution showing 3/5 sessions attributed.
Assert: blockquote note present with "3/5 sessions attributed".

### Attribution note omitted when fully attributed
Build report with attribution showing 5/5 sessions attributed.
Assert: no blockquote note.

### Knowledge reuse -- AC-14
Build report with FeatureKnowledgeReuse populated including category_gaps.
Assert: `## Knowledge Reuse` section with delivery count, cross-session count, and gaps.

### Rework and context reload -- FR-13
Build report with rework_session_count=2 and context_reload_pct=Some(0.345).
Assert: output contains "2 rework sessions" and "35% context reload".
Note: context_reload_pct is a fraction (0.0-1.0), multiplied by 100.0 for display.

### Zero-activity phase suppression -- R-09
Build phases map with a phase having tool_call_count=0, duration_secs=0.
Build phase-level baseline outlier for that phase.
Assert: the phase outlier row is absent from output.

### Duration formatting edge cases -- R-10
Assert: format_duration(0) -> "0m"
Assert: format_duration(3661) -> "1h 1m"
Assert: format_duration(3600) -> "1h"
Assert: format_duration(7200) -> "2h"
Assert: format_duration(45) -> "0m" (45 seconds rounds down to 0 minutes, but hours is also 0)

Note on duration edge: 45 seconds would render "0m" which is slightly misleading. If this is unacceptable, add a `< 60` case that renders "< 1m". Flag for implementer.

### F64 rendering -- R-12
Build finding with measured values that sum to a float with trailing digits (e.g., 3.1 + 2.2 = 5.300000000000001).
Assert: rendered total_events uses `.round() as u64` to avoid artifacts.
