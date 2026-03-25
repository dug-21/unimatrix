# Component 4: Formatter Overhaul

**File**: `crates/unimatrix-server/src/mcp/response/retrospective.rs`
**Action**: Modify — reorder sections, rebrand header, add Phase Timeline, What Went Well,
            burst notation, phase annotations, enhanced sessions table, extended Knowledge
            Reuse, threshold language post-processing via `format_claim_with_baseline`.

---

## Purpose

The formatter is the sole point of user-facing rendering. All new sections, new data, and
threshold language removal are implemented here only. Detection rules are not touched (ADR-004).
Section order is fixed by numbered comments in `format_retrospective_markdown` (NFR-05).

---

## New Import Requirements

The existing imports cover most needs. Additional:

```
use unimatrix_observe::{CycleEventRecord, EntryRef, GateResult, PhaseStats, ToolDistribution};
// Note: these come from the types.rs additions in Component 1
```

The regex crate is not currently a dependency of `unimatrix-server`. Two options:
1. Use the `regex` crate for threshold pattern matching — add to `Cargo.toml`.
2. Implement threshold stripping with `str::find` + manual substring search — no new dep.

RECOMMENDATION: Use `str::find` + manual extraction to avoid a new dependency. The pattern
`"threshold"` followed by optional `": "` and digits is simple enough for manual parsing.
See `format_claim_with_baseline` algorithm below.

---

## `format_retrospective_markdown` — New Section Order

Replace the existing function body with the new 12-section order. Each section is a numbered
comment per NFR-05:

```
pub fn format_retrospective_markdown(report: &RetrospectiveReport) -> CallToolResult
    let mut output = String::with_capacity(8192)

    // 1. Header (always rendered)
    output.push_str(&render_header(report))

    // 2. Recommendations (moved from position 9 — FR-12)
    if !report.recommendations.is_empty():
        output.push_str(&render_recommendations(&report.recommendations))

    // 3. Phase Timeline (new — FR-06/07/08) or "No phase information captured." line
    output.push_str(&render_phase_timeline(report))

    // 4. What Went Well (new — FR-11)
    if let Some(wgw) = render_what_went_well(report):
        output.push_str(&wgw)

    // 5. Sessions table (enhanced — FR-15/16)
    if let Some(summaries) = &report.session_summaries
        && !summaries.is_empty():
        output.push_str(&render_sessions(summaries))

    // 6. Attribution note (when partial attribution — existing)
    if let Some(attr) = &report.attribution
        && attr.attributed_session_count < attr.total_session_count:
        output.push_str(&render_attribution_note(attr))

    // 7. Baseline Outliers (universal — existing)
    if let Some(comparisons) = &report.baseline_comparison:
        let universal_outliers: Vec<&BaselineComparison> = comparisons.iter()
            .filter(|c| c.phase.is_none())
            .filter(|c| matches!(c.status, BaselineStatus::Outlier | BaselineStatus::NewSignal))
            .collect()
        if !universal_outliers.is_empty():
            output.push_str(&render_baseline_outliers(&universal_outliers))

    // 8. Findings (enhanced — FR-09/10/14)
    if !report.hotspots.is_empty():
        output.push_str(&render_findings(
            &report.hotspots,
            report.narratives.as_deref(),
            report.phase_stats.as_deref(),
            report.baseline_comparison.as_deref(),
            report.session_summaries.as_deref(),
        ))

    // 9. Phase Outliers (existing)
    if let Some(comparisons) = &report.baseline_comparison:
        let phase_outliers: Vec<&BaselineComparison> = comparisons.iter()
            .filter(|c| c.phase.is_some())
            .filter(|c| matches!(c.status, BaselineStatus::Outlier | BaselineStatus::NewSignal))
            .filter(|c| !is_zero_activity_phase(
                c.phase.as_deref().unwrap_or(""), &report.metrics.phases))
            .collect()
        if !phase_outliers.is_empty():
            output.push_str(&render_phase_outliers(&phase_outliers))

    // 10. Knowledge Reuse (extended — FR-13)
    if let Some(reuse) = &report.feature_knowledge_reuse:
        output.push_str(&render_knowledge_reuse(reuse, &report.feature_cycle))

    // 11. Rework & Context Reload (existing)
    let has_rework = report.rework_session_count.is_some_and(|n| n > 0)
    let has_reload = report.context_reload_pct.is_some()
    if has_rework || has_reload:
        output.push_str(&render_rework_reload(report.rework_session_count, report.context_reload_pct))

    // 12. Phase Narrative (existing — crt-025)
    if let Some(narrative) = &report.phase_narrative:
        output.push_str(&render_phase_narrative(narrative))

    CallToolResult::success(vec![Content::text(output)])
```

