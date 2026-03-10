# C6: Data Flow Debugging

## Purpose

Add `tracing::debug!` at data flow boundaries in `compute_knowledge_reuse_for_sessions` to make #193 diagnosable. No behavioral changes.

## File: `crates/unimatrix-server/src/mcp/tools.rs`

### Change 1: Return type (line 1622)

```
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<unimatrix_store::Store>,
    session_records: &[unimatrix_store::SessionRecord],
) -> std::result::Result<unimatrix_observe::FeatureKnowledgeReuse, Box<dyn std::error::Error + Send + Sync>>
//                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                        was: unimatrix_observe::KnowledgeReuse
```

### Change 2: Debug log after session_id_list (after line 1627)

```
let session_id_list: Vec<String> = session_records
    .iter()
    .map(|sr| sr.session_id.clone())
    .collect();

tracing::debug!(
    "col-020b: knowledge reuse data flow: {} session IDs",
    session_id_list.len()
);
```

### Change 3: Debug log after query_log load (after line 1636)

```
let query_logs = tokio::task::spawn_blocking(move || {
    let refs: Vec<&str> = ids_ql.iter().map(|s| s.as_str()).collect();
    store_ql.scan_query_log_by_sessions(&refs)
})
.await??;

tracing::debug!(
    "col-020b: knowledge reuse data flow: {} query_log records loaded",
    query_logs.len()
);
```

### Change 4: Debug log after injection_log load (after line 1645)

```
let injection_logs = tokio::task::spawn_blocking(move || {
    let refs: Vec<&str> = ids_il.iter().map(|s| s.as_str()).collect();
    store_il.scan_injection_log_by_sessions(&refs)
})
.await??;

tracing::debug!(
    "col-020b: knowledge reuse data flow: {} injection_log records loaded",
    injection_logs.len()
);
```

### Change 5: Debug log after active_cats load (after line 1650)

```
let active_cats =
    tokio::task::spawn_blocking(move || store_ac.count_active_entries_by_category()).await??;

tracing::debug!(
    "col-020b: knowledge reuse data flow: {} active categories",
    active_cats.len()
);
```

### Change 6: Debug log before return (after line 1664, before Ok(reuse))

```
let reuse = crate::mcp::knowledge_reuse::compute_knowledge_reuse(
    &query_logs,
    &injection_logs,
    &active_cats,
    move |entry_id| {
        store_for_lookup
            .get(entry_id)
            .ok()
            .map(|entry| entry.category)
    },
);

tracing::debug!(
    "col-020b: knowledge reuse result: delivery_count={}, cross_session_count={}",
    reuse.delivery_count,
    reuse.cross_session_count
);

Ok(reuse)
```

### Change 7: Update field name in caller (line 1288)

```
// Before:
match compute_knowledge_reuse_for_sessions(&store, &session_records).await {
    Ok(reuse) => report.knowledge_reuse = Some(reuse),
    ...
}

// After:
match compute_knowledge_reuse_for_sessions(&store, &session_records).await {
    Ok(reuse) => report.feature_knowledge_reuse = Some(reuse),
    ...
}
```

### Change 8: Update all RetrospectiveReport literal constructions in tests

In `tools.rs` tests (lines ~2050, ~2096, ~2140, ~2168), every `RetrospectiveReport` literal that has `knowledge_reuse: None` must change to `feature_knowledge_reuse: None`. Similarly, `knowledge_in`/`knowledge_out` in any SessionSummary literals must be renamed.

Locations found via grep:
- Line 1112: `knowledge_reuse: None` -> `feature_knowledge_reuse: None`
- Line 2053: `knowledge_reuse: None` -> `feature_knowledge_reuse: None`
- Line 2099: same
- Line 2143: same
- Line 2171: same

## Error Handling

No change to error handling. The existing `tracing::warn` on error (line 1289) is preserved. Debug logs are informational only.

## Key Test Scenarios

1. **Code review**: Verify 4 `tracing::debug!` statements are present at the correct locations
2. **No behavioral change**: Existing tests continue to pass without modification (beyond type/field renames from C4/C7)
3. **Manual validation path**: `RUST_LOG=unimatrix_server=debug` + `context_retrospective` call shows non-zero counts when MCP tools were used
