# nxs-010: Activity Schema Evolution -- Pseudocode Overview

## Components

| Component | File | Crate | Why |
|-----------|------|-------|-----|
| schema-ddl | `schema-ddl.md` | unimatrix-store | DDL for two new tables in `create_tables()` |
| migration | `migration.md` | unimatrix-store | v10->v11 migration with backfill |
| topic-deliveries | `topic-deliveries.md` | unimatrix-store | New module: TopicDeliveryRecord + Store CRUD |
| query-log | `query-log.md` | unimatrix-store | New module: QueryLogRecord + Store methods + shared constructor |
| search-pipeline-integration | `search-pipeline-integration.md` | unimatrix-server | Fire-and-forget query_log writes in UDS + MCP paths |

## Data Flow

```
Store::open()
  -> migrate_if_needed()   [C2: creates tables, backfills topic_deliveries, bumps to v11]
  -> create_tables()        [C1: IF NOT EXISTS -- no-op on migrated DB, creates on fresh DB]

UDS handle_context_search:
  SearchService::search() -> results
  -> injection_log batch write (existing)
  -> query_log write (NEW, fire-and-forget) [C5 -> C4: Store::insert_query_log]

MCP context_search tool:
  SearchService::search() -> results
  -> usage recording (existing)
  -> query_log write (NEW, fire-and-forget) [C5 -> C4: Store::insert_query_log]

Downstream (not in scope, consumes our API):
  col-020 -> Store::get_topic_delivery, Store::update_topic_delivery_counters
  crt-019 -> Store::scan_query_log_by_session
```

## Shared Types

### TopicDeliveryRecord (new, topic_deliveries.rs)

```
struct TopicDeliveryRecord {
    topic: String,              // TEXT PRIMARY KEY
    created_at: u64,            // unix timestamp
    completed_at: Option<u64>,  // nullable
    status: String,             // "active" or "completed"
    github_issue: Option<i64>,  // nullable
    total_sessions: i64,        // default 0
    total_tool_calls: i64,      // default 0
    total_duration_secs: i64,   // default 0
    phases_completed: Option<String>, // nullable
}
```

### QueryLogRecord (new, query_log.rs)

```
struct QueryLogRecord {
    query_id: i64,            // 0 on insert (AUTOINCREMENT allocates)
    session_id: String,       // TEXT NOT NULL
    query_text: String,       // TEXT NOT NULL
    ts: u64,                  // unix timestamp
    result_count: i64,        // INTEGER NOT NULL
    result_entry_ids: String, // JSON array of u64
    similarity_scores: String,// JSON array of f64
    retrieval_mode: String,   // "strict" or "flexible"
    source: String,           // "uds" or "mcp"
}
```

## Sequencing Constraints

1. **schema-ddl** and **migration** have no code dependency on each other but emit identical DDL. Build in parallel.
2. **topic-deliveries** and **query-log** are independent new modules. Build in parallel.
3. **search-pipeline-integration** depends on query-log (needs `QueryLogRecord` and `Store::insert_query_log`). Build last.
4. **lib.rs** module registration can happen with topic-deliveries and query-log.

Build order: (schema-ddl + migration + topic-deliveries + query-log) in parallel, then search-pipeline-integration.

## Module Registration (lib.rs)

Add to `crates/unimatrix-store/src/lib.rs`:

```
pub mod topic_deliveries;
pub mod query_log;
```

Add re-exports:

```
pub use topic_deliveries::TopicDeliveryRecord;
pub use query_log::QueryLogRecord;
```