---

## Section 1: `render_header` — Rebranded

```
fn render_header(report: &RetrospectiveReport) -> String
    let mut out = String::new()

    // Branding line — FR-01
    out.push_str(&format!("# Unimatrix Cycle Review — {}\n\n", report.feature_cycle))

    // Goal line — FR-02 (omit entirely when None)
    if let Some(goal) = &report.goal:
        // Security: strip embedded newlines to prevent markdown injection (R security note)
        let safe_goal = goal.replace('\n', " ").replace('\r', " ")
        out.push_str(&format!("**Goal**: {}\n", safe_goal))

    // Cycle type and attribution line — FR-03, FR-04
    let cycle_type = report.cycle_type.as_deref().unwrap_or("Unknown")
    let has_cycle_line = report.attribution_path.is_some() || report.is_in_progress.is_some()

    // Build the meta line pieces
    let mut meta_parts: Vec<String> = vec![]
    meta_parts.push(format!("**Cycle type**: {}", cycle_type))

    if let Some(path) = &report.attribution_path:
        meta_parts.push(format!("**Attribution**: {}", path))

    // Status line — FR-05: only Some(true)
    if report.is_in_progress == Some(true):
        meta_parts.push("**Status**: IN PROGRESS".to_string())
    // Some(false) or None: no Status in output

    if !meta_parts.is_empty():
        out.push_str(&meta_parts.join("  |  "))
        out.push('\n')

    // Summary line (existing fields)
    let duration = format_duration(report.metrics.universal.total_duration_secs)
    out.push_str(&format!(
        "**Sessions**: {}  |  **Records**: {}  |  **Duration**: {}\n",
        report.session_count, report.total_records, duration,
    ))

    out.push_str("\n---\n\n")
    out
```

---

## Section 3: `render_phase_timeline` — New

```
fn render_phase_timeline(report: &RetrospectiveReport) -> String
    let phase_stats = match &report.phase_stats:
        None => return "No phase information captured.\n\n".to_string()
        Some(ps) if ps.is_empty() => return "No phase information captured.\n\n".to_string()
        Some(ps) => ps

    let mut out = String::new()
    out.push_str("## Phase Timeline\n\n")
    out.push_str("| Phase | Duration | Passes | Records | Agents | Knowledge | Gate |\n")
    out.push_str("|-------|----------|--------|---------|--------|-----------|------|\n")

    // Build hotspot phase map first (populate hotspot_ids on each PhaseStats)
    // The formatter populates hotspot_ids here by building the inverse map.
    // We cannot mutate report.phase_stats (it's a &), so build a local hotspot_phase map.
    // phase_annotation_map: maps finding_index (0-based) → (phase: &str, pass_number: u32)
    let phase_annotation_map = build_phase_annotation_map(phase_stats, &report.hotspots)

    for ps in phase_stats:
        let phase_display = if ps.phase.is_empty() { "—" } else { &ps.phase }
        let duration = format_duration(ps.duration_secs)
        let passes = if ps.pass_count > 1 {
            format!("**{}**", ps.pass_count)
        } else {
            ps.pass_count.to_string()
        }
        let agents_str = if ps.agents.is_empty() {
            "—".to_string()
        } else {
            ps.agents.join(", ")
        }
        let knowledge = format!("{}↓ {}↑", ps.knowledge_served, ps.knowledge_stored)
        let gate_str = match ps.gate_result:
            GateResult::Pass    => "PASS"
            GateResult::Fail    => "FAIL"
            GateResult::Rework  => "PASS (rework)"
            GateResult::Unknown => "UNKNOWN"

        let _ = writeln!(out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            phase_display, duration, passes, ps.record_count, agents_str, knowledge, gate_str
        )

    // Rework annotations (FR-07): one line per reworked phase
    // Track which phase names we have already emitted (avoid duplicates)
    let mut rework_emitted: HashSet<&str> = HashSet::new()
    for ps in phase_stats:
        if ps.pass_count > 1 && ps.pass_number == 1 && !rework_emitted.contains(ps.phase.as_str()):
            rework_emitted.insert(&ps.phase)
            // Find pass 2 stats for this phase name
            let pass2_opt = phase_stats.iter().find(|p| p.phase == ps.phase && p.pass_number == 2)
            let outcome_text = ps.gate_outcome_text.as_deref().unwrap_or("(unknown)")
            // Security: strip newlines from outcome text (R security note)
            let safe_outcome = outcome_text.replace('\n', " ").replace('\r', " ")

            let pass2_info = match pass2_opt:
                Some(p2) =>
                    format!("Pass 2: {}, {} records", format_duration(p2.duration_secs), p2.record_count)
                None => "Pass 2: (no data)".to_string()

            out.push_str(&format!(
                "\n**Rework**: {} — pass 1 gate {}: {}. {}\n",
                ps.phase,
                match ps.gate_result { GateResult::Fail => "fail", _ => "result" },
                safe_outcome,
                pass2_info
            ))

    // Top file zones (FR-08): aggregate across all sessions
    if let Some(summaries) = &report.session_summaries:
        let top_zones = compute_top_file_zones(summaries, 5)
        if !top_zones.is_empty():
            let zones_str: Vec<String> = top_zones.iter()
                .map(|(path, count)| format!("{} ({})", path, count))
                .collect()
            out.push_str(&format!("\nTop file zones: {}\n", zones_str.join(", ")))

    out.push('\n')
    out
```

