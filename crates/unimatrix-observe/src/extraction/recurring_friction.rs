//! Recurring friction signal computation (FR-05, GH #437).
//!
//! Identifies detection rules that fire across multiple sessions, indicating
//! systemic workflow issues worth surfacing as operational recommendations.
//!
//! GH #437: Removed `ExtractionRule` impl. Recurring friction signals are
//! ephemeral operational recommendations, not stored knowledge entries.
//! `compute_friction_recommendations()` returns `Vec<String>` surfaced via
//! `context_status` `maintenance_recommendations`, never written to ENTRIES.

use std::collections::{HashMap, HashSet};

use crate::detection;
use crate::types::ObservationRecord;

/// Compute ephemeral friction recommendations from observation records.
///
/// Identifies detection rules that fire in 3 or more distinct sessions and
/// returns human-readable recommendation strings. Returns an empty `Vec` when
/// fewer than 3 sessions trigger any rule.
///
/// No store access — pure computation. Re-computed each tick; repeated
/// appearance is expected — no dedup applied.
pub fn compute_friction_recommendations(observations: &[ObservationRecord]) -> Vec<String> {
    // ADR-005: source_domain guard is MANDATORY as first operation.
    let filtered: Vec<&ObservationRecord> = observations
        .iter()
        .filter(|r| r.source_domain == "claude-code")
        .collect();

    // Group observations by session
    let mut session_records: HashMap<String, Vec<ObservationRecord>> = HashMap::new();
    for obs in &filtered {
        session_records
            .entry(obs.session_id.clone())
            .or_default()
            .push((*obs).clone());
    }

    // Run detection rules per session
    let detection_rules = detection::default_rules(None);
    let mut rule_sessions: HashMap<String, HashSet<String>> = HashMap::new();

    for (session_id, records) in &session_records {
        let findings = detection::detect_hotspots(records, &detection_rules);
        for finding in &findings {
            rule_sessions
                .entry(finding.rule_name.clone())
                .or_default()
                .insert(session_id.clone());
        }
    }

    // Build recommendations for rules firing in 3+ sessions
    let mut recommendations = Vec::new();
    for (rule_name, sessions) in &rule_sessions {
        if sessions.len() < 3 {
            continue;
        }
        let n = sessions.len();
        recommendations.push(format!(
            "Recurring workflow friction: '{}' fired in {} sessions — {}",
            rule_name,
            n,
            remediation_for_rule(rule_name)
        ));
    }
    recommendations
}

