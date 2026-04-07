//! Knowledge reuse computation for feature-scoped analysis (col-020 C3, col-020b C5, col-026 C3).
//!
//! Computes feature knowledge delivery and cross-session reuse by joining
//! query_log and injection_log entry references. `search_exposure_count` is the total
//! distinct entries delivered across all sessions. `cross_session_count` is the
//! subset appearing in 2+ distinct sessions.
//!
//! col-026 adds cross-feature vs. intra-cycle split via a batch metadata lookup
//! closure (ADR-003). The closure is called exactly once per invocation with all
//! distinct entry IDs collected from query_log + injection_log.
//!
//! crt-049 adds explicit read signal: extract_explicit_read_ids filters the attributed
//! observation slice for context_get and single-ID context_lookup PreToolUse events.
//! explicit_read_count and explicit_read_by_category are derived from the pre-fetched
//! explicit_read_meta map. total_served is redefined as |explicit_reads ∪ injections|.
//!
//! Lives server-side per ADR-001: requires multi-table Store joins that
//! would bloat the ObservationSource trait for a single consumer.

use std::collections::{HashMap, HashSet};

use unimatrix_core::observation::ObservationRecord;
use unimatrix_observe::{EntryRef, FeatureKnowledgeReuse, normalize_tool_name};
use unimatrix_store::InjectionLogRecord;
use unimatrix_store::QueryLogRecord;

/// Metadata for a single entry fetched in a batch lookup (col-026 ADR-003).
///
/// Visible within the `unimatrix-server` crate only. Not part of the public
/// `unimatrix-observe` API. The `pub` visibility is required because it appears
/// in the generic bound of the `pub fn compute_knowledge_reuse` signature.
#[derive(Debug, Clone)]
pub struct EntryMeta {
    pub title: String,
    pub feature_cycle: Option<String>,
    pub category: String,
}

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

