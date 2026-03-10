# Agent Report: nxs-010-agent-7-search-pipeline

## Component
search-pipeline-integration (C5)

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs` -- Added `QueryLogRecord` import and fire-and-forget query_log write in `handle_context_search` (step 10c, after injection_log write)
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs` -- Added `QueryLogRecord` import and fire-and-forget query_log write in `context_search` (step 7, after usage recording)

## Implementation Details

### UDS Path (listener.rs)
- Guard: `if let Some(ref sid) = session_id` + `!sid.is_empty()` (C-09)
- Extracts entry_ids and scores from `filtered: Vec<(EntryRecord, f64)>`
- Uses `QueryLogRecord::new()` shared constructor (FR-08.1)
- Source: "uds", retrieval_mode: "strict"
- Pattern: `spawn_blocking_fire_and_forget` (matches injection_log precedent)
- Error handling: `tracing::warn!` with session_id, query_len, error (C-03)

### MCP Path (tools.rs)
- No guard -- always writes (C-10). Uses `unwrap_or_default()` for session_id
- Extracts entry_ids and scores from `search_results.entries`
- Uses `QueryLogRecord::new()` shared constructor (FR-08.1)
- Source: "mcp", retrieval_mode: "flexible"
- Pattern: `let _ = tokio::task::spawn_blocking(...)` (matches existing MCP fire-and-forget)
- Error handling: `tracing::warn!` with query_len, error (C-03)

## Test Results
- 876 passed, 0 failed (unimatrix-server lib tests, excluding 1 pre-existing migration version assertion failure)
- Pre-existing failures: `test_migration_v7_to_v8_backfill` (asserts version==10, now 11) and `test_schema_version_is_9` in sqlite_parity (same root cause -- migration agent bumped version)

## Issues / Blockers
- None. The query_log module was already available from agent-6's work. Build and tests pass cleanly.
