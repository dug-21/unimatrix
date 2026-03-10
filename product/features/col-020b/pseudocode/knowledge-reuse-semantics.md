# C5: Knowledge Reuse Semantics Revision

## Purpose

Change `compute_knowledge_reuse` so the primary metric (`delivery_count`) counts ALL unique entries delivered to agents, not just entries in 2+ sessions. The 2+ session count becomes a sub-metric (`cross_session_count`). `by_category` and `category_gaps` also shift to all-delivery semantics.

## File: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`

### Change 1: Import rename (line 12)

```
// Before
use unimatrix_observe::KnowledgeReuse;
// After
use unimatrix_observe::FeatureKnowledgeReuse;
```

### Change 2: Return type of compute_knowledge_reuse (line 55-60)

```
pub fn compute_knowledge_reuse<F>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    entry_category_lookup: F,
) -> FeatureKnowledgeReuse          // was: KnowledgeReuse
where
    F: Fn(u64) -> Option<String>,
```

### Change 3: Revised Steps 3-7 (the core semantic change)

Current Step 3 (line 83-91) returns early with `KnowledgeReuse` when no refs exist.
Replace with `FeatureKnowledgeReuse`:

```
if !has_any_refs:
    return FeatureKnowledgeReuse {
        delivery_count: 0,
        cross_session_count: 0,
        by_category: HashMap::new(),
        category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
    }
```

Current Step 5 (line 108-121) filters to 2+ sessions and returns early if empty.
Replace with split logic:

```
// Step 5a: ALL distinct entry IDs (the new primary metric)
let all_entry_ids: HashSet<u64> = entry_sessions.keys().copied().collect()

// Step 5b: Entries in 2+ sessions (sub-metric)
let cross_session_ids: HashSet<u64> = entry_sessions.iter()
    .filter(|(_, sessions)| sessions.len() >= 2)
    .map(|(&id, _)| id)
    .collect()

if all_entry_ids.is_empty():
    return FeatureKnowledgeReuse {
        delivery_count: 0,
        cross_session_count: 0,
        by_category: HashMap::new(),
        category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
    }
```

Step 6 (category resolution) changes to iterate over `all_entry_ids` instead of `reused_entry_ids`:

```
// Step 6: Resolve categories for ALL delivered entries (not just cross-session)
let mut by_category: HashMap<String, u64> = HashMap::new()
let mut resolved_count: u64 = 0
for &entry_id in &all_entry_ids:
    if let Some(category) = entry_category_lookup(entry_id):
        *by_category.entry(category).or_insert(0) += 1
        resolved_count += 1
```

Step 6b: Count resolved cross-session entries:
```
let mut cross_session_resolved: u64 = 0
for &entry_id in &cross_session_ids:
    // Only count if the entry was resolvable (lookup succeeded)
    if entry_category_lookup(entry_id).is_some():
        cross_session_resolved += 1
```

NOTE: This calls `entry_category_lookup` twice for cross-session entries. Since the lookup is a cheap Store::get on an in-memory DB and the entry set is small (typically <100), this is acceptable. An optimization would be to collect resolved entries in Step 6 and filter in Step 6b, but that adds complexity for negligible benefit.

Alternative (preferred for cleanliness): collect resolved entries in a HashMap during Step 6, then count cross-session from it:

```
// Step 6: Resolve categories for ALL delivered entries
let mut resolved_entries: HashMap<u64, String> = HashMap::new()
for &entry_id in &all_entry_ids:
    if let Some(category) = entry_category_lookup(entry_id):
        resolved_entries.insert(entry_id, category)

let delivery_count = resolved_entries.len() as u64

let mut by_category: HashMap<String, u64> = HashMap::new()
for category in resolved_entries.values():
    *by_category.entry(category.clone()).or_insert(0) += 1

// Step 6b: Cross-session count from resolved entries only
let cross_session_count = cross_session_ids.iter()
    .filter(|id| resolved_entries.contains_key(id))
    .count() as u64
```

Step 7 (gaps) unchanged in logic but now uses the `by_category` from all deliveries:

```
// Step 7: Category gaps (unchanged logic, now based on all deliveries)
let delivered_categories: HashSet<String> = by_category.keys().cloned().collect()
let category_gaps = compute_gaps(active_category_counts, &delivered_categories)
```

Final return:
```
FeatureKnowledgeReuse {
    delivery_count,
    cross_session_count,
    by_category,
    category_gaps,
}
```

### Change 4: Update compute_gaps parameter name (line 33)

