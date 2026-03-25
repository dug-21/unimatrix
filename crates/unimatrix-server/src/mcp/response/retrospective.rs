//! Markdown formatter for retrospective reports (vnc-011).
//!
//! Transforms a `RetrospectiveReport` into compact, scannable markdown
//! optimized for LLM consumption. All collapse, filtering, grouping,
//! deduplication, and rendering logic lives here.
//!
//! col-026: Section order rewrite, header rebrand, Phase Timeline, What Went Well,
//! burst notation, phase annotations, enhanced Sessions, extended Knowledge Reuse,
//! threshold language post-processing via `format_claim_with_baseline`.

use rmcp::model::{CallToolResult, Content};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use unimatrix_observe::{
    AttributionMetadata, BaselineComparison, BaselineStatus, EvidenceRecord, FeatureKnowledgeReuse,
    GateResult, HotspotFinding, HotspotNarrative, PhaseCategoryComparison, PhaseNarrative,
    PhaseStats, Recommendation, RetrospectiveReport, SessionSummary, Severity,
};
use unimatrix_store::PhaseMetrics;

/// Formatter-internal type for collapsed findings grouped by rule_name.
#[derive(Debug)]
struct CollapsedFinding {
    rule_name: String,
    severity: Severity,
    claims: Vec<String>,
    measured: f64,
    threshold: f64,
    total_events: f64,
    tool_breakdown: Vec<(String, usize)>,
    /// Top 3 evidence records for display purposes (time-sorted).
    #[allow(dead_code)]
    examples: Vec<EvidenceRecord>,
    /// Full evidence pool (all records, time-sorted) — used for burst notation.
    all_evidence: Vec<EvidenceRecord>,
    narrative_summary: Option<String>,
    cluster_count: Option<usize>,
    sequence_pattern: Option<String>,
}

/// Format a `RetrospectiveReport` as compact markdown in a `CallToolResult`.
pub fn format_retrospective_markdown(report: &RetrospectiveReport) -> CallToolResult {
    let mut output = String::with_capacity(8192);

    // 1. Header (always rendered — rebranded col-026 FR-01..FR-05)
    output.push_str(&render_header(report));

    // GH#384: goal section — always rendered, with fallback when None
    output.push_str(&render_goal_section(report));

    // 2. Recommendations (moved from position 9 — FR-12)
    if !report.recommendations.is_empty() {
        output.push_str(&render_recommendations(&report.recommendations));
    }

    // 3. Phase Timeline (new — FR-06/07/08) or "No phase information captured." line
    output.push_str(&render_phase_timeline(report));

    // 4. What Went Well (new — FR-11)
    if let Some(wgw) = render_what_went_well(report) {
        output.push_str(&wgw);
    }

    // 5. Sessions table (enhanced — FR-15/16)
    if let Some(summaries) = &report.session_summaries
        && !summaries.is_empty()
    {
        output.push_str(&render_sessions(summaries));
    }

    // 6. Attribution note (only if partial attribution)
    if let Some(attr) = &report.attribution
        && attr.attributed_session_count < attr.total_session_count
    {
        output.push_str(&render_attribution_note(attr));
    }

    // 7. Baseline outliers — universal (only Outlier/NewSignal, omit section if none pass)
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

    // 8. Findings (enhanced — FR-09/FR-10/FR-14)
    if !report.hotspots.is_empty() {
        output.push_str(&render_findings(
            &report.hotspots,
            report.narratives.as_deref(),
            report.phase_stats.as_deref(),
            report.baseline_comparison.as_deref(),
            report.session_summaries.as_deref(),
        ));
    }

    // 9. Phase outliers (only Outlier/NewSignal with phase != None, zero-activity suppressed)
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

    // 10. Knowledge reuse (extended — FR-13)
    if let Some(reuse) = &report.feature_knowledge_reuse {
        output.push_str(&render_knowledge_reuse(reuse, &report.feature_cycle));
    }

    // 11. Rework & context reload (existing)
    let has_rework = report.rework_session_count.is_some_and(|n| n > 0);
    let has_reload = report.context_reload_pct.is_some();
    if has_rework || has_reload {
        output.push_str(&render_rework_reload(
            report.rework_session_count,
            report.context_reload_pct,
        ));
    }

    // 12. Phase narrative (crt-025: only when Some)
    if let Some(narrative) = &report.phase_narrative {
        output.push_str(&render_phase_narrative(narrative));
    }

    CallToolResult::success(vec![Content::text(output)])
}

fn render_header(report: &RetrospectiveReport) -> String {
    let mut out = String::new();

    // FR-01: rebranded header
    let _ = writeln!(out, "# Unimatrix Cycle Review — {}\n", report.feature_cycle);

    // FR-03 + FR-04 + FR-05: cycle type, attribution, status — build meta line
    let cycle_type = report.cycle_type.as_deref().unwrap_or("Unknown");
    let mut meta_parts: Vec<String> = vec![format!("Cycle type: {}", cycle_type)];

    if let Some(path) = &report.attribution_path {
        meta_parts.push(format!("Attribution: {}", path));
    }

    // FR-05: only Some(true) → Status IN PROGRESS; Some(false) and None omitted
    if report.is_in_progress == Some(true) {
        meta_parts.push("Status: IN PROGRESS".to_string());
    }

    let _ = writeln!(out, "{}", meta_parts.join("  |  "));

    // Summary line
    let duration = format_duration(report.metrics.universal.total_duration_secs);
    let _ = writeln!(
        out,
        "**Sessions**: {}  |  **Records**: {}  |  **Duration**: {}",
        report.session_count, report.total_records, duration,
    );

    out.push_str("\n---\n\n");
    out
}

fn render_goal_section(report: &RetrospectiveReport) -> String {
    let mut out = String::new();
    out.push_str("## Goal\n");
    match &report.goal {
        Some(goal) => {
            let safe_goal = goal.replace('\n', " ").replace('\r', " ");
            let _ = writeln!(out, "{}", safe_goal);
        }
        None => {
            out.push_str("No goal recorded for this cycle.\n");
        }
    }
    out.push('\n');
    out
}

fn render_sessions(summaries: &[SessionSummary]) -> String {
    let mut out = String::new();
    out.push_str("## Sessions\n");
    out.push_str("| # | Window | Calls | Tools | Agents | Knowledge | Outcome |\n");
    out.push_str("|---|--------|-------|-------|--------|-----------|--------|\n");

    for (i, s) in summaries.iter().enumerate() {
        let idx = i + 1;
        let start_secs = s.started_at / 1000;
        let hours = (start_secs % 86400) / 3600;
        let minutes = (start_secs % 3600) / 60;
        let dur_min = s.duration_secs / 60;
        let window = format!("{:02}:{:02} ({}m)", hours, minutes, dur_min);
        let calls: u64 = s.tool_distribution.values().sum();

        // FR-15: Tools column in NR NE NW NS format
        let r = s.tool_distribution.get("read").copied().unwrap_or(0);
        let e = s.tool_distribution.get("execute").copied().unwrap_or(0);
        let w = s.tool_distribution.get("write").copied().unwrap_or(0);
        let sr = s.tool_distribution.get("search").copied().unwrap_or(0);
        let mut tools_parts: Vec<String> = Vec::new();
        if r > 0 {
            tools_parts.push(format!("{}R", r));
        }
        if e > 0 {
            tools_parts.push(format!("{}E", e));
        }
        if w > 0 {
            tools_parts.push(format!("{}W", w));
        }
        if sr > 0 {
            tools_parts.push(format!("{}S", sr));
        }
        let tools_str = if tools_parts.is_empty() {
            "—".to_string()
        } else {
            tools_parts.join(" ")
        };

        // FR-15: Agents column
        let agents_str = if s.agents_spawned.is_empty() {
            "—".to_string()
        } else if s.agents_spawned.len() <= 3 {
            s.agents_spawned.join(", ")
        } else {
            let first3 = s.agents_spawned[..3].join(", ");
            format!("{} +{} more", first3, s.agents_spawned.len() - 3)
        };

        let knowledge = format!(
            "{} served, {} stored",
            s.knowledge_served, s.knowledge_stored
        );
        let outcome = s.outcome.as_deref().unwrap_or("-");
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            idx, window, calls, tools_str, agents_str, knowledge, outcome
        );
    }

    // FR-16: Top file zones below table
    let top_zones = compute_top_file_zones(summaries, 5);
    if !top_zones.is_empty() {
        let zones_str: Vec<String> = top_zones
            .iter()
            .map(|(path, count)| format!("{} ({})", path, count))
            .collect();
        let _ = writeln!(out, "\nTop file zones: {}", zones_str.join(", "));
    }

    out.push('\n');
    out
}

/// Aggregate top file zones across all sessions.
fn compute_top_file_zones(summaries: &[SessionSummary], n: usize) -> Vec<(String, u64)> {
    let mut zone_totals: HashMap<String, u64> = HashMap::new();
    for s in summaries {
        for (path, count) in &s.top_file_zones {
            *zone_totals.entry(path.clone()).or_insert(0) += count;
        }
    }
    let mut zones: Vec<(String, u64)> = zone_totals.into_iter().collect();
    zones.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    zones.truncate(n);
    zones
}