### `build_phase_annotation_map` helper

Builds a map from finding_index to (phase_name, pass_number) by looking at which phase
window contains each finding's earliest evidence timestamp.

```
fn build_phase_annotation_map(
    phase_stats: &[PhaseStats],
    hotspots: &[HotspotFinding],
) -> HashMap<usize, (String, u32)>
    // hotspots is the raw slice; collapse order matches the final rendering order.
    // We use a simple per-rule approach: for each distinct rule_name in encounter order,
    // find its earliest evidence timestamp, then find which phase window contains it.

    let mut map: HashMap<usize, (String, u32)> = HashMap::new()

    // Build ordered rule list (mirrors collapse_findings order)
    let mut seen_rules: Vec<&str> = vec![]
    for h in hotspots:
        if !seen_rules.contains(&h.rule_name.as_str()):
            seen_rules.push(&h.rule_name)

    for (finding_idx, rule_name) in seen_rules.iter().enumerate():
        // Collect all evidence timestamps for this rule
        let mut min_ts: Option<u64> = None
        for h in hotspots.iter().filter(|h| h.rule_name == *rule_name):
            for ev in &h.evidence:
                min_ts = Some(match min_ts:
                    None => ev.ts
                    Some(prev) => prev.min(ev.ts)
                )

        if let Some(earliest_ts) = min_ts:
            // Find phase window containing earliest_ts
            // Phase windows are defined by consecutive (start_ms, end_ms) pairs
            // We use (pass_number, phase) pairs from phase_stats ordered by... sequence.
            // NOTE: phase_stats does not carry absolute start_ms/end_ms.
            // The formatter cannot reconstruct exact boundaries without re-reading events.
            //
            // ALTERNATIVE APPROACH (simpler): count evidence per phase window by checking
            // which phases have hotspot_ids set. But hotspot_ids is populated here.
            //
            // CORRECT APPROACH: use the phase annotation by finding which phase has the
            // most evidence for this rule. Count how many observations per phase the
            // rule's evidence timestamps fall into. But phase windows are time-based,
            // and phase_stats only stores aggregate counts, not boundaries.
            //
            // PRACTICAL APPROACH per FR-09 / R-10:
            // "When the finding fired across multiple phases, use the phase with the
            // highest event count for the annotation."
            //
            // Since phase_stats doesn't carry window boundaries, and the formatter
            // doesn't have access to the original cycle_events, we must use a proxy:
            // map the earliest evidence timestamp to a phase by using the knowledge
            // that phase_stats[i].duration_secs accumulates, and comparing relative
            // timestamps to session-start relative positions.
            //
            // FLAGGED GAP: The formatter cannot determine phase boundaries from
            // PhaseStats alone (duration_secs alone is insufficient). The phase_stats
            // computation in Component 2 should add `start_ms` and `end_ms` fields
            // to PhaseStats, OR the formatter must receive the raw events slice.
            //
            // RESOLUTION for implementation: Add `start_ms: i64` and `end_ms: Option<i64>`
            // fields to PhaseStats (local to the server crate, not exposed in unimatrix-observe
            // public API). This is not in the current IMPLEMENTATION-BRIEF.
            // Until resolved: use a fallback that maps the finding to the first non-empty
            // phase (phase with record_count > 0) as a best-effort annotation.
            //
            // If start_ms/end_ms ARE added to PhaseStats:
            for ps in phase_stats:
                let window_end = ps.end_ms.unwrap_or(i64::MAX)  // hypothetical field
                if earliest_ts as i64 >= ps.start_ms && (earliest_ts as i64) < window_end:
                    map.insert(finding_idx, (ps.phase.clone(), ps.pass_number))
                    break

    map
```

