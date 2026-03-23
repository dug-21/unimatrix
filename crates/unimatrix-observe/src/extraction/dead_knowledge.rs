//! Dead knowledge detection helpers (GH #351).
//!
//! `DeadKnowledgeRule` (the `ExtractionRule` impl) has been removed from the
//! extraction pipeline. It incorrectly used an additive extraction abstraction to
//! signal a maintenance action — inserting `lesson-learned` entries instead of
//! directly deprecating stale entries. This created a self-replicating noise loop.
//!
//! The detection logic is preserved here as public free functions so that
//! `background::dead_knowledge_deprecation_pass()` can reuse the same session-window
//! and entry-access heuristics without duplicating them.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

use crate::types::ObservationRecord;

/// Find entry IDs that are candidates for deprecation based on the dead-knowledge
/// heuristic: accessed at some point (access_count > 0) but not present in any
/// of the most recent `window` sessions.
///
/// Returns `None` if there are fewer than `window` distinct sessions in
/// `observations` (not enough data to make a confident decision).
///
/// # Parameters
/// - `observations`: All observation records to analyse (should be source-domain-filtered
///   by the caller before passing in).
/// - `store`: Read-only store reference for querying active entries.
/// - `window`: How many most-recent sessions define "recently used" (default: 5).
///
/// This is a synchronous function that must be called from a context that can
/// block (e.g. `tokio::task::spawn_blocking`).
pub fn detect_dead_knowledge_candidates(
    observations: &[ObservationRecord],
    store: &SqlxStore,
    window: usize,
) -> Option<Vec<u64>> {
    // ADR-005: source_domain guard is MANDATORY as first operation.
    let filtered: Vec<&ObservationRecord> = observations
        .iter()
        .filter(|r| r.source_domain == "claude-code")
        .collect();

    // 1. Build session timestamps, sorted newest first.
    let mut session_times: HashMap<String, u64> = HashMap::new();
    for obs in &filtered {
        let ts = session_times.entry(obs.session_id.clone()).or_insert(0);
        if obs.ts > *ts {
            *ts = obs.ts;
        }
    }
    let mut sessions_sorted: Vec<(String, u64)> = session_times.into_iter().collect();
    sessions_sorted.sort_by(|a, b| b.1.cmp(&a.1)); // newest first

    if sessions_sorted.len() < window {
        return None; // insufficient data
    }

    let recent_window: HashSet<&str> = sessions_sorted[..window]
        .iter()
        .map(|(s, _)| s.as_str())
        .collect();

    // 2. Collect entry IDs accessed in the recent window sessions.
    let mut recent_entry_ids: HashSet<u64> = HashSet::new();
    for obs in &filtered {
        if !recent_window.contains(obs.session_id.as_str()) {
            continue;
        }
        let is_context_tool = obs.tool.as_deref().is_some_and(|t| {
            t.contains("context_get")
                || t.contains("context_lookup")
                || t.contains("context_search")
        });
        if !is_context_tool {
            continue;
        }
        if let Some(snippet) = &obs.response_snippet {
            for id in extract_entry_ids(snippet) {
                recent_entry_ids.insert(id);
            }
        }
    }

    // 3. Query active entries with access_count > 0 not in recent_entry_ids.
    let active_entries = match query_accessed_active_entries(store) {
        Ok(entries) => entries,
        Err(_) => return Some(vec![]),
    };

    let candidates: Vec<u64> = active_entries
        .into_iter()
        .filter_map(|(id, _title, _count)| {
            if recent_entry_ids.contains(&id) {
                None
            } else {
                Some(id)
            }
        })
        .collect();

    Some(candidates)
}