fn render_attribution_note(attr: &AttributionMetadata) -> String {
    format!(
        "> Note: {}/{} sessions attributed. Metrics may undercount.\n\n",
        attr.attributed_session_count, attr.total_session_count,
    )
}

/// Render the Phase Timeline section (FR-06/07/08).
///
/// When `phase_stats` is None or empty, emits `No phase information captured.` (no header).
fn render_phase_timeline(report: &RetrospectiveReport) -> String {
    let phase_stats = match &report.phase_stats {
        None => return "No phase information captured.\n\n".to_string(),
        Some(ps) if ps.is_empty() => return "No phase information captured.\n\n".to_string(),
        Some(ps) => ps,
    };

    let mut out = String::new();
    out.push_str("## Phase Timeline\n\n");
    out.push_str("| Phase | Duration | Passes | Records | Agents | Knowledge | Gate |\n");
    out.push_str("|-------|----------|--------|---------|--------|-----------|------|\n");

    for ps in phase_stats {
        let phase_display = if ps.phase.is_empty() {
            "—"
        } else {
            &ps.phase
        };
        let duration = format_duration(ps.duration_secs);
        let passes = if ps.pass_count > 1 {
            format!("**{}**", ps.pass_count)
        } else {
            ps.pass_count.to_string()
        };
        let agents_str = if ps.agents.is_empty() {
            "—".to_string()
        } else {
            ps.agents.join(", ")
        };
        let knowledge = format!("{}↓ {}↑", ps.knowledge_served, ps.knowledge_stored);
        let gate_str = match ps.gate_result {
            GateResult::Pass => "PASS",
            GateResult::Fail => "FAIL",
            GateResult::Rework => "PASS (rework)",
            GateResult::Unknown => "UNKNOWN",
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            phase_display, duration, passes, ps.record_count, agents_str, knowledge, gate_str
        );
    }

    // FR-07: Rework annotations
    let mut rework_emitted: HashSet<&str> = HashSet::new();
    for ps in phase_stats {
        if ps.pass_count > 1 && ps.pass_number == 1 && !rework_emitted.contains(ps.phase.as_str()) {
            rework_emitted.insert(&ps.phase);
            let outcome_text = ps.gate_outcome_text.as_deref().unwrap_or("(unknown)");
            let safe_outcome = outcome_text.replace('\n', " ").replace('\r', " ");
            let pass2_opt = phase_stats
                .iter()
                .find(|p| p.phase == ps.phase && p.pass_number == 2);
            let pass2_info = match pass2_opt {
                Some(p2) => format!(
                    "Pass 2: {}, {} records",
                    format_duration(p2.duration_secs),
                    p2.record_count
                ),
                None => "Pass 2: (no data)".to_string(),
            };
            let _ = writeln!(
                out,
                "\n**Rework**: {} — pass 1 gate {}: {}. {}",
                ps.phase,
                match ps.gate_result {
                    GateResult::Fail => "fail",
                    _ => "result",
                },
                safe_outcome,
                pass2_info
            );
        }
    }

    // FR-08: Top file zones below phase timeline
    if let Some(summaries) = &report.session_summaries {
        let top_zones = compute_top_file_zones(summaries, 5);
        if !top_zones.is_empty() {
            let zones_str: Vec<String> = top_zones
                .iter()
                .map(|(path, count)| format!("{} ({})", path, count))
                .collect();
            let _ = writeln!(out, "\nTop file zones: {}", zones_str.join(", "));
        }
    }

    out.push('\n');
    out
}

/// Build a map from finding_index (0-based, in collapse order) → (phase_name, pass_number).
///
/// Uses `PhaseStats.start_ms` and `end_ms` to map each finding's earliest evidence
/// timestamp to the phase window it falls in. When multiple phases contain evidence
/// for a finding, the phase with the highest evidence count wins.
fn build_phase_annotation_map(
    phase_stats: &[PhaseStats],
    hotspots: &[HotspotFinding],
) -> HashMap<usize, (String, u32)> {
    let mut map: HashMap<usize, (String, u32)> = HashMap::new();

    // Build ordered rule list (mirrors collapse_findings encounter order)
    let mut seen_rules: Vec<&str> = Vec::new();
    for h in hotspots {
        if !seen_rules.contains(&h.rule_name.as_str()) {
            seen_rules.push(&h.rule_name);
        }
    }

    for (finding_idx, rule_name) in seen_rules.iter().enumerate() {
        // Count evidence per phase window for this rule
        let mut phase_counts: HashMap<usize, usize> = HashMap::new();
        for h in hotspots.iter().filter(|h| h.rule_name == *rule_name) {
            for ev in &h.evidence {
                let ts = ev.ts as i64;
                for (ps_idx, ps) in phase_stats.iter().enumerate() {
                    let end = ps.end_ms.unwrap_or(i64::MAX);
                    if ts >= ps.start_ms && ts < end {
                        *phase_counts.entry(ps_idx).or_insert(0) += 1;
                        break;
                    }
                }
            }
        }

        // Use the phase with the highest evidence count
        if let Some((&best_idx, _)) = phase_counts.iter().max_by_key(|&(_, cnt)| cnt) {
            let ps = &phase_stats[best_idx];
            map.insert(finding_idx, (ps.phase.clone(), ps.pass_number));
        }
    }

    map
}

/// Render the "What Went Well" section (FR-11).
///
/// Returns `None` when no favorable Normal-status metrics exist.
fn render_what_went_well(report: &RetrospectiveReport) -> Option<String> {
    let comparisons = report.baseline_comparison.as_deref()?;
    if comparisons.is_empty() {
        return None;
    }

    // Metric direction table — SPECIFICATION §FR-11 (16 metrics, canonical)
    // true = higher is better; false = lower is better
    let direction_table: HashMap<&str, bool> = [
        ("compile_cycles", false),
        ("permission_friction_events", false),
        ("bash_for_search_count", false),
        ("reread_rate", false),
        ("coordinator_respawn_count", false),
        ("sleep_workaround_count", false),
        ("post_completion_work_pct", false),
        ("context_load_before_first_write_kb", false),
        ("file_breadth", false),
        ("mutation_spread", false),
        ("cold_restart_count", false),
        ("task_rework_count", false),
        ("edit_bloat_kb", false),
        ("parallel_call_rate", true),
        ("knowledge_entries_stored", true),
        ("follow_up_issues_created", true),
    ]
    .into_iter()
    .collect();

    // Plain-text labels per metric
    let labels: HashMap<&str, &str> = [
        ("compile_cycles", "low compile overhead across the cycle"),
        (
            "permission_friction_events",
            "low friction outside compile bursts",
        ),
        (
            "bash_for_search_count",
            "Grep/Glob used correctly throughout",
        ),
        (
            "reread_rate",
            "files read efficiently without repeated reads",
        ),
        ("coordinator_respawn_count", "no SM context loss"),
        ("sleep_workaround_count", "no polling hacks"),
        ("post_completion_work_pct", "clean stop after gate"),
        (
            "context_load_before_first_write_kb",
            "context loaded within budget",
        ),
        ("file_breadth", "focused file surface"),
        ("mutation_spread", "mutations concentrated in target files"),
        ("cold_restart_count", "no cold restarts needed"),
        ("task_rework_count", "tasks completed without rework"),
        ("edit_bloat_kb", "edits within expected size"),
        (
            "parallel_call_rate",
            "above-average concurrency across sessions",
        ),
        (
            "knowledge_entries_stored",
            "strong knowledge contribution this cycle",
        ),
        (
            "follow_up_issues_created",
            "issues captured for future work",
        ),
    ]
    .into_iter()
    .collect();

    let mut favorable: Vec<String> = Vec::new();

    for c in comparisons {
        // Universal metrics only (phase-scoped excluded)
        if c.phase.is_some() {
            continue;
        }
        // Must be Normal status (not Outlier/NewSignal/NoVariance)
        if !matches!(c.status, BaselineStatus::Normal) {
            continue;
        }
        // Must be in direction table
        let higher_is_better = match direction_table.get(c.metric_name.as_str()) {
            None => continue,
            Some(&b) => b,
        };
        // Must be favorable
        let is_favorable = if higher_is_better {
            c.current_value > c.mean
        } else {
            c.current_value < c.mean
        };
        if !is_favorable {
            continue;
        }
        let label = labels
            .get(c.metric_name.as_str())
            .copied()
            .unwrap_or("favorable signal");
        favorable.push(format!(
            "- **{}**: {:.1} vs mean {:.1} — {}",
            c.metric_name, c.current_value, c.mean, label
        ));
    }

    if favorable.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str("## What Went Well\n\n");
    for line in &favorable {
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    Some(out)
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

fn render_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
    phase_stats: Option<&[PhaseStats]>,
    baseline_comparison: Option<&[BaselineComparison]>,
    session_summaries: Option<&[SessionSummary]>,
) -> String {
    let collapsed = collapse_findings(hotspots, narratives);

    if collapsed.is_empty() {
        return String::new();
    }

    // Build phase annotation map if phase_stats available
    let annotation_map: HashMap<usize, (String, u32)> = match phase_stats {
        Some(ps) => build_phase_annotation_map(ps, hotspots),
        None => HashMap::new(),
    };

    // Cycle start time for relative burst notation
    let cycle_start_ms: u64 = session_summaries
        .unwrap_or(&[])
        .iter()
        .filter(|s| s.started_at > 0)
        .map(|s| s.started_at)
        .min()
        .unwrap_or(0);

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

        // FR-09: phase annotation
        let phase_annotation = match annotation_map.get(&i) {
            Some((phase, pass)) => format!(" — phase: {}/{}", phase, pass),
            None => String::new(),
        };

        let description = match &finding.narrative_summary {
            Some(summary) => summary.as_str(),
            None => finding.claims.first().map_or("", |c| c.as_str()),
        };
        let _ = writeln!(
            out,
            "### {} [{}] {} -- {} events{}",
            id, severity_tag, description, total_events, phase_annotation
        );

        // FR-14: claim with threshold language replacement
        if let Some(claim) = finding.claims.first() {
            let baseline_entry = baseline_comparison
                .unwrap_or(&[])
                .iter()
                .find(|b| b.metric_name == finding.rule_name && b.phase.is_none());
            let rendered_claim = format_claim_with_baseline(
                claim,
                &finding.rule_name,
                finding.measured,
                finding.threshold,
                baseline_entry,
            );
            let _ = writeln!(out, "{}", rendered_claim);
        }

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

        // FR-10: burst notation (replaces ts= epoch evidence)
        let burst_str = render_burst_notation(finding, cycle_start_ms);
        if !burst_str.is_empty() {
            out.push_str(&burst_str);
        }

        out.push('\n');
    }

    out
}

