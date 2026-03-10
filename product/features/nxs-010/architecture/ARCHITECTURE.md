# nxs-010: Activity Schema Evolution -- Architecture

## System Overview

nxs-010 extends the Unimatrix storage layer with two new tables that connect the observation pipeline to topic-level aggregation and search quality telemetry. It sits at the foundation of the Activity Intelligence milestone: `topic_deliveries` anchors cross-session analysis for col-020 and crt-018, while `query_log` captures search telemetry for crt-019 and col-021.

The feature touches three crates:
- **unimatrix-store**: New tables, migration v10->v11, Store API methods (two new modules)
- **unimatrix-server (UDS)**: query_log write in `handle_context_search`
- **unimatrix-server (MCP)**: query_log write in `context_search` tool handler

No new crates. No new dependencies. Purely additive schema evolution with backfill.

## Component Breakdown

### C1: Schema DDL (db.rs)

**Responsibility**: Define `topic_deliveries` and `query_log` tables plus indexes in `create_tables()` for fresh databases.

Two new CREATE TABLE IF NOT EXISTS blocks appended to the existing `create_tables()` function. Both tables use `IF NOT EXISTS` for idempotency. `query_log` uses `INTEGER PRIMARY KEY AUTOINCREMENT` (ADR-001). `topic_deliveries` uses `TEXT PRIMARY KEY` on the topic name.

### C2: Migration v10->v11 (migration.rs)

**Responsibility**: Upgrade existing v10 databases to v11 by creating both tables and backfilling `topic_deliveries` from attributed sessions.

Single `current_version < 11` block inside the existing `migrate_if_needed` transaction. The migration is additive (CREATE TABLE + INSERT) and runs within the main transaction -- no separate transaction needed (unlike v5->v6 or v8->v9 which drop/recreate tables).

