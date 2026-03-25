# Component 3: FeatureKnowledgeReuse Extension + Batch Lookup

**File**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` (computation)
         `crates/unimatrix-server/src/mcp/tools.rs` (call site)
         `crates/unimatrix-observe/src/types.rs` (struct — Component 1)
**Action**: Modify — extend `compute_knowledge_reuse` with second closure parameter `entry_meta_lookup`;
            add `EntryMeta` struct; populate new fields; update all three construction sites.

---

## Purpose

Fix GH#320: knowledge reuse is currently undercounted because the cross-feature split is
missing. Served entries from prior feature cycles are not distinguished from entries stored
during this cycle. This component adds the cross-feature vs. intra-cycle split and the
`total_served`, `total_stored`, `top_cross_feature_entries` metrics.

The fix uses a batch IN-clause query (ADR-003) to fetch `feature_cycle` metadata for all
served entry IDs at once. The `compute_knowledge_reuse` function stays pure — the batch query
is executed by the caller in `tools.rs` and passed as a closure.

---

## New Type: `EntryMeta`

Define in `knowledge_reuse.rs` (NOT in public `unimatrix-observe` API).

```
struct EntryMeta {
    pub title: String,
    pub feature_cycle: Option<String>,
    pub category: String,
}
```

`EntryMeta` is used inside `compute_knowledge_reuse` only. Not exported.

---

## Extended Function Signature

```
pub fn compute_knowledge_reuse<F, G>(
    query_log_records: &[QueryLogRecord],
    injection_log_records: &[InjectionLogRecord],
    active_category_counts: &HashMap<String, u64>,
    current_feature_cycle: &str,
    entry_category_lookup: F,       // existing closure: (u64) -> Option<String>
    entry_meta_lookup: G,           // NEW closure: (&[u64]) -> HashMap<u64, EntryMeta>
) -> FeatureKnowledgeReuse
where
    F: Fn(u64) -> Option<String>,
    G: Fn(&[u64]) -> HashMap<u64, EntryMeta>,
```

The existing `entry_category_lookup` closure is SUPERSEDED by the new `entry_meta_lookup`
closure (which also contains category). For backward compatibility during migration, the old
closure is retained in the signature. The implementation may derive category from `EntryMeta`
and ignore the old closure internally, or use the old closure as a fallback when meta is missing.

SIMPLIFICATION: The implementation agent may drop the old `entry_category_lookup` parameter
and synthesize category from `EntryMeta.category`. If dropped, all test fixtures that pass the
old closure must be updated. Recommended approach: keep both for backward compatibility; use
`EntryMeta.category` when available, fall back to `entry_category_lookup` when not.

---

## Algorithm: Extended `compute_knowledge_reuse`

The existing steps 1-6 remain unchanged. Insert after step 5b (cross_session_ids computation):

```
// Existing steps 1-5b run as before, producing:
// - entry_sessions: HashMap<u64, HashSet<&str>>
// - all_entry_ids: HashSet<u64>
// - cross_session_ids: HashSet<u64>

// [EXISTING] Step 6: Resolve categories for ALL delivered entries
// (unchanged — use entry_category_lookup as before for delivery_count / by_category)
let mut resolved_entries: HashMap<u64, String> = HashMap::new()
for &entry_id in &all_entry_ids:
    if let Some(category) = entry_category_lookup(entry_id):
        resolved_entries.insert(entry_id, category)
    // Entries that fail lookup silently skipped

let delivery_count = resolved_entries.len() as u64

let mut by_category: HashMap<String, u64> = HashMap::new()
for category in resolved_entries.values():
    *by_category.entry(category.clone()).or_insert(0) += 1

// Step 6b: cross_session_count (unchanged)
let cross_session_count = cross_session_ids.iter()
    .filter(|id| resolved_entries.contains_key(id))
    .count() as u64

// Step 7: category gaps (unchanged)
let delivered_categories: HashSet<String> = by_category.keys().cloned().collect()
let category_gaps = compute_gaps(active_category_counts, &delivered_categories)

// --- NEW: Steps 7a-7e (col-026) ---