OPEN GAP: `PhaseStats` as defined in Component 1 does not include `start_ms`/`end_ms`.
The formatter needs these to map finding timestamps to phases. Options:
1. Add `start_ms: i64` and `end_ms: Option<i64>` to `PhaseStats` — minor struct extension.
2. Pass the raw `events: &[CycleEventRecord]` to the formatter — requires changing
   `format_retrospective_markdown` signature.
3. Use a fallback heuristic (least accurate).

RECOMMENDATION: Add `start_ms` and `end_ms` to `PhaseStats` in Component 1.
Flag this gap to the implementation agent as a required addition.

### `compute_top_file_zones` helper

```
fn compute_top_file_zones(summaries: &[SessionSummary], n: usize) -> Vec<(String, u64)>
    let mut zone_totals: HashMap<String, u64> = HashMap::new()
    for s in summaries:
        for (path, count) in &s.top_file_zones:
            *zone_totals.entry(path.clone()).or_insert(0) += count

    let mut zones: Vec<(String, u64)> = zone_totals.into_iter().collect()
    zones.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)))
    zones.truncate(n)
    zones
```

---

## Section 4: `render_what_went_well` — New

```
fn render_what_went_well(report: &RetrospectiveReport) -> Option<String>
    let comparisons = report.baseline_comparison.as_deref()?
    if comparisons.is_empty():
        return None

    // Metric direction table (SPEC §FR-11 — 16 entries)
    // true = higher is better; false = lower is better
    let direction_table: HashMap<&str, bool> = [
        ("compile_cycles",                    false),
        ("permission_friction_events",        false),
        ("bash_for_search_count",             false),
        ("reread_rate",                       false),
        ("coordinator_respawn_count",         false),
        ("sleep_workaround_count",            false),
        ("post_completion_work_pct",          false),
        ("context_load_before_first_write_kb", false),
        ("file_breadth",                      false),
        ("mutation_spread",                   false),
        ("cold_restart_count",                false),
        ("task_rework_count",                 false),
        ("edit_bloat_kb",                     false),
        ("parallel_call_rate",                true),
        ("knowledge_entries_stored",          true),
        ("follow_up_issues_created",          true),
    ].into_iter().collect()

    // Plain-text labels per metric
    let labels: HashMap<&str, &str> = [
        ("compile_cycles",                    "low compile overhead across the cycle"),
        ("permission_friction_events",        "low friction outside compile bursts"),
        ("bash_for_search_count",             "Grep/Glob used correctly throughout"),
        ("reread_rate",                       "files read efficiently without repeated reads"),
        ("coordinator_respawn_count",         "no SM context loss"),
        ("sleep_workaround_count",            "no polling hacks"),
        ("post_completion_work_pct",          "clean stop after gate"),
        ("context_load_before_first_write_kb", "context loaded within budget"),
        ("file_breadth",                      "focused file surface"),
        ("mutation_spread",                   "mutations concentrated in target files"),
        ("cold_restart_count",               "no cold restarts needed"),
        ("task_rework_count",                "tasks completed without rework"),
        ("edit_bloat_kb",                    "edits within expected size"),
        ("parallel_call_rate",               "above-average concurrency across sessions"),
        ("knowledge_entries_stored",         "strong knowledge contribution this cycle"),
        ("follow_up_issues_created",         "issues captured for future work"),
    ].into_iter().collect()

    let mut favorable: Vec<String> = vec![]

    for c in comparisons:
        // Must be universal (no phase filter for What Went Well)
        if c.phase.is_some():
            continue
        // Must be Normal status (not Outlier/NewSignal/NoVariance)
        if !matches!(c.status, BaselineStatus::Normal):
            continue
        // Must be in direction table
        let higher_is_better = match direction_table.get(c.metric_name.as_str()):
            None => continue   // not in table, skip
            Some(&b) => b

        // Must be favorable
        let is_favorable = if higher_is_better {
            c.current_value > c.mean
        } else {
            c.current_value < c.mean
        }
        if !is_favorable:
            continue

        // Get label
        let label = labels.get(c.metric_name.as_str()).copied().unwrap_or("favorable signal")

        favorable.push(format!(
            "- **{}**: {:.1} vs mean {:.1} — {}",
            c.metric_name, c.current_value, c.mean, label
        ))

    if favorable.is_empty():
        return None

    let mut out = String::new()
    out.push_str("## What Went Well\n\n")
    for line in &favorable:
        out.push_str(line)
        out.push('\n')
    out.push('\n')
    Some(out)
```