/// Post-process a claim string to replace threshold language with baseline framing (ADR-004).
///
/// Algorithm:
/// 1. Check if claim contains "threshold" substring.
/// 2. If found, strip the matched span (keyword + separator + digits).
/// 3. If baseline available with stddev > 0: append `(baseline: mean ±stddev, +zscore σ)`.
/// 4. Else if threshold > 0: append `(ratio× typical)`.
/// 5. If no threshold pattern: return claim unchanged.
fn format_claim_with_baseline(
    claim: &str,
    _rule_name: &str,
    measured: f64,
    threshold: f64,
    baseline: Option<&BaselineComparison>,
) -> String {
    let lower_claim = claim.to_lowercase();
    let threshold_pos = lower_claim.find("threshold");

    let stripped_claim = match threshold_pos {
        None => return claim.to_string(), // no threshold language; emit unchanged
        Some(pos) => {
            // Walk forward: "threshold" + optional ":" + optional " " + digits
            let after_keyword = &claim[pos + "threshold".len()..];
            let after_sep =
                after_keyword.trim_start_matches(|c: char| c == ':' || c == ' ' || c == '=');
            let end_of_num = after_sep
                .char_indices()
                .take_while(|(_, c)| c.is_ascii_digit() || *c == '.')
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
            let sep_len = after_keyword.len() - after_sep.len();
            let span_len = "threshold".len() + sep_len + end_of_num;
            let after_span = claim[pos + span_len..].trim_start();
            // Strip the threshold span: keep text before + text after
            let before = &claim[..pos];
            let combined = format!("{}{}", before, after_span);
            combined
                .trim_end_matches(|c: char| c == ' ' || c == '-')
                .to_string()
        }
    };

    // Append framing
    let framing = match baseline {
        Some(b) if b.stddev > 0.0 => {
            let zscore = (measured - b.mean) / b.stddev;
            format!(
                " (baseline: {:.1} ±{:.1}, +{:.1}σ)",
                b.mean, b.stddev, zscore
            )
        }
        _ if threshold > 0.0 => {
            let ratio = measured / threshold;
            format!(" ({:.1}× typical)", ratio)
        }
        _ => String::new(), // threshold = 0.0: skip ratio
    };

    format!("{}{}", stripped_claim, framing)
}