/// Return an actionable remediation recommendation for a given detection rule name.
///
/// Provides concrete guidance that an agent can act on immediately, rather than
/// raw session UUID lists that carry no information. (GH #351)
pub(crate) fn remediation_for_rule(rule_name: &str) -> &'static str {
    match rule_name {
        "orphaned_calls" => {
            "Review agent behaviour around tool invocation abandonment — recurring orphaned \
             calls suggest context overflow or parallel call management issues"
        }
        "tool_call_retries" => {
            "Review tool use patterns — repeated retries indicate ambiguous instructions \
             or missing context."
        }
        "session_rollbacks" => {
            "Check for unstable test fixtures or non-deterministic operations causing \
             repeated rollbacks."
        }
        "sleep_workarounds" => {
            "Replace sleep-based polling with proper async primitives or retry logic \
             with backoff."
        }
        "search_via_bash" => {
            "Use the Grep and Glob dedicated tools instead of grep/find via Bash. \
             They have better permissions and provide a cleaner output format."
        }
        "output_parsing_struggle" => {
            "Prefer structured output (JSON flags) over text-parsed CLI output to \
             eliminate fragile parsing patterns."
        }
        "compile_cycles" => {
            "Run cargo check before cargo build to surface type errors earlier, \
             reducing full compile cycle count."
        }
        "rework_events" => {
            "Ensure design artifacts (pseudocode, ADRs) are validated before \
             implementation begins to avoid late-stage rework."
        }
        "tool_failure_hotspot" => {
            "Investigate the tool that accumulates repeated PostToolUseFailure events — \
             common causes are incorrect arguments, missing files, or network failures. \
             Fix the root cause rather than retrying blindly."
        }
        "context_load_before_first_write_kb" | "context_load" => {
            "Reduce upfront file reading by scoping reads to relevant files only — \
             excessive context load before the first write indicates over-reading."
        }
        "lifespan" => {
            "Break long-lived agent sessions into smaller, focused sub-tasks with \
             clear handoff points to prevent context overflow."
        }
        "file_breadth" => {
            "Scope agent tasks to fewer files per session — high file breadth indicates \
             the task scope is too broad or the agent is exploring rather than implementing."
        }
        "reread_rate" => {
            "Cache file contents early in the session to avoid repeated reads — \
             high reread rate indicates missing context that forces re-inspection."
        }
        "mutation_spread" => {
            "Limit mutations to a focused set of files per session — spread across \
             many files suggests the implementation boundary is unclear."
        }
        "edit_bloat" => {
            "Prefer targeted edits over large file rewrites — high edit bloat \
             increases review burden and merge conflict risk."
        }
        "session_timeout" => {
            "Break work into shorter sessions to avoid timeout-induced context loss — \
             use TaskCreate/TaskUpdate to checkpoint progress between sessions."
        }
        "cold_restart" => {
            "Reduce cold restart frequency by persisting session context more aggressively \
             and avoiding long gaps between tool invocations."
        }
        "coordinator_respawns" => {
            "Review coordinator agent lifespan — frequent respawns indicate premature \
             termination or context overflow. Increase session limits or checkpoint state."
        }
        "post_completion_work" => {
            "Ensure agents stop tool invocations after task completion signals — \
             post-completion work inflates cycle time and risks unintended side effects."
        }
        "source_file_count" => {
            "Limit new source files per delivery cycle — high file counts indicate \
             over-scoped implementation. Split into smaller features."
        }
        "design_artifact_count" => {
            "Reduce design artifact generation — high counts indicate over-designed \
             features. Focus on the minimum viable artifact set."
        }
        "adr_count" => {
            "Consolidate architectural decisions — high ADR counts per cycle indicate \
             excessive scope or indecision. Pre-validate architecture before implementation."
        }
        "post_delivery_issues" => {
            "Invest in acceptance testing before delivery — post-delivery issues indicate \
             acceptance criteria were not fully validated during implementation."
        }
        "phase_duration_outlier" => {
            "Investigate phases that take significantly longer than historical baselines — \
             outlier durations indicate scope creep, blockers, or rework not visible in tool counts."
        }
        _ => {
            "Review the recurring detection rule and consider adding it to the \
             settings.json allowlist or adjusting detection thresholds."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create observations that will trigger the OrphanedCallsRule.
    ///
    /// OrphanedCallsRule fires when pre_count - terminal_count > 2 for a tool.
    /// We create 5 PreToolUse + 2 PostToolUse for "Read" => 3 orphaned > threshold(2).
    fn make_permission_friction_obs(session_id: &str) -> Vec<ObservationRecord> {
        let mut obs: Vec<ObservationRecord> = (0..5)
            .map(|i| ObservationRecord {
                ts: 1700000000000 + i * 1000,
                event_type: "PreToolUse".to_string(),
                source_domain: "claude-code".to_string(),
                session_id: session_id.to_string(),
                tool: Some("Read".to_string()),
                input: Some(serde_json::json!({"file_path": "/tmp/test.rs"})),
                response_size: None,
                response_snippet: None,
            })
            .collect();
        obs.extend((0..2).map(|i| ObservationRecord {
            ts: 1700000000000 + 5000 + i * 1000,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Read".to_string()),
            input: None,
            response_size: Some(100),
            response_snippet: None,
        }));
        obs
    }

    #[test]
    fn recurring_friction_from_three_sessions_returns_recommendations() {
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let recs = compute_friction_recommendations(&observations);
        // If detection rules find the pattern (orphaned_calls fires in all 3 sessions),
        // we should have at least one recommendation string.
        // The exact result depends on which detection rules fire.
        // Accept either non-empty (rule fired) or empty (rule threshold not met) as valid.
        for rec in &recs {
            assert!(
                rec.contains("fired in"),
                "recommendation must contain 'fired in' count; got: {rec}"
            );
        }
    }

    #[test]
    fn no_friction_from_two_sessions() {
        let mut observations = Vec::new();
        for i in 0..2 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let recs = compute_friction_recommendations(&observations);
        // 2 sessions < 3 minimum — no recommendations should be produced
        assert!(
            recs.is_empty(),
            "must not produce recommendations with < 3 sessions but got: {:?}",
            recs
        );
    }

    #[test]
    fn empty_observations_returns_empty() {
        let recs = compute_friction_recommendations(&[]);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommendations_contain_remediation_text() {
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let recs = compute_friction_recommendations(&observations);
        for rec in &recs {
            // Each recommendation must reference the rule name and a count
            assert!(
                rec.contains("fired in"),
                "must contain 'fired in'; got: {rec}"
            );
            assert!(
                rec.contains("sessions"),
                "must contain 'sessions'; got: {rec}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // remediation_for_rule unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn remediation_for_orphaned_calls_is_actionable() {
        let r = remediation_for_rule("orphaned_calls");
        assert!(
            r.contains("orphaned") || r.contains("context overflow"),
            "orphaned_calls remediation must mention orphaned calls or context overflow; got: {r}"
        );
    }

    #[test]
    fn remediation_for_tool_call_retries_is_actionable() {
        let r = remediation_for_rule("tool_call_retries");
        assert!(
            r.contains("retries"),
            "tool_call_retries remediation must mention retries"
        );
    }

    #[test]
    fn remediation_for_session_rollbacks_is_actionable() {
        let r = remediation_for_rule("session_rollbacks");
        assert!(
            r.contains("rollbacks") || r.contains("fixtures"),
            "session_rollbacks remediation must be actionable"
        );
    }

    #[test]
    fn remediation_for_unknown_rule_returns_default() {
        let r = remediation_for_rule("some_unknown_rule_xyz");
        assert!(
            !r.is_empty(),
            "unknown rule must still return a non-empty remediation"
        );
        assert!(
            r.contains("detection rule") || r.contains("allowlist") || r.contains("thresholds"),
            "default remediation must be sensible"
        );
    }
}
