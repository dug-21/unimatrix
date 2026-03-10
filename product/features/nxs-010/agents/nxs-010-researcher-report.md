# Agent Report: nxs-010-researcher

## Task
Research problem space for nxs-010 (Activity Schema Evolution) and produce SCOPE.md.

## Deliverable
`/workspaces/unimatrix/product/features/nxs-010/SCOPE.md`

## Key Findings

### 1. Schema Version Conflict
`CURRENT_SCHEMA_VERSION` in `migration.rs` is already 10 (set by col-017 for the `topic_signal` ALTER TABLE). nxs-010 must target v11, not v10. The migration code at line 129-145 already handles the v9->v10 topic_signal migration. The nxs-010 migration block needs to be `current_version < 11`.

### 2. Backfill Data Availability
The `sessions` table has `feature_cycle` (nullable TEXT) which col-017 populates via hook-side attribution. Backfill can aggregate from sessions WHERE feature_cycle IS NOT NULL. However, `total_tool_calls` is not available from sessions -- it lives in `observation_metrics`. Recommendation: backfill tool_calls as 0, let col-020 recompute.

### 3. Two Search Paths Need query_log Writes
- UDS path: `handle_context_search` in `listener.rs` (line 812) -- already does fire-and-forget injection_log writes
- MCP path: `SearchService::search` in `search.rs` (line 108) -- emits audit events after results

Both have all data needed for query_log: query text, result entry IDs, similarity scores, retrieval mode, session_id.

### 4. AUTOINCREMENT vs Counter
ASS-018 proposes `query_id INTEGER PRIMARY KEY AUTOINCREMENT`. The rest of the schema uses named counters (`next_log_id`, `next_signal_id`). AUTOINCREMENT is simpler for append-only tables. Recommendation: use AUTOINCREMENT for query_log (matches the ASS-018 proposal, no new counter needed).

### 5. Naming Alignment
Per ASS-018 naming decision, new tables use `topic` (not `feature_cycle`). The `topic_deliveries` table uses `topic TEXT PRIMARY KEY`. This aligns with `entries.topic` on the knowledge side while `sessions.feature_cycle` remains unchanged for backward compatibility.

## Risks
- col-017 must land first; without attributed sessions, topic_deliveries backfill is empty
- Fire-and-forget query_log writes must not slow search latency (use existing spawn_blocking pattern)
- Schema version must be 11, not 10 -- easy to get wrong if someone doesn't check migration.rs

## Status
COMPLETE
