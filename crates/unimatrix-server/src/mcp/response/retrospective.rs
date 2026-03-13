//! Markdown formatter for retrospective reports (vnc-011).
//!
//! Transforms a `RetrospectiveReport` into compact, scannable markdown
//! optimized for LLM consumption. All collapse, filtering, grouping,
//! deduplication, and rendering logic lives here.

use rmcp::model::{CallToolResult, Content};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use unimatrix_observe::{
    AttributionMetadata, BaselineComparison, BaselineStatus, EvidenceRecord, FeatureKnowledgeReuse,
    HotspotFinding, HotspotNarrative, Recommendation, RetrospectiveReport, SessionSummary,
    Severity,
};
use unimatrix_store::PhaseMetrics;

/// Formatter-internal type for collapsed findings grouped by rule_name.
#[derive(Debug)]
struct CollapsedFinding {
    #[allow(dead_code)]
    rule_name: String,
    severity: Severity,
    claims: Vec<String>,
    total_events: f64,
    tool_breakdown: Vec<(String, usize)>,
    examples: Vec<EvidenceRecord>,
    narrative_summary: Option<String>,
    cluster_count: Option<usize>,
    sequence_pattern: Option<String>,
}

/// Format a `RetrospectiveReport` as compact markdown in a `CallToolResult`.
pub fn format_retrospective_markdown(report: &RetrospectiveReport) -> CallToolResult {
    let mut output = String::with_capacity(4096);

    // 1. Header (always rendered -- uses non-optional fields)
    output.push_str(&render_header(report));

    // 2. Sessions table (only if session_summaries is Some and non-empty)
    if let Some(summaries) = &report.session_summaries
        && !summaries.is_empty()
    {
        output.push_str(&render_sessions(summaries));
    }

    // 3. Attribution note (only if partial attribution)
    if let Some(attr) = &report.attribution
        && attr.attributed_session_count < attr.total_session_count
    {
        output.push_str(&render_attribution_note(attr));
    }

    // 4. Baseline outliers -- universal (only Outlier/NewSignal, omit section if none pass)
    if let Some(comparisons) = &report.baseline_comparison {
        let universal_outliers: Vec<&BaselineComparison> = comparisons
            .iter()
            .filter(|c| c.phase.is_none())
            .filter(|c| {
                matches!(
                    c.status,
                    BaselineStatus::Outlier | BaselineStatus::NewSignal
                )
            })
            .collect();
        if !universal_outliers.is_empty() {
            output.push_str(&render_baseline_outliers(&universal_outliers));
        }
    }

    // 5. Findings (group by rule_name, collapse, render)
    if !report.hotspots.is_empty() {
        output.push_str(&render_findings(
            &report.hotspots,
            report.narratives.as_deref(),
        ));
    }

    // 6. Phase outliers (only Outlier/NewSignal with phase != None, zero-activity suppressed)
    if let Some(comparisons) = &report.baseline_comparison {
        let phase_outliers: Vec<&BaselineComparison> = comparisons
            .iter()
            .filter(|c| c.phase.is_some())
            .filter(|c| {
                matches!(
                    c.status,
                    BaselineStatus::Outlier | BaselineStatus::NewSignal
                )
            })
            .filter(|c| {
                !is_zero_activity_phase(c.phase.as_deref().unwrap_or(""), &report.metrics.phases)
            })
            .collect();
        if !phase_outliers.is_empty() {
            output.push_str(&render_phase_outliers(&phase_outliers));
        }
    }

    // 7. Knowledge reuse (only if present)
    if let Some(reuse) = &report.feature_knowledge_reuse {
        output.push_str(&render_knowledge_reuse(reuse));
    }

    // 8. Rework & context reload (FR-13)
    let has_rework = report.rework_session_count.is_some_and(|n| n > 0);
    let has_reload = report.context_reload_pct.is_some();
    if has_rework || has_reload {
        output.push_str(&render_rework_reload(
            report.rework_session_count,
            report.context_reload_pct,
        ));
    }

    // 9. Recommendations (deduplicated by hotspot_type)
    if !report.recommendations.is_empty() {
        output.push_str(&render_recommendations(&report.recommendations));
    }

    CallToolResult::success(vec![Content::text(output)])
}

fn render_header(report: &RetrospectiveReport) -> String {
    let duration = format_duration(report.metrics.universal.total_duration_secs);
    format!(
        "# Retrospective: {}\n{} sessions | {} tool calls | {}\n\n",
        report.feature_cycle, report.session_count, report.total_records, duration,
    )
}

fn render_sessions(summaries: &[SessionSummary]) -> String {
    let mut out = String::new();
    out.push_str("## Sessions\n");
    out.push_str("| # | Window | Calls | Knowledge | Outcome |\n");
    out.push_str("|---|--------|-------|-----------|--------|\n");

    for (i, s) in summaries.iter().enumerate() {
        let idx = i + 1;
        let start_secs = s.started_at / 1000;
        let hours = (start_secs % 86400) / 3600;
        let minutes = (start_secs % 3600) / 60;
        let dur_min = s.duration_secs / 60;
        let window = format!("{:02}:{:02} ({}m)", hours, minutes, dur_min);
        let calls: u64 = s.tool_distribution.values().sum();
        let knowledge = format!(
            "{} served, {} stored",
            s.knowledge_served, s.knowledge_stored
        );
        let outcome = s.outcome.as_deref().unwrap_or("-");
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            idx, window, calls, knowledge, outcome
        );
    }

    out.push('\n');
    out
}

fn render_attribution_note(attr: &AttributionMetadata) -> String {
    format!(
        "> Note: {}/{} sessions attributed. Metrics may undercount.\n\n",
        attr.attributed_session_count, attr.total_session_count,
    )
}