// Step 7a: Batch metadata lookup (ADR-003)
// Call entry_meta_lookup ONCE with the full ID slice.
// Skip the call entirely when the set is empty.
let meta_map: HashMap<u64, EntryMeta> = if all_entry_ids.is_empty() {
    HashMap::new()
} else {
    let all_ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect()
    entry_meta_lookup(&all_ids_vec)
}

// Step 7b: Cross-feature vs intra-cycle split
let mut cross_feature_reuse: u64 = 0
let mut intra_cycle_reuse: u64 = 0

for &entry_id in resolved_entries.keys():
    match meta_map.get(&entry_id):
        Some(meta) =>
            match meta.feature_cycle.as_deref():
                Some(fc) if fc == current_feature_cycle =>
                    intra_cycle_reuse += 1
                Some(_) =>
                    // Stored in a prior cycle
                    cross_feature_reuse += 1
                None =>
                    // No feature_cycle on this entry — treat as intra (conservative)
                    intra_cycle_reuse += 1
        None =>
            // Entry has no metadata (quarantined/deleted after being served)
            // Exclude from both buckets. This means cross + intra <= delivery_count.
            // R-04: this is the documented behavior for missing metadata.
            ()

// Step 7c: total_served (= delivery_count for now; same value, distinct semantic name)
let total_served = delivery_count

// Step 7d: top_cross_feature_entries
// Filter meta_map for entries from prior cycles, sort by serve count, take top 5.
// serve_count = number of sessions the entry appeared in (entry_sessions set size).
let mut cross_feature_candidates: Vec<EntryRef> = Vec::new()
for (&entry_id, meta) in &meta_map:
    let feature_cycle_val = match meta.feature_cycle.as_deref():
        Some(fc) if fc != current_feature_cycle => fc.to_string()
        _ => continue   // skip intra-cycle or no-cycle entries

    // Only include entries that were actually resolved (in resolved_entries)
    if !resolved_entries.contains_key(&entry_id):
        continue

    let serve_count = entry_sessions.get(&entry_id).map(|s| s.len() as u64).unwrap_or(0)

    cross_feature_candidates.push(EntryRef {
        id: entry_id,
        title: meta.title.clone(),
        feature_cycle: feature_cycle_val,
        category: meta.category.clone(),
        serve_count,
    })

// Sort descending by serve_count, then by id for determinism on ties
cross_feature_candidates.sort_by(|a, b|
    b.serve_count.cmp(&a.serve_count).then_with(|| a.id.cmp(&b.id))
)
cross_feature_candidates.truncate(5)
let top_cross_feature_entries = cross_feature_candidates

// Return updated struct
FeatureKnowledgeReuse {
    delivery_count,
    cross_session_count,
    by_category,
    category_gaps,
    total_served,
    total_stored: 0,    // populated by caller in tools.rs — see below
    cross_feature_reuse,
    intra_cycle_reuse,
    top_cross_feature_entries,
}
```

NOTE on `total_stored`: `compute_knowledge_reuse` does not have access to the `feature_entries`
table count. The caller in `tools.rs` has this data (from the existing feature_entries query in
step 10g). After calling `compute_knowledge_reuse_for_sessions`, the caller must set
`reuse.total_stored` from the `feature_entries` count for this cycle. See caller update below.

---

## Early-exit paths in `compute_knowledge_reuse`

The existing early-exit at step 3 (`!has_any_refs`) and step 5a (`all_entry_ids.is_empty()`)
must also return the new fields. Update both early returns:

```
// Early exit at step 3:
return FeatureKnowledgeReuse {
    delivery_count: 0,
    cross_session_count: 0,
    by_category: HashMap::new(),
    category_gaps: compute_gaps(active_category_counts, &HashSet::new()),
    total_served: 0,
    total_stored: 0,       // caller sets this
    cross_feature_reuse: 0,
    intra_cycle_reuse: 0,
    top_cross_feature_entries: vec![],
}

