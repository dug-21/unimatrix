# nxs-010: Activity Schema Evolution -- Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-010/SCOPE.md |
| Scope Risk Assessment | product/features/nxs-010/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nxs-010/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-010/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/nxs-010/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-010/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-ddl | pseudocode/schema-ddl.md | test-plan/schema-ddl.md |
| migration | pseudocode/migration.md | test-plan/migration.md |
| topic-deliveries | pseudocode/topic-deliveries.md | test-plan/topic-deliveries.md |
| query-log | pseudocode/query-log.md | test-plan/query-log.md |
| search-pipeline-integration | pseudocode/search-pipeline-integration.md | test-plan/search-pipeline-integration.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add two new SQLite tables (`topic_deliveries` and `query_log`) to the Unimatrix storage engine, enabling cross-session topic aggregation and search query telemetry capture. Migrate existing databases from schema v10 to v11 with backfill of `topic_deliveries` from attributed session data, and wire fire-and-forget `query_log` writes into both the UDS and MCP search paths.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| query_log PK allocation strategy | Use SQLite AUTOINCREMENT, not a named counter. Boundary: append-only logs use AUTOINCREMENT; entity tables use counters. | SR-03, SCOPE open question | architecture/ADR-001-autoincrement-for-query-log.md |
| query_log write failure semantics | Fire-and-forget: warn-level log, no retry, no error propagation. UDS skips if session_id is None; MCP always writes (empty string if no session_id). | SR-05, SR-07 | architecture/ADR-002-fire-and-forget-query-log-writes.md |
| Backfill transaction scope | Run backfill within the main migration transaction (no separate transaction). Purely additive DDL + INSERT is safe for ~500 sessions. | SR-01, SCOPE constraint | architecture/ADR-003-backfill-in-main-migration-transaction.md |
| total_tool_calls backfill | Left at 0 during migration. col-020 will recompute from raw data. | SCOPE open question #1 | SPECIFICATION FR-03.7 |
| Query embedding hash | Deferred. Not needed for v1. | SCOPE open question #2 | SPECIFICATION NOT-in-scope |
| Foreign key enforcement | No FK between topic_deliveries and sessions. Application-level enforcement. | SCOPE open question #3 | SPECIFICATION domain model |
| Integration test strategy | Follow full pipeline pattern matching injection_log tests. | SCOPE open question #4 | RISK-TEST-STRATEGY R-04/R-05 |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-store/src/topic_deliveries.rs` | TopicDeliveryRecord struct + Store CRUD methods (upsert, get, update_counters, list) |
| `crates/unimatrix-store/src/query_log.rs` | QueryLogRecord struct + Store methods (insert, scan_by_session) |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-store/src/db.rs` | Add CREATE TABLE IF NOT EXISTS for topic_deliveries and query_log + indexes in create_tables() |
| `crates/unimatrix-store/src/migration.rs` | Add v10->v11 migration block; bump CURRENT_SCHEMA_VERSION to 11 |
| `crates/unimatrix-store/src/lib.rs` | Register new modules (pub mod + pub use re-exports) |
| `crates/unimatrix-server/src/uds/listener.rs` | Add fire-and-forget query_log write in handle_context_search after injection_log write |
| `crates/unimatrix-server/src/services/search.rs` (or tool handler) | Add fire-and-forget query_log write after MCP search via spawn_blocking |

## Data Structures

### TopicDeliveryRecord

```rust
pub struct TopicDeliveryRecord {
    pub topic: String,              // TEXT PRIMARY KEY
    pub created_at: u64,            // INTEGER NOT NULL (unix timestamp)
    pub completed_at: Option<u64>,  // INTEGER (nullable)
    pub status: String,             // TEXT NOT NULL DEFAULT 'active'
    pub github_issue: Option<i64>,  // INTEGER (nullable)
    pub total_sessions: i64,        // INTEGER NOT NULL DEFAULT 0
    pub total_tool_calls: i64,      // INTEGER NOT NULL DEFAULT 0
    pub total_duration_secs: i64,   // INTEGER NOT NULL DEFAULT 0
    pub phases_completed: Option<String>, // TEXT (nullable)
}
```

### QueryLogRecord

```rust
pub struct QueryLogRecord {
    pub query_id: i64,            // INTEGER PRIMARY KEY AUTOINCREMENT (0 on insert, populated on read)
    pub session_id: String,       // TEXT NOT NULL
    pub query_text: String,       // TEXT NOT NULL
    pub ts: u64,                  // INTEGER NOT NULL (unix timestamp)
    pub result_count: u32,        // INTEGER NOT NULL
    pub result_entry_ids: String, // TEXT (JSON array of u64)
    pub similarity_scores: String,// TEXT (JSON array of f64)
    pub retrieval_mode: String,   // TEXT ("strict" or "flexible")
    pub source: String,           // TEXT NOT NULL ("uds" or "mcp")
}
```

### Schema DDL

```sql
-- topic_deliveries
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

-- query_log
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

### Backfill SQL

```sql
INSERT OR IGNORE INTO topic_deliveries (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0, COALESCE(SUM(ended_at - started_at), 0)
FROM sessions
WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
GROUP BY feature_cycle;
```

## Function Signatures

### topic_deliveries.rs (Store impl)

