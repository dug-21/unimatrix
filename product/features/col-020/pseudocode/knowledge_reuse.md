# C3: Knowledge Reuse (unimatrix-server, inline in handler)

## Purpose

Compute Tier 1 cross-session knowledge reuse by joining query_log, injection_log, and entry metadata. Lives in the `context_retrospective` handler per ADR-001 (server-side for multi-table Store joins).

## Location

This is NOT a standalone module. The computation is inline in the handler (C6). This file documents the algorithm that the handler implements.

## Inputs

- `session_ids: Vec<String>` -- all session IDs for the topic
- `query_log_records: Vec<QueryLogRecord>` -- from `store.scan_query_log_by_sessions()`
- `injection_log_records: Vec<InjectionLogRecord>` -- from `store.scan_injection_log_by_sessions()`
- `session_records: Vec<SessionRecord>` -- from `store.scan_sessions_by_feature()`
- `active_category_counts: HashMap<String, u64>` -- from `store.count_active_entries_by_category()`
- `store: Arc<Store>` -- for entry metadata lookups

## Output

```
KnowledgeReuse {
    tier1_reuse_count: u64,
    by_category: HashMap<String, u64>,
    category_gaps: Vec<String>,
}
```

## Algorithm

```
function compute_knowledge_reuse(
    session_ids, query_log_records, injection_log_records,
    session_records, active_category_counts, store
):
    // Step 1: Determine which entries were STORED in each session.
    // An entry is "stored in session A" if its feature_cycle matches the topic
    // AND it was created in session A.
    // We need entry metadata to know the origin session.
    //
    // Approach: collect all entry IDs referenced in query_log and injection_log,
    // then load their metadata to determine origin session.

    // Step 2: Collect all entry IDs from query_log result_entry_ids
    let query_log_entry_ids: HashMap<String, HashSet<u64>> = empty
    // Maps session_id -> set of entry IDs retrieved in that session

    for record in query_log_records:
        let entry_ids = parse_result_entry_ids(&record.result_entry_ids)
        query_log_entry_ids
            .entry(record.session_id.clone())
            .or_default()
            .extend(entry_ids)

    // Step 3: Collect all entry IDs from injection_log
    let injection_entry_ids: HashMap<String, HashSet<u64>> = empty
    // Maps session_id -> set of entry IDs injected in that session

    for record in injection_log_records:
        injection_entry_ids
            .entry(record.session_id.clone())
            .or_default()
            .insert(record.entry_id)

    // Step 4: Union all referenced entry IDs for batch metadata lookup
    let all_referenced_ids: HashSet<u64> = union of all values in
        query_log_entry_ids and injection_entry_ids

    if all_referenced_ids is empty:
        return KnowledgeReuse {
            tier1_reuse_count: 0,
            by_category: empty,
            category_gaps: compute_gaps(active_category_counts, empty set),
        }

    // Step 5: Load entry metadata to determine origin session
    // For each entry, we need: feature_cycle (to confirm it belongs to this topic)
    // and the session_id that created it.
    //
    // Entries don't store origin session_id directly. Instead, we determine
    // "stored in session A" by checking which sessions had context_store calls
    // that created/modified entries. The injection_log tells us which entries
    // were RETRIEVED in a session. The query_log tells us which entries were
    // RETURNED by search in a session.
    //
    // For Tier 1 reuse: an entry counts as "reused" if it was retrieved
    // (query_log or injection_log) in a DIFFERENT session than where it was
    // stored (query_log or injection_log origin).
    //
    // Simplification: For Tier 1, we count entries that appear in the
    // retrieval records (query_log result_entry_ids or injection_log) of
    // session B, where session B is different from any session in which
    // that entry was injected or stored.
    //
    // Actually, the simpler interpretation from FR-02.1:
    // "(a) stored in session A within the topic" -- entry's origin
    // "(b) retrieved in session B (different session, same topic)"
    //
    // We use injection_log as the signal for "stored in session A":
    // If an entry appears in injection_log for session A, it was served
    // to session A. If it then appears in query_log for session B (A != B),
    // that's cross-session retrieval.
    //
    // But that's backwards. "Stored" means context_store created it.
    // "Retrieved" means context_search/lookup returned it.
    //
    // Better approach: An entry is "reused cross-session" if:
    // - It appears in query_log.result_entry_ids for session B, AND
    // - Session B is different from the session that originally created the entry, AND
    // - The entry was created within the topic's sessions
    //
    // OR:
    // - It appears in injection_log for session B, AND
    // - The entry was created in a different session within the topic
    //
    // To determine the creating session, we need the entry's feature_cycle
    // and to match against session timing or a direct link.
    //
    // PRACTICAL APPROACH (matching architecture):
    // Load entry metadata for all referenced IDs. Each entry has feature_cycle.
    // Entries whose feature_cycle matches the topic are "from this topic."
    // The entry was "stored" at some point. We approximate the storing session
    // as the earliest session that injected/stored it.
    //
    // SIMPLEST CORRECT APPROACH:
    // An entry is reused if it appears in retrieval records (query_log or
    // injection_log) for at least 2 different sessions within the topic.
    // The first appearance is "store/create"; subsequent appearances are "reuse."

    // Step 5b: For each entry ID, collect ALL sessions where it appears
    let entry_sessions: HashMap<u64, HashSet<String>> = empty

    for (session_id, entry_ids) in query_log_entry_ids:
        for entry_id in entry_ids:
            entry_sessions[entry_id].insert(session_id)

    for (session_id, entry_ids) in injection_entry_ids:
        for entry_id in entry_ids:
            entry_sessions[entry_id].insert(session_id)

    // Step 6: Filter to entries appearing in 2+ distinct sessions (cross-session reuse)
    let reused_entry_ids: HashSet<u64> = empty
    for (entry_id, sessions) in entry_sessions:
        if sessions.len() >= 2:
            reused_entry_ids.insert(entry_id)

    // Step 7: Load categories for reused entries
    let by_category: HashMap<String, u64> = empty
    for entry_id in reused_entry_ids:
        // Load entry to get category. Use store.get(entry_id).
        // If entry not found (deleted), skip silently.
        match store.get(entry_id):
            Ok(entry) =>
                by_category[entry.category] += 1
            Err(_) =>
                // Entry may have been deprecated/deleted. Skip.
                continue

    let tier1_reuse_count = reused_entry_ids.len() as u64

    // Step 8: Compute category gaps (FR-02.3)
    let reused_categories: HashSet<String> = by_category.keys().collect()
    let category_gaps = compute_gaps(active_category_counts, reused_categories)

    return KnowledgeReuse {
        tier1_reuse_count,
        by_category,
        category_gaps,
    }
```