fn render_baseline_outliers(outliers: &[&BaselineComparison]) -> String {
    let mut out = String::new();
    out.push_str("## Outliers\n");
    out.push_str("| Metric | Value | Mean | sigma |\n");
    out.push_str("|--------|-------|------|-------|\n");

    for c in outliers {
        let sigma_str = sigma_string(c);
        let _ = writeln!(
            out,
            "| {} | {:.1} | {:.1} | {} |",
            c.metric_name, c.current_value, c.mean, sigma_str
        );
    }

    out.push('\n');
    out
}

fn render_findings(hotspots: &[HotspotFinding], narratives: Option<&[HotspotNarrative]>) -> String {
    let collapsed = collapse_findings(hotspots, narratives);

    if collapsed.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let _ = writeln!(out, "## Findings ({})", collapsed.len());

    for (i, finding) in collapsed.iter().enumerate() {
        let id = format!("F-{:02}", i + 1);
        let severity_tag = match finding.severity {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        let total_events = finding.total_events.round() as u64;

        let description = match &finding.narrative_summary {
            Some(summary) => summary.as_str(),
            None => finding.claims.first().map_or("", |c| c.as_str()),
        };
        let _ = writeln!(
            out,
            "### {} [{}] {} -- {} events",
            id, severity_tag, description, total_events
        );

        if !finding.tool_breakdown.is_empty() {
            let breakdown: Vec<String> = finding
                .tool_breakdown
                .iter()
                .map(|(tool, count)| format!("{}({})", tool, count))
                .collect();
            let _ = writeln!(out, "{}", breakdown.join(", "));
        }

        if let Some(count) = finding.cluster_count {
            let _ = writeln!(out, "({} clusters)", count);
        }
        if let Some(pattern) = &finding.sequence_pattern {
            let _ = writeln!(out, "Escalation pattern: {}", pattern);
        }

        if !finding.examples.is_empty() {
            out.push_str("Examples:\n");
            for ex in &finding.examples {
                let _ = writeln!(out, "- {} at ts={}", ex.description, ex.ts);
            }
        }

        out.push('\n');
    }

    out
}

fn collapse_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
) -> Vec<CollapsedFinding> {
    let mut groups: HashMap<&str, Vec<&HotspotFinding>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for h in hotspots {
        let key = h.rule_name.as_str();
        if !groups.contains_key(key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(h);
    }

    let narrative_map: HashMap<&str, &HotspotNarrative> = match narratives {
        Some(narrs) => narrs.iter().map(|n| (n.hotspot_type.as_str(), n)).collect(),
        None => HashMap::new(),
    };

    let mut collapsed: Vec<CollapsedFinding> = Vec::new();

    for rule_name in &order {
        let findings = &groups[rule_name];

        let severity = findings
            .iter()
            .map(|f| &f.severity)
            .max_by_key(|s| severity_rank(s))
            .cloned()
            .unwrap_or(Severity::Info);

        let claims: Vec<String> = findings.iter().map(|f| f.claim.clone()).collect();
        let total_events: f64 = findings.iter().map(|f| f.measured).sum();

        let mut evidence_pool: Vec<&EvidenceRecord> =
            findings.iter().flat_map(|f| f.evidence.iter()).collect();
        evidence_pool.sort_by_key(|e| e.ts);
        let examples: Vec<EvidenceRecord> =
            evidence_pool.iter().take(3).map(|e| (*e).clone()).collect();

        let mut tool_counts: HashMap<String, usize> = HashMap::new();
        for ev in &evidence_pool {
            if let Some(tool) = &ev.tool {
                *tool_counts.entry(tool.clone()).or_insert(0) += 1;
            }
        }
        let mut tool_breakdown: Vec<(String, usize)> = tool_counts.into_iter().collect();
        tool_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

        let narrative = narrative_map.get(rule_name);
        let narrative_summary = narrative.map(|n| n.summary.clone());
        let cluster_count = narrative.map(|n| n.clusters.len());
        let sequence_pattern = narrative.and_then(|n| n.sequence_pattern.clone());

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
        });
    }

    collapsed.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then_with(|| {
                b.total_events
                    .partial_cmp(&a.total_events)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    collapsed
}

fn severity_rank(s: &Severity) -> u8 {
    match s {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Critical => 2,
    }
}

fn render_phase_outliers(outliers: &[&BaselineComparison]) -> String {
    let mut out = String::new();
    out.push_str("## Phase Outliers\n");
    out.push_str("| Phase | Metric | Value | Mean | sigma |\n");
    out.push_str("|-------|--------|-------|------|-------|\n");

    for c in outliers {
        let phase = c.phase.as_deref().unwrap_or("unknown");
        let sigma_str = sigma_string(c);
        let _ = writeln!(
            out,
            "| {} | {} | {:.1} | {:.1} | {} |",
            phase, c.metric_name, c.current_value, c.mean, sigma_str
        );
    }

    out.push('\n');
    out
}

fn is_zero_activity_phase(phase_name: &str, phases: &BTreeMap<String, PhaseMetrics>) -> bool {
    match phases.get(phase_name) {
        Some(pm) => pm.tool_call_count <= 1 && pm.duration_secs == 0,
        None => false,
    }
}

fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse) -> String {
    let mut out = String::new();
    out.push_str("## Knowledge Reuse\n");

    let mut parts: Vec<String> = vec![
        format!("{} entries delivered", reuse.delivery_count),
        format!("{} cross-session", reuse.cross_session_count),
    ];

    if !reuse.category_gaps.is_empty() {
        parts.push(format!("Gaps: {}", reuse.category_gaps.join(", ")));
    }

    let _ = writeln!(out, "{}", parts.join(" | "));
    out.push('\n');
    out
}

fn render_rework_reload(rework: Option<u64>, reload_pct: Option<f64>) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(n) = rework
        && n > 0
    {
        parts.push(format!("{} rework sessions", n));
    }

    if let Some(pct) = reload_pct {
        parts.push(format!("{:.0}% context reload", pct * 100.0));
    }

    if parts.is_empty() {
        return String::new();
    }

    format!("{}\n\n", parts.join(" | "))
}

