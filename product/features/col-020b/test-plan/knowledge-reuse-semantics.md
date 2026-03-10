# Test Plan: C5 — Knowledge Reuse Semantics Revision

**File:** `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`
**Function:** `compute_knowledge_reuse` (public)
**Risks:** R-04 (delivery_count miscount), R-05 (by_category wrong set)

## Existing Test Updates

All existing tests in `knowledge_reuse.rs::tests` must be updated:
- Change `result.tier1_reuse_count` to `result.delivery_count`
- Add `result.cross_session_count` assertions alongside `delivery_count`
- Return type is now `FeatureKnowledgeReuse`

### Semantic shift in existing tests

Several existing tests assert `tier1_reuse_count == 0` for single-session data (e.g., `test_knowledge_reuse_same_session_excluded`). After the semantic revision:
- `delivery_count` includes ALL entries regardless of session count
- `cross_session_count` retains the 2+ sessions filter

So `test_knowledge_reuse_same_session_excluded` must change:
- Old: `tier1_reuse_count == 0` (entry in 1 session, filtered out)
- New: `delivery_count == 1` (entry delivered), `cross_session_count == 0` (not cross-session)

Similarly, `test_knowledge_reuse_deleted_entry` stays `delivery_count == 0` because the lookup returns None (entry not counted).

## New Unit Test Expectations

### test_knowledge_reuse_single_session_delivery (NEW)

Regression test for #193. Single-session data must produce non-zero delivery_count.

```
Arrange: query_logs with 3 entries in session "s1" only
         injection_logs = []
         active_cats = {}
         category lookup returns "convention" for all entries
Act:     result = compute_knowledge_reuse(...)
Assert:  result.delivery_count == 3
         result.cross_session_count == 0
```

### test_knowledge_reuse_delivery_vs_cross_session (NEW)

Mix of single-session and multi-session entries. delivery_count > cross_session_count.

```
Arrange: query_logs:
           s1: [10, 11, 12]   -- entries 10, 11, 12
           s2: [10]           -- entry 10 also in s2
         injection_logs = []
         category lookup: all "convention"
Act:     result = compute_knowledge_reuse(...)
Assert:  result.delivery_count == 3        (10, 11, 12 all delivered)
         result.cross_session_count == 1   (only 10 in 2+ sessions)
```

### test_knowledge_reuse_by_category_includes_single_session (NEW)

by_category reflects all delivered entries, not just cross-session.

```
Arrange: query_logs: s1: [10, 20]  (single session only)
         injection_logs = []
         category lookup: 10 -> "convention", 20 -> "pattern"
         active_cats = {"convention": 5, "pattern": 3}
Act:     result = compute_knowledge_reuse(...)
Assert:  result.by_category.get("convention") == Some(&1)
         result.by_category.get("pattern") == Some(&1)
         result.delivery_count == 2
         result.cross_session_count == 0
```

### test_knowledge_reuse_category_gaps_delivery_based (NEW)

category_gaps based on delivery (not cross-session reuse).

```
Arrange: query_logs: s1: [10]  (single session)
         injection_logs = []
         category lookup: 10 -> "convention"
         active_cats = {"convention": 5, "pattern": 3, "procedure": 2}
Act:     result = compute_knowledge_reuse(...)
Assert:  result.category_gaps contains "pattern" and "procedure"
         result.category_gaps does NOT contain "convention"
         (convention has delivery even though it has no cross-session reuse)
```

### test_knowledge_reuse_dedup_across_query_and_injection_same_session (NEW)

Same entry ID in both query_log and injection_log for the same session counts as 1 delivery.

```
Arrange: query_logs: s1: [10]
         injection_logs: s1: entry_id=10
         category lookup: 10 -> "convention"
Act:     result = compute_knowledge_reuse(...)
Assert:  result.delivery_count == 1  (deduplicated)
         result.cross_session_count == 0
```

## Updated Existing Tests (Key Changes)

| Test | Old Assertion | New Assertion |
|------|--------------|---------------|
| test_knowledge_reuse_cross_session_query_log | tier1_reuse_count == 1 | delivery_count == 1, cross_session_count == 1 |
| test_knowledge_reuse_cross_session_injection_log | tier1_reuse_count == 1 | delivery_count == 1, cross_session_count == 1 |
| test_knowledge_reuse_same_session_excluded | tier1_reuse_count == 0 | delivery_count == 1, cross_session_count == 0 |
| test_knowledge_reuse_deduplication_across_sources | tier1_reuse_count == 1 | delivery_count == 1, cross_session_count == 1 |
| test_knowledge_reuse_deduplication_across_sessions | tier1_reuse_count == 1 | delivery_count == 1, cross_session_count == 1 |
| test_knowledge_reuse_by_category | tier1_reuse_count == 3 | delivery_count == 3, cross_session_count == 3 |
| test_knowledge_reuse_category_gaps | tier1_reuse_count == 1 | delivery_count == 1, cross_session_count == 1, gaps updated |
| test_knowledge_reuse_malformed_result_entry_ids | tier1_reuse_count == 0 | delivery_count == 0, cross_session_count == 0 |
| test_knowledge_reuse_both_sources_empty | tier1_reuse_count == 0 | delivery_count == 0, cross_session_count == 0 |
| test_knowledge_reuse_deleted_entry | tier1_reuse_count == 0 | delivery_count == 0, cross_session_count == 0 |
| test_knowledge_reuse_zero_sessions | tier1_reuse_count == 0 | delivery_count == 0, cross_session_count == 0 |

## Risk Coverage

- R-04: `test_knowledge_reuse_single_session_delivery` is the direct regression test for the 2+ sessions bug. `test_knowledge_reuse_delivery_vs_cross_session` and `test_knowledge_reuse_dedup_across_query_and_injection_same_session` test deduplication and the invariant `cross_session_count <= delivery_count`.
- R-05: `test_knowledge_reuse_by_category_includes_single_session` and `test_knowledge_reuse_category_gaps_delivery_based` verify by_category and category_gaps use all deliveries.