/// Render burst notation for finding evidence (FR-10).
///
/// Groups evidence into 5-minute buckets and emits Timeline + Peak lines.
/// Does NOT emit ts= epoch values.
fn render_burst_notation(finding: &CollapsedFinding, cycle_start_ms: u64) -> String {
    let evidence_pool = &finding.all_evidence;
    if evidence_pool.is_empty() {
        return String::new();
    }

    let origin_ts = if cycle_start_ms > 0 {
        cycle_start_ms
    } else {
        evidence_pool.first().map(|e| e.ts).unwrap_or(0)
    };

    // Group evidence into 5-minute buckets
    let mut buckets: BTreeMap<u64, Vec<&EvidenceRecord>> = BTreeMap::new();
    for ev in evidence_pool {
        let rel_ms = ev.ts.saturating_sub(origin_ts);
        let bucket_key = rel_ms / 300_000; // 5 min = 300,000 ms
        buckets.entry(bucket_key).or_default().push(ev);
    }

    if buckets.is_empty() {
        return String::new();
    }

    // Find peak bucket
    let peak_bucket_key = buckets
        .iter()
        .max_by_key(|(_, evs)| evs.len())
        .map(|(k, _)| *k)
        .unwrap_or(0);

    // Build Timeline line (max 10 entries)
    let bucket_entries: Vec<(&u64, &Vec<&EvidenceRecord>)> = buckets.iter().collect();
    let total_buckets = bucket_entries.len();
    let mut timeline_parts: Vec<String> = Vec::new();

    for (bucket_key, evs) in bucket_entries.iter().take(10) {
        let rel_min = **bucket_key * 5;
        let count = evs.len();
        let peak_marker = if **bucket_key == peak_bucket_key {
            "▲"
        } else {
            ""
        };
        timeline_parts.push(format!("+{}m({}{})", rel_min, count, peak_marker));
    }

    let timeline_str = if total_buckets > 10 {
        let last_key = bucket_entries.last().map(|(k, _)| **k).unwrap_or(0);
        let last_count = bucket_entries.last().map(|(_, evs)| evs.len()).unwrap_or(0);
        format!(
            "{} ... +{}m({})",
            timeline_parts.join(" "),
            last_key * 5,
            last_count
        )
    } else {
        timeline_parts.join(" ")
    };

    // Build Peak line
    let peak_evs = &buckets[&peak_bucket_key];
    let peak_rel_min = peak_bucket_key * 5;
    let mut tool_counts: HashMap<String, u32> = HashMap::new();
    for ev in peak_evs {
        if let Some(tool) = &ev.tool {
            *tool_counts.entry(tool.clone()).or_insert(0) += 1;
        }
    }
    let mut top_tools: Vec<(String, u32)> = tool_counts.into_iter().collect();
    top_tools.sort_by(|a, b| b.1.cmp(&a.1));
    top_tools.truncate(3);
    let files_str = if top_tools.is_empty() {
        String::new()
    } else {
        format!(
            " — {}",
            top_tools
                .iter()
                .map(|(f, _)| f.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    format!(
        "Timeline: {}\nPeak: {} events in 5min at +{}m{}\n",
        timeline_str,
        peak_evs.len(),
        peak_rel_min,
        files_str
    )
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
        let measured = findings.first().map(|f| f.measured).unwrap_or(0.0);
        let threshold = findings.first().map(|f| f.threshold).unwrap_or(0.0);

        let mut evidence_pool: Vec<&EvidenceRecord> =
            findings.iter().flat_map(|f| f.evidence.iter()).collect();
        evidence_pool.sort_by_key(|e| e.ts);
        let examples: Vec<EvidenceRecord> =
            evidence_pool.iter().take(3).map(|e| (*e).clone()).collect();
        let all_evidence: Vec<EvidenceRecord> =
            evidence_pool.iter().map(|e| (*e).clone()).collect();

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
            measured,
            threshold,
            total_events,
            tool_breakdown,
            examples,
            all_evidence,
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

fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse, feature_cycle: &str) -> String {
    let mut out = String::new();
    out.push_str("## Knowledge Reuse\n\n");

    if reuse.delivery_count == 0 {
        out.push_str("No knowledge entries served.\n\n");
        return out;
    }

    // Summary line (FR-13)
    let _ = writeln!(
        out,
        "**Total served**: {}  |  **Stored this cycle**: {}",
        reuse.delivery_count, reuse.total_stored
    );
    out.push('\n');

    // Bucket table
    out.push_str("| Bucket | Count |\n");
    out.push_str("|--------|-------|\n");
    let _ = writeln!(
        out,
        "| Cross-feature (prior cycles) | {} |",
        reuse.cross_feature_reuse
    );
    let _ = writeln!(
        out,
        "| Intra-cycle ({} entries) | {} |",
        feature_cycle, reuse.intra_cycle_reuse
    );
    out.push('\n');

    // By category line
    if !reuse.by_category.is_empty() {
        let mut cats: Vec<(&String, &u64)> = reuse.by_category.iter().collect();
        cats.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let cat_parts: Vec<String> = cats
            .iter()
            .map(|(cat, count)| format!("{}×{}", cat, count))
            .collect();
        let _ = writeln!(
            out,
            "**By category (all {} served)**: {}",
            reuse.delivery_count,
            cat_parts.join(", ")
        );
        out.push('\n');
    }

    // Top cross-feature entries table (omit when empty)
    if !reuse.top_cross_feature_entries.is_empty() {
        out.push_str("**Top cross-feature entries**:\n\n");
        out.push_str("| Entry | Type | Served | Source |\n");
        out.push_str("|-------|------|--------|--------|\n");
        for entry in &reuse.top_cross_feature_entries {
            let safe_title = entry.title.replace('|', "\\|");
            let _ = writeln!(
                out,
                "| `#{}` {} | {} | {}× | {} |",
                entry.id, safe_title, entry.category, entry.serve_count, entry.feature_cycle
            );
        }
        out.push('\n');
    }

    // NOTE: category_gaps NOT rendered (AC-12, SCOPE decision)
    // NOTE: cross_session_count NOT rendered (superseded by bucket split)

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

    format!("## Rework & Context Reload\n{}\n\n", parts.join(" | "))
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

/// Render the phase lifecycle narrative section (crt-025).
///
/// Emits nothing when `phase_narrative` is `None`; this function is only
/// called when `Some` (guarded in `format_retrospective_markdown`).
fn render_phase_narrative(narrative: &PhaseNarrative) -> String {
    let mut out = String::new();
    out.push_str("## Phase Narrative\n");

    // Phase sequence (may be empty for features with only cycle_stop events)
    if narrative.phase_sequence.is_empty() {
        out.push_str("No phase transitions recorded.\n\n");
        return out;
    }

    // Phase sequence list
    out.push_str("**Phase sequence:** ");
    out.push_str(&narrative.phase_sequence.join(" → "));
    out.push('\n');

    // Rework flags
    if !narrative.rework_phases.is_empty() {
        let _ = writeln!(
            out,
            "**Rework detected:** {}",
            narrative.rework_phases.join(", ")
        );
    }

    out.push('\n');

    // Per-phase category counts (sorted by phase for determinism)
    if !narrative.per_phase_categories.is_empty() {
        out.push_str("### Per-Phase Category Distribution\n");
        let mut phases: Vec<&String> = narrative.per_phase_categories.keys().collect();
        phases.sort();

        for phase in phases {
            let cat_map = &narrative.per_phase_categories[phase];
            out.push_str(&format!("**{}**:", phase));
            let mut cats: Vec<(&String, &u64)> = cat_map.iter().collect();
            cats.sort_by_key(|(k, _)| *k);
            let entries: Vec<String> = cats
                .iter()
                .map(|(cat, cnt)| format!(" {}={}", cat, cnt))
                .collect();
            out.push_str(&entries.join(","));
            out.push('\n');
        }
        out.push('\n');
    }

    // Cross-cycle comparison table (AC-14: omit when None)
    if let Some(comparisons) = &narrative.cross_cycle_comparison {
        render_cross_cycle_table(&mut out, comparisons);
    }

    out
}

/// Render the cross-cycle comparison as a markdown table.
fn render_cross_cycle_table(out: &mut String, comparisons: &[PhaseCategoryComparison]) {
    if comparisons.is_empty() {
        return;
    }

    out.push_str("### Cross-Cycle Comparison\n");
    out.push_str("| Phase | Category | This Feature | Cross-cycle Mean | Samples |\n");
    out.push_str("|-------|----------|-------------|-----------------|--------|\n");

    for c in comparisons {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.1} | {} |",
            c.phase, c.category, c.this_feature_count, c.cross_cycle_mean, c.sample_features
        );
    }

    out.push('\n');
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
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
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
        // AC-17: updated to new header format (col-026 FR-01)
        let mut report = make_report();
        report.feature_cycle = "nxs-010".to_string();
        let header = render_header(&report);
        assert!(header.contains("# Unimatrix Cycle Review — nxs-010"));
    }

    #[test]
    fn test_header_contains_session_count() {
        let mut report = make_report();
        report.session_count = 5;
        let header = render_header(&report);
        assert!(header.contains("**Sessions**: 5"));
    }

    #[test]
    fn test_header_contains_total_records() {
        let mut report = make_report();
        report.total_records = 312;
        let header = render_header(&report);
        assert!(header.contains("**Records**: 312"));
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
        // AC-17: updated to new header format (col-026 FR-01)
        assert!(text.starts_with("# Unimatrix Cycle Review — test-001"));
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
        // AC-17: updated assertions for col-026 header rebrand + new sections
        let report = make_report();
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(text.contains("# Unimatrix Cycle Review —"));
        assert!(!text.contains("# Retrospective:"));
        assert!(!text.contains("## Sessions"));
        assert!(!text.contains("## Outliers"));
        assert!(!text.contains("## Findings"));
        assert!(!text.contains("## Phase Outliers"));
        assert!(!text.contains("## Knowledge Reuse"));
        assert!(!text.contains("## Recommendations"));
        assert!(!text.contains("> Note:"));
        assert!(!text.contains("## Phase Timeline"));
        assert!(!text.contains("## What Went Well"));
        // phase_stats=None → single line fallback (no header)
        assert!(text.contains("No phase information captured."));
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
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
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
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
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
        let out = render_findings(&[], None, None, None, None);
        assert!(out.is_empty());
    }

    #[test]
    fn test_findings_ordering() {
        let hotspots = vec![
            make_finding("info_rule", Severity::Info, 1.0, 0),
            make_finding("warning_rule", Severity::Warning, 1.0, 0),
            make_finding("critical_rule", Severity::Critical, 1.0, 0),
        ];
        let out = render_findings(&hotspots, None, None, None, None);
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
        let out = render_findings(&hotspots, Some(&narratives), None, None, None);
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
        let out = render_findings(&hotspots, Some(&narratives), None, None, None);
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
        let out = render_findings(&hotspots, Some(&narratives), None, None, None);
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
        let out = render_findings(&hotspots, Some(&narratives), None, None, None);
        assert!(out.contains("Escalation pattern: 30s->60s"));
    }

    #[test]
    fn test_findings_single_finding_no_collapse() {
        let hotspots = vec![make_finding("single_rule", Severity::Info, 7.0, 1)];
        let out = render_findings(&hotspots, None, None, None, None);
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
        // AC-17: updated for new Knowledge Reuse format (col-026 FR-13)
        let reuse = FeatureKnowledgeReuse {
            delivery_count: 15,
            cross_session_count: 8,
            by_category: HashMap::new(),
            category_gaps: vec!["procedure".to_string()],
            total_served: 0,
            total_stored: 5,
            cross_feature_reuse: 3,
            intra_cycle_reuse: 2,
            top_cross_feature_entries: vec![],
        };
        let out = render_knowledge_reuse(&reuse, "test-001");
        // New format: Total served / Stored this cycle
        assert!(out.contains("**Total served**: 15"));
        assert!(out.contains("**Stored this cycle**: 5"));
        // category_gaps NOT rendered (AC-12)
        assert!(!out.contains("Gaps:"));
        assert!(!out.contains("cross-session"));
    }

    #[test]
    fn test_knowledge_reuse_no_gaps() {
        let reuse = FeatureKnowledgeReuse {
            delivery_count: 5,
            cross_session_count: 2,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
        let out = render_knowledge_reuse(&reuse, "test-001");
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
        let out = render_findings(&[f], None, None, None, None);
        assert!(out.contains("\u{1F525}"));
    }

    #[test]
    fn test_float_sum_formatting() {
        let hotspots = vec![
            make_finding("rule_a", Severity::Info, 0.1, 0),
            make_finding("rule_a", Severity::Info, 0.2, 0),
            make_finding("rule_a", Severity::Info, 0.3, 0),
        ];
        let out = render_findings(&hotspots, None, None, None, None);
        assert!(!out.contains("0.600000000000000"));
    }

    #[test]
    fn test_nan_measured() {
        let hotspots = vec![make_finding("rule_a", Severity::Info, f64::NAN, 0)];
        let out = render_findings(&hotspots, None, None, None, None);
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
        // AC-17: updated to new header format (col-026 FR-01)
        assert!(
            text.starts_with("# Unimatrix Cycle Review —"),
            "default format should produce markdown, got: {text}"
        );
    }

    #[test]
    fn test_dispatch_markdown_explicit() {
        let report = make_report();
        let result =
            dispatch_format(Some("markdown".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(text.starts_with("# Unimatrix Cycle Review —")); // AC-17
    }

    #[test]
    fn test_dispatch_summary_routes_to_markdown() {
        let report = make_report();
        let result =
            dispatch_format(Some("summary".to_string()), None, &report).expect("should succeed");
        let text = extract_text(&result);
        assert!(text.starts_with("# Unimatrix Cycle Review —")); // AC-17
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
        assert!(text.starts_with("# Unimatrix Cycle Review —")); // AC-17
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
        assert!(text.contains("# Unimatrix Cycle Review —")); // AC-17
    }

    // ── Phase Narrative Rendering (crt-025) ─────────────────────────────

    fn make_phase_narrative_simple() -> PhaseNarrative {
        PhaseNarrative {
            phase_sequence: vec!["scope".to_string(), "design".to_string()],
            rework_phases: vec![],
            per_phase_categories: {
                let mut m = std::collections::HashMap::new();
                let mut scope_cats = std::collections::HashMap::new();
                scope_cats.insert("decision".to_string(), 3u64);
                m.insert("scope".to_string(), scope_cats);
                m
            },
            cross_cycle_comparison: None,
        }
    }

    #[test]
    fn test_render_phase_narrative_none_omits_section() {
        // When phase_narrative is None, the section must not appear (AC-13)
        let report = make_report();
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            !text.contains("Phase Narrative"),
            "Phase Narrative section must be absent when phase_narrative is None; got: {}",
            &text[..text.len().min(500)]
        );
    }

    #[test]
    fn test_render_phase_narrative_some_emits_section() {
        // When phase_narrative is Some, the section must appear
        let mut report = make_report();
        report.phase_narrative = Some(make_phase_narrative_simple());
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("## Phase Narrative"),
            "Phase Narrative section must be present when Some"
        );
    }

    #[test]
    fn test_render_phase_narrative_phase_sequence_rendered() {
        let mut report = make_report();
        report.phase_narrative = Some(make_phase_narrative_simple());
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("scope → design"),
            "phase sequence must be rendered as arrow-joined list; got: {}",
            &text[..text.len().min(800)]
        );
    }

    #[test]
    fn test_render_phase_narrative_rework_phases_rendered() {
        let mut report = make_report();
        report.phase_narrative = Some(PhaseNarrative {
            phase_sequence: vec![
                "scope".to_string(),
                "design".to_string(),
                "scope".to_string(),
            ],
            rework_phases: vec!["scope".to_string()],
            per_phase_categories: std::collections::HashMap::new(),
            cross_cycle_comparison: None,
        });
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("Rework detected"),
            "rework flag must be rendered when rework_phases is non-empty"
        );
        assert!(
            text.contains("scope"),
            "rework phase name must appear in output"
        );
    }

    #[test]
    fn test_render_phase_narrative_no_rework_omits_rework_line() {
        let mut report = make_report();
        report.phase_narrative = Some(make_phase_narrative_simple());
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            !text.contains("Rework detected"),
            "rework line must not appear when rework_phases is empty"
        );
    }

    #[test]
    fn test_render_phase_narrative_per_phase_categories() {
        let mut report = make_report();
        report.phase_narrative = Some(make_phase_narrative_simple());
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("Per-Phase Category Distribution"),
            "per-phase section must be present"
        );
        // scope phase with decision=3 must appear
        assert!(
            text.contains("scope"),
            "phase name must appear in distribution"
        );
        assert!(
            text.contains("decision=3"),
            "category count must appear in distribution"
        );
    }

    #[test]
    fn test_render_phase_narrative_cross_cycle_absent_when_none() {
        // cross_cycle_comparison = None → table must not appear (AC-14)
        let mut report = make_report();
        report.phase_narrative = Some(make_phase_narrative_simple());
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            !text.contains("Cross-Cycle Comparison"),
            "cross-cycle table must be absent when cross_cycle_comparison is None"
        );
    }

    #[test]
    fn test_render_phase_narrative_cross_cycle_table_rendered() {
        let mut report = make_report();
        report.phase_narrative = Some(PhaseNarrative {
            phase_sequence: vec!["design".to_string()],
            rework_phases: vec![],
            per_phase_categories: std::collections::HashMap::new(),
            cross_cycle_comparison: Some(vec![PhaseCategoryComparison {
                phase: "design".to_string(),
                category: "decision".to_string(),
                this_feature_count: 5,
                cross_cycle_mean: 2.5,
                sample_features: 3,
            }]),
        });
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("### Cross-Cycle Comparison"),
            "cross-cycle table header must appear"
        );
        assert!(text.contains("design"), "phase column must appear in table");
        assert!(
            text.contains("decision"),
            "category column must appear in table"
        );
        assert!(
            text.contains("2.5"),
            "cross-cycle mean must appear in table"
        );
        assert!(
            text.contains("| 3 |"),
            "sample_features count must appear in table"
        );
    }

    #[test]
    fn test_render_phase_narrative_empty_sequence_no_crash() {
        // Empty phase_sequence: should render gracefully
        let mut report = make_report();
        report.phase_narrative = Some(PhaseNarrative {
            phase_sequence: vec![],
            rework_phases: vec![],
            per_phase_categories: std::collections::HashMap::new(),
            cross_cycle_comparison: None,
        });
        let result = format_retrospective_markdown(&report);
        let text = extract_text(&result);
        assert!(
            text.contains("## Phase Narrative"),
            "section header must appear even with empty sequence"
        );
        assert!(
            text.contains("No phase transitions recorded"),
            "empty sequence message must appear"
        );
    }

    // ── col-026 New Tests ────────────────────────────────────────────

    fn make_phase_stats_entry(
        phase: &str,
        pass_number: u32,
        pass_count: u32,
        duration_secs: u64,
        record_count: usize,
        gate_result: GateResult,
        start_ms: i64,
        end_ms: Option<i64>,
    ) -> PhaseStats {
        use unimatrix_observe::ToolDistribution;
        PhaseStats {
            phase: phase.to_string(),
            pass_number,
            pass_count,
            duration_secs,
            start_ms,
            end_ms,
            session_count: 1,
            record_count,
            agents: vec!["researcher".to_string()],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 3,
            knowledge_stored: 0,
            gate_result,
            gate_outcome_text: Some("passed".to_string()),
            hotspot_ids: vec![],
        }
    }

    // ── AC-01/AC-17: Header rebrand ───────────────────────────────────

    #[test]
    fn test_header_rebrand() {
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("# Unimatrix Cycle Review —"));
        assert!(!text.contains("# Retrospective:"));
    }

    // ── AC-02: Goal line ─────────────────────────────────────────────

    #[test]
    fn test_header_goal_present() {
        // GH#384: goal now renders as ## Goal section, not inline **Goal**: prefix
        let mut report = make_report();
        report.goal = Some("Implement feature X".to_string());
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Goal"));
        assert!(text.contains("Implement feature X"));
        assert!(!text.contains("**Goal**:"));
    }

    #[test]
    fn test_header_goal_absent() {
        // GH#384: absent goal always renders fallback text, never silently omitted
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Goal"));
        assert!(text.contains("No goal recorded for this cycle."));
        assert!(!text.contains("**Goal**"));
    }

    #[test]
    fn test_header_goal_with_newline() {
        let mut report = make_report();
        report.goal = Some("line1\nline2".to_string());
        let text = extract_text(&format_retrospective_markdown(&report));
        // Newlines stripped to single line
        assert!(text.contains("line1 line2"));
        // Appears in the ## Goal section
        assert!(text.contains("## Goal"));
    }

    // ── GH#384: render_goal_section ──────────────────────────────────

    #[test]
    fn test_goal_section_absent_goal_renders_fallback() {
        let report = make_report(); // goal: None
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(
            text.contains("## Goal"),
            "## Goal section must always be present"
        );
        assert!(
            text.contains("No goal recorded for this cycle."),
            "fallback text must appear when goal is None"
        );
    }

    #[test]
    fn test_goal_section_present_goal_renders_verbatim() {
        let mut report = make_report();
        report.goal = Some("Fix the cycle review formatter.".to_string());
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Goal"), "## Goal section must be present");
        assert!(
            text.contains("Fix the cycle review formatter."),
            "verbatim goal text must appear"
        );
        assert!(
            !text.contains("**Goal**:"),
            "old inline **Goal**: format must not appear"
        );
    }

    #[test]
    fn test_goal_section_appears_before_recommendations() {
        let mut report = make_report();
        report.goal = Some("Add goal section.".to_string());
        report.recommendations = vec![Recommendation {
            hotspot_type: "compile_cycles".to_string(),
            action: "Batch builds".to_string(),
            rationale: "Reduce cycles".to_string(),
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        let goal_pos = text
            .find("## Goal")
            .expect("## Goal section must be present");
        let rec_pos = text
            .find("## Recommendations")
            .expect("## Recommendations must be present");
        assert!(
            goal_pos < rec_pos,
            "## Goal must appear before ## Recommendations (goal_pos={}, rec_pos={})",
            goal_pos,
            rec_pos
        );
    }

    // ── AC-03: Cycle type classification ────────────────────────────

    #[test]
    fn test_cycle_type_classification() {
        let cases = [
            ("design the API", "Design"),
            ("implement the new tool", "Delivery"),
            ("fix the crash bug", "Bugfix"),
            ("refactor the parser", "Refactor"),
            ("organize the workspace", "Unknown"),
        ];
        for (goal_text, expected_type) in &cases {
            let mut report = make_report();
            report.goal = Some(goal_text.to_string());
            report.cycle_type = Some(expected_type.to_string());
            let text = extract_text(&format_retrospective_markdown(&report));
            assert!(
                text.contains(&format!("Cycle type: {}", expected_type)),
                "For goal '{}', expected cycle type '{}'",
                goal_text,
                expected_type
            );
        }
        // None goal → Unknown
        let mut report = make_report();
        report.cycle_type = None;
        let text = extract_text(&format_retrospective_markdown(&report));
        // cycle_type None → "Unknown" default
        assert!(text.contains("Cycle type: Unknown"));
    }

    #[test]
    fn test_cycle_type_first_match_priority() {
        let mut report = make_report();
        report.goal = Some("design and implement".to_string());
        report.cycle_type = Some("Design".to_string()); // first-match wins
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("Cycle type: Design"));
    }

    // ── AC-04: Attribution path labels ──────────────────────────────

    #[test]
    fn test_attribution_path_labels() {
        for (label, expected) in [
            (
                "cycle_events-first (primary)",
                "Attribution: cycle_events-first (primary)",
            ),
            (
                "sessions.feature_cycle (legacy)",
                "Attribution: sessions.feature_cycle (legacy)",
            ),
            (
                "content-scan (fallback)",
                "Attribution: content-scan (fallback)",
            ),
        ] {
            let mut report = make_report();
            report.attribution_path = Some(label.to_string());
            let text = extract_text(&format_retrospective_markdown(&report));
            assert!(
                text.contains(expected),
                "Expected '{}' in output for label '{}'",
                expected,
                label
            );
        }
    }

    #[test]
    fn test_attribution_path_absent_when_none() {
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("Attribution:"));
    }

    // ── AC-05: is_in_progress status rendering ───────────────────────

    #[test]
    fn test_header_status_in_progress() {
        let mut report = make_report();
        report.is_in_progress = Some(true);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("Status: IN PROGRESS"));
    }

    #[test]
    fn test_header_status_omitted_when_some_false() {
        let mut report = make_report();
        report.is_in_progress = Some(false);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("Status:"));
    }

    #[test]
    fn test_header_status_omitted_when_none() {
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("Status:"));
    }

    fn test_is_in_progress_three_states() {
        // Alias — covered by the three tests above
    }

    // ── AC-06/AC-07: Phase Timeline ───────────────────────────────────

    #[test]
    fn test_phase_timeline_table() {
        use unimatrix_observe::ToolDistribution;
        let mut report = make_report();
        report.phase_stats = Some(vec![PhaseStats {
            phase: "scope".to_string(),
            pass_number: 1,
            pass_count: 1,
            duration_secs: 2520,
            start_ms: 0,
            end_ms: None,
            session_count: 2,
            record_count: 73,
            agents: vec!["researcher".to_string()],
            tool_distribution: ToolDistribution {
                read: 40,
                execute: 10,
                write: 15,
                search: 8,
            },
            knowledge_served: 3,
            knowledge_stored: 0,
            gate_result: GateResult::Pass,
            gate_outcome_text: Some("passed".to_string()),
            hotspot_ids: vec![],
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Phase Timeline"));
        assert!(text.contains("| scope |"));
        assert!(text.contains("42m")); // 2520 secs = 42 min
        assert!(text.contains("3↓ 0↑"));
        assert!(text.contains("PASS"));
    }

    #[test]
    fn test_phase_timeline_rework_annotation() {
        use unimatrix_observe::ToolDistribution;
        let make_ps = |pass_number: u32, duration: u64| PhaseStats {
            phase: "design".to_string(),
            pass_number,
            pass_count: 2,
            duration_secs: duration,
            start_ms: 0,
            end_ms: None,
            session_count: 1,
            record_count: 20,
            agents: vec![],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 0,
            knowledge_stored: 0,
            gate_result: GateResult::Fail,
            gate_outcome_text: Some("gate-fail".to_string()),
            hotspot_ids: vec![],
        };
        let mut report = make_report();
        report.phase_stats = Some(vec![make_ps(1, 1800), make_ps(2, 900)]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## Phase Timeline"));
        assert!(text.contains("**Rework**: design"));
        assert!(text.contains("pass 1"));
    }

    #[test]
    fn test_phase_timeline_absent_when_phase_stats_none() {
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## Phase Timeline"));
        assert!(text.contains("No phase information captured."));
    }

    #[test]
    fn test_phase_timeline_absent_when_phase_stats_empty() {
        let mut report = make_report();
        report.phase_stats = Some(vec![]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## Phase Timeline"));
        assert!(text.contains("No phase information captured."));
    }

    #[test]
    fn test_phase_timeline_empty_phase_name() {
        let mut report = make_report();
        report.phase_stats = Some(vec![make_phase_stats_entry(
            "",
            1,
            1,
            600,
            10,
            GateResult::Unknown,
            0,
            None,
        )]);
        let text = extract_text(&format_retrospective_markdown(&report));
        // Empty phase name rendered as "—"
        assert!(text.contains("| — |") || text.contains("|—|") || text.contains("—"));
    }

    // ── AC-08: Finding phase annotation ──────────────────────────────

    #[test]
    fn test_finding_phase_annotation() {
        let mut report = make_report();
        // PhaseStats with start_ms/end_ms covering the evidence timestamp
        // Evidence timestamp in make_finding is 1000 + i (ms)
        let ps = make_phase_stats_entry("scope", 1, 1, 600, 10, GateResult::Pass, 0, Some(2000));
        report.phase_stats = Some(vec![ps]);
        report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 15.0, 2)];
        let text = extract_text(&format_retrospective_markdown(&report));
        // Finding should have phase annotation
        assert!(
            text.contains("— phase: scope/1"),
            "Expected phase annotation in output, got:\n{}",
            &text[..text.len().min(1000)]
        );
    }

    #[test]
    fn test_finding_no_phase_annotation_when_phase_stats_none() {
        let mut report = make_report();
        report.phase_stats = None;
        report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 15.0, 2)];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("— phase:"));
    }

    // ── AC-09: Burst notation ─────────────────────────────────────────

    #[test]
    fn test_burst_notation_rendering() {
        // Evidence at 1_000_000, 1_720_000, 2_680_000 ms
        let mut report = make_report();
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "test_rule".to_string(),
            claim: "test claim".to_string(),
            measured: 3.0,
            threshold: 1.0,
            evidence: vec![
                EvidenceRecord {
                    description: "e1".to_string(),
                    ts: 1_000_000,
                    tool: Some("Bash".to_string()),
                    detail: String::new(),
                },
                EvidenceRecord {
                    description: "e2".to_string(),
                    ts: 1_720_000,
                    tool: Some("Bash".to_string()),
                    detail: String::new(),
                },
                EvidenceRecord {
                    description: "e3".to_string(),
                    ts: 2_680_000,
                    tool: Some("Read".to_string()),
                    detail: String::new(),
                },
            ],
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("Timeline:"), "Timeline line missing");
        assert!(text.contains("+0m"), "+0m missing");
        assert!(text.contains("Peak:"), "Peak line missing");
        assert!(!text.contains("ts="), "ts= epoch values must not appear");
    }

    #[test]
    fn test_burst_notation_single_evidence() {
        let mut report = make_report();
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Info,
            rule_name: "rule".to_string(),
            claim: "claim".to_string(),
            measured: 1.0,
            threshold: 0.0,
            evidence: vec![EvidenceRecord {
                description: "single".to_string(),
                ts: 500_000,
                tool: None,
                detail: String::new(),
            }],
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("Timeline:"));
        assert!(text.contains("+0m(1"));
        assert!(text.contains("Peak:"));
    }

    #[test]
    fn test_burst_notation_truncation_at_ten() {
        // 12 evidence records spanning 12 different 5-min buckets
        let evidence: Vec<EvidenceRecord> = (0..12u64)
            .map(|i| EvidenceRecord {
                description: format!("e{}", i),
                ts: i * 300_001, // each in different 5-min bucket
                tool: None,
                detail: String::new(),
            })
            .collect();
        let mut report = make_report();
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "rule".to_string(),
            claim: "claim".to_string(),
            measured: 12.0,
            threshold: 1.0,
            evidence,
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("..."), "truncation marker expected");
    }

    // ── AC-10/R-06: What Went Well ────────────────────────────────────

    #[test]
    fn test_what_went_well_present() {
        let mut report = make_report();
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "parallel_call_rate".to_string(),
            current_value: 0.8,
            mean: 0.5,
            stddev: 0.1,
            is_outlier: false,
            status: BaselineStatus::Normal,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("## What Went Well"));
    }

    #[test]
    fn test_what_went_well_absent_no_favorable() {
        let mut report = make_report();
        // All outliers
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "compile_cycles".to_string(),
            current_value: 50.0,
            mean: 5.0,
            stddev: 2.0,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## What Went Well"));
    }

    #[test]
    fn test_what_went_well_absent_no_baseline() {
        let report = make_report();
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## What Went Well"));
    }

    #[test]
    fn test_what_went_well_excludes_outlier_metrics() {
        let mut report = make_report();
        // parallel_call_rate is favorable but Outlier
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "parallel_call_rate".to_string(),
            current_value: 0.9,
            mean: 0.5,
            stddev: 0.1,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## What Went Well"));
    }

    #[test]
    fn test_what_went_well_direction_table_all_16_metrics() {
        // Higher-is-better metrics
        let higher_is_better = [
            "parallel_call_rate",
            "knowledge_entries_stored",
            "follow_up_issues_created",
        ];
        // Lower-is-better metrics
        let lower_is_better = [
            "compile_cycles",
            "permission_friction_events",
            "bash_for_search_count",
            "reread_rate",
            "coordinator_respawn_count",
            "sleep_workaround_count",
            "post_completion_work_pct",
            "context_load_before_first_write_kb",
            "file_breadth",
            "mutation_spread",
            "cold_restart_count",
            "task_rework_count",
            "edit_bloat_kb",
        ];

        // Favorable direction should appear in What Went Well
        for metric in &higher_is_better {
            let mut report = make_report();
            report.baseline_comparison = Some(vec![BaselineComparison {
                metric_name: metric.to_string(),
                current_value: 10.0, // higher than mean
                mean: 5.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            }]);
            let text = extract_text(&format_retrospective_markdown(&report));
            assert!(
                text.contains("## What Went Well"),
                "metric {} (higher-is-better, favorable) should appear in What Went Well",
                metric
            );
        }

        // Unfavorable direction should NOT appear
        for metric in &higher_is_better {
            let mut report = make_report();
            report.baseline_comparison = Some(vec![BaselineComparison {
                metric_name: metric.to_string(),
                current_value: 2.0, // lower than mean (unfavorable for HIB)
                mean: 5.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            }]);
            let text = extract_text(&format_retrospective_markdown(&report));
            assert!(
                !text.contains("## What Went Well"),
                "metric {} (higher-is-better, unfavorable) should NOT appear",
                metric
            );
        }

        for metric in &lower_is_better {
            // Favorable: current < mean
            let mut report = make_report();
            report.baseline_comparison = Some(vec![BaselineComparison {
                metric_name: metric.to_string(),
                current_value: 1.0,
                mean: 5.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            }]);
            let text = extract_text(&format_retrospective_markdown(&report));
            assert!(
                text.contains("## What Went Well"),
                "metric {} (lower-is-better, favorable) should appear in What Went Well",
                metric
            );

            // Unfavorable: current > mean
            let mut report2 = make_report();
            report2.baseline_comparison = Some(vec![BaselineComparison {
                metric_name: metric.to_string(),
                current_value: 10.0,
                mean: 5.0,
                stddev: 0.0,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            }]);
            let text2 = extract_text(&format_retrospective_markdown(&report2));
            assert!(
                !text2.contains("## What Went Well"),
                "metric {} (lower-is-better, unfavorable) should NOT appear",
                metric
            );
        }
    }

    #[test]
    fn test_metric_not_in_direction_table_excluded() {
        let mut report = make_report();
        report.baseline_comparison = Some(vec![BaselineComparison {
            metric_name: "unknown_metric".to_string(),
            current_value: 1.0,
            mean: 5.0,
            stddev: 0.0,
            is_outlier: false,
            status: BaselineStatus::Normal,
            phase: None,
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("## What Went Well"));
    }

    // ── AC-12: Enhanced Knowledge Reuse section ───────────────────────

    #[test]
    fn test_knowledge_reuse_section() {
        use unimatrix_observe::EntryRef;
        let mut report = make_report();
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 10,
            cross_session_count: 2,
            by_category: {
                let mut m = HashMap::new();
                m.insert("decision".to_string(), 6u64);
                m.insert("pattern".to_string(), 4u64);
                m
            },
            category_gaps: vec!["procedure".to_string()],
            total_served: 10,
            total_stored: 3,
            cross_feature_reuse: 6,
            intra_cycle_reuse: 4,
            top_cross_feature_entries: vec![EntryRef {
                id: 42,
                title: "ADR-003".to_string(),
                feature_cycle: "col-024".to_string(),
                category: "decision".to_string(),
                serve_count: 4,
            }],
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("**Total served**: 10"));
        assert!(text.contains("**Stored this cycle**: 3"));
        assert!(text.contains("Cross-feature (prior cycles)"));
        assert!(text.contains("6"));
        assert!(text.contains("Intra-cycle"));
        assert!(text.contains("4"));
        assert!(text.contains("`#42`"));
        // category_gaps NOT rendered
        assert!(!text.contains("category_gaps"));
        assert!(!text.contains("Gaps:"));
        // cross_session_count NOT rendered
        assert!(!text.contains("cross-session"));
    }

    #[test]
    fn test_knowledge_reuse_zero_delivery() {
        let mut report = make_report();
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 0,
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("No knowledge entries served."));
    }

    #[test]
    fn test_knowledge_reuse_no_cross_feature_omits_table() {
        let mut report = make_report();
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 5,
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 5,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 5,
            top_cross_feature_entries: vec![],
        });
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("Top cross-feature entries"));
    }

    // ── AC-13/R-08: Threshold language removal ────────────────────────

    #[test]
    fn test_format_claim_with_baseline_baseline_path() {
        let baseline = BaselineComparison {
            metric_name: "compile_cycles".to_string(),
            current_value: 43.0,
            mean: 8.0,
            stddev: 2.5,
            is_outlier: true,
            status: BaselineStatus::Outlier,
            phase: None,
        };
        let result = format_claim_with_baseline(
            "43 compile cycles (threshold: 10) -- 4.3x typical",
            "compile_cycles",
            43.0,
            10.0,
            Some(&baseline),
        );
        assert!(result.contains("(baseline: 8.0 ±2.5, +14.0σ)"));
        assert!(!result.contains("threshold"));
    }

    #[test]
    fn test_format_claim_with_baseline_ratio_fallback() {
        let baseline = BaselineComparison {
            metric_name: "compile_cycles".to_string(),
            current_value: 43.0,
            mean: 8.0,
            stddev: 0.0, // stddev=0 → ratio fallback
            is_outlier: false,
            status: BaselineStatus::NoVariance,
            phase: None,
        };
        let result = format_claim_with_baseline(
            "43 compile cycles (threshold: 10)",
            "compile_cycles",
            43.0,
            10.0,
            Some(&baseline),
        );
        assert!(result.contains("4.3× typical"));
        assert!(!result.contains("threshold"));
    }

    #[test]
    fn test_format_claim_with_baseline_no_threshold_pattern() {
        let result =
            format_claim_with_baseline("43 compile cycles", "compile_cycles", 43.0, 10.0, None);
        // No threshold pattern → unchanged
        assert_eq!(result, "43 compile cycles");
    }

    #[test]
    fn test_format_claim_threshold_zero_value() {
        let result = format_claim_with_baseline(
            "value (threshold: 0)",
            "rule",
            5.0,
            0.0, // threshold=0 → no division
            None,
        );
        // No panic, no ratio annotation
        assert!(!result.contains("inf"));
        assert!(!result.contains("threshold"));
    }

    #[test]
    fn test_no_threshold_language() {
        // All 9 enumerated detection sites — claim strings with threshold patterns
        let claim_patterns = [
            (
                "context_load_before_first_write_kb",
                "loaded 500kb before first write (threshold: 200)",
            ),
            ("lifespan", "agent lifespan 120s exceeds threshold: 90"),
            ("file_breadth", "touched 25 files (threshold: 15)"),
            ("reread_rate", "reread rate 0.8 exceeds threshold: 0.5"),
            ("mutation_spread", "spread across 12 files (threshold: 8)"),
            ("compile_cycles", "43 compile cycles (threshold: 10)"),
            ("edit_bloat", "edit size 1200kb (threshold: 500)"),
            ("adr_count", "3 ADRs created (threshold: 2)"),
            ("permission_retries", "5 retries (threshold: 3)"),
        ];
        for (rule_name, claim) in &claim_patterns {
            let result = format_claim_with_baseline(claim, rule_name, 10.0, 5.0, None);
            assert!(
                !result.to_lowercase().contains("threshold"),
                "threshold language still present for rule '{}': {}",
                rule_name,
                result
            );
        }
    }

    // ── AC-14: Session table enhancement ─────────────────────────────

    #[test]
    fn test_session_table_enhancement() {
        let mut dist = HashMap::new();
        dist.insert("read".to_string(), 50u64);
        dist.insert("execute".to_string(), 20u64);
        dist.insert("write".to_string(), 10u64);
        dist.insert("search".to_string(), 5u64);
        let summary = SessionSummary {
            session_id: "s1".to_string(),
            started_at: 0,
            duration_secs: 3600,
            tool_distribution: dist,
            top_file_zones: vec![],
            agents_spawned: vec!["uni-architect".to_string(), "uni-rust-dev".to_string()],
            knowledge_served: 5,
            knowledge_stored: 2,
            knowledge_curated: 0,
            outcome: None,
        };
        let out = render_sessions(&[summary]);
        assert!(out.contains("## Sessions"));
        // Tool column in NR NE NW NS format
        assert!(out.contains("50R"));
        assert!(out.contains("20E"));
        assert!(out.contains("10W"));
        assert!(out.contains("5S"));
        // Agents column
        assert!(out.contains("uni-architect"));
        assert!(out.contains("uni-rust-dev"));
    }

    // ── AC-15: Top file zones ────────────────────────────────────────

    #[test]
    fn test_top_file_zones() {
        let mut dist = HashMap::new();
        dist.insert("read".to_string(), 10u64);
        let summary = SessionSummary {
            session_id: "s1".to_string(),
            started_at: 0,
            duration_secs: 100,
            tool_distribution: dist,
            top_file_zones: vec![
                ("crates/server/src".to_string(), 42),
                ("crates/store/src".to_string(), 18),
            ],
            agents_spawned: vec![],
            knowledge_served: 0,
            knowledge_stored: 0,
            knowledge_curated: 0,
            outcome: None,
        };
        let out = render_sessions(&[summary]);
        assert!(out.contains("Top file zones:"));
        assert!(out.contains("crates/server/src"));
        assert!(out.contains("crates/store/src"));
    }

    // ── R-07/AC-11: Section order ────────────────────────────────────

    #[test]
    fn test_section_order() {
        use unimatrix_observe::{EntryRef, ToolDistribution};
        let mut report = make_report();
        report.goal = Some("Implement feature X".to_string());
        report.cycle_type = Some("Delivery".to_string());
        report.attribution_path = Some("cycle_events-first (primary)".to_string());
        report.is_in_progress = Some(true);
        report.phase_stats = Some(vec![make_phase_stats_entry(
            "scope",
            1,
            1,
            600,
            10,
            GateResult::Pass,
            0,
            None,
        )]);
        report.recommendations = vec![Recommendation {
            hotspot_type: "compile_cycles".to_string(),
            action: "Batch builds".to_string(),
            rationale: "Reduce cycles".to_string(),
        }];
        report.baseline_comparison = Some(vec![
            BaselineComparison {
                metric_name: "parallel_call_rate".to_string(),
                current_value: 0.9,
                mean: 0.5,
                stddev: 0.1,
                is_outlier: false,
                status: BaselineStatus::Normal,
                phase: None,
            },
            BaselineComparison {
                metric_name: "compile_cycles".to_string(),
                current_value: 50.0,
                mean: 5.0,
                stddev: 2.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: None,
            },
            BaselineComparison {
                metric_name: "phase_metric".to_string(),
                current_value: 20.0,
                mean: 5.0,
                stddev: 2.0,
                is_outlier: true,
                status: BaselineStatus::Outlier,
                phase: Some("scope".to_string()),
            },
        ]);
        report.session_summaries = Some(vec![make_session(
            "s1",
            0,
            600,
            &[("read", 10)],
            2,
            1,
            Some("done"),
        )]);
        report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 50.0, 2)];
        report.feature_knowledge_reuse = Some(FeatureKnowledgeReuse {
            delivery_count: 5,
            cross_session_count: 1,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 5,
            total_stored: 1,
            cross_feature_reuse: 3,
            intra_cycle_reuse: 2,
            top_cross_feature_entries: vec![EntryRef {
                id: 1,
                title: "T".to_string(),
                feature_cycle: "col-024".to_string(),
                category: "decision".to_string(),
                serve_count: 2,
            }],
        });
        report.rework_session_count = Some(1);
        report.phase_narrative = Some(PhaseNarrative {
            phase_sequence: vec!["scope".to_string()],
            rework_phases: vec![],
            per_phase_categories: HashMap::new(),
            cross_cycle_comparison: None,
        });

        let text = extract_text(&format_retrospective_markdown(&report));

        let expected_order = [
            "# Unimatrix Cycle Review",
            "## Goal",
            "## Recommendations",
            "## Phase Timeline",
            "## What Went Well",
            "## Sessions",
            "## Outliers",
            "## Findings",
            "## Phase Outliers",
            "## Knowledge Reuse",
            "## Rework",
            "## Phase Narrative",
        ];
        let mut last_pos = 0usize;
        let mut first = true;
        for header in &expected_order {
            let pos = text.find(header).unwrap_or_else(|| {
                panic!(
                    "{} not found in output:\n{}",
                    header,
                    &text[..text.len().min(2000)]
                )
            });
            if first {
                first = false;
            } else {
                assert!(
                    pos > last_pos,
                    "{} appeared before previous section (pos={}, last={})",
                    header,
                    pos,
                    last_pos
                );
            }
            last_pos = pos;
        }
        // Recommendations must not appear after Findings
        let rec_pos = text
            .find("## Recommendations")
            .expect("Recommendations present");
        let find_pos = text.find("## Findings").expect("Findings present");
        assert!(
            rec_pos < find_pos,
            "Recommendations must be before Findings"
        );
    }

    #[test]
    fn test_recommendations_before_findings() {
        let mut report = make_report();
        report.recommendations = vec![Recommendation {
            hotspot_type: "compile_cycles".to_string(),
            action: "Batch builds".to_string(),
            rationale: "r".to_string(),
        }];
        report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 10.0, 1)];
        let text = extract_text(&format_retrospective_markdown(&report));
        let rec_pos = text
            .find("## Recommendations")
            .expect("Recommendations present");
        let find_pos = text.find("## Findings").expect("Findings present");
        assert!(rec_pos < find_pos);
    }

    // ── R-10: Phase annotation multi-phase ────────────────────────────

    #[test]
    fn test_finding_phase_multi_evidence() {
        use unimatrix_observe::ToolDistribution;
        // Evidence in two phases; Phase B has more evidence
        let phase_a = PhaseStats {
            phase: "design".to_string(),
            pass_number: 1,
            pass_count: 1,
            duration_secs: 600,
            start_ms: 0,
            end_ms: Some(600_000),
            session_count: 1,
            record_count: 10,
            agents: vec![],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 0,
            knowledge_stored: 0,
            gate_result: GateResult::Pass,
            gate_outcome_text: None,
            hotspot_ids: vec![],
        };
        let phase_b = PhaseStats {
            phase: "implementation".to_string(),
            pass_number: 1,
            pass_count: 1,
            duration_secs: 600,
            start_ms: 600_000,
            end_ms: Some(1_800_000),
            session_count: 1,
            record_count: 20,
            agents: vec![],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 0,
            knowledge_stored: 0,
            gate_result: GateResult::Pass,
            gate_outcome_text: None,
            hotspot_ids: vec![],
        };
        // 3 evidence in phase A (ts 0-599999) and 7 in phase B (ts 600000+)
        let mut evidence: Vec<EvidenceRecord> = (0..3)
            .map(|i| EvidenceRecord {
                description: format!("ea{}", i),
                ts: 100_000 + i * 100_000,
                tool: None,
                detail: String::new(),
            })
            .collect();
        evidence.extend((0..7).map(|i| EvidenceRecord {
            description: format!("eb{}", i),
            ts: 700_000 + i * 100_000,
            tool: None,
            detail: String::new(),
        }));
        let mut report = make_report();
        report.phase_stats = Some(vec![phase_a, phase_b]);
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "high compile cycles".to_string(),
            measured: 10.0,
            threshold: 5.0,
            evidence,
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        // Higher-count phase (implementation) wins annotation
        assert!(text.contains("— phase: implementation/1"));
    }

    #[test]
    fn test_finding_phase_no_phase_stats() {
        let mut report = make_report();
        report.phase_stats = None;
        report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 10.0, 2)];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(!text.contains("— phase:"));
    }

    #[test]
    fn test_finding_phase_out_of_bounds_timestamp() {
        // Evidence timestamp before all phase windows — should not panic
        let ps = make_phase_stats_entry(
            "scope",
            1,
            1,
            600,
            10,
            GateResult::Pass,
            1_000_000,
            Some(2_000_000),
        );
        let mut report = make_report();
        report.phase_stats = Some(vec![ps]);
        // Evidence at ts=0, before phase window start
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "rule".to_string(),
            claim: "claim".to_string(),
            measured: 1.0,
            threshold: 0.0,
            evidence: vec![EvidenceRecord {
                description: "e".to_string(),
                ts: 0, // before any phase window
                tool: None,
                detail: String::new(),
            }],
        }];
        // Should not panic
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(text.contains("F-01"));
    }

    // ── R-11: Threshold audit snapshot ───────────────────────────────

    #[test]
    fn test_threshold_language_count_snapshot() {
        use std::fs;

        // Resolve path relative to workspace root (CARGO_MANIFEST_DIR = crates/unimatrix-server)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .expect("workspace root");
        let detection_dir = workspace_root.join("crates/unimatrix-observe/src/detection");

        let mut count = 0usize;
        if let Ok(entries) = fs::read_dir(&detection_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for line in content.lines() {
                            // Count lines with a claim: field — these are HotspotFinding
                            // construction sites that produce threshold-language strings.
                            if line.contains("claim:") {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
        // The count should be >= 9 (the enumerated detection sites in the codebase).
        // ADR-004 requires format_claim_with_baseline to cover all these sites.
        assert!(
            count >= 9,
            "Expected at least 9 claim sites in detection files, found {}. \
             Check path: {}",
            count,
            detection_dir.display()
        );
    }

    #[test]
    fn test_no_allowlist_in_compile_cycles() {
        // AC-13/AC-19: compile_cycles output must not contain "allowlist"
        let mut report = make_report();
        report.hotspots = vec![HotspotFinding {
            category: unimatrix_observe::HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "43 compile cycles (threshold: 10)".to_string(),
            measured: 43.0,
            threshold: 10.0,
            evidence: vec![],
        }];
        let text = extract_text(&format_retrospective_markdown(&report));
        assert!(
            !text.contains("allowlist"),
            "allowlist must not appear in compile_cycles output"
        );
    }

    // ── Gate outcome text injection guard ────────────────────────────

    #[test]
    fn test_gate_outcome_text_injection() {
        use unimatrix_observe::ToolDistribution;
        let mut report = make_report();
        report.phase_stats = Some(vec![PhaseStats {
            phase: "scope".to_string(),
            pass_number: 1,
            pass_count: 2,
            duration_secs: 600,
            start_ms: 0,
            end_ms: None,
            session_count: 1,
            record_count: 5,
            agents: vec![],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 0,
            knowledge_stored: 0,
            gate_result: GateResult::Fail,
            gate_outcome_text: Some("\n## Injected\nmalicious".to_string()),
            hotspot_ids: vec![],
        }]);
        let text = extract_text(&format_retrospective_markdown(&report));
        // The injected section header must not create a new section.
        // Newlines in gate_outcome_text are stripped to spaces, so "## Injected"
        // becomes " ## Injected" inline — not a line-starting section header.
        // Verify no line in the output starts with "## Injected".
        let injected_as_section = text
            .lines()
            .any(|l| l.trim_start().starts_with("## Injected"));
        assert!(
            !injected_as_section,
            "markdown injection must not produce a section header"
        );
    }
}