Rename parameter `reused_categories` to `delivered_categories` for clarity. The function itself is unchanged in logic.

```
fn compute_gaps(
    active_category_counts: &HashMap<String, u64>,
    delivered_categories: &HashSet<String>,    // was: reused_categories
) -> Vec<String>
```

## Error Handling

No new error paths. The `entry_category_lookup` closure already handles failures by returning `None` (entries silently skipped). This behavior is preserved.

## Existing Test Updates

All tests referencing `KnowledgeReuse` change to `FeatureKnowledgeReuse`. All `.tier1_reuse_count` change to `.delivery_count`. Tests that assert cross-session behavior need `cross_session_count` assertions added.

### test_knowledge_reuse_cross_session_query_log
- `result.tier1_reuse_count` -> `result.delivery_count`
- Add: `assert_eq!(result.cross_session_count, 1);`
- `delivery_count` stays 1 (same entry in 2 sessions = 1 unique delivery)

### test_knowledge_reuse_cross_session_injection_log
- Same pattern: `delivery_count == 1`, add `cross_session_count == 1`

### test_knowledge_reuse_same_session_excluded
**SEMANTIC CHANGE**: This test previously asserted `tier1_reuse_count == 0` because the entry was in only 1 session. Under new semantics:
- `delivery_count == 1` (entry WAS delivered, just to 1 session)
- `cross_session_count == 0` (not in 2+ sessions)
Update test name consideration: rename to `test_knowledge_reuse_single_session_not_cross_session`

### test_knowledge_reuse_deduplication_across_sources
- `delivery_count == 1`, `cross_session_count == 1` (entry in s1 and s2)

### test_knowledge_reuse_deduplication_across_sessions
- `delivery_count == 1`, `cross_session_count == 1` (entry in s1, s2, s3)

### test_knowledge_reuse_by_category
- `delivery_count == 3`, `cross_session_count == 3`

### test_knowledge_reuse_category_gaps
- `delivery_count == 1`, `cross_session_count == 1`
- Gaps logic unchanged (still 2 gaps)

### test_knowledge_reuse_no_gaps_all_reused
- `delivery_count == 2`, `cross_session_count == 2`

### test_knowledge_reuse_deleted_entry
- `delivery_count == 0` (entry unresolvable), `cross_session_count == 0`

### test_knowledge_reuse_zero_sessions
- No changes needed (already 0)

### test_knowledge_reuse_both_sources_empty
- No changes needed

### test_knowledge_reuse_no_query_log_data, test_knowledge_reuse_no_injection_log_data
- `delivery_count == 1`, add `cross_session_count == 1`

### test_knowledge_reuse_mixed_query_and_injection_cross_session
- `delivery_count == 1`, add `cross_session_count == 1`

### test_knowledge_reuse_malformed/empty/null_result_entry_ids
- `tier1_reuse_count` -> `delivery_count`, no behavior change

### test_knowledge_reuse_duplicate_ids_in_result
- `delivery_count == 2`, add `cross_session_count == 2`

## New Tests

### test_knowledge_reuse_single_session_delivery (regression for #193)
```
query_logs = [make_query_log("s1", "[10, 11, 12]")]
injection_logs = []
active_cats = {}
cats = {10: "convention", 11: "convention", 12: "pattern"}

result = compute_knowledge_reuse(...)

assert result.delivery_count == 3       // ALL entries counted
assert result.cross_session_count == 0  // none in 2+ sessions
assert result.by_category["convention"] == 2
assert result.by_category["pattern"] == 1
```

### test_knowledge_reuse_delivery_vs_cross_session
```
// E10 in s1+s2 (cross-session), E11 in s1 only (single-session), E12 in s2 only (single-session)
query_logs = [make_query_log("s1", "[10, 11]"), make_query_log("s2", "[10, 12]")]
cats = {10: "convention", 11: "convention", 12: "pattern"}

result = compute_knowledge_reuse(...)

assert result.delivery_count == 3
assert result.cross_session_count == 1  // only E10
assert result.delivery_count > result.cross_session_count
```

### test_knowledge_reuse_by_category_includes_single_session
```
// Single session, entries in 1 session only
query_logs = [make_query_log("s1", "[10, 20]")]
active_cats = {"convention": 5, "pattern": 3, "procedure": 2}
cats = {10: "convention", 20: "pattern"}

result = compute_knowledge_reuse(...)

assert result.delivery_count == 2
assert result.by_category.len() == 2   // both categories present
assert !result.by_category.is_empty()  // NOT filtered out
assert result.category_gaps == ["procedure"]  // only procedure has zero delivery
```