---

## Section 5: `render_sessions` — Enhanced

Add `Tools` and `Agents` columns (FR-15). Add Top file zones line (FR-16).

```
fn render_sessions(summaries: &[SessionSummary]) -> String
    let mut out = String::new()
    out.push_str("## Sessions\n")
    out.push_str("| # | Window | Calls | Tools | Agents | Knowledge | Outcome |\n")
    out.push_str("|---|--------|-------|-------|--------|-----------|--------|\n")

    for (i, s) in summaries.iter().enumerate():
        let idx = i + 1
        let start_secs = s.started_at / 1000
        let hours = (start_secs % 86400) / 3600
        let minutes = (start_secs % 3600) / 60
        let dur_min = s.duration_secs / 60
        let window = format!("{:02}:{:02} ({}m)", hours, minutes, dur_min)
        let calls: u64 = s.tool_distribution.values().sum()

        // Tools column: NR NE NW NS format (FR-15)
        let r = s.tool_distribution.get("read").copied().unwrap_or(0)
        let e = s.tool_distribution.get("execute").copied().unwrap_or(0)
        let w = s.tool_distribution.get("write").copied().unwrap_or(0)
        let sr = s.tool_distribution.get("search").copied().unwrap_or(0)
        let mut tools_parts: Vec<String> = vec![]
        if r > 0: tools_parts.push(format!("{}R", r))
        if e > 0: tools_parts.push(format!("{}E", e))
        if w > 0: tools_parts.push(format!("{}W", w))
        if sr > 0: tools_parts.push(format!("{}S", sr))
        let tools_str = if tools_parts.is_empty() { "—".to_string() } else { tools_parts.join(" ") }

        // Agents column (FR-15)
        let agents_str = if s.agents_spawned.is_empty():
            "—".to_string()
        else if s.agents_spawned.len() <= 3:
            s.agents_spawned.join(", ")
        else:
            let first3 = s.agents_spawned[..3].join(", ")
            format!("{} +{} more", first3, s.agents_spawned.len() - 3)

        let knowledge = format!("{} served, {} stored", s.knowledge_served, s.knowledge_stored)
        let outcome = s.outcome.as_deref().unwrap_or("-")

        let _ = writeln!(out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            idx, window, calls, tools_str, agents_str, knowledge, outcome
        )

    // Top file zones (FR-16): aggregated across all sessions
    let top_zones = compute_top_file_zones(summaries, 5)
    if !top_zones.is_empty():
        let zones_str: Vec<String> = top_zones.iter()
            .map(|(path, count)| format!("{} ({})", path, count))
            .collect()
        out.push_str(&format!("\nTop file zones: {}\n", zones_str.join(", ")))

    out.push('\n')
    out
```

---

## Section 8: `render_findings` — Enhanced Signature + Burst Notation + Phase Annotations

