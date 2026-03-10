# nxs-010: Activity Schema Evolution

## Problem Statement

The observation pipeline captures thousands of events per day but cannot connect them to topics or evaluate search quality. Two structural gaps block the Activity Intelligence milestone:

1. **No topic delivery grouping.** Sessions are the finest-grained unit of activity tracking. A topic (e.g., "col-016") spans 5-10 sessions, but there is no table to aggregate them. Cross-session analysis, multi-session retrospectives (col-020), and knowledge effectiveness scoring (crt-018) all require a `topic_deliveries` anchor table.

2. **No query text capture.** The `injection_log` records which entries were served, and `audit_log` records "returned N results," but neither stores the query that triggered the retrieval. Without query text, search quality evaluation (crt-019), knowledge gap detection, and embedding tuning data export (col-021) are impossible.

Additionally, sessions already attributed to topics via col-017's hook-side attribution need somewhere to roll up their aggregate counters. And all existing unattributed sessions with topic-identifiable observations need backfilling.

This is a Wave 2 feature in the Activity Intelligence milestone, depending on col-017 (hook-side topic attribution) and col-019 (PostToolUse response capture) from Wave 1.

## Goals

1. Create the `topic_deliveries` table linking topic names to aggregate session counters, lifecycle status, and optional GitHub issue numbers
2. Create the `query_log` table capturing search query text alongside result metadata (result count, entry IDs, similarity scores, retrieval mode, source transport)
3. Migrate existing databases from schema v9 to v10 (note: `CURRENT_SCHEMA_VERSION` is already 10 due to col-017's `topic_signal` ALTER TABLE; this feature bumps to v11)
4. Backfill `topic_deliveries` rows from all existing attributed sessions (those with non-NULL `feature_cycle`)
5. Add Store API methods for `topic_deliveries` CRUD and `query_log` batch insert + query
6. Add `query_log` counter (`next_query_id`) to the counters table
7. Wire `query_log` writes into the search pipeline (both UDS `handle_context_search` and MCP `SearchService::search`)

## Non-Goals

- **No changes to the observations table** -- the `topic_signal` column was added by col-017 (already in v10 migration)
- **No topic attribution logic** -- attribution is col-017's responsibility; nxs-010 only provides the storage layer that receives attribution results
- **No multi-session retrospective computation** -- that is col-020, which consumes `topic_deliveries` after nxs-010 creates it
- **No knowledge effectiveness analysis** -- that is crt-018, downstream of nxs-010
- **No search quality analysis or gap detection** -- that is crt-019, consuming `query_log` data
- **No query data export pipeline** -- that is col-021
- **No GC policy for query_log** -- will be designed separately when data volume warrants it (estimated ~6K rows/year is negligible)
- **No `topic` column renaming of existing `feature_cycle` columns** -- backward compatibility maintained; new tables use `topic` as the canonical name per ASS-018 naming decision
- **No changes to SearchService pipeline logic** -- only adding a fire-and-forget write after results are computed

## Background Research

### Existing Schema State (v9/v10)

The database currently has 18 tables at schema v9 (production) or v10 (with col-017's `topic_signal` column). `CURRENT_SCHEMA_VERSION` in `migration.rs` is already set to 10. The nxs-010 migration will target v11.

Key tables relevant to this feature:
- `sessions` -- has `feature_cycle TEXT` (nullable, currently populated by col-017 attribution)
- `observation_metrics` -- keyed by `feature_cycle`, stores 22 computed metrics per topic
- `injection_log` -- records (session_id, entry_id, confidence, timestamp) but no query text
- `entries` -- has `topic TEXT` for knowledge-side grouping, semantically equivalent to activity-side `feature_cycle`

### Migration Pattern

All migrations follow the same pattern in `migration.rs`:
1. Check `current_version < N` guard
2. Guard against partial re-runs (check column/table existence via `pragma_table_info`)
3. Execute DDL in a transaction
4. Backfill data if needed
5. Update schema_version counter

Heavy migrations (v5->v6, v8->v9) that drop/recreate tables run in separate transactions after the main migration commits. The nxs-010 migration is additive (CREATE TABLE + INSERT backfill) and can run within the main transaction.

### Search Pipeline Write Points

Two search paths exist:
1. **UDS path** (`handle_context_search` in `listener.rs`): Hook-triggered, Strict retrieval mode, already writes to `injection_log` fire-and-forget
2. **MCP path** (`SearchService::search` in `search.rs`): Tool-invoked, Flexible retrieval mode, audit_log records "returned N results"

Both paths produce the data needed for `query_log`: query text, result entry IDs, similarity scores, retrieval mode, and result count. The write can follow the existing fire-and-forget pattern used by injection_log.

### Naming Decision (from ASS-018)

New tables use `topic` as the canonical grouping name. Existing `feature_cycle` columns remain for backward compatibility. `topic` and `feature_cycle` are semantically equivalent -- both refer to the body of work that groups sessions and knowledge entries.

### Counter Pattern

The store uses named counters in the `counters` table for ID allocation: `next_entry_id`, `next_signal_id`, `next_log_id`, `next_audit_event_id`. `query_log` will use `next_query_id` (or AUTOINCREMENT, since `query_log.query_id` is defined as `INTEGER PRIMARY KEY AUTOINCREMENT` in the research proposal -- AUTOINCREMENT handles ID allocation natively in SQLite without a counter).

## Proposed Approach

### 1. Schema DDL (in `create_tables()`)

Add two new tables to the `create_tables()` function in `db.rs`:

```sql
CREATE TABLE IF NOT EXISTS topic_deliveries (
    topic TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL DEFAULT 'active',
    github_issue INTEGER,
    total_sessions INTEGER NOT NULL DEFAULT 0,
    total_tool_calls INTEGER NOT NULL DEFAULT 0,
    total_duration_secs INTEGER NOT NULL DEFAULT 0,
    phases_completed TEXT
);

CREATE TABLE IF NOT EXISTS query_log (
    query_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    ts INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,
    similarity_scores TEXT,
    retrieval_mode TEXT,
    source TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id);
CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts);
```

### 2. Migration (v10 -> v11)

In `migration.rs`, add a `current_version < 11` block that:
- Creates both tables (idempotent via IF NOT EXISTS)
- Backfills `topic_deliveries` from existing attributed sessions:
  ```sql
  INSERT OR IGNORE INTO topic_deliveries (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
  SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0, COALESCE(SUM(ended_at - started_at), 0)
  FROM sessions
  WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
  GROUP BY feature_cycle;
  ```
- Updates `CURRENT_SCHEMA_VERSION` to 11

### 3. Store API

New module `crates/unimatrix-store/src/topic_deliveries.rs`:
- `TopicDeliveryRecord` struct (mirrors table columns)
- `Store::upsert_topic_delivery(&self, record: &TopicDeliveryRecord) -> Result<()>`
- `Store::get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>>`
- `Store::update_topic_delivery_counters(&self, topic: &str, sessions_delta: i64, tool_calls_delta: i64, duration_delta: i64) -> Result<()>` -- atomic counter increment
- `Store::list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>>` -- for status reporting

New module `crates/unimatrix-store/src/query_log.rs`:
- `QueryLogRecord` struct (mirrors table columns)
- `Store::insert_query_log(&self, record: &QueryLogRecord) -> Result<()>` -- single insert (AUTOINCREMENT handles ID)
- `Store::scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>>` -- for analysis

### 4. Search Pipeline Integration

After search results are computed, insert a `query_log` row:
- In `handle_context_search` (UDS): after injection_log write, add fire-and-forget query_log insert
- In `SearchService::search` (MCP): after audit event emission, add fire-and-forget query_log insert via `spawn_blocking`

Both paths capture: query text, session_id, result count, entry IDs (JSON array), similarity scores (JSON array), retrieval mode ("strict"/"flexible"), source ("uds"/"mcp").

## Acceptance Criteria

- AC-01: `topic_deliveries` table exists in schema v11 with columns: topic (PK), created_at, completed_at, status, github_issue, total_sessions, total_tool_calls, total_duration_secs, phases_completed
- AC-02: `query_log` table exists in schema v11 with columns: query_id (PK AUTOINCREMENT), session_id, query_text, ts, result_count, result_entry_ids, similarity_scores, retrieval_mode, source
- AC-03: `query_log` has indexes on session_id and ts
- AC-04: Migration from v10 to v11 creates both tables and backfills `topic_deliveries` from existing attributed sessions
- AC-05: Migration is idempotent -- running on a v11 database is a no-op
- AC-06: Migration handles empty sessions table (no attributed sessions) without error
- AC-07: `Store::upsert_topic_delivery` inserts a new row or replaces an existing row
- AC-08: `Store::get_topic_delivery` returns None for nonexistent topics
- AC-09: `Store::update_topic_delivery_counters` atomically increments counters on an existing row
- AC-10: `Store::insert_query_log` writes a row with auto-allocated query_id
- AC-11: `Store::scan_query_log_by_session` returns all query_log rows for a given session_id, ordered by ts
- AC-12: UDS `handle_context_search` writes a `query_log` row after every successful search (fire-and-forget; search latency unaffected)
- AC-13: MCP `SearchService::search` writes a `query_log` row after every successful search (fire-and-forget)
- AC-14: `query_log.result_entry_ids` is a JSON array of entry ID integers
- AC-15: `query_log.similarity_scores` is a JSON array of f64 values, parallel to result_entry_ids
- AC-16: `query_log.source` is "uds" for hook-triggered searches and "mcp" for tool-invoked searches
- AC-17: `query_log.retrieval_mode` is "strict" or "flexible" matching the search path
- AC-18: Backfill computes `total_sessions` and `total_duration_secs` from existing session data
- AC-19: Backfill sets `status = 'completed'` for historically attributed topics (conservative default)
- AC-20: All existing unit and integration tests continue to pass (no regressions)

## Constraints

- **Schema version conflict**: `CURRENT_SCHEMA_VERSION` is already 10 (set by col-017). This feature must target v11, not v10. The migration guard must be `current_version < 11`.
- **col-017 dependency**: The `topic_signal` column on observations and the hook-side attribution that populates `sessions.feature_cycle` must land before nxs-010. Without attributed sessions, backfill produces empty `topic_deliveries`.
- **Fire-and-forget latency**: Query log writes must not add observable latency to search responses. Use `spawn_blocking` or inline writes after the response is prepared, matching the injection_log pattern.
- **AUTOINCREMENT vs counter**: `query_log` uses SQLite AUTOINCREMENT (already proposed in ASS-018). This avoids adding another named counter but means IDs are not contiguous after deletions. This is acceptable -- query_log rows are append-only.
- **JSON encoding consistency**: `result_entry_ids` and `similarity_scores` must use the same JSON serialization as other JSON columns in the schema (e.g., `signal_queue.entry_ids`, `agent_registry.capabilities`). Use `serde_json::to_string`.
- **Transaction scope**: Backfill runs within the main migration transaction. If the sessions table is large, this could hold the write lock briefly. Acceptable given typical session counts (~500 sessions per 60 days based on ASS-018 data projections).
- **Backward compatibility**: Opening a v11 database with older server code that expects v10 will see unknown tables but not crash (SQLite ignores unknown tables). The `counters.schema_version` check will fail fast if a version mismatch causes issues.

## Open Questions

1. **Should `topic_deliveries.total_tool_calls` be backfilled from `observation_metrics`?** The sessions table has no tool_call count. We could join with `observation_metrics.total_tool_calls` during backfill, or leave it at 0 and let col-020 (multi-session retrospective) populate it. Recommendation: leave at 0 during migration to keep the backfill SQL simple; col-020 will recompute.

2. **Should `query_log` capture the query embedding hash?** This would enable deduplication detection (same query text, different embedding due to model change). Not in ASS-018 proposal. Likely unnecessary for v1 -- defer.

3. **Should `topic_deliveries` have a foreign key to `sessions.feature_cycle`?** SQLite FK enforcement is opt-in and the relationship is 1:N with sessions pointing to topic_deliveries, not the reverse. Adding a FK on sessions would require ALTER TABLE. Recommendation: no FK; enforce at application level.

4. **Integration test strategy**: Should tests use the full search pipeline (requiring embedding model) or mock the search results and only test the query_log write? The existing injection_log tests in listener.rs use the full pipeline. Follow the same pattern.

## Tracking

https://github.com/dug-21/unimatrix/issues/185