```rust
pub fn upsert_topic_delivery(&self, record: &TopicDeliveryRecord) -> Result<()>
pub fn get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>>
pub fn update_topic_delivery_counters(&self, topic: &str, sessions_delta: i64, tool_calls_delta: i64, duration_delta: i64) -> Result<()>
pub fn list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>>
```

### query_log.rs (Store impl)

```rust
pub fn insert_query_log(&self, record: &QueryLogRecord) -> Result<()>
pub fn scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>>
```

### Shared construction (FR-08.1)

Both UDS and MCP paths must construct QueryLogRecord using a shared builder/constructor to ensure field parity:

```rust
QueryLogRecord {
    query_id: 0,
    session_id: session_id.clone(),
    query_text: query.clone(),
    ts: unix_now_secs(),
    result_count: results.len() as u32,
    result_entry_ids: serde_json::to_string(&entry_ids).unwrap_or_default(),
    similarity_scores: serde_json::to_string(&scores).unwrap_or_default(),
    retrieval_mode: mode_str.to_string(),
    source: source_str.to_string(),
}
```

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | CURRENT_SCHEMA_VERSION must be 11; migration guard is `current_version < 11` | SR-02 |
| C-02 | migrate_if_needed() runs before create_tables() in Store::open() -- verify, do not change | SR-01, Unimatrix #375 |
| C-03 | query_log write failures: log warn, no retry, no error propagation to caller | SR-05, ADR-002 |
| C-04 | AUTOINCREMENT for query_log; named counters for entity tables | SR-03, ADR-001 |
| C-05 | nxs-010 must merge after col-017 (v10 migration dependency) | SR-08 |
| C-06 | Backfill runs in main migration transaction (no separate transaction) | ADR-003 |
| C-07 | No GC policy for query_log -- deferred | SCOPE non-goal |
| C-08 | JSON serialization via serde_json::to_string for result_entry_ids and similarity_scores | NFR-06 |
| C-09 | UDS path: skip query_log write if session_id is None/empty | ADR-002 |
| C-10 | MCP path: write query_log with empty string session_id if None | ADR-002 |

## Dependencies

### Crate Dependencies (all existing -- no new crates)

| Crate | Usage |
|-------|-------|
| rusqlite | DDL, DML, migration, parameterized queries |
| serde_json | JSON serialization of result_entry_ids and similarity_scores |
| tracing | warn-level logging for fire-and-forget write failures |
| tokio | spawn_blocking for fire-and-forget writes in MCP path |

### Feature Dependencies

| Feature | Relationship |
|---------|-------------|
| col-017 (Hook-Side Topic Attribution) | **Hard dependency**. Must land first. Populates sessions.feature_cycle; sets v10 schema. |
| col-019 (PostToolUse Response Capture) | Wave 1 peer. No direct dependency. |

### Downstream Consumers (not in scope, but informs API design)

| Feature | Consumes |
|---------|----------|
| col-020 (Multi-Session Retrospective) | topic_deliveries: read + update_counters |
| crt-018 (Knowledge Effectiveness) | topic_deliveries: read |
| crt-019 (Search Quality) | query_log: scan_by_session |
| col-021 (Query Data Export) | query_log: scan |

## NOT in Scope

- No changes to the observations table (topic_signal added by col-017)
- No topic attribution logic (col-017 responsibility)
- No multi-session retrospective computation (col-020)
- No knowledge effectiveness analysis (crt-018)
- No search quality analysis or gap detection (crt-019)
- No query data export pipeline (col-021)
- No GC policy for query_log
- No renaming of existing feature_cycle columns
- No changes to SearchService pipeline logic (only adding write-after-search)
- No total_tool_calls backfill from observation_metrics (left at 0)
- No query embedding hash capture
- No foreign key enforcement between topic_deliveries and sessions

## Alignment Status

5 checks PASS, 1 WARN, 0 requiring human approval.

**WARN: Scope Additions** -- Two items were added beyond SCOPE.md:
1. FR-08 (shared QueryLogRecord constructor) -- responds to SR-07 risk; proportionate quality measure.
2. NFR-03 (capacity sizing at 30K rows/year) -- responds to SR-06 recommendation; proportionate.

Both additions are direct responses to scope risk assessment recommendations. No feature scope expansion beyond what the Activity Intelligence milestone requires. All 4 SCOPE.md open questions resolved with documented rationale. No vision principles contradicted.

## Critical Risks for Implementation

| Risk | Priority | Mitigation |
|------|----------|-----------|
| R-02: Backfill SQL produces incorrect aggregates | Critical | Integration test with controlled session data covering NULL ended_at, empty feature_cycle, zero attributed sessions |
| R-04: Fire-and-forget panic in spawn_blocking | Critical | Catch all errors before they become panics; integration test verifying warn log on failure |
| R-10: INSERT OR REPLACE destroys concurrent counter updates | Critical | Document replace semantics; confirm no concurrent upsert+update workflow exists |
| R-01: Migration partial application | High | INSERT OR IGNORE + IF NOT EXISTS ensures idempotent re-run |
| R-05: UDS/MCP field divergence | High | Shared QueryLogRecord construction (FR-08.1) |
| R-07: update_counters on nonexistent topic | High | Return error (not silent Ok) when 0 rows affected |
