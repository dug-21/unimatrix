//! Knowledge gap extraction rule (FR-02).
//!
//! Identifies topics that agents repeatedly search for without finding results,
//! indicating missing documentation or knowledge entries.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

use super::{ExtractionRule, ProposedEntry};
use crate::types::ObservationRecord;

pub struct KnowledgeGapRule;

impl ExtractionRule for KnowledgeGapRule {
    fn name(&self) -> &str {
        "knowledge-gap"
    }

    fn evaluate(
        &self,
        observations: &[ObservationRecord],
        _store: &SqlxStore,
    ) -> Vec<ProposedEntry> {
        // ADR-005: source_domain guard is MANDATORY as first operation.
        let observations: Vec<&ObservationRecord> = observations
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        // Collect zero-result context_search calls grouped by query and session
        let mut query_sessions: HashMap<String, HashSet<String>> = HashMap::new();

        for obs in &observations {
            // Look for PostToolUse context_search with zero results
            if obs.event_type != "PostToolUse" {
                continue;
            }
            let tool = match &obs.tool {
                Some(t) if t.contains("context_search") => t,
                _ => continue,
            };
            let _ = tool; // used above in pattern match

            let is_zero = obs.response_size == Some(0)
                || obs
                    .response_snippet
                    .as_ref()
                    .is_some_and(|s| s.contains("No results") || s.contains("no results"));
            if !is_zero {
                continue;
            }

            let query = extract_search_query(&obs.input);
            if query.is_empty() {
                continue;
            }

            let normalized = query.trim().to_lowercase();
            query_sessions
                .entry(normalized)
                .or_default()
                .insert(obs.session_id.clone());
        }

        // Build proposals for queries appearing in 2+ distinct sessions
        let mut proposals = Vec::new();
        for (query, sessions) in &query_sessions {
            if sessions.len() < 2 {
                continue;
            }
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.4 + 0.1 * features.len() as f64).min(0.8);
            proposals.push(ProposedEntry {
                title: format!("Knowledge gap: {}", query),
                content: format!(
                    "Agents searched for '{}' across {} sessions with no results. \
                     This topic may need explicit documentation.",
                    query,
                    features.len()
                ),
                category: "gap".to_string(),
                topic: "knowledge-management".to_string(),
                tags: vec!["auto-extracted".to_string(), "knowledge-gap".to_string()],
                source_rule: "knowledge-gap".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}

fn extract_search_query(input: &Option<serde_json::Value>) -> String {
    match input {
        Some(serde_json::Value::Object(map)) => map
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_search_obs(
        session_id: &str,
        query: &str,
        response_size: Option<u64>,
        snippet: Option<&str>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts: 1700000000000,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("mcp__unimatrix__context_search".to_string()),
            input: Some(serde_json::json!({"query": query})),
            response_size,
            response_snippet: snippet.map(|s| s.to_string()),
        }
    }

    async fn make_store() -> SqlxStore {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.db");
        // Leak the tempdir so it isn't cleaned up while the Store is alive
        std::mem::forget(dir);
        SqlxStore::open(&path, unimatrix_store::pool_config::PoolConfig::default())
            .await
            .expect("open store")
    }

    #[tokio::test]
    async fn produces_gap_from_two_sessions() {
        let store = make_store().await;
        let observations = vec![
            make_search_obs("session-1", "deployment rollback", Some(0), None),
            make_search_obs("session-2", "deployment rollback", Some(0), None),
        ];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].category, "gap");
        assert!(proposals[0].title.contains("deployment rollback"));
        assert_eq!(proposals[0].source_features.len(), 2);
    }

    #[tokio::test]
    async fn no_gap_from_single_session() {
        let store = make_store().await;
        let observations = vec![make_search_obs(
            "session-1",
            "deployment rollback",
            Some(0),
            None,
        )];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }

    #[tokio::test]
    async fn no_gap_when_results_found() {
        let store = make_store().await;
        let observations = vec![
            make_search_obs(
                "session-1",
                "deployment rollback",
                Some(100),
                Some("Found 3 results"),
            ),
            make_search_obs(
                "session-2",
                "deployment rollback",
                Some(50),
                Some("Found 1 result"),
            ),
        ];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }

    #[tokio::test]
    async fn normalizes_query_case() {
        let store = make_store().await;
        let observations = vec![
            make_search_obs("session-1", "Deployment Rollback", Some(0), None),
            make_search_obs("session-2", "deployment rollback", Some(0), None),
        ];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
    }

    #[tokio::test]
    async fn detects_zero_via_snippet() {
        let store = make_store().await;
        let observations = vec![
            make_search_obs("session-1", "test query", None, Some("No results found")),
            make_search_obs("session-2", "test query", None, Some("No results")),
        ];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
    }

    #[tokio::test]
    async fn confidence_scales_with_features() {
        let store = make_store().await;
        let observations = vec![
            make_search_obs("s1", "missing topic", Some(0), None),
            make_search_obs("s2", "missing topic", Some(0), None),
            make_search_obs("s3", "missing topic", Some(0), None),
            make_search_obs("s4", "missing topic", Some(0), None),
        ];
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
        // 0.4 + 0.1*4 = 0.8
        assert!((proposals[0].extraction_confidence - 0.8).abs() < 0.01);
    }

    #[tokio::test]
    async fn confidence_capped_at_0_8() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..10 {
            observations.push(make_search_obs(
                &format!("s{}", i),
                "capped topic",
                Some(0),
                None,
            ));
        }
        let rule = KnowledgeGapRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
        assert!((proposals[0].extraction_confidence - 0.8).abs() < f64::EPSILON);
    }
}