/// Extract entry IDs from a response snippet.
/// Looks for patterns like `"id": 42` or `#42` in the text.
pub(crate) fn extract_entry_ids(snippet: &str) -> Vec<u64> {
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
///
/// This is a synchronous call that must be driven from a blocking context
/// (e.g. inside `tokio::task::spawn_blocking`).
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

    fn make_obs(
        session_id: &str,
        ts: u64,
        tool: Option<&str>,
        snippet: Option<&str>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
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

    // -----------------------------------------------------------------------
    // GH #351: Verify DeadKnowledgeRule is NOT in the extraction defaults.
    // -----------------------------------------------------------------------

    #[test]
    fn test_dead_knowledge_rule_removed_from_defaults() {
        use crate::extraction::default_extraction_rules;
        let rules = default_extraction_rules();
        let names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        assert!(
            !names.contains(&"dead-knowledge"),
            "DeadKnowledgeRule must not be in default_extraction_rules (GH #351): \
             found names {:?}",
            names
        );
    }

    // -----------------------------------------------------------------------
    // Detection logic: returns None when insufficient sessions
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_detect_returns_none_with_insufficient_sessions() {
        let store = make_store().await;
        let observations: Vec<ObservationRecord> = (0..4)
            .map(|i| make_obs(&format!("s{}", i), 1000 + i as u64, None, None))
            .collect();
        // window=5, only 4 sessions → None
        let result = detect_dead_knowledge_candidates(&observations, &store, 5);
        assert!(
            result.is_none(),
            "should return None when fewer sessions than window"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_detect_returns_empty_with_no_accessed_entries() {
        let store = make_store().await;
        let observations: Vec<ObservationRecord> = (0..6)
            .map(|i| make_obs(&format!("s{}", i), 1000 + i as u64, None, None))
            .collect();
        let result = detect_dead_knowledge_candidates(&observations, &store, 5);
        assert_eq!(result, Some(vec![]), "empty store → empty candidate list");
    }

    // -----------------------------------------------------------------------
    // GH #351: Cap-at-50 logic is validated in background.rs unit test.
    // Here we verify the detection filter correctly identifies candidates.
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dead_knowledge_deprecation_pass_caps_at_50() {
        // This test verifies the detection filter (the logic that would be capped).
        // Insert 60 entries with access_count > 0 so that detect_dead_knowledge_candidates
        // returns all 60 as candidates (none were accessed in recent sessions).
        let store = make_store().await;

        for i in 0..60usize {
            let entry = unimatrix_store::NewEntry {
                title: format!("Stale entry number {}", i),
                content: format!("Content for stale entry {}", i),
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
            store
                .record_usage(&[id], &[id], &[], &[], &[], &[])
                .await
                .expect("record usage");
        }

        // 6 sessions, none accessing any of the 60 entries
        let observations: Vec<ObservationRecord> = (0..6)
            .map(|i| {
                make_obs(
                    &format!("s{}", i),
                    (1000 + i * 100) as u64,
                    Some("mcp__unimatrix__context_search"),
                    Some("No results"),
                )
            })
            .collect();

        let result = detect_dead_knowledge_candidates(&observations, &store, 5);
        let candidates = result.expect("should have candidates with 6 sessions");
        // All 60 qualify as dead knowledge — the cap-at-50 is enforced by the
        // deprecation pass in background.rs, not by the detector itself.
        assert_eq!(
            candidates.len(),
            60,
            "detector should return all 60 un-accessed entries as candidates"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recently_accessed_entry_not_a_candidate() {
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
        store
            .record_usage(&[id], &[id], &[], &[], &[], &[])
            .await
            .expect("record usage");

        // Most recent session references this entry in its snippet
        let mut observations = Vec::new();
        for i in 0..6 {
            let snippet = if i == 5 {
                Some(format!("Found entry with \"id\": {}", id))
            } else {
                Some("No results".to_string())
            };
            observations.push(ObservationRecord {
                ts: (1000 + i * 100) as u64,
                event_type: "PostToolUse".to_string(),
                source_domain: "claude-code".to_string(),
                session_id: format!("s{}", i),
                tool: Some("mcp__unimatrix__context_search".to_string()),
                input: None,
                response_size: None,
                response_snippet: snippet,
            });
        }

        let result = detect_dead_knowledge_candidates(&observations, &store, 5);
        let candidates = result.expect("should return Some with 6 sessions");
        assert!(
            !candidates.contains(&id),
            "recently accessed entry must not be a deprecation candidate"
        );
    }

    // -----------------------------------------------------------------------
    // extract_entry_ids unit tests (helper preserved for reuse)
    // -----------------------------------------------------------------------

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
}
