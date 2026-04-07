# Component 4: compute_knowledge_reuse_for_sessions — `unimatrix-server/src/mcp/tools.rs`

## Purpose

Orchestrate data loading for the knowledge reuse sub-pipeline. Extend the function to
accept the attributed observation slice, extract explicit read IDs from it, perform the
second `batch_entry_meta_lookup` call for the explicit read category join (with cardinality
cap), and pass the results into `compute_knowledge_reuse`. Update the single call site in
`context_cycle_review` to pass `&attributed`.

---

## Constant Definition

Add the following constant in `tools.rs`, placed immediately before or after
`compute_knowledge_reuse_for_sessions` (near line 3198):

```
/// Maximum number of distinct explicit read IDs passed to batch_entry_meta_lookup
/// for the explicit_read_by_category category join (ADR-004).
/// The cap applies only to the category join input.
/// explicit_read_count is always computed from the full uncapped HashSet.
const EXPLICIT_READ_META_CAP: usize = 500;
```

---

## Modified Function Signature

```
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<unimatrix_store::SqlxStore>,
    session_records: &[unimatrix_store::SessionRecord],
    current_feature_cycle: &str,
    attributed: &[ObservationRecord],           // NEW parameter
) -> Result<FeatureKnowledgeReuse, Box<dyn std::error::Error + Send + Sync>>
```

The `attributed` parameter is the full unfiltered observation slice loaded at step 12 of
`context_cycle_review`. Constraint C-04: this slice must not be pre-filtered before being
passed in.

Required import additions (if not already present):
```
use unimatrix_core::observation::ObservationRecord;
// (or whichever path exposes ObservationRecord — verify against existing imports in tools.rs)
```

`extract_explicit_read_ids` is referenced as:
```
crate::mcp::knowledge_reuse::extract_explicit_read_ids(attributed)
```

---

## Algorithm

The function body extends the existing steps. Insert the new steps between the existing
`all_entry_ids` collection and the first `batch_entry_meta_lookup` call.

```
FUNCTION compute_knowledge_reuse_for_sessions(
    store, session_records, current_feature_cycle, attributed
) -> Result<FeatureKnowledgeReuse, ...>:

    // --- EXISTING STEPS (unchanged) ---

    // Build session_id_list from session_records
    let session_id_list = session_records.iter().map(|sr| sr.session_id.clone()).collect()

    // Load query_log
    let query_logs = store.scan_query_log_by_sessions(&refs_ql).await?

    // Load injection_log
    let injection_logs = store.scan_injection_log_by_sessions(&refs_il).await?

    // Load active category counts
    let active_cats = store.count_active_entries_by_category().await?

    // Collect all distinct IDs from query_log + injection_log for existing meta lookup
    let mut all_entry_ids: HashSet<u64> = HashSet::new()
    FOR record IN &query_logs:
        ids = from_str(record.result_entry_ids) unwrap_or_default
        all_entry_ids.extend(ids)
    FOR record IN &injection_logs:
        all_entry_ids.insert(record.entry_id)

    // --- NEW STEPS (crt-049) ---

    // Step A: Extract explicit read IDs from in-memory attributed slice (no DB call)
    let explicit_ids: HashSet<u64> =
        crate::mcp::knowledge_reuse::extract_explicit_read_ids(attributed)

    tracing::debug!(
        "crt-049: explicit read IDs extracted: {} distinct",
        explicit_ids.len()
    )

    // Step B: Apply cardinality cap before category join lookup (ADR-004)
    // Cap applies only to the batch lookup input, NOT to explicit_read_count.
    let explicit_ids_vec: Vec<u64> = explicit_ids.iter().copied().collect()

    let lookup_ids: &[u64] = if explicit_ids_vec.len() > EXPLICIT_READ_META_CAP {
        tracing::warn!(
            "crt-049: explicit read ID set ({}) exceeds cap {}; \
             explicit_read_by_category will be partial",
            explicit_ids_vec.len(),
            EXPLICIT_READ_META_CAP
        )
        &explicit_ids_vec[..EXPLICIT_READ_META_CAP]
    } else {
        &explicit_ids_vec
    }

    // Step C: Batch metadata lookup for explicit read category join (new DB call)
    // Uses same batch_entry_meta_lookup function as existing call (pattern #883, col-026).
    // Chunked at 100 IDs per IN-clause; no N+1 per-ID queries.
    // Returns empty map when lookup_ids is empty (no DB call made in that case).
    let explicit_meta_map: HashMap<u64, EntryMeta> =
        batch_entry_meta_lookup(store, lookup_ids).await

    // --- EXISTING STEPS (continued, unchanged) ---

    // Existing batch metadata lookup for query_log + injection_log IDs
    let ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect()
    let meta_map_owned: HashMap<u64, EntryMeta> =
        batch_entry_meta_lookup(store, &ids_vec).await

    // Build category_map closure (unchanged)
    let category_map: HashMap<u64, String> = meta_map_owned.iter()
        .map(|(&id, meta)| (id, meta.category.clone()))
        .collect()

    // Delegate to compute_knowledge_reuse (extended signature)
    let reuse = crate::mcp::knowledge_reuse::compute_knowledge_reuse(
        &query_logs,
        &injection_logs,
        &active_cats,
        current_feature_cycle,
        |entry_id| category_map.get(&entry_id).cloned(),   // existing closure
        |ids| {                                             // existing closure
            ids.iter()
                .filter_map(|id| {
                    meta_map_owned.get(id).map(|m| {
                        (*id, EntryMeta {
                            title: m.title.clone(),
                            feature_cycle: m.feature_cycle.clone(),
                            category: m.category.clone(),
                        })
                    })
                })
                .collect()
        },
        &explicit_ids,          // NEW arg
        &explicit_meta_map,     // NEW arg
    )

    tracing::debug!(
        "crt-049: knowledge reuse result: search_exposure_count={}, \
         explicit_read_count={}, total_served={}, cross_feature={}, intra_cycle={}",
        reuse.search_exposure_count,
        reuse.explicit_read_count,
        reuse.total_served,
        reuse.cross_feature_reuse,
        reuse.intra_cycle_reuse,
    )

    Ok(reuse)

END FUNCTION
```

