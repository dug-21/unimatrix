//! Dead knowledge extraction rule (FR-04).
//!
//! Identifies knowledge entries that were previously accessed but have not been
//! used in the most recent sessions, suggesting possible deprecation.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

use super::{ExtractionRule, ProposedEntry};
use crate::types::ObservationRecord;

pub struct DeadKnowledgeRule;

impl ExtractionRule for DeadKnowledgeRule {
    fn name(&self) -> &str {
        "dead-knowledge"
    }

    fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry> {
        // 1. Get session timestamps, sorted newest first
        let mut session_times: HashMap<String, u64> = HashMap::new();
        for obs in observations {
            let ts = session_times.entry(obs.session_id.clone()).or_insert(0);
            if obs.ts > *ts {
                *ts = obs.ts;
            }
        }
        let mut sessions_sorted: Vec<(String, u64)> = session_times.into_iter().collect();
        sessions_sorted.sort_by(|a, b| b.1.cmp(&a.1)); // newest first

        if sessions_sorted.len() < 5 {
            return vec![];
        }

        let recent_5: HashSet<&str> = sessions_sorted[..5]
            .iter()
            .map(|(s, _)| s.as_str())
            .collect();

        // 2. Collect entry IDs accessed in recent 5 sessions
        // Look for context_get, context_lookup, context_search PostToolUse with entry IDs in response
        let mut recent_entry_ids: HashSet<u64> = HashSet::new();
        for obs in observations {
            if !recent_5.contains(obs.session_id.as_str()) {
                continue;
            }
            let tool = match &obs.tool {
                Some(t)
                    if t.contains("context_get")
                        || t.contains("context_lookup")
                        || t.contains("context_search") =>
                {
                    t
                }
                _ => continue,
            };
            let _ = tool;
            if let Some(snippet) = &obs.response_snippet {
                for id in extract_entry_ids(snippet) {
                    recent_entry_ids.insert(id);
                }
            }
        }

        // 3. Query active entries with access_count > 0 that are NOT in recent_entry_ids
        let active_entries = match query_accessed_active_entries(store) {
            Ok(entries) => entries,
            Err(_) => return vec![],
        };

        let all_features: Vec<String> = sessions_sorted.iter().map(|(s, _)| s.clone()).collect();

        let mut proposals = Vec::new();
        for (id, title, access_count) in &active_entries {
            if recent_entry_ids.contains(id) {
                continue;
            }

            proposals.push(ProposedEntry {
                title: format!("Possible dead knowledge: {}", title),
                content: format!(
                    "Entry '{}' (ID: {}) has {} accesses but was not used in the last 5 sessions. \
                     Consider deprecating.",
                    title, id, access_count
                ),
                category: "lesson-learned".to_string(),
                topic: "knowledge-management".to_string(),
                tags: vec![
                    "auto-extracted".to_string(),
                    "dead-knowledge".to_string(),
                    "deprecation-signal".to_string(),
                ],
                source_rule: "dead-knowledge".to_string(),
                source_features: all_features.clone(),
                extraction_confidence: 0.5,
            });
        }
        proposals
    }
}

