# Component 3: compute_knowledge_reuse — `unimatrix-server/src/mcp/knowledge_reuse.rs`

## Purpose

Extend the existing pure computation function with two new parameters (`explicit_read_ids`
and `explicit_read_meta`) and three new derived values (`explicit_read_count`,
`explicit_read_by_category`, and the redefined `total_served`). Update the early-return
guards to use the corrected zero-delivery condition.

---

## Modified Function Signature

The function currently uses two generic type parameters (`F`, `G`) for the lookup closures.
Two concrete parameters are appended after the existing closure parameters.

```
pub fn compute_knowledge_reuse<F, G>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    current_feature_cycle: &str,
    entry_category_lookup: F,    // existing: Fn(u64) -> Option<String>
    entry_meta_lookup: G,        // existing: Fn(&[u64]) -> HashMap<u64, EntryMeta>
    explicit_read_ids: &HashSet<u64>,          // NEW
    explicit_read_meta: &HashMap<u64, EntryMeta>,  // NEW
) -> FeatureKnowledgeReuse
where
    F: Fn(u64) -> Option<String>,
    G: Fn(&[u64]) -> HashMap<u64, EntryMeta>,
```

The new parameters are borrowed references — they are pre-computed by the caller
(`compute_knowledge_reuse_for_sessions` in `tools.rs`). No heap allocation in this function.

---

## Updated Module Doc Comment

Update the module-level doc comment (`//!` block at top of file) to describe the new
computation:

```
//! col-026 adds cross-feature vs. intra-cycle split via a batch metadata lookup
//! closure (ADR-003). The closure is called exactly once per invocation with all
//! distinct entry IDs collected from query_log + injection_log.
//!
//! crt-049 adds explicit read signal: extract_explicit_read_ids filters the attributed
//! observation slice for context_get and single-ID context_lookup PreToolUse events.
//! explicit_read_count and explicit_read_by_category are derived from the pre-fetched
//! explicit_read_meta map. total_served is redefined as |explicit_reads u injections|.
```

---

## Algorithm

### Early-Return Guards (Updated)

The function currently has two early-return paths that return a zero `FeatureKnowledgeReuse`.
Both must be updated to:
1. Use `search_exposure_count` instead of `delivery_count` in the returned struct
2. Populate `explicit_read_count` and `explicit_read_by_category` with zero/empty
3. Use the new `total_served` semantics (still 0 in these paths since there are no entries)

The CONDITION for triggering early-return must also be updated:

```
// CURRENT (WRONG for crt-049):
let has_any_refs = !query_log_entry_ids.is_empty() || !injection_entry_ids.is_empty();
if !has_any_refs { return zero_struct; }

// NEW (CORRECT for crt-049):
// Must also consider explicit reads as a signal source.
// The early-return is for "no entries to compute from" — if explicit reads exist,
// we still need to compute even with zero query_log/injection_log refs.
let has_any_refs = !query_log_entry_ids.is_empty()
    || !injection_entry_ids.is_empty()
    || !explicit_read_ids.is_empty();
if !has_any_refs { return zero_struct; }
```

Note: The `render_knowledge_reuse` guard (`total_served == 0 && search_exposure_count == 0`)
is a render-layer guard, not a compute-layer guard. The compute function always runs to
completion when called; the early-return here is for the case where no data at all exists
in any source.

Zero struct shape (returned from early-return paths):
```
FeatureKnowledgeReuse {
    search_exposure_count: 0,
    explicit_read_count: explicit_read_ids.len() as u64,
        // if early-return fired because injection/query were empty but explicit reads exist,
        // this would be non-zero — but the guard above prevents that case now.
        // In the true zero case (no refs from any source), this is 0.
    explicit_read_by_category: HashMap::new(),
    cross_session_count: 0,
    by_category: HashMap::new(),
    category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
    total_served: 0,
    total_stored: 0,
    cross_feature_reuse: 0,
    intra_cycle_reuse: 0,
    top_cross_feature_entries: vec![],
}
```

### New Computation Steps (after existing Steps 1-7d)

Insert the following steps before constructing the final `FeatureKnowledgeReuse` return value:

```
// Step 8: Compute explicit_read_count from the full (uncapped) explicit_read_ids set.
// The cap (500) was applied to the lookup_ids in tools.rs, not to explicit_read_ids.
// explicit_read_count therefore always reflects the true distinct count.
let explicit_read_count: u64 = explicit_read_ids.len() as u64

// Step 9: Compute explicit_read_by_category from explicit_read_meta.
// Tally category strings from EntryMeta for IDs present in explicit_read_meta.
// IDs absent from explicit_read_meta (deleted/quarantined entries, or capped above 500)
// are silently skipped — explicit_read_count remains accurate, category map is partial.
let mut explicit_read_by_category: HashMap<String, u64> = HashMap::new()
FOR each id IN explicit_read_ids:
    IF let Some(meta) = explicit_read_meta.get(&id):
        *explicit_read_by_category.entry(meta.category.clone()).or_insert(0) += 1

// Step 10: Compute total_served — redefined as |explicit_reads u injection_ids| (ADR-003).
// Build the flat injection ID set from the injection_entry_ids map
// (which maps session_id -> HashSet<u64>, built in Step 2).
let all_injection_ids: HashSet<u64> =
    injection_entry_ids.values()
        .flat_map(|set| set.iter().copied())
        .collect()

// Set union: any ID in either set is counted once (deduplication by HashSet semantics)
let total_served: u64 =
    explicit_read_ids.union(&all_injection_ids).count() as u64
// NOTE: search exposure IDs (query_log_entry_ids) are NOT included in this union.
// Rationale: search exposure means "appeared in results", not "agent consumed".
```

### Updated Field Name in Existing Steps

Step 6 computes `delivery_count`:
```
// OLD:
let delivery_count = resolved_entries.len() as u64;

// NEW:
let search_exposure_count = resolved_entries.len() as u64;
```

Update all references to `delivery_count` within the function body (Steps 6, 7c, by_category
display, debug log) to `search_exposure_count`.

Step 7c (the old `total_served = delivery_count`) is replaced entirely by Step 10 above.
Remove Step 7c from the function.

Update the debug log at the call site (in `tools.rs`) to use `search_exposure_count`.

### Updated Return Value

```
FeatureKnowledgeReuse {
    search_exposure_count,           // renamed from delivery_count
    explicit_read_count,             // new
    explicit_read_by_category,       // new
    cross_session_count,             // unchanged
    by_category,                     // unchanged
    category_gaps,                   // unchanged
    total_served,                    // redefined (Step 10)
    total_stored: 0,                 // populated by caller in tools.rs
    cross_feature_reuse,             // unchanged
    intra_cycle_reuse,               // unchanged
    top_cross_feature_entries,       // unchanged
}
```

---

## Data Flow Diagram

```
INPUTS:
  query_log_records   -> query_log_entry_ids (HashMap<session, HashSet<u64>>)
  injection_log_records -> injection_entry_ids (HashMap<session, HashSet<u64>>)
  active_category_counts -> compute_gaps()
  entry_category_lookup -> resolved_entries, by_category, search_exposure_count
  entry_meta_lookup     -> meta_map -> cross_feature_reuse, intra_cycle_reuse, top_cross_feature_entries
  explicit_read_ids   -> explicit_read_count, total_served (union with injection_ids)
  explicit_read_meta  -> explicit_read_by_category

OUTPUTS (FeatureKnowledgeReuse fields):
  search_exposure_count   <- resolved_entries.len()
  explicit_read_count     <- explicit_read_ids.len()
  explicit_read_by_category <- tally of explicit_read_meta.category
  total_served            <- |explicit_read_ids u all_injection_ids|
  cross_session_count     <- cross_session_ids filtered by resolved_entries
  by_category             <- tally of resolved_entries categories (search exposures)
  category_gaps           <- compute_gaps(active_category_counts, delivered_categories)
  cross_feature_reuse     <- count of resolved entries from prior feature cycles
  intra_cycle_reuse       <- count of resolved entries from current feature cycle
  top_cross_feature_entries <- top 5 cross-feature entries by serve_count
  total_stored            <- 0 (set by caller in tools.rs)
```

---

## Error Handling

This function has no error return (unchanged from current). All failure modes are silent:
- `entry_category_lookup` returns `None` → entry excluded from `search_exposure_count`
- `explicit_read_meta` missing an ID → that ID excluded from `explicit_read_by_category`
  (but still counted in `explicit_read_count` via `explicit_read_ids.len()`)
- Empty `explicit_read_ids` → `explicit_read_count = 0`, `explicit_read_by_category = {}`,
  `total_served = |{} u injection_ids| = injection_count`

---

## Key Test Scenarios

All tests in the `#[cfg(test)]` module in `knowledge_reuse.rs`. Extend the existing module.

### Helper for tests

The existing `make_query_log` and `make_injection_log` helpers are already in the test
module. Add a helper for explicit read data:

```
fn make_explicit_read_meta(entries: &[(u64, &str)]) -> HashMap<u64, EntryMeta>:
    // entries: slice of (id, category_string)
    entries.iter().map(|(id, cat)| {
        (*id, EntryMeta {
            title: format!("Entry {}", id),
            feature_cycle: Some("test-cycle".to_string()),
            category: cat.to_string(),
        })
    }).collect()
```