// Early exit at step 5a:
// Same structure as above.
```

---

## Caller Update: `compute_knowledge_reuse_for_sessions` in `tools.rs`

The current implementation at lines ~2110-2188 performs N individual `store.get(entry_id)`
calls. This is the GH#320 N+1 pattern that must be replaced.

### Replace the per-ID get loop

Current (lines ~2166-2172):
```
for entry_id in &all_entry_ids {
    if let Ok(entry) = store.get(*entry_id).await {
        category_map.insert(*entry_id, entry.category);
    }
}
```

Replace with a single batch IN-clause query:

```
fn build_batch_meta_query(ids: &[u64]) -> String
    // SQL: SELECT id, title, category, feature_cycle FROM entries
    //      WHERE id IN (?, ...) AND status != 'quarantined'
    // Build the ?-list dynamically from ids.len()
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ")
    format!(
        "SELECT id, title, category, feature_cycle \
           FROM entries \
          WHERE id IN ({}) AND status != 'quarantined'",
        placeholders
    )

async fn batch_entry_meta_lookup(
    store: &Arc<SqlxStore>,
    ids: &[u64],
) -> HashMap<u64, EntryMeta>
    // ADR-003: chunked at 100 IDs per IN-clause (pattern #883)
    if ids.is_empty():
        return HashMap::new()

    let mut result: HashMap<u64, EntryMeta> = HashMap::new()

    for chunk in ids.chunks(100):
        let sql = build_batch_meta_query(chunk)
        let mut query = sqlx::query(&sql)
        for &id in chunk:
            query = query.bind(id as i64)   // SQLite stores u64 as i64

        match query.fetch_all(store.write_pool_server()).await:
            Ok(rows) =>
                for row in rows:
                    use sqlx::Row
                    let id: i64 = row.try_get("id").unwrap_or(0)
                    let title: String = row.try_get("title").unwrap_or_default()
                    let category: String = row.try_get("category").unwrap_or_default()
                    let feature_cycle: Option<String> = row.try_get("feature_cycle").ok().flatten()
                    result.insert(id as u64, EntryMeta { title, feature_cycle, category })
            Err(e) =>
                tracing::warn!("col-026: batch entry meta lookup chunk failed: {e}")
                // Continue with partial results; missing entries silently excluded

    result
```

### Updated `compute_knowledge_reuse_for_sessions`

```
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<SqlxStore>,
    session_records: &[SessionRecord],
    feature_cycle: &str,                // NEW param: needed for cross-feature split
) -> Result<FeatureKnowledgeReuse, Box<dyn Error + Send + Sync>>

    // ... existing query_logs, injection_logs, active_cats loading ...

    // Collect all distinct entry IDs (moved before the batch call)
    let mut all_entry_ids: HashSet<u64> = HashSet::new()
    for record in &query_logs:
        let ids: Vec<u64> = serde_json::from_str(&record.result_entry_ids).unwrap_or_default()
        all_entry_ids.extend(ids)
    for record in &injection_logs:
        all_entry_ids.insert(record.entry_id)

    // Batch lookup (single call, chunked internally)
    let ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect()
    let meta_map_owned = batch_entry_meta_lookup(&store, &ids_vec).await

    // Build category_map from meta_map for backward-compatible closure
    let category_map: HashMap<u64, String> = meta_map_owned.iter()
        .map(|(&id, meta)| (id, meta.category.clone()))
        .collect()

    // Call compute_knowledge_reuse with BOTH closures
    let mut reuse = compute_knowledge_reuse(
        &query_logs,
        &injection_logs,
        &active_cats,
        feature_cycle,
        |entry_id| category_map.get(&entry_id).cloned(),
        |ids| {
            // Closure receives the same IDs that were already fetched.
            // Return a filtered view of meta_map_owned.
            ids.iter()
               .filter_map(|id| meta_map_owned.get(id).map(|m| (*id, m.clone_meta())))
               .collect()
        },
    )

    // Caller sets total_stored from feature_entries count
    // (The caller is the handler in context_cycle_review; it calls this function
    // and then sets reuse.total_stored. See handler update below.)

    Ok(reuse)
```

NOTE: The `feature_cycle` parameter is new. The call site at line ~1472 must be updated:
```
// Old:
compute_knowledge_reuse_for_sessions(&store, &session_records).await