/// Extract entry IDs from a response snippet.
/// Looks for patterns like `"id": 42` or `#42` in the text.
fn extract_entry_ids(snippet: &str) -> Vec<u64> {
    let mut ids = Vec::new();
    // Pattern: "id": NNN or "id":NNN
    for segment in snippet.split("\"id\"") {
        if let Some(rest) = segment.strip_prefix(':') {
            let rest = rest.trim().trim_start_matches(':').trim();
            if let Some(id) = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&id| id > 0)
            {
                ids.push(id);
            }
        }
    }
    // Pattern: #NNN (common in markdown-style responses)
    for segment in snippet.split('#') {
        if let Some(id) = segment
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&id| id > 0)
        {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Query active entries with access_count > 0.
/// Returns (id, title, access_count) tuples.
fn query_accessed_active_entries(store: &SqlxStore) -> Result<Vec<(u64, String, u32)>, String> {
    let entries = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| {
            handle.block_on(store.query_by_status(unimatrix_store::Status::Active))
        }),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(store.query_by_status(unimatrix_store::Status::Active))
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(entries
        .into_iter()
        .filter(|e| e.access_count > 0)
        .map(|e| (e.id, e.title.clone(), e.access_count))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_core::HookType;

    fn make_obs(
        session_id: &str,
        ts: u64,
        tool: Option<&str>,
        snippet: Option<&str>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: session_id.to_string(),
            tool: tool.map(|t| t.to_string()),
            input: None,
            response_size: None,
            response_snippet: snippet.map(|s| s.to_string()),
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

    #[tokio::test(flavor = "multi_thread")]
    async fn needs_at_least_five_sessions() {
        let store = make_store().await;
        let observations: Vec<ObservationRecord> = (0..4)
            .map(|i| make_obs(&format!("s{}", i), 1000 + i as u64, None, None))
            .collect();
        let rule = DeadKnowledgeRule;
        assert!(rule.evaluate(&observations, &store).is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_proposals_with_empty_store() {
        let store = make_store().await;
        let observations: Vec<ObservationRecord> = (0..6)
            .map(|i| make_obs(&format!("s{}", i), 1000 + i as u64, None, None))
            .collect();
        let rule = DeadKnowledgeRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty()); // no entries in store
    }

    #[tokio::test]
    async fn extract_entry_ids_from_json() {
        let snippet = r#"{"id": 42, "title": "test"} and {"id": 99}"#;
        let ids = extract_entry_ids(snippet);
        assert!(ids.contains(&42));
        assert!(ids.contains(&99));
    }

    #[tokio::test]
    async fn extract_entry_ids_from_hash() {
        let snippet = "Found entry #15 and #27";
        let ids = extract_entry_ids(snippet);
        assert!(ids.contains(&15));
        assert!(ids.contains(&27));
    }

    #[tokio::test]
    async fn extract_entry_ids_empty() {
        assert!(extract_entry_ids("no ids here").is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dead_knowledge_with_accessed_entry() {
        let store = make_store().await;

        // Insert an entry and bump its access count
        let entry = unimatrix_store::NewEntry {
            title: "Test entry".to_string(),
            content: "Some content".to_string(),
            topic: "test".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "human".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "test-feature".to_string(),
            trust_source: "human".to_string(),
        };
        let id = store.insert(entry).await.expect("insert");
        // Bump access_count via record_usage (batch API)
        store
            .record_usage(&[id], &[id], &[], &[], &[], &[])
            .await
            .expect("record usage");

        // Create 6 sessions where recent ones don't access this entry
        let mut observations = Vec::new();
        for i in 0..6 {
            observations.push(make_obs(
                &format!("s{}", i),
                (1000 + i * 100) as u64,
                Some("mcp__unimatrix__context_search"),
                Some("No results"),
            ));
        }

        let rule = DeadKnowledgeRule;
        let proposals = rule.evaluate(&observations, &store);
        // Entry has access_count > 0 but isn't in any recent session snippet
        assert_eq!(proposals.len(), 1);
        assert!(proposals[0].title.contains("Test entry"));
        assert_eq!(proposals[0].extraction_confidence, 0.5);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_dead_knowledge_if_recently_accessed() {
        let store = make_store().await;

        let entry = unimatrix_store::NewEntry {
            title: "Active entry".to_string(),
            content: "Still used".to_string(),
            topic: "test".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "human".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "test-feature".to_string(),
            trust_source: "human".to_string(),
        };
        let id = store.insert(entry).await.expect("insert");
        // Bump access_count via record_usage (batch API)
        store
            .record_usage(&[id], &[id], &[], &[], &[], &[])
            .await
            .expect("record usage");

        // Most recent session's snippet references this entry ID
        let mut observations = Vec::new();
        for i in 0..6 {
            let snippet = if i == 5 {
                // Most recent session references the entry
                Some(
                    format!("Found entry with \"id\": {}", id)
                        .as_str()
                        .to_string(),
                )
            } else {
                Some("No results".to_string())
            };
            observations.push(ObservationRecord {
                ts: (1000 + i * 100) as u64,
                hook: HookType::PostToolUse,
                session_id: format!("s{}", i),
                tool: Some("mcp__unimatrix__context_search".to_string()),
                input: None,
                response_size: None,
                response_snippet: snippet,
            });
        }

        let rule = DeadKnowledgeRule;
        let proposals = rule.evaluate(&observations, &store);
        assert!(proposals.is_empty());
    }
}
