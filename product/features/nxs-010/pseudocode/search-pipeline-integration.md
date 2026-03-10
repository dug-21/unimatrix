# C5: Search Pipeline Integration (listener.rs + tools.rs)

## Purpose

Wire fire-and-forget `query_log` writes into both search paths (UDS and MCP) after search results are computed. Both paths use the shared `QueryLogRecord::new` constructor and the same `Store::insert_query_log` method.

## UDS Path: handle_context_search (listener.rs)

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Function**: `handle_context_search`
**Location**: After the injection_log batch write block (step 10b, ~line 906), before the co-access pair recording (step 11, ~line 909).

### Pseudocode

```
// 10c. nxs-010: Persist query_log row (fire-and-forget, ADR-002)
if let Some(ref sid) = session_id {
    if !sid.is_empty() {
        // Extract entry IDs and similarity scores as parallel arrays
        let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect();
        let scores: Vec<f64> = filtered.iter().map(|(_, sim)| *sim).collect();

        let record = QueryLogRecord::new(
            sid.clone(),
            query.clone(),          // query: String is available in scope
            &entry_ids,
            &scores,
            "strict",               // UDS always uses strict retrieval mode
            "uds",                  // source transport
        );

        let store_clone = Arc::clone(store);
        let sid_clone = sid.clone();
        spawn_blocking_fire_and_forget(move || {
            if let Err(e) = store_clone.insert_query_log(&record) {
                tracing::warn!(
                    session_id = %sid_clone,
                    query_len = record.query_text.len(),
                    error = %e,
                    "query_log write failed"
                );
            }
        });
    }
}
```

### Guard condition (ADR-002, C-09)

Skip the write if `session_id` is None or empty. This matches the injection_log guard pattern on line 881-882 of the existing code. A query_log row without session_id has limited analytical value for UDS-originated queries.

### Data extraction

- `filtered: Vec<(EntryRecord, f64)>` is already available in scope (computed at step 4, line 863).
- `query: String` is a parameter of `handle_context_search`.
- `store: &Arc<Store>` is a parameter.
- Entry IDs and similarity scores are extracted as parallel `Vec` from `filtered`.

### Fire-and-forget pattern

Uses `spawn_blocking_fire_and_forget` (existing helper at line 73). Matches the injection_log pattern exactly. The `move` closure captures `store_clone`, `record`, and `sid_clone` (for the warn log).

### Error handling (ADR-002, C-03)

On `insert_query_log` error: log at `warn` level with session_id, query length, and error message. No retry. No error propagation. The search response (HookResponse::Entries) is already being constructed at step 12, unaffected.

## MCP Path: context_search tool handler (tools.rs)

**File**: `crates/unimatrix-server/src/mcp/tools.rs`
**Function**: `context_search` (the `#[tool]` handler)
**Location**: After usage recording (step 6, ~line 328), before `Ok(result)` (line 330).

### Pseudocode

```
// 7. nxs-010: Query log recording (fire-and-forget, ADR-002)
{
    let entry_ids: Vec<u64> = search_results.entries.iter().map(|se| se.entry.id).collect();
    let scores: Vec<f64> = search_results.entries.iter().map(|se| se.similarity).collect();

    let session_id_for_log = ctx.audit_ctx.session_id.clone().unwrap_or_default();

    let record = QueryLogRecord::new(
        session_id_for_log,
        params.query.clone(),
        &entry_ids,
        &scores,
        "flexible",             // MCP always uses flexible retrieval mode
        "mcp",                  // source transport
    );

    let store_clone = Arc::clone(&self.store);
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = store_clone.insert_query_log(&record) {
            tracing::warn!(
                query_len = record.query_text.len(),
                error = %e,
                "query_log write failed (mcp)"
            );
        }
    });
}
```

### Guard condition (ADR-002, C-10)

MCP path never skips the write. If `session_id` is None, use empty string (`unwrap_or_default()`). MCP queries are always analytically valuable regardless of session attribution.

### Data extraction

- `search_results: SearchResults` is available (computed at step 4-5).
- `params.query: String` is available from the tool parameters.
- `self.services.store: Arc<Store>` is available via the server struct.
- `ctx.audit_ctx.session_id: Option<String>` provides session attribution.

### Fire-and-forget pattern

Uses `tokio::task::spawn_blocking` with dropped `JoinHandle` (the `let _ =` pattern). This matches the existing pattern used in the MCP path for other fire-and-forget operations. The UDS path's `spawn_blocking_fire_and_forget` helper is defined in `listener.rs` and is not accessible from `tools.rs`, so the MCP path uses the direct tokio API.

### Error handling (ADR-002, C-03)

Identical to UDS: warn log on failure, no retry, no propagation. The `Ok(result)` return is already prepared and returns the search results to the agent.

## Import requirements

### listener.rs

Add to existing imports:
```
use unimatrix_store::QueryLogRecord;
```

`QueryLogRecord` is re-exported from `unimatrix-store/src/lib.rs` (see OVERVIEW.md module registration).

### tools.rs

Add to existing imports:
```
use unimatrix_store::QueryLogRecord;
```

The store is accessible as `self.store: Arc<Store>` directly on the `UnimatrixServer` struct (field at server.rs:112). This is the same `Arc<Store>` used throughout the server.

## Key Test Scenarios

1. **UDS search writes query_log**: Execute a UDS search via `handle_context_search`. After completion, verify a query_log row exists with source="uds", retrieval_mode="strict", correct session_id, and result_count matching the number of results. (AC-12)

2. **MCP search writes query_log**: Execute an MCP search via the `context_search` tool. After completion, verify a query_log row exists with source="mcp", retrieval_mode="flexible". (AC-13)

3. **UDS skips on empty session_id**: Call `handle_context_search` with session_id=None. Verify no query_log row is written. (C-09, R-04 scenario 4)

4. **MCP writes with empty session_id**: Call `context_search` with no session_id in audit context. Verify query_log row written with session_id="". (C-10)

5. **Field parity between paths**: Execute one search via UDS and one via MCP with the same query yielding the same results. Compare the two query_log rows. Verify result_entry_ids and similarity_scores JSON arrays have identical structure. Only source, retrieval_mode, and session_id differ. (R-05, AC-16, AC-17)

6. **Fire-and-forget on failure**: Simulate insert_query_log failure (e.g., closed store). Verify warn log is emitted and search response is returned normally. No panic. (R-04, C-03)

7. **Zero results**: Execute a search that returns 0 results. Verify query_log row is still written with result_count=0, result_entry_ids="[]", similarity_scores="[]". (R-06)
