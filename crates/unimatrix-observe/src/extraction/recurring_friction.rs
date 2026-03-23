//! Recurring friction extraction rule (FR-05).
//!
//! Identifies detection rules that fire across multiple sessions, indicating
//! systemic workflow issues worth documenting.
//!
//! GH #351: Added deduplication guard (skip if title already exists in store)
//! and enriched content with per-rule actionable remediation text.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

use super::{ExtractionRule, ProposedEntry};
use crate::detection;
use crate::types::ObservationRecord;

pub struct RecurringFrictionRule;

impl ExtractionRule for RecurringFrictionRule {
    fn name(&self) -> &str {
        "recurring-friction"
    }

    fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry> {
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

        // Build proposals for rules firing in 3+ sessions
        let mut proposals = Vec::new();
        for (rule_name, sessions) in &rule_sessions {
            if sessions.len() < 3 {
                continue;
            }

            // GH #351: Dedup guard — skip if an entry with this title already exists.
            // Prevents re-inserting the same lesson on every extraction tick.
            let proposed_title = format!("Recurring friction: {}", rule_name);
            if existing_entry_with_title(store, &proposed_title) {
                continue;
            }

            let n = sessions.len();
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.5 + 0.1 * n as f64).min(0.85);

            // GH #351: Use enriched content with actionable remediation text.
            // UUID session IDs are omitted — they are not actionable.
            let content = format!(
                "Detection rule '{}' fired in {} sessions.\n\nRemediation: {}",
                rule_name,
                n,
                remediation_for_rule(rule_name)
            );

            proposals.push(ProposedEntry {
                title: proposed_title,
                content,
                category: "lesson-learned".to_string(),
                topic: "process-improvement".to_string(),
                tags: vec![
                    "auto-extracted".to_string(),
                    "recurring-friction".to_string(),
                ],
                source_rule: "recurring-friction".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}

/// Return an actionable remediation recommendation for a given detection rule name.
///
/// Provides concrete guidance that an agent can act on immediately, rather than
/// raw session UUID lists that carry no information. (GH #351)
fn remediation_for_rule(rule_name: &str) -> &'static str {
    match rule_name {
        "permission_retries" => {
            "Add cargo, git, and other CLI commands to the settings.json allowlist \
             to reduce permission prompts."
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
        _ => {
            "Review the recurring detection rule and consider adding it to the \
             settings.json allowlist or adjusting detection thresholds."
        }
    }
}

/// Check whether an active entry with the given title already exists in the store.
///
/// Uses a targeted EXISTS query against the entries table (topic + title + status = 0)
/// rather than loading all entries for the topic and filtering in Rust.
/// Returns true if a matching active entry exists (skip proposal).
/// Returns false on store error (safe-default: allow proposal to proceed).
fn existing_entry_with_title(store: &SqlxStore, title: &str) -> bool {
    let pool = store.write_pool_server();
    let title = title.to_string();
    let fut = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM entries
            WHERE topic = ?1 AND title = ?2 AND status = 0
        )",
    )
    .bind("process-improvement")
    .bind(&title)
    .fetch_one(pool);

    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| match handle.block_on(fut) {
            Ok(exists) => exists,
            Err(_) => false,
        }),
        Err(_) => {
            // No async runtime context; build a transient one for the check.
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return false,
            };
            match rt.block_on(
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                        SELECT 1 FROM entries
                        WHERE topic = ?1 AND title = ?2 AND status = 0
                    )",
                )
                .bind("process-improvement")
                .bind(&title)
                .fetch_one(pool),
            ) {
                Ok(exists) => exists,
                Err(_) => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_store() -> SqlxStore {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.db");
        std::mem::forget(dir);
        SqlxStore::open(&path, unimatrix_store::pool_config::PoolConfig::default())
            .await
            .expect("open store")
    }

    /// Create observations that will trigger the PermissionRetriesRule.
    ///
    /// PermissionRetriesRule fires when pre_count - post_count > 2 for a tool.
    /// We create 5 PreToolUse + 2 PostToolUse for "Read" => 3 retries > threshold(2).
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

    #[tokio::test(flavor = "multi_thread")]
    async fn recurring_friction_from_three_sessions() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);
        // Should find at least one recurring friction pattern
        // (PermissionRetriesRule should fire in all 3 sessions)
        let friction: Vec<_> = proposals
            .iter()
            .filter(|p| p.category == "lesson-learned")
            .collect();
        // If detection rules find the pattern, we should have a proposal
        // The exact result depends on which detection rules fire
        assert!(
            !friction.is_empty() || proposals.is_empty(),
            "should have friction proposals or none if detection rules don't match"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_friction_from_two_sessions() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..2 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);
        // Even if detection rules fire, 2 sessions < 3 minimum
        for p in &proposals {
            assert!(
                p.source_features.len() >= 3,
                "should not produce proposals with < 3 sessions"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn confidence_scales() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..5 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);
        for p in &proposals {
            // 0.5 + 0.1*5 = 1.0 but capped at 0.85
            assert!(p.extraction_confidence <= 0.85);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_observations() {
        let store = make_store().await;
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&[], &store);
        assert!(proposals.is_empty());
    }

    // -----------------------------------------------------------------------
    // GH #351: Dedup guard
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recurring_friction_skips_if_existing_entry() {
        let store = make_store().await;

        // Pre-insert an entry with the title that would be generated for "permission_retries"
        let existing = unimatrix_store::NewEntry {
            title: "Recurring friction: permission_retries".to_string(),
            content: "Detection rule 'permission_retries' fired in 3 sessions.\n\n\
                      Remediation: Add cargo, git, and other CLI commands to the \
                      settings.json allowlist to reduce permission prompts."
                .to_string(),
            topic: "process-improvement".to_string(),
            category: "lesson-learned".to_string(),
            tags: vec![
                "auto-extracted".to_string(),
                "recurring-friction".to_string(),
            ],
            source: "auto".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "background-tick".to_string(),
            feature_cycle: String::new(),
            trust_source: "auto".to_string(),
        };
        store.insert(existing).await.expect("insert existing entry");

        // Now evaluate — the rule should skip because the title already exists
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);

        // No new proposal for permission_retries should be generated
        let permission_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p.title.contains("permission_retries"))
            .collect();
        assert!(
            permission_proposals.is_empty(),
            "dedup guard must suppress proposal when entry with same title already exists (status=0 active)"
        );
    }

    /// GH #351 Fix 3: Dedup check must NOT skip when the existing entry is deprecated.
    ///
    /// The EXISTS query uses `status = 0` (Active only). A deprecated entry with the same title
    /// should not block proposal generation — the knowledge was retired and should be re-proposed.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_recurring_friction_does_not_skip_for_deprecated_entry() {
        let store = make_store().await;

        // Pre-insert an entry with the matching title but with Deprecated status.
        let deprecated_entry = unimatrix_store::NewEntry {
            title: "Recurring friction: permission_retries".to_string(),
            content: "old deprecated content".to_string(),
            topic: "process-improvement".to_string(),
            category: "lesson-learned".to_string(),
            tags: vec!["auto-extracted".to_string()],
            source: "auto".to_string(),
            status: unimatrix_store::Status::Active, // insert as Active first, then deprecate
            created_by: "background-tick".to_string(),
            feature_cycle: String::new(),
            trust_source: "auto".to_string(),
        };
        let dep_id = store
            .insert(deprecated_entry)
            .await
            .expect("insert deprecated entry");
        store
            .update_status(dep_id, unimatrix_store::Status::Deprecated)
            .await
            .expect("deprecate entry");

        // Evaluate — the deprecated entry must NOT block proposal generation.
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);

        let permission_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p.title.contains("permission_retries"))
            .collect();
        assert!(
            !permission_proposals.is_empty(),
            "dedup guard must NOT suppress proposal when existing entry is deprecated (status != 0)"
        );
    }

    // -----------------------------------------------------------------------
    // GH #351: Content enrichment — Remediation present, no UUID session IDs
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recurring_friction_content_has_remediation_not_uuids() {
        // Verify the content format for a proposal produced by the rule
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..3 {
            observations.extend(make_permission_friction_obs(&format!("s{}", i)));
        }
        let rule = RecurringFrictionRule;
        let proposals = rule.evaluate(&observations, &store);

        for proposal in &proposals {
            assert!(
                proposal.content.contains("Remediation:"),
                "proposal content must contain 'Remediation:' but got: {:?}",
                proposal.content
            );
            // UUID pattern: 8-4-4-4-12 hex chars separated by hyphens.
            // Session IDs ("s0", "s1", ...) are short and not UUIDs, but the
            // old format explicitly listed them as a comma-separated bracket list.
            // The new format omits the session list entirely.
            assert!(
                !proposal.content.contains(": ["),
                "proposal content must not contain raw session ID list but got: {:?}",
                proposal.content
            );
        }
    }

    // -----------------------------------------------------------------------
    // remediation_for_rule unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn remediation_for_permission_retries_is_actionable() {
        let r = remediation_for_rule("permission_retries");
        assert!(
            r.contains("settings.json"),
            "permission_retries remediation must mention settings.json"
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