## Helper: parse_result_entry_ids

```
function parse_result_entry_ids(json_str: &str) -> Vec<u64>:
    // SR-01: Defensive JSON parsing
    match serde_json::from_str::<Vec<u64>>(json_str):
        Ok(ids) => ids
        Err(e) =>
            tracing::debug!("col-020: failed to parse result_entry_ids: {e}")
            return empty vec
```

Note: Duplicate IDs within a single query_log row are handled by the HashSet -- `[1,1,1,2]` becomes `{1, 2}` in the entry_sessions map.

## Helper: compute_gaps

```
function compute_gaps(
    active_category_counts: HashMap<String, u64>,
    reused_categories: HashSet<String>
) -> Vec<String>:
    let gaps: Vec<String> = empty
    for (category, count) in active_category_counts:
        if count > 0 && !reused_categories.contains(&category):
            gaps.push(category)
    gaps.sort()  // deterministic output
    return gaps
```

## Error Handling

The entire knowledge reuse computation is wrapped in best-effort error handling by the handler (C6). If any step fails (Store query error, entry lookup failure), the handler catches the error and sets `report.knowledge_reuse = None` with a warning log.

Individual entry lookups that fail (entry deleted between log write and retrospective) are silently skipped -- they reduce the reuse count rather than aborting.

## Key Test Scenarios

These are integration tests requiring a seeded Store.

1. **Cross-session reuse (AC-06, R-04)**: Entry stored in session A, query_log returns it in session B. tier1_reuse_count = 1.
2. **Same-session not counted (R-04 edge)**: Entry appears in query_log and injection_log for the SAME session. Not counted as reuse.
3. **Deduplication across sources (R-12)**: Entry appears in both query_log and injection_log for session B (but originated in session A). tier1_reuse_count = 1, not 2.
4. **By-category breakdown (AC-07)**: 2 decision entries and 1 convention entry reused. by_category = {"decision": 2, "convention": 1}.
5. **Category gaps (AC-08)**: Active entries exist in "pattern" and "procedure" categories, but no reuse for "procedure". category_gaps includes "procedure".
6. **Malformed result_entry_ids (R-01)**: query_log row with `result_entry_ids = "not json"`. Produces empty entry set, no panic, no abort.
7. **Empty query_log + injection_log (R-02)**: All data sources empty. tier1_reuse_count = 0.
8. **Deleted entry**: Entry ID in query_log, but entry deleted from store. Skipped silently, count reduced.
9. **Duplicate entry IDs in result_entry_ids**: `"[1,1,1,2]"` counted as 2 distinct entries, not 4.
