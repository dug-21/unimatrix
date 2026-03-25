//! Knowledge reuse computation for feature-scoped analysis (col-020 C3, col-020b C5).
//!
//! Computes feature knowledge delivery and cross-session reuse by joining
//! query_log and injection_log entry references. `delivery_count` is the total
//! distinct entries delivered across all sessions. `cross_session_count` is the
//! subset appearing in 2+ distinct sessions.
//!
//! Lives server-side per ADR-001: requires multi-table Store joins that
//! would bloat the ObservationSource trait for a single consumer.

use std::collections::{HashMap, HashSet};

use unimatrix_observe::FeatureKnowledgeReuse;
use unimatrix_store::InjectionLogRecord;
use unimatrix_store::QueryLogRecord;

/// Parse `result_entry_ids` JSON string into a vector of entry IDs.
///
/// Defensive parsing per SR-01: malformed JSON, empty strings, or `"null"`
/// return an empty Vec with a debug-level log. No panic, no error propagation.
fn parse_result_entry_ids(json_str: &str) -> Vec<u64> {
    match serde_json::from_str::<Vec<u64>>(json_str) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::debug!("col-020: failed to parse result_entry_ids: {e}");
            Vec::new()
        }
    }
}

/// Compute category gaps: categories with active entries but zero delivery.
///
/// Returns a sorted Vec for deterministic output.
fn compute_gaps(
    active_category_counts: &HashMap<String, u64>,
    delivered_categories: &HashSet<String>,
) -> Vec<String> {
    let mut gaps: Vec<String> = active_category_counts
        .iter()
        .filter(|(_, count)| **count > 0)
        .filter(|(category, _)| !delivered_categories.contains(*category))
        .map(|(category, _)| category.clone())
        .collect();
    gaps.sort();
    gaps
}