### Two batch_entry_meta_lookup Calls

After this change, `compute_knowledge_reuse_for_sessions` makes two calls to
`batch_entry_meta_lookup`:

1. Existing call (line ~3257): `batch_entry_meta_lookup(store, &ids_vec).await`
   — for query_log + injection_log IDs. Unchanged.

2. New call (Step C): `batch_entry_meta_lookup(store, lookup_ids).await`
   — for explicit read IDs (capped at 500). Inserted before existing call.

Both calls use the same `store` reference and `await` sequentially. The pool connection
is released between each `await` — no connection pinning across two calls (I-02 from
risk strategy is safe in the sequential async pattern).

### Call Site Update (context_cycle_review, step 13-14)

In `context_cycle_review` handler in `tools.rs` at the existing call to
`compute_knowledge_reuse_for_sessions` (line ~1949):

```
// CURRENT:
let reuse = compute_knowledge_reuse_for_sessions(
    &store, &session_records, feature_cycle.as_str()
).await?;

// NEW:
let reuse = compute_knowledge_reuse_for_sessions(
    &store, &session_records, feature_cycle.as_str(), &attributed
).await?;
```

`attributed` is already in scope at this point (loaded at step 12, line ~1945).
Constraint C-04: pass the unfiltered slice. Do not filter by session before passing.

---

## Error Handling

The function returns `Result<..., Box<dyn Error + Send + Sync>>`. Error propagation
is unchanged from the existing function — `?` on each `store.*` call. No new error
variants are introduced.

`batch_entry_meta_lookup` is infallible (returns empty map on failure, per its existing
implementation at line 3143). The new call for explicit reads follows the same pattern:
if the lookup fails (e.g., store error), `explicit_meta_map` is empty, `explicit_read_by_category`
is empty, but `explicit_read_count` is still accurate (it comes from `explicit_ids.len()`).

`extract_explicit_read_ids` is infallible (returns `HashSet<u64>`, no error path).

---

## Key Test Scenarios

All tests in the `#[cfg(test)]` module in `tools.rs`. Extend the existing module.

### Update existing test: test_compute_knowledge_reuse_for_sessions_no_block_on_panic

This test (line ~4753) calls `compute_knowledge_reuse_for_sessions` with the old signature.
It must be updated to pass `&[]` for the new `attributed` parameter:

```
// CURRENT call in the test:
compute_knowledge_reuse_for_sessions(&store, &sessions, "test-cycle").await

// UPDATED:
compute_knowledge_reuse_for_sessions(&store, &sessions, "test-cycle", &[]).await
```

This is a compile-time change — the test will fail to compile until updated.

### AC-05 — explicit reads produce non-zero explicit_read_count

```
Test: test_compute_knowledge_reuse_for_sessions_with_explicit_reads

Setup:
    - Store an entry with category "decision"
    - Build a synthetic ObservationRecord slice with one PreToolUse context_get event
      targeting the stored entry's ID
    - Create a session record for the cycle

Call:
    reuse = compute_knowledge_reuse_for_sessions(
        &store, &[session], "test-cycle", &attributed
    ).await?

Assertions:
    assert reuse.explicit_read_count == 1
    assert reuse.explicit_read_by_category.get("decision") == Some(&1)
    assert reuse.total_served >= 1   // at least the one explicit read
```

### AC-07 cardinality cap boundary (from R-04)

```
Test: test_explicit_read_meta_cap_applied

Setup:
    - Build an attributed slice with 501 distinct PreToolUse context_get events
      (501 different entry IDs)
    - Do not require storing 501 real entries — explicit_ids.len() = 501 is enough
      to trigger the warn; the meta lookup returns empty for non-existent IDs.

Assertions:
    assert reuse.explicit_read_count == 501   // full uncapped count
    // explicit_read_by_category may be empty or partial (IDs don't exist in store)
    // The tracing::warn must be emitted — verify via log capturing if available
    // Otherwise: structural check that EXPLICIT_READ_META_CAP constant == 500
```

### Structural: EXPLICIT_READ_META_CAP constant exists and equals 500

```
Test: test_explicit_read_meta_cap_constant
    assert EXPLICIT_READ_META_CAP == 500
```

---

## Integration Surface

| Name | Signature | Notes |
|------|-----------|-------|
| `compute_knowledge_reuse_for_sessions` | `async fn(&Arc<SqlxStore>, &[SessionRecord], &str, &[ObservationRecord]) -> Result<FeatureKnowledgeReuse, ...>` | One call site in `context_cycle_review` |
| `EXPLICIT_READ_META_CAP` | `const usize = 500` | Private to `tools.rs` module |
| `batch_entry_meta_lookup` | existing `async fn` | Called twice: once for ql+inj IDs, once for explicit read IDs |
| `extract_explicit_read_ids` | `fn(&[ObservationRecord]) -> HashSet<u64>` | From `knowledge_reuse.rs` via `crate::mcp::knowledge_reuse::` |