```
fn render_findings(
    hotspots: &[HotspotFinding],
    narratives: Option<&[HotspotNarrative]>,
    phase_stats: Option<&[PhaseStats]>,         // NEW
    baseline_comparison: Option<&[BaselineComparison]>,  // NEW
    session_summaries: Option<&[SessionSummary]>,         // NEW (for cycle_start time)
) -> String

    let collapsed = collapse_findings(hotspots, narratives)
    if collapsed.is_empty():
        return String::new()

    // Build phase annotation map if phase_stats available
    let annotation_map: HashMap<usize, (String, u32)> = match phase_stats:
        Some(ps) => build_phase_annotation_map(ps, hotspots)
        None => HashMap::new()

    // Cycle start time for relative burst notation
    let cycle_start_ms: u64 = session_summaries
        .unwrap_or(&[])
        .iter()
        .filter(|s| s.started_at > 0)
        .map(|s| s.started_at)
        .min()
        .unwrap_or(0)

    let mut out = String::new()
    let _ = writeln!(out, "## Findings ({})", collapsed.len())

    for (i, finding) in collapsed.iter().enumerate():
        let id = format!("F-{:02}", i + 1)
        let severity_tag = match finding.severity:
            Severity::Critical => "critical"
            Severity::Warning  => "warning"
            Severity::Info     => "info"

        // Phase annotation (FR-09)
        let phase_annotation = match annotation_map.get(&i):
            Some((phase, pass)) => format!(" — phase: {}/{}", phase, pass)
            None => String::new()

        // Finding header — use narrative_summary or first claim
        // (threshold language NOT applied to the header — it's in the claim below)
        let description = match &finding.narrative_summary:
            Some(s) => s.as_str()
            None => finding.claims.first().map_or("", |c| c.as_str())

        let _ = writeln!(out,
            "### {} [{}] {}{}",
            id, severity_tag, description, phase_annotation
        )

        // Claim with threshold language replacement (FR-14)
        if let Some(claim) = finding.claims.first():
            let baseline_entry = baseline_comparison
                .unwrap_or(&[])
                .iter()
                .find(|b| b.metric_name == finding.rule_name && b.phase.is_none())
            let rendered_claim = format_claim_with_baseline(
                claim,
                &finding.rule_name,
                finding.measured,
                finding.threshold,
                baseline_entry,
            )
            let _ = writeln!(out, "{}", rendered_claim)

        // Tool breakdown (existing)
        if !finding.tool_breakdown.is_empty():
            let breakdown: Vec<String> = finding.tool_breakdown.iter()
                .map(|(tool, count)| format!("{}({})", tool, count))
                .collect()
            let _ = writeln!(out, "{}", breakdown.join(", "))

        // Burst notation (FR-10)
        let burst_str = render_burst_notation(
            finding,
            cycle_start_ms,
        )
        if !burst_str.is_empty():
            out.push_str(&burst_str)

        out.push('\n')

    out
```

---

## `format_claim_with_baseline` — Threshold Language Replacement (ADR-004)

```
fn format_claim_with_baseline(
    claim: &str,
    rule_name: &str,
    measured: f64,
    threshold: f64,
    baseline: Option<&BaselineComparison>,
) -> String
    // Find threshold pattern in claim: "threshold" followed by optional ": " and digits
    // Pattern: r"threshold[:\s]+[\d.]+"  (case-insensitive)
    let lower_claim = claim.to_lowercase()
    let threshold_pos = lower_claim.find("threshold")

    let stripped_claim = match threshold_pos:
        None => return claim.to_string()   // no threshold language; emit unchanged
        Some(pos) =>
            // Find end of threshold-containing phrase
            // Walk forward from pos to find: "threshold" + optional ":" + optional " " + digits
            let after_keyword = &claim[pos + "threshold".len()..]
            // Skip ": " or " " separator
            let after_sep = after_keyword.trim_start_matches(|c: char| c == ':' || c == ' ')
            // Skip digits and decimal point
            let end_of_num = after_sep.char_indices()
                .take_while(|(_, c)| c.is_ascii_digit() || *c == '.')
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0)
            // The full matched span: from pos to pos + "threshold".len() + sep_len + end_of_num
            // Strip this span from the claim
            let span_start = pos
            let span_len = "threshold".len()
                + (after_keyword.len() - after_sep.len())
                + end_of_num
            let stripped = format!("{}{}", &claim[..span_start], &claim[span_start + span_len..].trim_start())
            // Clean up leading/trailing punctuation artefacts (e.g., " -- " left behind)
            stripped.trim_end_matches(|c: char| c == ' ' || c == '-').to_string()

    // Append framing
    let framing = match baseline:
        Some(b) if b.stddev > 0.0 =>
            let zscore = if b.stddev > 0.0 { (measured - b.mean) / b.stddev } else { 0.0 }
            format!(" (baseline: {:.1} ±{:.1}, +{:.1}σ)", b.mean, b.stddev, zscore)
        _ if threshold > 0.0 =>
            let ratio = measured / threshold
            format!(" ({:.1}× typical)", ratio)
        _ =>
            String::new()   // threshold = 0.0: skip ratio (no division by zero)

    format!("{}{}", stripped_claim, framing)
```

---

## `render_burst_notation` — Burst Notation for Evidence (FR-10)