/// Extract the set of distinct entry IDs explicitly read by agents during a cycle.
///
/// Filters `attributed` for `PreToolUse` events where the normalized tool name is
/// `"context_get"` or `"context_lookup"` and the input contains a parseable `id` field.
///
/// Two-branch input parse (ADR-001 correction): hook-listener path delivers
/// `input` as `Value::String(raw_json)`, not `Value::Object`. Direct MCP calls
/// deliver `Value::Object`. Both branches are handled here; any other form is skipped.
///
/// Both integer-form (`{"id": 42}`) and string-form (`{"id": "42"}`) IDs are accepted
/// (AC-16 GATE). Filter-based `context_lookup` calls (no `id` field) are excluded
/// naturally because `obj["id"]` returns `Value::Null` for missing fields.
pub(crate) fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64> {
    let mut result: HashSet<u64> = HashSet::new();

    for record in attributed {
        // Condition 1: must be a PreToolUse event
        if record.event_type != "PreToolUse" {
            continue;
        }

        // Condition 2: normalized tool name must be "context_get" or "context_lookup"
        let raw_tool = record.tool.as_deref().unwrap_or("");
        let normalized = normalize_tool_name(raw_tool);
        if normalized != "context_get" && normalized != "context_lookup" {
            continue;
        }

        // Condition 3+4: parse input into a JSON object via two-branch parse.
        // Hook listener path: Some(Value::String(raw_json)) — must call from_str.
        // Direct MCP path: Some(Value::Object(_)) — use as-is.
        let obj: Option<serde_json::Value> = match &record.input {
            Some(serde_json::Value::Object(_)) => record.input.clone(),
            Some(serde_json::Value::String(s)) => serde_json::from_str(s).ok(),
            _ => None,
        };

        let obj = match obj {
            Some(v) => v,
            None => continue,
        };

        // Condition 5: extract id as u64.
        // Try integer form first: {"id": 42}
        // Fall back to string form: {"id": "42"}
        let id_val = &obj["id"];
        if let Some(n) = id_val
            .as_u64()
            .or_else(|| id_val.as_str().and_then(|s| s.parse::<u64>().ok()))
        {
            result.insert(n);
        }
    }

    result
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
///
/// The `entry_meta_lookup` closure (col-026 ADR-003) is called **exactly once**
/// per invocation with the full set of distinct entry IDs. It returns a
/// `HashMap<u64, EntryMeta>` with title, feature_cycle, and category for each
/// ID. The call is skipped entirely when the ID set is empty. Chunking (100 IDs
/// per IN-clause per pattern #883) is handled at the call site in `tools.rs`.
pub fn compute_knowledge_reuse<F, G>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    current_feature_cycle: &str,
    entry_category_lookup: F,
    entry_meta_lookup: G,
    explicit_read_ids: &HashSet<u64>,
    explicit_read_meta: &HashMap<u64, EntryMeta>,
) -> FeatureKnowledgeReuse
where
    F: Fn(u64) -> Option<String>,
    G: Fn(&[u64]) -> HashMap<u64, EntryMeta>,
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

    // Step 3: Check if any referenced entries exist from any source
    // (query_log, injection_log, or explicit reads via context_get/context_lookup).
    let has_any_refs = !query_log_entry_ids.is_empty()
        || !injection_entry_ids.is_empty()
        || !explicit_read_ids.is_empty();
    if !has_any_refs {
        return FeatureKnowledgeReuse {
            search_exposure_count: 0,
            explicit_read_count: 0,
            explicit_read_by_category: HashMap::new(),
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
        // entry_meta_lookup is NOT called when no refs exist (ADR-003)
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

    // Only trigger this early return if BOTH the query/injection ID set AND explicit reads
    // are empty. If explicit reads exist, we must continue to compute their metrics.
    if all_entry_ids.is_empty() && explicit_read_ids.is_empty() {
        // entry_meta_lookup is NOT called when ID set is empty (ADR-003)
        return FeatureKnowledgeReuse {
            search_exposure_count: 0,
            explicit_read_count: 0,
            explicit_read_by_category: HashMap::new(),
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

    let search_exposure_count = resolved_entries.len() as u64;

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

    // Step 7a: Batch metadata lookup (col-026 ADR-003).
    // Called ONCE with the full ID slice. Skipped when set is empty (already guarded above).
    let all_ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect();
    let meta_map: HashMap<u64, EntryMeta> = entry_meta_lookup(&all_ids_vec);

    // Step 7b: Cross-feature vs intra-cycle split.
    // Only entries already in `resolved_entries` (category-resolved) are classified.
    // Entries absent from meta_map are excluded from both buckets (R-04).
    let mut cross_feature_reuse: u64 = 0;
    let mut intra_cycle_reuse: u64 = 0;

    for &entry_id in resolved_entries.keys() {
        match meta_map.get(&entry_id) {
            Some(meta) => match meta.feature_cycle.as_deref() {
                Some(fc) if fc == current_feature_cycle => {
                    intra_cycle_reuse += 1;
                }
                Some(_) => {
                    // Stored in a prior feature cycle
                    cross_feature_reuse += 1;
                }
                None => {
                    // No feature_cycle on entry — treat as intra-cycle (conservative)
                    intra_cycle_reuse += 1;
                }
            },
            None => {
                // Entry absent from meta_map (quarantined/deleted after being served).
                // Excluded from both buckets; cross + intra <= search_exposure_count (R-04).
            }
        }
    }

    // Step 8: Compute explicit_read_count from the full (uncapped) explicit_read_ids set.
    // The cap (500) was applied to lookup_ids in tools.rs, not to explicit_read_ids.
    // explicit_read_count therefore always reflects the true distinct count.
    let explicit_read_count: u64 = explicit_read_ids.len() as u64;

    // Step 9: Compute explicit_read_by_category from explicit_read_meta.
    // Tally category strings for IDs present in explicit_read_meta.
    // IDs absent from meta (deleted/quarantined entries, or capped above 500) are silently
    // skipped — explicit_read_count remains accurate, category map may be partial.
    let mut explicit_read_by_category: HashMap<String, u64> = HashMap::new();
    for id in explicit_read_ids {
        if let Some(meta) = explicit_read_meta.get(id) {
            *explicit_read_by_category
                .entry(meta.category.clone())
                .or_insert(0) += 1;
        }
    }

    // Step 10: Compute total_served — redefined as |explicit_reads ∪ injection_ids| (ADR-003).
    // Search exposures (query_log) are intentionally excluded: appearing in results ≠ consumed.
    let all_injection_ids: HashSet<u64> = injection_entry_ids
        .values()
        .flat_map(|set| set.iter().copied())
        .collect();
    let total_served: u64 = explicit_read_ids.union(&all_injection_ids).count() as u64;

    // Step 7d: Top cross-feature entries by serve_count (top 5, sorted descending).
    // serve_count = number of distinct sessions the entry appeared in.
    let mut cross_feature_candidates: Vec<EntryRef> = Vec::new();
    for (&entry_id, meta) in &meta_map {
        let feature_cycle_val = match meta.feature_cycle.as_deref() {
            Some(fc) if fc != current_feature_cycle => fc.to_string(),
            _ => continue, // skip intra-cycle or no-cycle entries
        };

        // Only include entries that were resolved (i.e., category-resolved)
        if !resolved_entries.contains_key(&entry_id) {
            continue;
        }

        let serve_count = entry_sessions
            .get(&entry_id)
            .map(|s| s.len() as u64)
            .unwrap_or(0);

        cross_feature_candidates.push(EntryRef {
            id: entry_id,
            title: meta.title.clone(),
            feature_cycle: feature_cycle_val,
            category: meta.category.clone(),
            serve_count,
        });
    }

    // Sort descending by serve_count, then by id for determinism on ties
    cross_feature_candidates.sort_by(|a, b| {
        b.serve_count
            .cmp(&a.serve_count)
            .then_with(|| a.id.cmp(&b.id))
    });
    cross_feature_candidates.truncate(5);
    let top_cross_feature_entries = cross_feature_candidates;

    FeatureKnowledgeReuse {
        search_exposure_count,
        explicit_read_count,
        explicit_read_by_category,
        cross_session_count,
        by_category,
        category_gaps,
        total_served,
        total_stored: 0, // populated by caller in tools.rs from feature_entries count
        cross_feature_reuse,
        intra_cycle_reuse,
        top_cross_feature_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
            phase: None, // col-028: compile fix
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

    /// Helper: empty meta lookup — returns no metadata for any ID.
    fn empty_meta_lookup() -> impl Fn(&[u64]) -> HashMap<u64, EntryMeta> {
        |_ids| HashMap::new()
    }

    /// Helper: meta lookup from a fixed mapping.
    fn meta_lookup_from(
        mapping: HashMap<u64, EntryMeta>,
    ) -> impl Fn(&[u64]) -> HashMap<u64, EntryMeta> {
        move |ids| {
            ids.iter()
                .filter_map(|id| {
                    mapping.get(id).map(|m| {
                        (
                            *id,
                            EntryMeta {
                                title: m.title.clone(),
                                feature_cycle: m.feature_cycle.clone(),
                                category: m.category.clone(),
                            },
                        )
                    })
                })
                .collect()
        }
    }

    /// Helper: build a synthetic EntryMeta.
    fn make_meta(title: &str, feature_cycle: Option<&str>, category: &str) -> EntryMeta {
        EntryMeta {
            title: title.to_string(),
            feature_cycle: feature_cycle.map(|s| s.to_string()),
            category: category.to_string(),
        }
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // Delivered to 1 session, but NOT cross-session
        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // Deduplicated: 1 entry, not 2
        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // Still just 1 distinct entry
        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 3);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 2);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // No panic, computation completes, zero delivery
        assert_eq!(result.search_exposure_count, 0);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 0);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 0);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // 2 distinct entries, not 4+2
        assert_eq!(result.search_exposure_count, 2);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 0);
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
            "test-cycle",
            |_| None, // all lookups fail
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // Entry skipped, count reduced to 0
        assert_eq!(result.search_exposure_count, 0);
        assert_eq!(result.cross_session_count, 0);
        assert!(result.by_category.is_empty());
    }

    #[test]
    fn test_knowledge_reuse_zero_sessions() {
        // No data at all.
        let result = compute_knowledge_reuse(
            &[],
            &[],
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 0);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 3);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 3);
        assert_eq!(result.cross_session_count, 1); // only E10
        assert!(result.search_exposure_count > result.cross_session_count);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 2);
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1); // deduplicated
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
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 1);
        assert_eq!(result.cross_session_count, 1);
        assert_eq!(result.by_category.get("convention"), Some(&1));
    }

    // -- col-026 Component 3: new field tests --

    #[test]
    fn test_total_served_distinct_ids() {
        // Same ID served in multiple query logs — counted once in total_served.
        let query_logs = vec![
            make_query_log("s1", "[10]"),
            make_query_log("s2", "[10]"),
            make_query_log("s3", "[20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> =
            [(10, "convention".to_string()), (20, "pattern".to_string())]
                .into_iter()
                .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // search_exposure_count = 2 distinct IDs in query_log (10 and 20)
        assert_eq!(result.search_exposure_count, 2);
        // total_served = |explicit_reads ∪ injections| = |{} ∪ {}| = 0
        // search exposures are NOT included in total_served (crt-049 redefinition)
        assert_eq!(result.total_served, 0);
        assert_eq!(result.explicit_read_count, 0);
    }

    #[test]
    fn test_cross_feature_vs_intra_cycle_split() {
        // 4 entries: 2 from prior cycle (cross-feature), 2 from current cycle (intra).
        let query_logs = vec![
            make_query_log("s1", "[10, 20, 30, 40]"),
            make_query_log("s2", "[10, 20, 30, 40]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
            (30, "pattern".to_string()),
            (40, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("col-023"), "convention")),
            (20, make_meta("Entry 20", Some("col-023"), "convention")),
            (30, make_meta("Entry 30", Some("col-026"), "pattern")),
            (40, make_meta("Entry 40", Some("col-026"), "pattern")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.cross_feature_reuse, 2);
        assert_eq!(result.intra_cycle_reuse, 2);
        assert_eq!(
            result.cross_feature_reuse + result.intra_cycle_reuse,
            result.search_exposure_count
        );
    }

    #[test]
    fn test_entry_meta_lookup_called_once() {
        // Verify the closure is called exactly once regardless of ID count.
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[30]"),
        ];
        let injection_logs = vec![make_injection_log("s3", 40), make_injection_log("s3", 50)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
            (30, "pattern".to_string()),
            (40, "pattern".to_string()),
            (50, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);
        let meta_lookup = move |ids: &[u64]| -> HashMap<u64, EntryMeta> {
            cc.fetch_add(1, Ordering::SeqCst);
            ids.iter()
                .map(|&id| (id, make_meta("t", Some("prior"), "decision")))
                .collect()
        };

        let _result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup,
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "lookup called exactly once"
        );
    }

    #[test]
    fn test_entry_meta_lookup_skipped_on_empty() {
        // When no IDs, the closure must NOT be called.
        let panic_lookup = |_: &[u64]| -> HashMap<u64, EntryMeta> {
            panic!("entry_meta_lookup must not be called when ID set is empty")
        };

        let result = compute_knowledge_reuse(
            &[],
            &[],
            &HashMap::new(),
            "col-026",
            |_| None,
            panic_lookup,
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.search_exposure_count, 0);
        assert_eq!(result.cross_feature_reuse, 0);
    }

    #[test]
    fn test_top_cross_feature_entries_top_5() {
        // 7 cross-feature entries — only top 5 by serve_count returned.
        // Each appears in a different number of sessions for serve_count differentiation.
        // IDs 10-70; serve counts determined by how many sessions each appears in.
        // Use multiple sessions for entries with higher desired serve_counts.
        let query_logs = vec![
            make_query_log("s1", "[10, 20, 30, 40, 50, 60, 70]"),
            make_query_log("s2", "[10, 20, 30, 40, 50, 60]"),
            make_query_log("s3", "[10, 20, 30, 40, 50]"),
            make_query_log("s4", "[10, 20, 30, 40]"),
            make_query_log("s5", "[10, 20, 30]"),
            make_query_log("s6", "[10, 20]"),
            make_query_log("s7", "[10]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "decision".to_string()),
            (20, "decision".to_string()),
            (30, "decision".to_string()),
            (40, "decision".to_string()),
            (50, "decision".to_string()),
            (60, "decision".to_string()),
            (70, "decision".to_string()),
        ]
        .into_iter()
        .collect();

        // All 7 are cross-feature (from "col-023")
        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("col-023"), "decision")),
            (20, make_meta("Entry 20", Some("col-023"), "decision")),
            (30, make_meta("Entry 30", Some("col-023"), "decision")),
            (40, make_meta("Entry 40", Some("col-023"), "decision")),
            (50, make_meta("Entry 50", Some("col-023"), "decision")),
            (60, make_meta("Entry 60", Some("col-023"), "decision")),
            (70, make_meta("Entry 70", Some("col-023"), "decision")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.top_cross_feature_entries.len(), 5, "capped at 5");
        // Entry 10 has highest serve_count (7 sessions), should be first
        assert_eq!(result.top_cross_feature_entries[0].id, 10);
        // Serve counts should be descending
        for i in 0..result.top_cross_feature_entries.len() - 1 {
            assert!(
                result.top_cross_feature_entries[i].serve_count
                    >= result.top_cross_feature_entries[i + 1].serve_count,
                "entries not sorted descending by serve_count"
            );
        }
    }

    #[test]
    fn test_knowledge_reuse_partial_meta_lookup() {
        // 5 served entries; meta_lookup returns only 3 (IDs 40, 50 absent — quarantined).
        let query_logs = vec![make_query_log("s1", "[10, 20, 30, 40, 50]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
            (30, "convention".to_string()),
            (40, "convention".to_string()),
            (50, "convention".to_string()),
        ]
        .into_iter()
        .collect();

        // Only 3 of 5 entries have metadata
        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("prior"), "convention")),
            (20, make_meta("Entry 20", Some("prior"), "convention")),
            (30, make_meta("Entry 30", Some("col-026"), "convention")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        // No panic
        assert_eq!(result.search_exposure_count, 5);
        // cross + intra <= delivery_count (IDs 40, 50 excluded from both buckets)
        assert!(
            result.cross_feature_reuse + result.intra_cycle_reuse <= result.search_exposure_count
        );
        assert_eq!(result.cross_feature_reuse, 2); // IDs 10, 20 from "prior"
        assert_eq!(result.intra_cycle_reuse, 1); // ID 30 from "col-026"
    }

    #[test]
    fn test_knowledge_reuse_all_meta_missing() {
        // All served entries return no metadata (empty HashMap from lookup).
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.cross_feature_reuse, 0);
        assert_eq!(result.intra_cycle_reuse, 0);
        assert!(result.top_cross_feature_entries.is_empty());
        // No panic; delivery_count unchanged
        assert_eq!(result.search_exposure_count, 2);
    }

    #[test]
    fn test_knowledge_reuse_all_cross_feature() {
        // All served entries are from prior feature cycles.
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
        ]
        .into_iter()
        .collect();

        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("col-023"), "convention")),
            (20, make_meta("Entry 20", Some("col-023"), "convention")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.intra_cycle_reuse, 0);
        assert_eq!(result.cross_feature_reuse, result.search_exposure_count);
    }

    #[test]
    fn test_knowledge_reuse_all_intra_cycle() {
        // All served entries have feature_cycle == current_cycle.
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
        ]
        .into_iter()
        .collect();

        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("col-026"), "convention")),
            (20, make_meta("Entry 20", Some("col-026"), "convention")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.cross_feature_reuse, 0);
        assert_eq!(result.intra_cycle_reuse, result.search_exposure_count);
        assert!(result.top_cross_feature_entries.is_empty());
    }

    #[test]
    fn test_top_cross_feature_entries_fewer_than_three() {
        // Only 2 cross-feature entries — no padding with dummy entries.
        let query_logs = vec![
            make_query_log("s1", "[10, 20]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
        ]
        .into_iter()
        .collect();

        let meta: HashMap<u64, EntryMeta> = [
            (10, make_meta("Entry 10", Some("col-023"), "convention")),
            (20, make_meta("Entry 20", Some("col-023"), "convention")),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.top_cross_feature_entries.len(), 2);
    }

    #[test]
    fn test_top_cross_feature_entries_empty_when_none() {
        // No cross-feature entries (all intra-cycle).
        let query_logs = vec![make_query_log("s1", "[10]")];
        let injection_logs = vec![];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [(10, "convention".to_string())].into_iter().collect();

        let meta: HashMap<u64, EntryMeta> =
            [(10, make_meta("Entry 10", Some("col-026"), "convention"))]
                .into_iter()
                .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            meta_lookup_from(meta),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert!(result.top_cross_feature_entries.is_empty());
    }

    #[test]
    fn test_total_served_injection_only_no_explicit_reads() {
        // total_served = |explicit_reads ∪ injections| when no explicit reads.
        // crt-049: total_served no longer equals search_exposure_count.
        let query_logs = vec![
            make_query_log("s1", "[10, 20, 30]"),
            make_query_log("s2", "[10, 20]"),
        ];
        let injection_logs = vec![make_injection_log("s3", 40)];
        let active_cats = HashMap::new();
        let cats: HashMap<u64, String> = [
            (10, "convention".to_string()),
            (20, "convention".to_string()),
            (30, "pattern".to_string()),
            (40, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &active_cats,
            "col-026",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // search_exposure_count = 4 (IDs 10, 20, 30, 40 from query_log + injection_log via category lookup)
        // total_served = |{} ∪ {40}| = 1 (only the injected entry; search exposures excluded)
        assert_eq!(result.total_served, 1);
        assert_eq!(result.search_exposure_count, 4);
        assert_eq!(result.explicit_read_count, 0);
    }

    #[test]
    fn test_knowledge_reuse_serde_backward_compat() {
        // Old JSON without new fields deserializes with defaults.
        let json =
            r#"{"delivery_count":5,"cross_session_count":2,"by_category":{},"category_gaps":[]}"#;
        let reuse: unimatrix_observe::FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("old JSON should deserialize");
        assert_eq!(reuse.cross_feature_reuse, 0);
        assert_eq!(reuse.intra_cycle_reuse, 0);
        assert_eq!(reuse.total_served, 0);
        assert_eq!(reuse.total_stored, 0);
        assert!(reuse.top_cross_feature_entries.is_empty());
    }

    // ── extract_explicit_read_ids tests (crt-049) ────────────────────────────

    /// Build a synthetic ObservationRecord for tests.
    fn make_obs(
        event_type: &str,
        tool: Option<&str>,
        input: Option<serde_json::Value>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts: 1000,
            event_type: event_type.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "test-session".to_string(),
            tool: tool.map(str::to_string),
            input,
            response_size: None,
            response_snippet: None,
        }
    }

    /// Build a HashMap<u64, EntryMeta> from (id, category) pairs for tests.
    fn make_explicit_read_meta(entries: &[(u64, &str)]) -> HashMap<u64, EntryMeta> {
        entries
            .iter()
            .map(|(id, cat)| {
                (
                    *id,
                    EntryMeta {
                        title: format!("Entry {id}"),
                        feature_cycle: Some("test-cycle".to_string()),
                        category: cat.to_string(),
                    },
                )
            })
            .collect()
    }

    // AC-12(a): context_get with integer id — included
    #[test]
    fn test_extract_explicit_read_ids_context_get_included() {
        let obs = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 42})),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.contains(&42u64));
        assert_eq!(result.len(), 1);
    }

    // AC-12(b): filter-based context_lookup (no id field) — excluded
    #[test]
    fn test_extract_explicit_read_ids_filter_lookup_excluded() {
        let obs = make_obs(
            "PreToolUse",
            Some("context_lookup"),
            Some(serde_json::json!({"query": "some text"})),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.is_empty());
    }

    // AC-12(b) variant: null id also excluded
    #[test]
    fn test_extract_explicit_read_ids_null_id_excluded() {
        let obs = make_obs(
            "PreToolUse",
            Some("context_lookup"),
            Some(serde_json::json!({"id": null})),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.is_empty());
    }

    // AC-12(c): single-ID context_lookup (with id) — included
    #[test]
    fn test_extract_explicit_read_ids_single_id_lookup_included() {
        let obs = make_obs(
            "PreToolUse",
            Some("context_lookup"),
            Some(serde_json::json!({"id": 99})),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.contains(&99u64));
        assert_eq!(result.len(), 1);
    }

    // AC-12(d) [GATE]: prefixed tool name normalized correctly
    #[test]
    fn test_extract_explicit_read_ids_prefixed_context_get_matched() {
        let raw = r#"{"id": 7}"#.to_string();
        let obs = make_obs(
            "PreToolUse",
            Some("mcp__unimatrix__context_get"),
            Some(serde_json::Value::String(raw)),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.contains(&7u64));
        assert_eq!(result.len(), 1);
    }

    // AC-12(d) variant: prefixed context_lookup also matched
    #[test]
    fn test_extract_explicit_read_ids_prefixed_context_lookup_matched() {
        let raw = r#"{"id": 8}"#.to_string();
        let obs = make_obs(
            "PreToolUse",
            Some("mcp__unimatrix__context_lookup"),
            Some(serde_json::Value::String(raw)),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.contains(&8u64));
        assert_eq!(result.len(), 1);
    }

    // AC-12(e): empty slice returns empty set
    #[test]
    fn test_extract_explicit_read_ids_empty_slice_returns_empty() {
        let result = extract_explicit_read_ids(&[]);
        assert!(result.is_empty());
    }

    // AC-16 [GATE]: string-form ID {"id": "42"} handled correctly
    #[test]
    fn test_extract_explicit_read_ids_string_form_id_handled() {
        let obs_int = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 42})),
        );
        let obs_str = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": "99"})),
        );
        let result = extract_explicit_read_ids(&[obs_int, obs_str]);
        assert!(result.contains(&42u64));
        assert!(result.contains(&99u64));
        assert_eq!(result.len(), 2);
    }

    // AC-04: non-PreToolUse events excluded
    #[test]
    fn test_extract_explicit_read_ids_non_pretooluse_excluded() {
        let post = make_obs(
            "PostToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 5})),
        );
        let pre = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 6})),
        );
        let result = extract_explicit_read_ids(&[post, pre]);
        assert!(result.contains(&6u64));
        assert_eq!(result.len(), 1);
    }

    // AC-03: context_search excluded even if input has "id"
    #[test]
    fn test_extract_explicit_read_ids_search_tool_excluded() {
        let obs1 = make_obs(
            "PreToolUse",
            Some("context_search"),
            Some(serde_json::json!({"id": 5})),
        );
        let obs2 = make_obs(
            "PreToolUse",
            Some("mcp__unimatrix__context_search"),
            Some(serde_json::json!({"id": 6})),
        );
        let result = extract_explicit_read_ids(&[obs1, obs2]);
        assert!(result.is_empty());
    }

    // E-02: duplicate calls for same entry deduplicated
    #[test]
    fn test_extract_explicit_read_ids_deduplication() {
        let obs1 = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 42})),
        );
        let obs2 = make_obs(
            "PreToolUse",
            Some("context_get"),
            Some(serde_json::json!({"id": 42})),
        );
        let result = extract_explicit_read_ids(&[obs1, obs2]);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&42u64));
    }

    // I-04: None tool field skipped without panic
    #[test]
    fn test_extract_explicit_read_ids_none_tool_skipped() {
        let obs = make_obs("PreToolUse", None, Some(serde_json::json!({"id": 10})));
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.is_empty());
    }

    // Hook listener path: input as Value::String (not Value::Object)
    #[test]
    fn test_extract_explicit_read_ids_hook_path_string_input() {
        let raw = r#"{"id": 55}"#.to_string();
        let obs = make_obs(
            "PreToolUse",
            Some("mcp__unimatrix__context_get"),
            Some(serde_json::Value::String(raw)),
        );
        let result = extract_explicit_read_ids(&[obs]);
        assert!(result.contains(&55u64));
        assert_eq!(result.len(), 1);
    }

    // ── compute_knowledge_reuse new field tests (crt-049) ────────────────────

    // AC-13 [GATE]: explicit_read_by_category populated correctly
    #[test]
    fn test_compute_knowledge_reuse_explicit_read_by_category_populated() {
        let explicit_read_ids: HashSet<u64> = [10u64, 11u64, 12u64].into_iter().collect();
        let explicit_read_meta =
            make_explicit_read_meta(&[(10, "decision"), (11, "decision"), (12, "pattern")]);

        let result = compute_knowledge_reuse(
            &[],
            &[],
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &explicit_read_ids,
            &explicit_read_meta,
        );

        assert_eq!(result.explicit_read_by_category.get("decision"), Some(&2));
        assert_eq!(result.explicit_read_by_category.get("pattern"), Some(&1));
        assert_eq!(result.explicit_read_count, 3);
    }

    // AC-13 variant: empty explicit_read_ids produces empty by_category map
    #[test]
    fn test_compute_knowledge_reuse_explicit_read_by_category_empty_when_no_reads() {
        let result = compute_knowledge_reuse(
            &[],
            &[],
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert!(result.explicit_read_by_category.is_empty());
        assert_eq!(result.explicit_read_count, 0);
    }

    // AC-14 [GATE] + AC-15 [GATE]: total_served = |explicit_reads ∪ injections|,
    // search exposures excluded
    #[test]
    fn test_compute_knowledge_reuse_total_served_union_of_reads_and_injections() {
        // explicit_reads = {1, 2}, injections = {2, 3}, search exposures = {4, 5, 6}
        let query_logs = vec![make_query_log("s1", "[4, 5, 6]")];
        let injection_logs = vec![make_injection_log("s1", 2), make_injection_log("s1", 3)];
        let explicit_read_ids: HashSet<u64> = [1u64, 2u64].into_iter().collect();
        let explicit_read_meta = make_explicit_read_meta(&[(1, "decision"), (2, "pattern")]);
        let cats: HashMap<u64, String> = [
            (4u64, "decision".to_string()),
            (5, "decision".to_string()),
            (6, "decision".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &injection_logs,
            &HashMap::new(),
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &explicit_read_ids,
            &explicit_read_meta,
        );

        // |{1,2} ∪ {2,3}| = |{1,2,3}| = 3
        assert_eq!(result.total_served, 3);
        // search exposures still counted separately
        assert_eq!(result.search_exposure_count, 3); // {4,5,6} resolved
        // total_served must NOT be 6 (would mean search exposures included)
        assert!(result.total_served < result.search_exposure_count + 4);
    }

    // AC-15 [GATE]: search-only cycle has total_served == 0
    #[test]
    fn test_compute_knowledge_reuse_total_served_excludes_search_exposures() {
        let query_logs = vec![make_query_log("s1", "[1, 2, 3]")];
        let cats: HashMap<u64, String> = [
            (1u64, "decision".to_string()),
            (2, "decision".to_string()),
            (3, "pattern".to_string()),
        ]
        .into_iter()
        .collect();

        let result = compute_knowledge_reuse(
            &query_logs,
            &[],
            &HashMap::new(),
            "test-cycle",
            category_lookup(&cats),
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.total_served, 0);
        assert_eq!(result.search_exposure_count, 3);
    }

    // AC-15 [GATE]: overlapping explicit read and injection deduplicated
    #[test]
    fn test_compute_knowledge_reuse_total_served_deduplication_overlap() {
        let explicit_read_ids: HashSet<u64> = [1u64].into_iter().collect();
        let injection_logs = vec![make_injection_log("s1", 1)];
        let explicit_read_meta = make_explicit_read_meta(&[(1, "decision")]);

        let result = compute_knowledge_reuse(
            &[],
            &injection_logs,
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &explicit_read_ids,
            &explicit_read_meta,
        );

        assert_eq!(result.total_served, 1); // not 2
    }

    // Disjoint explicit reads and injections
    #[test]
    fn test_compute_knowledge_reuse_total_served_disjoint_sets() {
        let explicit_read_ids: HashSet<u64> = [1u64, 2u64].into_iter().collect();
        let injection_logs = vec![make_injection_log("s1", 3)];
        let explicit_read_meta = make_explicit_read_meta(&[(1, "decision"), (2, "pattern")]);

        let result = compute_knowledge_reuse(
            &[],
            &injection_logs,
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &explicit_read_ids,
            &explicit_read_meta,
        );

        assert_eq!(result.total_served, 3);
    }

    // AC-09: explicit-read-only cycle does not trigger early-return
    #[test]
    fn test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle() {
        let explicit_read_ids: HashSet<u64> = [5u64].into_iter().collect();
        let explicit_read_meta = make_explicit_read_meta(&[(5, "pattern")]);

        let result = compute_knowledge_reuse(
            &[],
            &[],
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &explicit_read_ids,
            &explicit_read_meta,
        );

        assert_eq!(result.explicit_read_count, 1);
        assert_eq!(result.explicit_read_by_category.get("pattern"), Some(&1));
        assert_eq!(result.total_served, 1);
        assert_eq!(result.search_exposure_count, 0);
    }

    // AC-17 (partial): injection-only cycle has non-zero total_served
    #[test]
    fn test_compute_knowledge_reuse_injection_only_cycle_has_nonzero_total_served() {
        let injection_logs = vec![make_injection_log("s1", 7), make_injection_log("s1", 8)];

        let result = compute_knowledge_reuse(
            &[],
            &injection_logs,
            &HashMap::new(),
            "test-cycle",
            |_| None,
            empty_meta_lookup(),
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(result.total_served, 2);
        assert_eq!(result.search_exposure_count, 0);
        assert_eq!(result.explicit_read_count, 0);
    }
}