### AC-14 GATE — total_served excludes search exposures

```
Test: test_total_served_excludes_search_exposures
    // explicit_reads = {1, 2}, injections = {2, 3}, search_exposures = {4, 5, 6}
    query_logs = [make_query_log("s1", "[4, 5, 6]")]
    injection_logs = [make_injection_log("s1", 2), make_injection_log("s1", 3)]
    explicit_read_ids = HashSet::from([1u64, 2])
    explicit_read_meta = make_explicit_read_meta(&[(1, "decision"), (2, "pattern")])

    result = compute_knowledge_reuse(
        &query_logs, &injection_logs, &empty_cats, "test-cycle",
        |id| Some("decision".to_string()),  // stub
        |ids| make_meta_map(ids),           // stub
        &explicit_read_ids,
        &explicit_read_meta,
    )

    assert result.total_served == 3   // |{1,2} u {2,3}| = |{1,2,3}| = 3
    assert result.search_exposure_count == 3  // resolved query_log entries {4,5,6}
    // total_served must NOT be 6 (would mean search exposures included)
```

### AC-15 GATE — total_served deduplication

```
Test: test_total_served_deduplication
    // overlapping explicit read and injection
    explicit_read_ids = HashSet::from([1u64, 2])
    injection_logs = [make_injection_log("s1", 2), make_injection_log("s1", 3)]
    query_logs = []

    result = compute_knowledge_reuse(
        &[], &injection_logs, &empty_cats, "test-cycle",
        |_| None,   // no query_log entries to resolve
        |_| HashMap::new(),
        &explicit_read_ids,
        &make_explicit_read_meta(&[(1, "decision"), (2, "pattern")]),
    )

    assert result.total_served == 3   // |{1,2} u {2,3}| = 3, not 4
    assert result.explicit_read_count == 2
```

### AC-13 GATE — explicit_read_by_category populated

```
Test: test_explicit_read_by_category_populated
    explicit_read_ids = HashSet::from([1u64, 2, 3])
    explicit_read_meta = make_explicit_read_meta(&[
        (1, "decision"), (2, "decision"), (3, "pattern")
    ])

    result = compute_knowledge_reuse(
        &[], &[], &empty_cats, "test-cycle",
        |_| None, |_| HashMap::new(),
        &explicit_read_ids,
        &explicit_read_meta,
    )

    assert result.explicit_read_by_category.get("decision") == Some(&2)
    assert result.explicit_read_by_category.get("pattern") == Some(&1)
    assert result.explicit_read_count == 3
```

### AC-09 — explicit-read-only cycle does not short-circuit

```
Test: test_explicit_read_only_cycle_not_short_circuited
    // zero query_log, zero injection_log, one explicit read
    explicit_read_ids = HashSet::from([5u64])
    explicit_read_meta = make_explicit_read_meta(&[(5, "pattern")])

    result = compute_knowledge_reuse(
        &[], &[], &empty_cats, "test-cycle",
        |_| None, |_| HashMap::new(),
        &explicit_read_ids,
        &explicit_read_meta,
    )

    assert result.explicit_read_count == 1
    assert result.total_served == 1   // |{5} u {}| = 1
    assert result.search_exposure_count == 0
    // Must NOT return a zero-default struct — the early-return guard must not fire
```

### Additional: empty explicit_read_ids with injection-only cycle

```
Test: test_injection_only_cycle_total_served
    // AC-06 side scenario: injection-only cycle
    injection_logs = [make_injection_log("s1", 10), make_injection_log("s1", 11)]
    explicit_read_ids = HashSet::new()  // empty
    explicit_read_meta = HashMap::new()

    result = compute_knowledge_reuse(
        &[], &injection_logs, &empty_cats, "test-cycle",
        |id| Some("decision".to_string()),
        |ids| make_meta_map(ids),
        &explicit_read_ids,
        &explicit_read_meta,
    )

    assert result.total_served == 2   // |{} u {10, 11}| = 2
    assert result.explicit_read_count == 0
    assert result.search_exposure_count == 0
```

### Existing tests must pass unchanged

All existing tests that call `compute_knowledge_reuse` directly must be updated to pass
two additional arguments: `&HashSet::new()` and `&HashMap::new()` for the new parameters.
Their existing assertions remain valid — the new parameters do not affect the fields they
already test.

---

## Integration Surface

| Name | Type | Caller | Notes |
|------|------|--------|-------|
| `compute_knowledge_reuse` | `fn(..., &HashSet<u64>, &HashMap<u64, EntryMeta>) -> FeatureKnowledgeReuse` | `compute_knowledge_reuse_for_sessions` in `tools.rs` | Signature extended — all callers and test fixtures must pass new args |