```
fn render_burst_notation(finding: &CollapsedFinding, cycle_start_ms: u64) -> String
    // Collect all evidence timestamps across all claims
    let evidence_pool = &finding.examples   // already sorted by ts (from collapse_findings)

    if evidence_pool.is_empty():
        return String::new()

    // Use narratives clusters if available (already computed in step 10e)
    if let Some(cluster_count) = finding.cluster_count:
        // Clusters are in finding.narratives — but CollapsedFinding only stores cluster_count.
        // We need the actual cluster data. This requires extending CollapsedFinding to include
        // the raw clusters from HotspotNarrative.clusters.
        // FLAGGED GAP: CollapsedFinding currently does not store Vec<EvidenceCluster>.
        // If clusters are stored: use them directly. Otherwise fall back to raw evidence.
        ()

    // Fallback: group raw evidence by 5-minute buckets (FR-10)
    let origin_ts = if cycle_start_ms > 0 { cycle_start_ms } else {
        evidence_pool.first().map(|e| e.ts).unwrap_or(0)
    }

    // Group evidence into 5-minute buckets
    // Bucket key = (relative_ts_ms / 300_000) — integer division gives 5-min bucket index
    let mut buckets: BTreeMap<u64, Vec<&EvidenceRecord>> = BTreeMap::new()
    for ev in evidence_pool:
        let rel_ms = ev.ts.saturating_sub(origin_ts)
        let bucket_key = rel_ms / 300_000   // 5 min = 300,000 ms
        buckets.entry(bucket_key).or_default().push(ev)

    if buckets.is_empty():
        return String::new()

    // Find peak bucket
    let peak_bucket_key = buckets.iter()
        .max_by_key(|(_, evs)| evs.len())
        .map(|(k, _)| *k)
        .unwrap_or(0)

    // Build Timeline line (max 10 entries)
    let mut timeline_parts: Vec<String> = vec![]
    let bucket_entries: Vec<(&u64, &Vec<&EvidenceRecord>)> = buckets.iter().collect()
    let total_buckets = bucket_entries.len()

    for (bucket_key, evs) in bucket_entries.iter().take(10):
        let rel_min = **bucket_key * 5   // each bucket = 5 minutes
        let count = evs.len()
        let peak_marker = if **bucket_key == peak_bucket_key { "▲" } else { "" }
        timeline_parts.push(format!("+{}m({}{})", rel_min, count, peak_marker))

    let timeline_str = if total_buckets > 10:
        let last_key = bucket_entries.last().map(|(k, _)| **k).unwrap_or(0)
        let last_count = bucket_entries.last().map(|(_, evs)| evs.len()).unwrap_or(0)
        format!("{} ... +{}m({})", timeline_parts.join(" "), last_key * 5, last_count)
    else:
        timeline_parts.join(" ")

    // Build Peak line
    let peak_evs = &buckets[&peak_bucket_key]
    let peak_rel_min = peak_bucket_key * 5
    let peak_file_counts: HashMap<String, u32> = {
        let mut fc = HashMap::new()
        for ev in peak_evs:
            if let Some(tool) = &ev.tool:
                *fc.entry(tool.clone()).or_insert(0) += 1
        fc
    }
    let mut top_files: Vec<(String, u32)> = peak_file_counts.into_iter().collect()
    top_files.sort_by(|a, b| b.1.cmp(&a.1))
    top_files.truncate(3)
    let files_str = if top_files.is_empty() {
        String::new()
    } else {
        format!(" — {}", top_files.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>().join(", "))
    }

    format!(
        "Timeline: {}\nPeak: {} events in 5min at +{}m{}\n",
        timeline_str, peak_evs.len(), peak_rel_min, files_str
    )
```

FLAGGED GAP: `CollapsedFinding` must store `raw_narratives_clusters: Option<Vec<EvidenceCluster>>`
to use the pre-computed cluster data. Without this, burst notation falls back to raw evidence
bucketing, which is correct per FR-10 but less precise than the narrative cluster path.
Implementation agent may add this field to `CollapsedFinding` (a private type in retrospective.rs).

---

## Section 10: `render_knowledge_reuse` — Extended