Key ordering constraint (Unimatrix #375, #376, SR-01): `migrate_if_needed` runs BEFORE `create_tables()` in `Store::open()`. Both emit identical DDL via `IF NOT EXISTS`, so no conflict. The migration creates the tables first on existing databases; `create_tables()` is a no-op for those tables. On fresh databases, migration is skipped entirely (no entries table detected), and `create_tables()` creates everything.

### C3: topic_deliveries Module (topic_deliveries.rs)

**Responsibility**: Rust types and Store API methods for the `topic_deliveries` table.

New file: `crates/unimatrix-store/src/topic_deliveries.rs`

### C4: query_log Module (query_log.rs)

**Responsibility**: Rust types and Store API methods for the `query_log` table.

New file: `crates/unimatrix-store/src/query_log.rs`

### C5: Search Pipeline Integration (listener.rs + tools.rs)

**Responsibility**: Write a `query_log` row after every successful search, in both the UDS and MCP paths.

Both paths use fire-and-forget semantics matching the injection_log precedent (ADR-004, Unimatrix #101). Failures are logged at `warn` level and do not affect search results.

## Component Interactions

```
Store::open()
    |
    v
PRAGMAs -> migrate_if_needed() -> create_tables()
               |                        |
               | (v10 DB)               | (fresh DB)
               v                        v
         CREATE TABLE IF NOT EXISTS     CREATE TABLE IF NOT EXISTS
         topic_deliveries + query_log   topic_deliveries + query_log
               |
               v
         Backfill topic_deliveries
         from sessions WHERE feature_cycle IS NOT NULL
               |
               v
         UPDATE schema_version = 11
```

### Search Pipeline Data Flow

```
UDS handle_context_search            MCP context_search
    |                                    |
    v                                    v
SearchService::search()             SearchService::search()
    |                                    |
    v                                    v
[results computed]                  [results computed]
    |                                    |
    |  injection_log write               |  usage recording
    |  (existing, fire-and-forget)       |  (existing, fire-and-forget)
    |                                    |
    v                                    v
query_log write                     query_log write
(fire-and-forget,                   (fire-and-forget,
 spawn_blocking_fire_and_forget)     spawn_blocking)
    |                                    |
    v                                    v
Store::insert_query_log()           Store::insert_query_log()
```

Both paths construct a `QueryLogRecord` with identical fields and call the same Store method. The `source` field distinguishes the transport ("uds" vs "mcp"). The `retrieval_mode` field captures "strict" (UDS) vs "flexible" (MCP).

### Batching Consideration (Unimatrix #731, #735)

The query_log write is a single INSERT per search invocation (not a batch). It does not compound with other fire-and-forget writes in the same call path:
- UDS path: injection_log batch + co-access pairs + query_log = 3 spawn_blocking tasks. This matches the existing pattern before the vnc-010 batching fix. However, the UDS path processes one request at a time per connection (sequential message loop), so blocking pool saturation is not a concern.
- MCP path: The MCP path already batches usage+confidence+feature_entries+co_access in one spawn_blocking call (vnc-010 fix). The query_log write is a separate single INSERT. Adding one more spawn_blocking task is acceptable because MCP concurrency is bounded by rmcp's sequential stdin processing.

If future profiling shows contention, the query_log write can be folded into the existing batched task. For now, keeping it separate simplifies the implementation and matches the injection_log precedent.

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| AUTOINCREMENT for query_log PK | ADR-001 | Append-only log; no need for counter-based allocation. Matches observations table precedent. |
| No counter for query_log IDs | ADR-001 | AUTOINCREMENT handles allocation natively. Avoids adding a 6th named counter. |
| Fire-and-forget query_log writes | ADR-002 | Query telemetry is a side effect. Search latency must not be affected. Matches injection_log and usage recording precedent. |
| Backfill in main migration transaction | ADR-003 | Additive INSERT from ~500 sessions is fast. No need for separate transaction. |
| topic_deliveries TEXT PRIMARY KEY | -- | Natural key (topic name) is the query pattern. No synthetic ID needed. |

## Integration Points

### Dependency: col-017 (v10 migration)

nxs-010 migration guard is `current_version < 11`. This requires col-017's v10 migration to have already run. Merge order: col-017 first, then nxs-010 (SR-02).

The backfill query reads `sessions.feature_cycle` which is populated by col-017's hook-side topic attribution. Without col-017, the backfill produces zero rows (harmless but useless).

### Downstream consumers

- **col-020** (Multi-Session Retrospective): reads `topic_deliveries`, calls `update_topic_delivery_counters`
- **crt-018** (Knowledge Effectiveness): reads `topic_deliveries` for per-topic scoring
- **crt-019** (Search Quality): reads `query_log` for zero-result analysis, reformulation detection
- **col-021** (Query Data Export): reads `query_log` for (query, results, outcome) triple export

### Existing crate interfaces consumed

- `Store::lock_conn()` for direct SQL in migration
- `crate::counters::{read_counter, set_counter}` for schema_version update
- `spawn_blocking_fire_and_forget` in UDS listener (existing helper)
- `tokio::task::spawn_blocking` in MCP tools (existing pattern)
- `SearchService` result types: `SearchResults`, `ScoredEntry`

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `TopicDeliveryRecord` | `struct { topic: String, created_at: u64, completed_at: Option<u64>, status: String, github_issue: Option<i64>, total_sessions: i64, total_tool_calls: i64, total_duration_secs: i64, phases_completed: Option<String> }` | `crates/unimatrix-store/src/topic_deliveries.rs` (new) |
| `Store::upsert_topic_delivery` | `(&self, record: &TopicDeliveryRecord) -> Result<()>` | `topic_deliveries.rs` (new) |
| `Store::get_topic_delivery` | `(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>>` | `topic_deliveries.rs` (new) |
| `Store::update_topic_delivery_counters` | `(&self, topic: &str, sessions_delta: i64, tool_calls_delta: i64, duration_delta: i64) -> Result<()>` | `topic_deliveries.rs` (new) |
| `Store::list_topic_deliveries` | `(&self) -> Result<Vec<TopicDeliveryRecord>>` | `topic_deliveries.rs` (new) |
| `QueryLogRecord` | `struct { query_id: i64, session_id: String, query_text: String, ts: u64, result_count: u32, result_entry_ids: String, similarity_scores: String, retrieval_mode: String, source: String }` | `crates/unimatrix-store/src/query_log.rs` (new) |
| `Store::insert_query_log` | `(&self, record: &QueryLogRecord) -> Result<()>` | `query_log.rs` (new) |
| `Store::scan_query_log_by_session` | `(&self, session_id: &str) -> Result<Vec<QueryLogRecord>>` | `query_log.rs` (new) |
| `CURRENT_SCHEMA_VERSION` | `const u64 = 11` | `migration.rs` (update) |
| `spawn_blocking_fire_and_forget` | `fn<F: FnOnce() + Send + 'static>(f: F)` | `uds/listener.rs` (existing) |

### Record Construction Pattern for query_log Writes

Both UDS and MCP paths must construct `QueryLogRecord` identically. The construction pattern:

```rust
QueryLogRecord {
    query_id: 0,  // ignored; AUTOINCREMENT allocates
    session_id: session_id.clone(),
    query_text: query.clone(),
    ts: unix_now_secs(),
    result_count: results.len() as u32,
    result_entry_ids: serde_json::to_string(&entry_ids).unwrap_or_default(),
    similarity_scores: serde_json::to_string(&scores).unwrap_or_default(),
    retrieval_mode: "strict".to_string(),  // or "flexible"
    source: "uds".to_string(),            // or "mcp"
}
```

Where `entry_ids: Vec<u64>` and `scores: Vec<f64>` are extracted from `SearchResults.entries` in parallel arrays. `serde_json::to_string` produces JSON arrays consistent with other JSON columns (`signal_queue.entry_ids`, `agent_registry.capabilities`).

### Module Registration

Both new modules must be registered in `crates/unimatrix-store/src/lib.rs`:
```rust
pub mod topic_deliveries;
pub mod query_log;
```

And re-export their public types:
```rust
pub use topic_deliveries::TopicDeliveryRecord;
pub use query_log::QueryLogRecord;
```

## Open Questions

1. **UDS session_id availability for query_log**: The UDS path has `session_id: Option<String>`. When `session_id` is None (rare edge case -- hook payload missing session_id), should the query_log write be skipped or use a sentinel value like `"uds-anon"`? Recommendation: skip the write. Query log without a session_id has limited analytical value, and this matches the injection_log guard pattern (`if !sid.is_empty()`).

2. **MCP session_id availability for query_log**: The MCP path has `ctx.audit_ctx.session_id: Option<String>`. Same question. Recommendation: write with empty string if None -- MCP queries are always analytically interesting even without session attribution. The `source: "mcp"` field distinguishes them.