// New:
compute_knowledge_reuse_for_sessions(&store, &session_records, &feature_cycle).await
```

### Handler: set `total_stored` after calling compute_knowledge_reuse_for_sessions

In the handler, after step 13-14 sets `report.feature_knowledge_reuse = Some(reuse)`:

```
// total_stored: count of feature_entries rows for this cycle.
// feature_entries query is already executed in step 10g for per_phase_categories.
// Either reuse that count or issue a simple COUNT query here.
if let Some(ref mut reuse) = report.feature_knowledge_reuse:
    match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?"
    )
    .bind(&feature_cycle)
    .fetch_one(store.write_pool_server())
    .await:
        Ok(count) => reuse.total_stored = count as u64
        Err(e) => tracing::warn!("col-026: total_stored count failed: {e}")
        // reuse.total_stored remains 0; not fatal
```

---

## Construction Sites to Update

### Site 1: `knowledge_reuse.rs` early returns (2 sites within the function)

Both early return `FeatureKnowledgeReuse { ... }` literals must add the five new fields.
See algorithm above.

### Site 2: `types.rs` test fixtures

Search for `FeatureKnowledgeReuse {` in `crates/unimatrix-observe/src/types.rs` test module.
Found at lines ~458 and ~585. Add five new fields:
```
total_served: 0,        // or the appropriate test value
total_stored: 0,
cross_feature_reuse: 0,
intra_cycle_reuse: 0,
top_cross_feature_entries: vec![],
```

### Site 3: `retrospective.rs` test fixtures

Search for `FeatureKnowledgeReuse {` in `crates/unimatrix-server/src/mcp/response/retrospective.rs`
test module. Add same five fields with appropriate test values.

---

## Test Updates in `knowledge_reuse.rs`

All existing test fixtures pass `entry_category_lookup` only. Update each to also pass
`entry_meta_lookup`. Use a synthetic closure that returns a `HashMap<u64, EntryMeta>`:

```
fn empty_meta_lookup() -> impl Fn(&[u64]) -> HashMap<u64, EntryMeta>
    |_ids| HashMap::new()

fn meta_lookup_from(mapping: HashMap<u64, EntryMeta>) -> impl Fn(&[u64]) -> HashMap<u64, EntryMeta>
    move |ids| ids.iter().filter_map(|id| mapping.get(id).map(|m| (*id, m.clone()))).collect()
```

Also add `current_feature_cycle: &str` parameter ("test-cycle" in existing tests).

Existing tests must pass `empty_meta_lookup()` for `entry_meta_lookup`. Existing assertions
must still pass (cross_feature_reuse=0, intra_cycle_reuse=0, top_cross_feature_entries=[]).

### New tests to add

**T-KR-01** (R-04): Batch lookup returns fewer rows than requested
- All served 5 entry IDs; meta_lookup returns metadata for only 3
- Assert: no panic; cross + intra <= delivery_count; missing 2 excluded from buckets

**T-KR-02** (R-04): All metadata missing
- meta_lookup returns empty HashMap
- Assert: cross_feature_reuse=0, intra_cycle_reuse=0, delivery_count unchanged

**T-KR-03** (R-04): Empty entry set skips batch call
- No query_log or injection_log data
- meta_lookup closure should NOT be called (test with a side-effect counter)
- Assert call_count = 0

**T-KR-04**: Cross-feature entries populated
- 2 entries from "col-023", 1 entry from current cycle "col-026"
- meta_lookup returns all three
- Assert: cross_feature_reuse=2, intra_cycle_reuse=1
- Assert: top_cross_feature_entries has 2 entries from "col-023"

**T-KR-05**: top_cross_feature_entries sorted by serve_count descending
- 3 cross-feature entries with serve_counts 5, 2, 8
- Assert order: serve_count 8, 5, 2

**T-KR-06**: top_cross_feature_entries truncated at 5
- 7 cross-feature entries
- Assert top_cross_feature_entries.len() == 5

**T-KR-07**: total_served equals delivery_count
- Assert reuse.total_served == reuse.delivery_count after computation

**T-KR-08**: entry_meta_lookup called exactly once per invocation
- Wrap meta_lookup in a counter; verify call count = 1 per compute_knowledge_reuse call

---

## Error Handling

- Batch query chunk failure: `warn!` and continue with partial results
- Missing entry metadata: silently excluded from cross/intra split; counted in delivery_count
- Empty ID set: batch call skipped; all new fields = 0
- `total_stored` query failure: `warn!`, field remains 0 (not fatal)