fn render_recommendations(recs: &[Recommendation]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut deduped: Vec<&Recommendation> = Vec::new();

    for r in recs {
        if !seen.contains(&r.hotspot_type.as_str()) {
            seen.push(&r.hotspot_type);
            deduped.push(r);
        }
    }

    if deduped.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("## Recommendations\n");

    for r in &deduped {
        let _ = writeln!(out, "- {}", r.action);
    }

    out.push('\n');
    out
}

fn format_duration(secs: u64) -> String {
    if secs == 0 {
        return "0m".to_string();
    }

    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;

    if hours > 0 && minutes > 0 {
        format!("{}h {}m", hours, minutes)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", minutes)
    }
}

/// Compute sigma string for a baseline comparison.
fn sigma_string(c: &BaselineComparison) -> String {
    match c.status {
        BaselineStatus::NewSignal => "new".to_string(),
        _ => {
            if c.stddev > 0.0 {
                format!("{:.1}", (c.current_value - c.mean) / c.stddev)
            } else {
                "inf".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ERROR_INVALID_PARAMS;
    use crate::mcp::response::format_retrospective_report;
    use unimatrix_observe::{EvidenceCluster, HotspotCategory};
    use unimatrix_store::MetricVector;

    // ── Test Helpers ─────────────────────────────────────────────────

    fn make_report() -> RetrospectiveReport {
        RetrospectiveReport {
            feature_cycle: "test-001".to_string(),
            session_count: 1,
            total_records: 10,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
        }
    }

    fn make_finding(
        rule_name: &str,
        severity: Severity,
        measured: f64,
        evidence_count: usize,
    ) -> HotspotFinding {
        let evidence: Vec<EvidenceRecord> = (0..evidence_count)
            .map(|i| EvidenceRecord {
                description: format!("evidence-{}", i),
                ts: 1000 + i as u64,
                tool: Some(format!("tool-{}", i % 3)),
                detail: format!("detail-{}", i),
            })
            .collect();
        HotspotFinding {
            category: HotspotCategory::Friction,
            severity,
            rule_name: rule_name.to_string(),
            claim: format!("claim for {}", rule_name),
            measured,
            threshold: 0.0,
            evidence,
        }
    }

    fn make_session(
        id: &str,
        started_at: u64,
        duration_secs: u64,
        calls: &[(&str, u64)],
        served: u64,
        stored: u64,
        outcome: Option<&str>,
    ) -> SessionSummary {
        let mut tool_distribution = HashMap::new();
        for (k, v) in calls {
            tool_distribution.insert(k.to_string(), *v);
        }
        SessionSummary {
            session_id: id.to_string(),
            started_at,
            duration_secs,
            tool_distribution,
            top_file_zones: vec![],
            agents_spawned: vec![],
            knowledge_served: served,
            knowledge_stored: stored,
            knowledge_curated: 0,
            outcome: outcome.map(|s| s.to_string()),
        }
    }

    fn extract_text(result: &CallToolResult) -> String {
        result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }

    fn make_report_with_evidence(evidence_count: usize) -> RetrospectiveReport {
        let evidence: Vec<EvidenceRecord> = (0..evidence_count)
            .map(|i| EvidenceRecord {
                description: format!("evidence-{i}"),
                ts: 1000 + i as u64,
                tool: Some("context_search".to_string()),
                detail: format!("detail-{i}"),
            })
            .collect();
        let mut report = make_report();
        report.session_count = 2;
        report.total_records = 50;
        report.hotspots = vec![HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "test_rule".to_string(),
            claim: "test claim".to_string(),
            measured: 5.0,
            threshold: 3.0,
            evidence,
        }];
        report
    }

    fn dispatch_format(
        format: Option<String>,
        evidence_limit: Option<usize>,
        report: &RetrospectiveReport,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        let fmt = format.as_deref().unwrap_or("markdown");
        match fmt {
            "markdown" | "summary" => Ok(format_retrospective_markdown(report)),
            "json" => {
                let limit = evidence_limit.unwrap_or(3);
                if limit > 0 {
                    let mut truncated = report.clone();
                    for hotspot in &mut truncated.hotspots {
                        hotspot.evidence.truncate(limit);
                    }
                    Ok(format_retrospective_report(&truncated))
                } else {
                    Ok(format_retrospective_report(report))
                }
            }
            _ => Err(rmcp::model::ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!(
                    "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
                    fmt
                ),
                None,
            )),
        }
    }

    // ── format_duration ──────────────────────────────────────────────

    #[test]
    fn test_duration_zero() {
        assert_eq!(format_duration(0), "0m");
    }

    #[test]
    fn test_duration_minutes_only() {
        assert_eq!(format_duration(3420), "57m");
    }

    #[test]
    fn test_duration_hours_and_minutes() {
        assert_eq!(format_duration(6840), "1h 54m");
    }

    #[test]
    fn test_duration_over_24h() {
        assert_eq!(format_duration(90000), "25h");
    }

    #[test]
    fn test_duration_exact_hour() {
        assert_eq!(format_duration(3600), "1h");
    }

    // ── render_header ────────────────────────────────────────────────

    #[test]
    fn test_header_contains_feature_cycle() {
        let mut report = make_report();
        report.feature_cycle = "nxs-010".to_string();
        let header = render_header(&report);
        assert!(header.contains("# Retrospective: nxs-010"));
    }

    #[test]
    fn test_header_contains_session_count() {
        let mut report = make_report();
        report.session_count = 5;
        let header = render_header(&report);
        assert!(header.contains("5 sessions"));
    }

    #[test]
    fn test_header_contains_total_records() {
        let mut report = make_report();
        report.total_records = 312;
        let header = render_header(&report);
        assert!(header.contains("312 tool calls"));
    }

    #[test]
    fn test_header_contains_duration() {
        let mut report = make_report();
        report.metrics.universal.total_duration_secs = 6840;
        let header = render_header(&report);
        assert!(header.contains("1h 54m"));
    }

    // ── format_retrospective_markdown (top-level) ────────────────────

    #[test]
    fn test_markdown_output_starts_with_header() {
        let report = make_report();
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(text.starts_with("# Retrospective: test-001"));
    }

    #[test]
    fn test_markdown_output_is_call_tool_result() {
        let report = make_report();
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(!text.is_empty());
    }

    #[test]
    fn test_all_none_optional_fields_valid_markdown() {
        let report = make_report();
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(text.contains("# Retrospective:"));
        assert!(!text.contains("## Sessions"));
        assert!(!text.contains("## Outliers"));
        assert!(!text.contains("## Findings"));
        assert!(!text.contains("## Phase Outliers"));
        assert!(!text.contains("## Knowledge Reuse"));
        assert!(!text.contains("## Recommendations"));
        assert!(!text.contains("> Note:"));
    }

    #[test]
    fn test_single_optional_session_summaries() {
        let mut report = make_report();
        report.session_summaries = Some(vec![make_session(
            "s1",
            35520000,
            3420,
            &[("read", 100), ("write", 50)],
            5,
            2,
            Some("success"),
        )]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Sessions"));
        assert!(!text.contains("## Outliers"));
    }

    #[test]
    fn test_single_optional_baseline_comparison() {
        let mut report = make_report();
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "test_metric".to_string(),
            current_value: 10.0,
            mean: 5.0,
            stddev: 2.0,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Outliers"));
    }

    #[test]
    fn test_single_optional_feature_knowledge_reuse() {
        let mut report = make_report();
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 10,
            cross_session_count: 3,
            by_category: HashMap::new(),
            category_gaps: vec![],
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Knowledge Reuse"));
    }

    #[test]
    fn test_single_optional_attribution() {
        let mut report = make_report();
        report.attribution = Some(AttributionMetadata {
            attributed_session_count: 3,
            total_session_count: 5,
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("> Note: 3/5 sessions attributed"));
    }

    #[test]
    fn test_single_optional_rework() {
        let mut report = make_report();
        report.rework_session_count = Some(2);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("2 rework sessions"));
    }

    #[test]
    fn test_single_optional_reload() {
        let mut report = make_report();
        report.context_reload_pct = Some(0.35);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("35% context reload"));
    }

    #[test]
    fn test_full_report_all_sections() {
        let mut report = make_report();
        report.session_summaries = Some(vec![make_session(
            "s1",
            35520000,
            3420,
            &[("read", 100)],
            5,
            2,
            Some("success"),
        )]);
        report.attribution = Some(AttributionMetadata {
            attributed_session_count: 1,
            total_session_count: 2,
        });
        report.baseline_comparison = Some(vec![
            BaselineComparison {
                metric_name: "universal_metric".to_string(),
                current_value: 10.0,
                mean: 5.0,
                stddev: 2.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: None,
            },
            BaselineComparison {
                metric_name: "phase_metric".to_string(),
                current_value: 20.0,
                mean: 8.0,
                stddev: 3.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: Some("design".to_string()),
            },
        ]);
        report.hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 2)];
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 15,
            cross_session_count: 8,
            by_category: HashMap::new(),
            category_gaps: vec!["procedure".to_string()],
        });
        report.rework_session_count = Some(1);
        report.context_reload_pct = Some(0.25);
        report.recommendations = vec![Recommendation {
            hotspot_type: "retry_storms".to_string(),
            action: "Add retry backoff".to_string(),
            rationale: "Reduces churn".to_string(),
        }];

        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Sessions"));
        assert!(text.contains("## Outliers"));
        assert!(text.contains("## Findings"));
        assert!(text.contains("## Phase Outliers"));
        assert!(text.contains("## Knowledge Reuse"));
        assert!(text.contains("## Recommendations"));
        assert!(text.contains("> Note:"));
    }

    // ── render_sessions ──────────────────────────────────────────────

    #[test]
    fn test_session_table_two_rows() {
        let summaries = vec![
            make_session(
                "s1",
                35520000,
                3420,
                &[("read", 200), ("write", 112)],
                5,
                2,
                Some("success"),
            ),
            make_session("s2", 39120000, 1800, &[("read", 50)], 3, 1, None),
        ];
        let out = render_sessions(&summaries);
        assert!(out.contains("## Sessions"));
        assert!(out.contains("| 1 |"));
        assert!(out.contains("| 2 |"));
    }

    #[test]
    fn test_session_empty_tool_dist() {
        let summaries = vec![make_session("s1", 35520000, 3420, &[], 0, 0, None)];
        let out = render_sessions(&summaries);
        assert!(out.contains("| 0 |"));
    }

    #[test]
    fn test_session_zero_duration() {
        let summaries = vec![make_session("s1", 35520000, 0, &[("read", 5)], 1, 0, None)];
        let out = render_sessions(&summaries);
        assert!(out.contains("(0m)"));
    }

    #[test]
    fn test_session_with_outcome() {
        let summaries = vec![make_session("s1", 35520000, 60, &[], 0, 0, Some("success"))];
        let out = render_sessions(&summaries);
        assert!(out.contains("success"));
    }

    #[test]
    fn test_session_no_outcome() {
        let summaries = vec![make_session("s1", 35520000, 60, &[], 0, 0, None)];
        let out = render_sessions(&summaries);
        assert!(out.contains("| - |"));
    }

    // ── render_attribution_note ──────────────────────────────────────

    #[test]
    fn test_attribution_partial() {
        let attr = AttributionMetadata {
            attributed_session_count: 3,
            total_session_count: 5,
        };
        let out = render_attribution_note(&attr);
        assert!(out.contains("> Note: 3/5 sessions attributed"));
    }

    #[test]
    fn test_attribution_full_not_rendered() {
        let mut report = make_report();
        report.attribution = Some(AttributionMetadata {
            attributed_session_count: 5,
            total_session_count: 5,
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("> Note:"));
    }

    // ── render_baseline_outliers ─────────────────────────────────────

    #[test]
    fn test_baseline_all_normal_omits() {
        let mut report = make_report();
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "m1".to_string(),
            current_value: 5.0,
            mean: 5.0,
            stddev: 1.0,
            is_outlier: false,
            status: BaselineStatus::Normal,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## Outliers"));
    }

    #[test]
    fn test_baseline_mixed_statuses() {
        let comparisons = vec![
            BaselineComparison {
                metric_name: "normal_m".to_string(),
                current_value: 5.0,
                mean: 5.0,
                stddev: 1.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            },
            BaselineComparison {
                metric_name: "outlier_m".to_string(),
                current_value: 10.0,
                mean: 5.0,
                stddev: 2.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: None,
            },
            BaselineComparison {
                metric_name: "new_m".to_string(),
                current_value: 3.0,
                mean: 0.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::NewSignal,
                phase: None,
            },
            BaselineComparison {
                metric_name: "novar_m".to_string(),
                current_value: 5.0,
                mean: 5.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::NoVariance,
                phase: None,
            },
        ];
        let filtered: Vec<&BaselineComparison> = comparisons
            .iter()
            .filter(|c| c.phase.is_none())
            .filter(|c| {
                matches!(
                    c.status,
                    BaselineStatus::Outlier | BaselineStatus::NewSignal
                )
            })
            .collect();
        let out = render_baseline_outliers(&filtered);
        assert!(out.contains("outlier_m"));
        assert!(out.contains("new_m"));
        assert!(!out.contains("normal_m"));
        assert!(!out.contains("novar_m"));
    }

    #[test]
    fn test_baseline_empty_vec() {
        let empty: Vec<&BaselineComparison> = vec![];
        let out = render_baseline_outliers(&empty);
        assert!(out.contains("## Outliers"));
    }

    #[test]
    fn test_baseline_single_outlier() {
        let c = BaselineComparison {
            metric_name: "tool_calls".to_string(),
            current_value: 500.0,
            mean: 200.0,
            stddev: 50.0,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        };
        let out = render_baseline_outliers(&[&c]);
        assert!(out.contains("## Outliers"));
        assert!(out.contains("tool_calls"));
        assert!(out.contains("500.0"));
    }

    #[test]
    fn test_baseline_new_signal_included() {
        let c = BaselineComparison {
            metric_name: "new_metric".to_string(),
            current_value: 3.0,
            mean: 0.0,
            stddev: 0.0,
            is_outlier: false,
            status: BaselineStatus::NewSignal,
            phase: None,
        };
        let out = render_baseline_outliers(&[&c]);
        assert!(out.contains("new_metric"));
        assert!(out.contains("new"));
    }

    // ── collapse_findings ────────────────────────────────────────────

    #[test]
    fn test_collapse_groups_by_rule_name() {
        let hotspots = vec![
            make_finding("retry_storms", Severity::Info, 2.0, 1),
            make_finding("retry_storms", Severity::Critical, 3.0, 1),
            make_finding("sleep_workarounds", Severity::Warning, 4.0, 1),
            make_finding("sleep_workarounds", Severity::Warning, 1.0, 1),
        ];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed.len(), 2);
    }

    #[test]
    fn test_collapse_mixed_severity_picks_highest() {
        let hotspots = vec![
            make_finding("retry_storms", Severity::Info, 1.0, 0),
            make_finding("retry_storms", Severity::Warning, 1.0, 0),
            make_finding("retry_storms", Severity::Critical, 1.0, 0),
        ];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].severity, Severity::Critical);
    }

    #[test]
    fn test_collapse_same_severity() {
        let hotspots = vec![
            make_finding("rule_a", Severity::Warning, 1.0, 0),
            make_finding("rule_a", Severity::Warning, 2.0, 0),
        ];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed[0].severity, Severity::Warning);
    }

    #[test]
    fn test_collapse_total_events_summed() {
        let hotspots = vec![
            make_finding("rule_a", Severity::Info, 5.0, 0),
            make_finding("rule_a", Severity::Info, 3.0, 0),
            make_finding("rule_a", Severity::Info, 2.0, 0),
        ];
        let collapsed = collapse_findings(&hotspots, None);
        assert!((collapsed[0].total_events - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_collapse_tool_breakdown() {
        let mut f1 = make_finding("rule_a", Severity::Info, 1.0, 0);
        f1.evidence = vec![
            EvidenceRecord {
                description: "e1".to_string(),
                ts: 100,
                tool: Some("Bash".to_string()),
                detail: String::new(),
            },
            EvidenceRecord {
                description: "e2".to_string(),
                ts: 200,
                tool: Some("Bash".to_string()),
                detail: String::new(),
            },
            EvidenceRecord {
                description: "e3".to_string(),
                ts: 300,
                tool: Some("Read".to_string()),
                detail: String::new(),
            },
        ];
        let mut f2 = make_finding("rule_a", Severity::Info, 1.0, 0);
        f2.evidence = vec![EvidenceRecord {
            description: "e4".to_string(),
            ts: 400,
            tool: Some("Bash".to_string()),
            detail: String::new(),
        }];

        let collapsed = collapse_findings(&[f1, f2], None);
        assert_eq!(collapsed.len(), 1);
        let bd = &collapsed[0].tool_breakdown;
        let bash_count = bd.iter().find(|(t, _)| t == "Bash").map(|(_, c)| *c);
        let read_count = bd.iter().find(|(t, _)| t == "Read").map(|(_, c)| *c);
        assert_eq!(bash_count, Some(3));
        assert_eq!(read_count, Some(1));
    }

    #[test]
    fn test_collapse_evidence_pool_combined() {
        let hotspots = vec![
            make_finding("rule_a", Severity::Info, 1.0, 2),
            make_finding("rule_a", Severity::Info, 1.0, 2),
            make_finding("rule_a", Severity::Info, 1.0, 2),
        ];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed[0].examples.len(), 3);
    }

    #[test]
    fn test_collapse_narrative_summary_populated() {
        let hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 1)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "retry_storms".to_string(),
            summary: "High tool churn".to_string(),
            clusters: vec![EvidenceCluster {
                window_start: 100,
                event_count: 3,
                description: "cluster".to_string(),
            }],
            top_files: vec![],
            sequence_pattern: None,
        }];
        let collapsed = collapse_findings(&hotspots, Some(&narratives));
        assert_eq!(
            collapsed[0].narrative_summary,
            Some("High tool churn".to_string())
        );
    }

    #[test]
    fn test_collapse_narrative_summary_none_when_no_match() {
        let hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 1)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "unrelated_type".to_string(),
            summary: "Unrelated".to_string(),
            clusters: vec![],
            top_files: vec![],
            sequence_pattern: None,
        }];
        let collapsed = collapse_findings(&hotspots, Some(&narratives));
        assert!(collapsed[0].narrative_summary.is_none());
    }

    // ── Evidence selection (k=3, earliest by timestamp) ──────────────

    #[test]
    fn test_evidence_empty_pool() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, 1.0, 0)];
        let collapsed = collapse_findings(&hotspots, None);
        assert!(collapsed[0].examples.is_empty());
    }

    #[test]
    fn test_evidence_one_record() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, 1.0, 1)];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed[0].examples.len(), 1);
    }

    #[test]
    fn test_evidence_three_records() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, 1.0, 3)];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed[0].examples.len(), 3);
    }

    #[test]
    fn test_evidence_ten_records_earliest_three() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, 1.0, 10)];
        let collapsed = collapse_findings(&hotspots, None);
        assert_eq!(collapsed[0].examples.len(), 3);
        assert_eq!(collapsed[0].examples[0].ts, 1000);
        assert_eq!(collapsed[0].examples[1].ts, 1001);
        assert_eq!(collapsed[0].examples[2].ts, 1002);
    }

    #[test]
    fn test_evidence_same_timestamp() {
        let mut f = make_finding("rule_a", Severity::Info, 1.0, 0);
        f.evidence = (0..5)
            .map(|i| EvidenceRecord {
                description: format!("ev-{}", i),
                ts: 100,
                tool: None,
                detail: String::new(),
            })
            .collect();
        let collapsed = collapse_findings(&[f], None);
        assert_eq!(collapsed[0].examples.len(), 3);
    }

    // ── render_findings ──────────────────────────────────────────────

    #[test]
    fn test_findings_empty() {
        let out = render_findings(&[], None);
        assert!(out.is_empty());
    }

    #[test]
    fn test_findings_ordering() {
        let hotspots = vec![
            make_finding("info_rule", Severity::Info, 1.0, 0),
            make_finding("warning_rule", Severity::Warning, 1.0, 0),
            make_finding("critical_rule", Severity::Critical, 1.0, 0),
        ];
        let out = render_findings(&hotspots, None);
        let f01_pos = out.find("F-01").expect("F-01 present");
        let f02_pos = out.find("F-02").expect("F-02 present");
        let f03_pos = out.find("F-03").expect("F-03 present");
        assert!(f01_pos < f02_pos);
        assert!(f02_pos < f03_pos);
        assert!(out.contains("F-01 [critical]"));
        assert!(out.contains("F-02 [warning]"));
        assert!(out.contains("F-03 [info]"));
    }

    #[test]
    fn test_findings_with_narrative_match() {
        let hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 2)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "retry_storms".to_string(),
            summary: "Tool overuse detected".to_string(),
            clusters: vec![
                EvidenceCluster {
                    window_start: 100,
                    event_count: 3,
                    description: "c1".to_string(),
                },
                EvidenceCluster {
                    window_start: 200,
                    event_count: 2,
                    description: "c2".to_string(),
                },
            ],
            top_files: vec![],
            sequence_pattern: Some("30s->60s".to_string()),
        }];
        let out = render_findings(&hotspots, Some(&narratives));
        assert!(out.contains("(2 clusters)"));
        assert!(out.contains("Tool overuse detected"));
        assert!(out.contains("Escalation pattern: 30s->60s"));
    }

    #[test]
    fn test_findings_narrative_summary_replaces_claim() {
        let hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 0)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "retry_storms".to_string(),
            summary: "Tool overuse detected".to_string(),
            clusters: vec![],
            top_files: vec![],
            sequence_pattern: None,
        }];
        let out = render_findings(&hotspots, Some(&narratives));
        let heading_line = out.lines().find(|l| l.contains("F-01")).unwrap_or("");
        assert!(heading_line.contains("Tool overuse detected"));
        assert!(!heading_line.contains("claim for retry_storms"));
    }

    #[test]
    fn test_findings_narrative_no_match() {
        let hotspots = vec![make_finding("retry_storms", Severity::Warning, 5.0, 0)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "unrelated".to_string(),
            summary: "Unrelated".to_string(),
            clusters: vec![],
            top_files: vec![],
            sequence_pattern: None,
        }];
        let out = render_findings(&hotspots, Some(&narratives));
        assert!(out.contains("claim for retry_storms"));
        assert!(!out.contains("Unrelated"));
    }

    #[test]
    fn test_findings_sequence_pattern() {
        let hotspots = vec![make_finding("sleep_workarounds", Severity::Warning, 3.0, 0)];
        let narratives = vec![HotspotNarrative {
            hotspot_type: "sleep_workarounds".to_string(),
            summary: "Sleep escalation".to_string(),
            clusters: vec![],
            top_files: vec![],
            sequence_pattern: Some("30s->60s".to_string()),
        }];
        let out = render_findings(&hotspots, Some(&narratives));
        assert!(out.contains("Escalation pattern: 30s->60s"));
    }

    #[test]
    fn test_findings_single_finding_no_collapse() {
        let hotspots = vec![make_finding("single_rule", Severity::Info, 7.0, 1)];
        let out = render_findings(&hotspots, None);
        assert!(out.contains("F-01"));
        assert!(out.contains("7 events"));
    }

    // ── render_phase_outliers ────────────────────────────────────────

    #[test]
    fn test_phase_outliers_filters() {
        let comparisons = vec![
            BaselineComparison {
                metric_name: "outlier_m".to_string(),
                current_value: 20.0,
                mean: 8.0,
                stddev: 3.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: Some("design".to_string()),
            },
            BaselineComparison {
                metric_name: "normal_m".to_string(),
                current_value: 5.0,
                mean: 5.0,
                stddev: 1.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: Some("design".to_string()),
            },
        ];
        let filtered: Vec<&BaselineComparison> = comparisons
            .iter()
            .filter(|c| c.phase.is_some())
            .filter(|c| {
                matches!(
                    c.status,
                    BaselineStatus::Outlier | BaselineStatus::NewSignal
                )
            })
            .collect();
        let out = render_phase_outliers(&filtered);
        assert!(out.contains("outlier_m"));
        assert!(!out.contains("normal_m"));
    }

    #[test]
    fn test_phase_outlier_zero_activity_suppressed() {
        let mut report = make_report();
        let mut phases = BTreeMap::new();
        phases.insert(
            "empty_phase".to_string(),
            PhaseMetrics {
                duration_secs: 0,
                tool_call_count: 0,
            },
        );
        phases.insert(
            "active_phase".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 50,
            },
        );
        report.metrics.phases = phases;
        report.baseline_comparison = Some(vec![
            BaselineComparison {
                metric_name: "m1".to_string(),
                current_value: 10.0,
                mean: 2.0,
                stddev: 1.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: Some("empty_phase".to_string()),
            },
            BaselineComparison {
                metric_name: "m2".to_string(),
                current_value: 20.0,
                mean: 5.0,
                stddev: 2.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: Some("active_phase".to_string()),
            },
        ]);

        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("empty_phase"));
        assert!(text.contains("active_phase"));
    }

    // ── render_knowledge_reuse ───────────────────────────────────────

    #[test]
    fn test_knowledge_reuse_full() {
        let reuse = FeatureKnowledgeReuse {
            delivery_count: 15,
            cross_session_count: 8,
            by_category: HashMap::new(),
            category_gaps: vec!["procedure".to_string()],
        };
        let out = render_knowledge_reuse(&reuse);
        assert!(out.contains("15 entries delivered"));
        assert!(out.contains("8 cross-session"));
        assert!(out.contains("Gaps: procedure"));
    }

    #[test]
    fn test_knowledge_reuse_no_gaps() {
        let reuse = FeatureKnowledgeReuse {
            delivery_count: 5,
            cross_session_count: 2,
            by_category: HashMap::new(),
            category_gaps: vec![],
        };
        let out = render_knowledge_reuse(&reuse);
        assert!(!out.contains("Gaps:"));
    }

    // ── render_rework_reload ─────────────────────────────────────────

    #[test]
    fn test_rework_present() {
        let out = render_rework_reload(Some(2), None);
        assert!(out.contains("2 rework sessions"));
    }

    #[test]
    fn test_rework_zero() {
        let out = render_rework_reload(Some(0), None);
        assert!(out.is_empty());
    }

    #[test]
    fn test_reload_present() {
        let out = render_rework_reload(None, Some(0.35));
        assert!(out.contains("35% context reload"));
    }

    #[test]
    fn test_both_present() {
        let out = render_rework_reload(Some(2), Some(0.35));
        assert!(out.contains("2 rework sessions"));
        assert!(out.contains("35% context reload"));
    }

    #[test]
    fn test_both_none() {
        let out = render_rework_reload(None, None);
        assert!(out.is_empty());
    }

    // ── render_recommendations ───────────────────────────────────────

    #[test]
    fn test_recommendations_dedup() {
        let recs = vec![
            Recommendation {
                hotspot_type: "retry_storms".to_string(),
                action: "Add backoff".to_string(),
                rationale: "r1".to_string(),
            },
            Recommendation {
                hotspot_type: "retry_storms".to_string(),
                action: "Different action".to_string(),
                rationale: "r2".to_string(),
            },
            Recommendation {
                hotspot_type: "sleep_workarounds".to_string(),
                action: "Fix sleeps".to_string(),
                rationale: "r3".to_string(),
            },
        ];
        let out = render_recommendations(&recs);
        assert!(out.contains("Add backoff"));
        assert!(!out.contains("Different action"));
        assert!(out.contains("Fix sleeps"));
    }

    #[test]
    fn test_recommendations_distinct() {
        let recs = vec![
            Recommendation {
                hotspot_type: "a".to_string(),
                action: "Action A".to_string(),
                rationale: "r".to_string(),
            },
            Recommendation {
                hotspot_type: "b".to_string(),
                action: "Action B".to_string(),
                rationale: "r".to_string(),
            },
            Recommendation {
                hotspot_type: "c".to_string(),
                action: "Action C".to_string(),
                rationale: "r".to_string(),
            },
        ];
        let out = render_recommendations(&recs);
        assert!(out.contains("Action A"));
        assert!(out.contains("Action B"));
        assert!(out.contains("Action C"));
    }

    #[test]
    fn test_recommendations_empty() {
        let out = render_recommendations(&[]);
        assert!(out.is_empty());
    }

    // ── Edge Cases ───────────────────────────────────────────────────

    #[test]
    fn test_unicode_in_claim() {
        let mut f = make_finding("unicode_rule", Severity::Info, 1.0, 0);
        f.claim = "Detected \u{1F525} in agent output".to_string();
        let out = render_findings(&[f], None);
        assert!(out.contains("\u{1F525}"));
    }

    #[test]
    fn test_float_sum_formatting() {
        let hotspots = vec![
            make_finding("rule_a", Severity::Info, 0.1, 0),
            make_finding("rule_a", Severity::Info, 0.2, 0),
            make_finding("rule_a", Severity::Info, 0.3, 0),
        ];
        let out = render_findings(&hotspots, None);
        assert!(!out.contains("0.600000000000000"));
    }

    #[test]
    fn test_nan_measured() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, f64::NAN, 0)];
        let out = render_findings(&hotspots, None);
        assert!(out.contains("F-01"));
    }

    #[test]
    fn test_pipe_in_metric_name() {
        let c = BaselineComparison {
            metric_name: "metric|with|pipes".to_string(),
            current_value: 10.0,
            mean: 5.0,
            stddev: 2.0,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        };
        let out = render_baseline_outliers(&[&c]);
        assert!(out.contains("metric|with|pipes"));
    }

    #[test]
    fn test_large_report_performance() {
        use std::time::Instant;

        let mut report = make_report();
        report.hotspots = (0..50)
            .map(|i| make_finding(&format!("rule_{}", i), Severity::Warning, 5.0, 5))
            .collect();
        report.baseline_comparison = Some(
            (0..100)
                .map(|i| BaselineComparison {
                    metric_name: format!("metric_{}", i),
                    current_value: 10.0,
                    mean: 5.0,
                    stddev: 2.0,
                    is_outlier: true,
                    status: if i % 2 == 0 {
                        BaselineStatus::Outlier
                    } else {
                        BaselineStatus::Normal
                    },
                    phase: if i < 50 {
                        None
                    } else {
                        Some("design".to_string())
                    },
                })
                .collect(),
        );

        let start = Instant::now();
        let _result = format_retrospective_markdown(&report);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 5,
            "Formatting took {}ms, expected <5ms",
            elapsed.as_millis()
        );
    }

    // ── Dispatch tests (preserved from handler-dispatch agent) ───────

    #[test]
    fn test_dispatch_markdown_default() {
        let report = make_report();
        let result = dispatch_format(None, None, &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(
            text.starts_with("# Retrospective:"),
            "default format should produce markdown, got: {text}"
        );
    }

    #[test]
    fn test_dispatch_markdown_explicit() {
        let report = make_report();
        let result =
            dispatch_format(Some("markdown".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(text.starts_with("# Retrospective:"));
    }

    #[test]
    fn test_dispatch_summary_routes_to_markdown() {
        let report = make_report();
        let result =
            dispatch_format(Some("summary".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(text.starts_with("# Retrospective:"));
    }

    #[test]
    fn test_dispatch_json_explicit() {
        let report = make_report();
        let result =
            dispatch_format(Some("json".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("json format should produce valid JSON");
        assert_eq!(parsed["feature_cycle"], "test-001");
    }

    #[test]
    fn test_dispatch_invalid_format_returns_error() {
        let report = make_report();
        let err = dispatch_format(Some("xml".to_string()), None, &report)
            .expect_err("invalid format should return error");
        assert_eq!(err.code, ERROR_INVALID_PARAMS);
        assert!(err.message.contains("xml"));
        assert!(err.message.contains("markdown"));
        assert!(err.message.contains("json"));
    }

    #[test]
    fn test_json_evidence_limit_default_3() {
        let report = make_report_with_evidence(10);
        let result =
            dispatch_format(Some("json".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let evidence = parsed["hotspots"][0]["evidence"]
            .as_array()
            .expect("should have evidence array");
        assert_eq!(evidence.len(), 3);
    }

    #[test]
    fn test_json_evidence_limit_explicit_5() {
        let report = make_report_with_evidence(10);
        let result =
            dispatch_format(Some("json".to_string()), Some(5), &report).expect("should succeed");
        let text = extract_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let evidence = parsed["hotspots"][0]["evidence"]
            .as_array()
            .expect("should have evidence array");
        assert_eq!(evidence.len(), 5);
    }

    #[test]
    fn test_json_evidence_limit_explicit_0_no_truncation() {
        let report = make_report_with_evidence(10);
        let result =
            dispatch_format(Some("json".to_string()), Some(0), &report).expect("should succeed");
        let text = extract_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let evidence = parsed["hotspots"][0]["evidence"]
            .as_array()
            .expect("should have evidence array");
        assert_eq!(evidence.len(), 10);
    }

    #[test]
    fn test_markdown_ignores_evidence_limit() {
        let report = make_report_with_evidence(10);
        let result = dispatch_format(None, Some(1), &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(text.starts_with("# Retrospective:"));
        assert!(serde_json::from_str::<serde_json::Value>(&text).is_err());
    }

    #[test]
    fn test_json_output_matches_direct_call() {
        let report = make_report();
        let direct = format_retrospective_report(&report);
        let via_dispatch =
            dispatch_format(Some("json".to_string()), Some(0), &report).expect("should succeed");
        assert_eq!(extract_text(&direct), extract_text(&via_dispatch));
    }

    #[test]
    fn test_json_path_produces_valid_json() {
        let report = make_report_with_evidence(5);
        let result =
            dispatch_format(Some("json".to_string()), Some(0), &report).expect("should succeed");
        let text = extract_text(&result);
        serde_json::from_str::<serde_json::Value>(&text)
            .expect("JSON format should produce valid pretty-printed JSON");
    }

    #[test]
    fn test_format_retrospective_markdown_callable() {
        let report = make_report();
        let result = crate::mcp::response::format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(text.contains("# Retrospective:"));
    }
}