```
fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse, feature_cycle: &str) -> String
    let mut out = String::new()
    out.push_str("## Knowledge Reuse\n\n")

    if reuse.delivery_count == 0:
        out.push_str("No knowledge entries served.\n\n")
        return out

    // Summary line
    out.push_str(&format!(
        "**Total served**: {}  |  **Stored this cycle**: {}\n\n",
        reuse.delivery_count, reuse.total_stored
    ))

    // Bucket table
    out.push_str("| Bucket | Count |\n")
    out.push_str("|--------|-------|\n")
    out.push_str(&format!(
        "| Cross-feature (prior cycles) | {} |\n", reuse.cross_feature_reuse
    ))
    out.push_str(&format!(
        "| Intra-cycle ({} entries) | {} |\n", feature_cycle, reuse.intra_cycle_reuse
    ))
    out.push('\n')

    // By category line
    if !reuse.by_category.is_empty():
        let mut cats: Vec<(&String, &u64)> = reuse.by_category.iter().collect()
        cats.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)))
        let cat_parts: Vec<String> = cats.iter()
            .map(|(cat, count)| format!("{}×{}", cat, count))
            .collect()
        out.push_str(&format!(
            "**By category (all {} served)**: {}\n\n",
            reuse.delivery_count, cat_parts.join(", ")
        ))

    // Top cross-feature entries table (omit when empty)
    if !reuse.top_cross_feature_entries.is_empty():
        out.push_str("**Top cross-feature entries**:\n\n")
        out.push_str("| Entry | Type | Served | Source |\n")
        out.push_str("|-------|------|--------|--------|\n")
        for entry in &reuse.top_cross_feature_entries:
            // Security: escape pipe characters in title (R security note)
            let safe_title = entry.title.replace('|', "\\|")
            let _ = writeln!(out,
                "| `#{}` {} | {} | {}× | {} |",
                entry.id, safe_title, entry.category, entry.serve_count, entry.feature_cycle
            )
        out.push('\n')

    // category_gaps: NOT rendered (AC-12, SCOPE decision)
    // cross_session_count: NOT rendered (superseded by bucket split)

    out
```

---

## Key Test Scenarios

**T-FM-01** (R-07): Section order — all 12 headers in sequence
- Full-report test with all sections populated
- Assert each section header string appears after the previous one in output
- Assert "## Recommendations" appears before "## Phase Timeline"
- Assert "# Unimatrix Cycle Review" appears, "# Retrospective:" does not appear

**T-FM-02** (AC-01): Header branding
- Assert output.starts_with("# Unimatrix Cycle Review —")
- Assert "# Retrospective:" absent

**T-FM-03** (AC-02): Goal line absent when None
- `goal = None` → "Goal" not in output

**T-FM-04** (AC-02): Goal line present when Some
- `goal = Some("test goal")` → output contains "**Goal**: test goal"

**T-FM-05** (AC-05): Status line
- `is_in_progress = Some(true)` → output contains "**Status**: IN PROGRESS"
- `is_in_progress = Some(false)` → "Status" not in output
- `is_in_progress = None` → "Status" not in output

**T-FM-06** (AC-06): Phase Timeline present with correct columns
- `phase_stats = Some(vec![one_phase])` → "## Phase Timeline" in output
- Assert table contains PASS/FAIL/UNKNOWN column value

**T-FM-07** (AC-07): Rework annotation
- Phase with pass_count=2 → "**Rework**:" line below table

**T-FM-08** (AC-09): Burst notation, no ts= epoch values
- Finding with 5 evidence records → output contains "Timeline:" and "Peak:"
- Assert "ts=" not in output

**T-FM-09** (AC-10): What Went Well appears/absent
- One favorable Normal metric → section present
- All metrics unfavorable or Outlier → section absent

**T-FM-10** (AC-13, R-08): Threshold language eliminated
- Finding with claim containing "threshold: 10" → output contains no "threshold: 10"
- With baseline: output contains "(baseline: mean ±stddev, +zscore σ)"
- Without baseline, threshold > 0: output contains "(Nx typical)"
- threshold = 0.0: no ratio annotation (no "inf× typical")
- Claim with no threshold pattern: emitted unchanged

**T-FM-11** (AC-12): Knowledge Reuse shows new fields, category_gaps absent
- Populated FeatureKnowledgeReuse → "Total served", "Stored this cycle", bucket table present
- "Gaps:" not in output

**T-FM-12** (AC-14): Sessions table Tools and Agents columns
- Session with tool_distribution {"read":5, "execute":2} → "5R 2E" in output
- agents_spawned with 4 agents → "agent1, agent2, agent3 +1 more" in output

**T-FM-13** (AC-15): Top file zones line
- Sessions with top_file_zones populated → "Top file zones:" in output

**T-FM-14** (AC-13): allowlist absent from compile_cycles output
- compile_cycles finding rendered → "allowlist" not in output (also covered by R-07 in recommendation-fix)

**T-FM-15** (R-10): Phase annotation multi-phase tie-breaking
- Finding evidence spans two phases; higher-count phase wins annotation
- Finding with no phase_stats → header without "— phase:" suffix

**T-FM-16**: Phase Timeline empty/None case
- `phase_stats = None` → output contains "No phase information captured." (single line, no header)
- `phase_stats = Some(vec![])` → same output