/// Compute feature-scoped knowledge delivery and cross-session reuse.
///
/// `delivery_count` counts ALL distinct entries delivered to agents across
/// all sessions (union of query_log + injection_log). `cross_session_count`
/// counts entries appearing in 2+ distinct sessions. `by_category` reflects
/// all delivered entries, not just cross-session ones. `category_gaps` lists
/// categories with active entries but zero delivery.
///
/// The `entry_category_lookup` closure resolves entry IDs to their category
/// string. Entries that fail lookup (deleted/deprecated) are silently skipped,
/// reducing the delivery count rather than aborting.
pub fn compute_knowledge_reuse<F>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    entry_category_lookup: F,
) -> FeatureKnowledgeReuse
where
    F: Fn(u64) -> Option<String>,
{
    // Step 1: Collect entry IDs from query_log, grouped by session
    let mut query_log_entry_ids: HashMap<&str, HashSet<u64>> = HashMap::new();
    for record in query_log_records {
        let entry_ids = parse_result_entry_ids(&record.result_entry_ids);
        query_log_entry_ids
            .entry(&record.session_id)
            .or_default()
            .extend(entry_ids);
    }

    // Step 2: Collect entry IDs from injection_log, grouped by session
    let mut injection_entry_ids: HashMap<&str, HashSet<u64>> = HashMap::new();
    for record in injection_log_records {
        injection_entry_ids
            .entry(&record.session_id)
            .or_default()
            .insert(record.entry_id);
    }

    // Step 3: Check if any referenced entries exist
    let has_any_refs = !query_log_entry_ids.is_empty() || !injection_entry_ids.is_empty();
    if !has_any_refs {
        return FeatureKnowledgeReuse {
            delivery_count: 0,
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
    }

    // Step 4: For each entry ID, collect ALL sessions where it appears
    let mut entry_sessions: HashMap<u64, HashSet<&str>> = HashMap::new();

    for (session_id, entry_ids) in &query_log_entry_ids {
        for &entry_id in entry_ids {
            entry_sessions
                .entry(entry_id)
                .or_default()
                .insert(session_id);
        }
    }

    for (session_id, entry_ids) in &injection_entry_ids {
        for &entry_id in entry_ids {
            entry_sessions
                .entry(entry_id)
                .or_default()
                .insert(session_id);
        }
    }

    // Step 5a: ALL distinct entry IDs (the primary metric)
    let all_entry_ids: HashSet<u64> = entry_sessions.keys().copied().collect();

    // Step 5b: Entries in 2+ sessions (sub-metric)
    let cross_session_ids: HashSet<u64> = entry_sessions
        .iter()
        .filter(|(_, sessions)| sessions.len() >= 2)
        .map(|(&entry_id, _)| entry_id)
        .collect();

    if all_entry_ids.is_empty() {
        return FeatureKnowledgeReuse {
            delivery_count: 0,
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
    }

    // Step 6: Resolve categories for ALL delivered entries
    let mut resolved_entries: HashMap<u64, String> = HashMap::new();
    for &entry_id in &all_entry_ids {
        if let Some(category) = entry_category_lookup(entry_id) {
            resolved_entries.insert(entry_id, category);
        }
        // Entries that fail lookup (deleted) are silently skipped
    }

    let delivery_count = resolved_entries.len() as u64;

    let mut by_category: HashMap<String, u64> = HashMap::new();
    for category in resolved_entries.values() {
        *by_category.entry(category.clone()).or_insert(0) += 1;
    }

    // Step 6b: Cross-session count from resolved entries only
    let cross_session_count = cross_session_ids
        .iter()
        .filter(|id| resolved_entries.contains_key(id))
        .count() as u64;

    // Step 7: Compute category gaps (based on all deliveries)
    let delivered_categories: HashSet<String> = by_category.keys().cloned().collect();
    let category_gaps = compute_gaps(active_category_counts, &delivered_categories);

    FeatureKnowledgeReuse {
        delivery_count,
        cross_session_count,
        by_category,
        category_gaps,
        total_served: delivery_count,
        total_stored: 0,
        cross_feature_reuse: 0,
        intra_cycle_reuse: 0,
        top_cross_feature_entries: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a QueryLogRecord with specified session_id and result_entry_ids JSON.
    fn make_query_log(session_id: &str, result_entry_ids: &str) -> QueryLogRecord {
        QueryLogRecord {
            query_id: 0,
            session_id: session_id.to_string(),
            query_text: "test query".to_string(),
            ts: 1000,
            result_count: 0,
            result_entry_ids: result_entry_ids.to_string(),
            similarity_scores: "[]".to_string(),
            retrieval_mode: "strict".to_string(),
            source: "mcp".to_string(),
        }
    }

    /// Helper: build an InjectionLogRecord with specified session_id and entry_id.
    fn make_injection_log(session_id: &str, entry_id: u64) -> InjectionLogRecord {
        InjectionLogRecord {
            log_id: 0,
            session_id: session_id.to_string(),
            entry_id,
            confidence: 0.9,
            timestamp: 1000,
        }
    }

    /// Helper: simple category lookup that maps entry IDs to categories.
    fn category_lookup(mapping: &HashMap<u64, String>) -> impl Fn(u64) -> Option<String> + '_ {
        move |entry_id| mapping.get(&entry_id).cloned()
    }

    // -- Core delivery and cross-session computation --

    #[test]
    fn test_knowledge_reuse_cross_session_query_log() {
        // Entry E1 in query_log for session s1, also in query_log for session s2.
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
        assert_eq!(result.by_category.get("convention"), Some(&1));
    }

    #[test]
    fn test_knowledge_reuse_cross_session_injection_log() {
        // Entry E1 in injection_log for s1 and s2.
        let query_logs = vec![];
        let injection_logs = vec![make_injection_log("s1", 10), make_injection_log("s2", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
    }

    #[test]
    fn test_knowledge_reuse_single_session_not_cross_session() {
        // Entry E1 appears in query_log and injection_log for SAME session s1.
        // Under revised semantics: delivered but not cross-session.
        let query_logs = vec![make_query_log("s1", "[10]")];
        let injection_logs = vec![make_injection_log("s1", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // Delivered to 1 session, but NOT cross-session
        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 0);
    }

    #[test]
    fn test_knowledge_reuse_deduplication_across_sources() {
        // Entry E1 in both query_log AND injection_log for s2, originated in s1.
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![make_injection_log("s2", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // Deduplicated: 1 entry, not 2
        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
    }

    #[test]
    fn test_knowledge_reuse_deduplication_across_sessions() {
        // Entry E1 in query_log for s2, injection_log for s3. All different sessions.
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![make_injection_log("s3", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // Still just 1 distinct entry
        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
    }

    // -- by_category breakdown --

    #[test]
    fn test_knowledge_reuse_by_category() {
        // 2 convention entries + 1 pattern entry reused cross-session.
        let query_logs = vec![
            make_query_log("s1", "[10, 11, 20]"),
            make_query_log("s2", "[10, 11, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (11, "convention".to_string()),
            (20, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 3);
        assert_eq!(result.cross_session_count, 3);
        assert_eq!(result.by_category.get("convention"), Some(&2));
        assert_eq!(result.by_category.get("pattern"), Some(&1));
    }

    // -- category_gaps --

    #[test]
    fn test_knowledge_reuse_category_gaps() {
        // Active entries in convention, pattern, procedure. Only convention delivered.
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![];
        let active_cats: HashMap<String, u64> = [
            ("convention".to_string(), 5),
            ("pattern".to_string(), 3),
            ("procedure".to_string(), 2),
        ]
        .into_iter()
        .collect();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
        assert_eq!(result.category_gaps.len(), 2);
        assert!(result.category_gaps.contains(&"pattern".to_string()));
        assert!(result.category_gaps.contains(&"procedure".to_string()));
        // Sorted
        assert_eq!(result.category_gaps[0], "pattern");
        assert_eq!(result.category_gaps[1], "procedure");
    }

    #[test]
    fn test_knowledge_reuse_no_gaps_all_reused() {
        // Both active categories have delivery.
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats: HashMap<String, u64> =
            [("convention".to_string(), 5), ("pattern".to_string(), 3)]
                .into_iter()
                .collect();
        let cats: HashMap<u64, String> =
            [(10, "convention".to_string()), (20, "pattern".to_string())]
                .into_iter()
                .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 2);
        assert_eq!(result.cross_session_count, 2);
        assert!(result.category_gaps.is_empty());
    }

    // -- JSON parsing robustness --

    #[test]
    fn test_knowledge_reuse_malformed_result_entry_ids() {
        let query_logs = vec![make_query_log("s1", "not json")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = HashMap::new();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // No panic, computation completes, zero delivery
        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
    }

    #[test]
    fn test_knowledge_reuse_empty_result_entry_ids() {
        let query_logs = vec![make_query_log("s1", "")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = HashMap::new();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
    }

    #[test]
    fn test_knowledge_reuse_null_result_entry_ids() {
        let query_logs = vec![make_query_log("s1", "null")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = HashMap::new();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
    }

    #[test]
    fn test_knowledge_reuse_duplicate_ids_in_result() {
        // result_entry_ids = "[1,1,1,2]" should deduplicate to {1, 2}
        let query_logs = vec![
            make_query_log("s1", "[1,1,1,2]"),
            make_query_log("s2", "[1,2]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> =
            [(1, "convention".to_string()), (2, "pattern".to_string())]
                .into_iter()
                .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // 2 distinct entries, not 4+2
        assert_eq!(result.delivery_count, 2);
        assert_eq!(result.cross_session_count, 2);
    }

    // -- Data gap handling --

    #[test]
    fn test_knowledge_reuse_no_query_log_data() {
        // Only injection_log, no query_log.
        let query_logs = vec![];
        let injection_logs = vec![make_injection_log("s1", 10), make_injection_log("s2", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
    }

    #[test]
    fn test_knowledge_reuse_no_injection_log_data() {
        // Only query_log, no injection_log.
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
    }

    #[test]
    fn test_knowledge_reuse_both_sources_empty() {
        let query_logs = vec![];
        let injection_logs = vec![];
        let active_cats: HashMap<String, u64> =
            [("convention".to_string(), 5), ("pattern".to_string(), 3)]
                .into_iter()
                .collect();
        let cats: HashMap<u64, String> = HashMap::new();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
        assert!(result.by_category.is_empty());
        // All active categories should be gaps
        assert_eq!(result.category_gaps.len(), 2);
        assert!(result.category_gaps.contains(&"convention".to_string()));
        assert!(result.category_gaps.contains(&"pattern".to_string()));
    }

    #[test]
    fn test_knowledge_reuse_deleted_entry() {
        // Entry ID 10 in query_log for 2 sessions, but lookup returns None (deleted).
        let query_logs = vec![make_query_log("s1", "[10]"), make_query_log("s2", "[10]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            |_| None, // all lookups fail
        );

        // Entry skipped, count reduced to 0
        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
        assert!(result.by_category.is_empty());
    }

    #[test]
    fn test_knowledge_reuse_zero_sessions() {
        // No data at all.
        let result = compute_knowledge_reuse(&[], &[], &HashMap::new(), |_| None);

        assert_eq!(result.delivery_count, 0);
        assert_eq!(result.cross_session_count, 0);
        assert!(result.by_category.is_empty());
        assert!(result.category_gaps.is_empty());
    }

    // -- New tests for revised semantics (col-020b) --

    #[test]
    fn test_knowledge_reuse_single_session_delivery() {
        // Regression test for #193: single-session data must produce non-zero delivery_count.
        let query_logs = vec![make_query_log("s1", "[10, 11, 12]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (11, "convention".to_string()),
            (12, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 3);
        assert_eq!(result.cross_session_count, 0);
        assert_eq!(result.by_category.get("convention"), Some(&2));
        assert_eq!(result.by_category.get("pattern"), Some(&1));
    }

    #[test]
    fn test_knowledge_reuse_delivery_vs_cross_session() {
        // E10 in s1+s2 (cross-session), E11 in s1 only, E12 in s2 only.
        let query_logs = vec![
            make_query_log("s1", "[10, 11]"),
            make_query_log("s2", "[10, 12]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (11, "convention".to_string()),
            (12, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 3);
        assert_eq!(result.cross_session_count, 1); // only E10
        assert!(result.delivery_count > result.cross_session_count);
    }

    #[test]
    fn test_knowledge_reuse_by_category_includes_single_session() {
        // Single session, entries in 1 session only -- by_category must reflect all deliveries.
        let query_logs = vec![make_query_log("s1", "[10, 20]")];
        let injection_logs = vec![];
        let active_cats: HashMap<String, u64> = [
            ("convention".to_string(), 5),
            ("pattern".to_string(), 3),
            ("procedure".to_string(), 2),
        ]
        .into_iter()
        .collect();
        let cats: HashMap<u64, String> =
            [(10, "convention".to_string()), (20, "pattern".to_string())]
                .into_iter()
                .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 2);
        assert_eq!(result.cross_session_count, 0);
        assert_eq!(result.by_category.len(), 2);
        assert_eq!(result.by_category.get("convention"), Some(&1));
        assert_eq!(result.by_category.get("pattern"), Some(&1));
        assert!(!result.by_category.is_empty());
        // Only procedure has zero delivery
        assert_eq!(result.category_gaps, vec!["procedure"]);
    }

    #[test]
    fn test_knowledge_reuse_category_gaps_delivery_based() {
        // category_gaps based on delivery, not cross-session reuse.
        let query_logs = vec![make_query_log("s1", "[10]")];
        let injection_logs = vec![];
        let active_cats: HashMap<String, u64> = [
            ("convention".to_string(), 5),
            ("pattern".to_string(), 3),
            ("procedure".to_string(), 2),
        ]
        .into_iter()
        .collect();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        // Convention has delivery even in single session, so NOT a gap
        assert!(!result.category_gaps.contains(&"convention".to_string()));
        assert!(result.category_gaps.contains(&"pattern".to_string()));
        assert!(result.category_gaps.contains(&"procedure".to_string()));
    }

    #[test]
    fn test_knowledge_reuse_dedup_across_query_and_injection_same_session() {
        // Same entry ID in both query_log and injection_log for the same session.
        let query_logs = vec![make_query_log("s1", "[10]")];
        let injection_logs = vec![make_injection_log("s1", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1); // deduplicated
        assert_eq!(result.cross_session_count, 0);
    }

    // -- Helper unit tests --

    #[test]
    fn test_parse_result_entry_ids_valid() {
        assert_eq!(parse_result_entry_ids("[1,2,3]"), vec![1u64, 2, 3]);
    }

    #[test]
    fn test_parse_result_entry_ids_empty_array() {
        assert_eq!(parse_result_entry_ids("[]"), Vec::<u64>::new());
    }

    #[test]
    fn test_parse_result_entry_ids_malformed() {
        assert_eq!(parse_result_entry_ids("not json"), Vec::<u64>::new());
    }

    #[test]
    fn test_parse_result_entry_ids_null() {
        assert_eq!(parse_result_entry_ids("null"), Vec::<u64>::new());
    }

    #[test]
    fn test_parse_result_entry_ids_empty_string() {
        assert_eq!(parse_result_entry_ids(""), Vec::<u64>::new());
    }

    #[test]
    fn test_compute_gaps_basic() {
        let active: HashMap<String, u64> = [
            ("convention".to_string(), 5),
            ("pattern".to_string(), 3),
            ("procedure".to_string(), 0), // zero count, should NOT be a gap
        ]
        .into_iter()
        .collect();
        let delivered: HashSet<String> = ["convention".to_string()].into_iter().collect();

        let gaps = compute_gaps(&active, &delivered);
        assert_eq!(gaps, vec!["pattern"]);
    }

    #[test]
    fn test_compute_gaps_all_reused() {
        let active: HashMap<String, u64> =
            [("convention".to_string(), 5), ("pattern".to_string(), 3)]
                .into_iter()
                .collect();
        let delivered: HashSet<String> = ["convention".to_string(), "pattern".to_string()]
            .into_iter()
            .collect();

        let gaps = compute_gaps(&active, &delivered);
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_compute_gaps_sorted() {
        let active: HashMap<String, u64> = [
            ("zebra".to_string(), 1),
            ("alpha".to_string(), 1),
            ("middle".to_string(), 1),
        ]
        .into_iter()
        .collect();
        let delivered: HashSet<String> = HashSet::new();

        let gaps = compute_gaps(&active, &delivered);
        assert_eq!(gaps, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn test_knowledge_reuse_mixed_query_and_injection_cross_session() {
        // Entry 10 in query_log for s1, injection_log for s2 -- cross-session via different sources.
        let query_logs = vec![make_query_log("s1", "[10]")];
        let injection_logs = vec![make_injection_log("s2", 10)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            category_lookup(&cats),
        );

        assert_eq!(result.delivery_count, 1);
        assert_eq!(result.cross_session_count, 1);
        assert_eq!(result.by_category.get("convention"), Some(&1));
    }
}
