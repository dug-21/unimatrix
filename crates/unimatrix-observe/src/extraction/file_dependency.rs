//! File dependency extraction rule (FR-06).
//!
//! Identifies consistent read-before-edit chains across sessions,
//! suggesting implicit file dependencies worth documenting.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

use super::{ExtractionRule, ProposedEntry, extract_file_path, is_read_tool, is_write_tool};
use crate::types::ObservationRecord;

pub struct FileDependencyRule;

/// Maximum time window (in milliseconds) between Read(A) and Write(B) to count as a chain.
const DEPENDENCY_WINDOW_MS: u64 = 60_000;

impl ExtractionRule for FileDependencyRule {
    fn name(&self) -> &str {
        "file-dependency"
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

        // Group observations by session
        let mut session_records: HashMap<String, Vec<&ObservationRecord>> = HashMap::new();
        for obs in &observations {
            session_records
                .entry(obs.session_id.clone())
                .or_default()
                .push(obs);
        }

        // Find Read(A) -> Write(B) chains within time window, per session
        let mut pair_sessions: HashMap<(String, String), HashSet<String>> = HashMap::new();

        for (session_id, records) in &session_records {
            let mut sorted: Vec<&&ObservationRecord> = records.iter().collect();
            sorted.sort_by_key(|r| r.ts);

            for (i, read_obs) in sorted.iter().enumerate() {
                let read_tool = match &read_obs.tool {
                    Some(t) if is_read_tool(t) => t,
                    _ => continue,
                };
                let _ = read_tool;
                let read_path = extract_file_path(&read_obs.input);
                if read_path.is_empty() {
                    continue;
                }

                for write_obs in &sorted[i + 1..] {
                    // Check time window
                    if write_obs.ts > read_obs.ts + DEPENDENCY_WINDOW_MS {
                        break; // beyond window, sorted by ts
                    }
                    let write_tool = match &write_obs.tool {
                        Some(t) if is_write_tool(t) => t,
                        _ => continue,
                    };
                    let _ = write_tool;
                    let write_path = extract_file_path(&write_obs.input);
                    if write_path.is_empty() || write_path == read_path {
                        continue; // same file or no path
                    }

                    pair_sessions
                        .entry((read_path.clone(), write_path.clone()))
                        .or_default()
                        .insert(session_id.clone());
                }
            }
        }

        // Build proposals for pairs appearing in 3+ sessions
        let mut proposals = Vec::new();
        for ((file_a, file_b), sessions) in &pair_sessions {
            if sessions.len() < 3 {
                continue;
            }
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.4 + 0.1 * features.len() as f64).min(0.8);
            proposals.push(ProposedEntry {
                title: format!("File dependency: {} -> {}", file_a, file_b),
                content: format!(
                    "Read of '{}' consistently followed by write to '{}' within 60s, \
                     observed in {} sessions. This dependency may warrant documentation.",
                    file_a,
                    file_b,
                    features.len()
                ),
                category: "pattern".to_string(),
                topic: "workflow".to_string(),
                tags: vec!["auto-extracted".to_string(), "file-dependency".to_string()],
                source_rule: "file-dependency".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_read_obs(session_id: &str, ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_edit_obs(session_id: &str, ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    async fn make_store() -> SqlxStore {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.db");
        std::mem::forget(dir);
        SqlxStore::open(&path, unimatrix_store::pool_config::PoolConfig::default())
            .await
            .expect("open store")
    }

    #[tokio::test]
    async fn detects_read_edit_chain_in_three_sessions() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..3 {
            let base_ts = 1700000000000 + i * 1000000;
            let sid = format!("s{}", i);
            observations.push(make_read_obs(&sid, base_ts, "/tmp/config.toml"));
            observations.push(make_edit_obs(&sid, base_ts + 5000, "/tmp/main.rs"));
        }
        let rule = FileDependencyRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
        assert!(proposals[0].title.contains("config.toml"));
        assert!(proposals[0].title.contains("main.rs"));
        assert_eq!(proposals[0].category, "pattern");
    }

    #[tokio::test]
    async fn no_chain_from_two_sessions() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..2 {
            let base_ts = 1700000000000 + i * 1000000;
            let sid = format!("s{}", i);
            observations.push(make_read_obs(&sid, base_ts, "/tmp/config.toml"));
            observations.push(make_edit_obs(&sid, base_ts + 5000, "/tmp/main.rs"));
        }
        let rule = FileDependencyRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }

    #[tokio::test]
    async fn no_chain_beyond_time_window() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..3 {
            let base_ts = 1700000000000 + i * 1000000;
            let sid = format!("s{}", i);
            observations.push(make_read_obs(&sid, base_ts, "/tmp/config.toml"));
            // Edit happens 90 seconds later (beyond 60s window)
            observations.push(make_edit_obs(&sid, base_ts + 90_000, "/tmp/main.rs"));
        }
        let rule = FileDependencyRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }

    #[tokio::test]
    async fn no_chain_same_file() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..3 {
            let base_ts = 1700000000000 + i * 1000000;
            let sid = format!("s{}", i);
            // Read and edit same file
            observations.push(make_read_obs(&sid, base_ts, "/tmp/main.rs"));
            observations.push(make_edit_obs(&sid, base_ts + 5000, "/tmp/main.rs"));
        }
        let rule = FileDependencyRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }

    #[tokio::test]
    async fn confidence_scales_with_sessions() {
        let store = make_store().await;
        let mut observations = Vec::new();
        for i in 0..5 {
            let base_ts = 1700000000000 + i * 1000000;
            let sid = format!("s{}", i);
            observations.push(make_read_obs(&sid, base_ts, "/tmp/a.rs"));
            observations.push(make_edit_obs(&sid, base_ts + 5000, "/tmp/b.rs"));
        }
        let rule = FileDependencyRule;
        let proposals = rule.evaluate(&observations, &store);
        assert_eq!(proposals.len(), 1);
        // 0.4 + 0.1*5 = 0.9 but capped at 0.8
        assert!((proposals[0].extraction_confidence - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn empty_observations() {
        let store = make_store().await;
        let rule = FileDependencyRule;
        assert!(rule.evaluate(&[], &store).is_empty());
    }
}
