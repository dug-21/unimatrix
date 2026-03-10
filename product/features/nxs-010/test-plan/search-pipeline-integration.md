# Test Plan: search-pipeline-integration (C5)

## Component

`crates/unimatrix-server/src/uds/listener.rs` (UDS path) and `crates/unimatrix-server/src/services/search.rs` or tool handler (MCP path). Fire-and-forget `query_log` writes after search.

## Risks Covered

| Risk ID | Priority | Scenarios |
|---------|----------|-----------|
| R-04 | Critical | Fire-and-forget panic safety |
| R-05 | High | UDS/MCP field parity |

## Integration Tests (Rust, in unimatrix-server crate)

These tests require the server crate's test infrastructure. They verify that search operations produce query_log rows with correct field values.

### test_uds_search_writes_query_log

**Arrange**: Start a test server with UDS listener. Store an entry. Prepare a search request with session_id="test-sess".
**Act**: Send search request via UDS. Wait for fire-and-forget write to complete (short sleep or poll).
**Assert**:
- `store.scan_query_log_by_session("test-sess")` returns 1 row.
- Row has `source = "uds"`.
- Row has `retrieval_mode = "strict"`.
- Row has `query_text` matching the search query.
- Row has `result_count` matching the number of search results.
- Row has `ts > 0`.

**AC**: AC-12, AC-16, AC-17

### test_mcp_search_writes_query_log

**Arrange**: Start a test MCP server. Store an entry via context_store. Prepare a context_search request.
**Act**: Invoke context_search tool. Wait for fire-and-forget write.
**Assert**:
- Query log contains a row with `source = "mcp"`.
- Row has `retrieval_mode = "flexible"`.
- Row has correct `result_count` and `result_entry_ids`.

**AC**: AC-13, AC-16, AC-17

### test_uds_search_skips_query_log_when_no_session_id

**Arrange**: Start UDS listener. Send a search request with session_id = None or empty.
**Act**: Execute search.
**Assert**:
- Search results returned normally.
- `query_log` table is empty (write skipped per C-09).

**AC**: C-09 (R-04 guard condition)

### test_mcp_search_writes_query_log_with_empty_session_id

**Arrange**: Start MCP server. Invoke context_search with no session_id in audit context.
**Act**: Execute search.
**Assert**:
- `query_log` row exists with `session_id = ""` (empty string, per C-10).

**AC**: C-10

### test_query_log_write_failure_does_not_affect_search

**Arrange**: This is a structural verification. The code must:
1. Compute search results first.
2. Send/return results to the caller.
3. Then spawn the fire-and-forget write.

**Assert**: Code review verification that the spawn_blocking (or spawn_blocking_fire_and_forget) call occurs after the response is prepared. If the write panics or errors, the result is already delivered.

**AC**: C-03 (R-04 mitigation)

### test_uds_mcp_field_parity

**Arrange**: Execute one search via UDS path and one via MCP path with the same query text and comparable result sets.
**Act**: Read both query_log rows.
**Assert**:
- Both rows have `result_entry_ids` as valid JSON arrays of u64.
- Both rows have `similarity_scores` as valid JSON arrays of f64.
- `result_count` matches the length of `result_entry_ids` in both rows.
- Fields that differ: `source` ("uds" vs "mcp"), `retrieval_mode` ("strict" vs "flexible"), `session_id`.
- Fields with identical structure: `query_text`, `ts` (both > 0), `result_entry_ids` format, `similarity_scores` format.

**AC**: AC-14, AC-15 (R-05 mitigation via shared constructor FR-08.1)

## Integration Harness Tests (infra-001)

No new harness tests needed. query_log is not exposed via any MCP tool, so its correctness cannot be verified through the MCP interface. Existing `tools` suite tests validate that `context_search` still returns correct results after the code change (no regression from the added fire-and-forget write).

Run existing suites as regression check:
- `test_tools.py` -- context_search still works
- `test_lifecycle.py` -- restart persistence unaffected
- `test_edge_cases.py` -- boundary values unaffected

## Notes

- The shared `QueryLogRecord` constructor (FR-08.1) is verified by the field parity test. If UDS and MCP use different construction paths, the test catches divergence.
- Fire-and-forget error handling is verified by code structure (warn log, no error propagation). A true error-injection test would require mocking the Store, which is not practical in integration tests. The critical assertion is that no panic propagates -- this is ensured by catching all `Result::Err` before the `unwrap` boundary.
- R-09 (concurrent Store::open) is accepted risk. SQLite's exclusive transaction during migration provides serialization. No test needed.
- R-11 (write lock contention) is accepted risk. Sequential UDS and MCP processing prevents concurrent writes.
