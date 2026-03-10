# nxs-010: Activity Schema Evolution — Specification

## Objective

Add two new SQLite tables (`topic_deliveries` and `query_log`) to the Unimatrix storage engine, enabling cross-session topic aggregation and search query capture. Migrate existing databases from schema v10 to v11 with backfill of `topic_deliveries` from attributed session data. Wire `query_log` writes into both search paths (UDS and MCP) using the established fire-and-forget pattern.

---

## Functional Requirements

### FR-01: `topic_deliveries` Table Creation

1. FR-01.1: `create_tables()` in `db.rs` must include a `CREATE TABLE IF NOT EXISTS topic_deliveries` statement with columns: `topic TEXT PRIMARY KEY`, `created_at INTEGER NOT NULL`, `completed_at INTEGER`, `status TEXT NOT NULL DEFAULT 'active'`, `github_issue INTEGER`, `total_sessions INTEGER NOT NULL DEFAULT 0`, `total_tool_calls INTEGER NOT NULL DEFAULT 0`, `total_duration_secs INTEGER NOT NULL DEFAULT 0`, `phases_completed TEXT`.
2. FR-01.2: No additional indexes required on `topic_deliveries` (primary key index on `topic` is sufficient for all planned access patterns).

### FR-02: `query_log` Table Creation

1. FR-02.1: `create_tables()` in `db.rs` must include a `CREATE TABLE IF NOT EXISTS query_log` statement with columns: `query_id INTEGER PRIMARY KEY AUTOINCREMENT`, `session_id TEXT NOT NULL`, `query_text TEXT NOT NULL`, `ts INTEGER NOT NULL`, `result_count INTEGER NOT NULL`, `result_entry_ids TEXT`, `similarity_scores TEXT`, `retrieval_mode TEXT`, `source TEXT NOT NULL`.
2. FR-02.2: Two indexes must be created: `idx_query_log_session ON query_log(session_id)` and `idx_query_log_ts ON query_log(ts)`.
3. FR-02.3: `query_log` uses SQLite AUTOINCREMENT for ID allocation, not a named counter in the `counters` table. This diverges from the entity-table counter pattern intentionally: `query_log` is an append-only log where non-contiguous IDs after vacuum are acceptable.

### FR-03: Schema Migration (v10 to v11)

1. FR-03.1: Add a `current_version < 11` guard block in `migrate_if_needed()`.
2. FR-03.2: The migration block must CREATE both tables with `IF NOT EXISTS` (idempotent DDL).
3. FR-03.3: Backfill `topic_deliveries` from existing attributed sessions using `INSERT OR IGNORE`:
   ```sql
   INSERT OR IGNORE INTO topic_deliveries (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
   SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0, COALESCE(SUM(ended_at - started_at), 0)
   FROM sessions
   WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
   GROUP BY feature_cycle;
   ```
4. FR-03.4: Update `CURRENT_SCHEMA_VERSION` constant to `11`.
5. FR-03.5: Update the `schema_version` counter to 11 within the migration transaction.
6. FR-03.6: The migration runs within the main migration transaction (no separate transaction needed, since this is purely additive DDL + INSERT). [Ref: SR-01 — init sequence safety]
7. FR-03.7: Backfill sets `total_tool_calls` to 0 (not backfilled from `observation_metrics`). Downstream feature col-020 will recompute accurate values. [Scope open question #1 resolution]
8. FR-03.8: Backfill sets `status = 'completed'` for all historically attributed topics (conservative default). [AC-19]

### FR-04: `TopicDeliveryRecord` Struct and Store API

1. FR-04.1: Define `TopicDeliveryRecord` struct in a new module `crates/unimatrix-store/src/topic_deliveries.rs` with fields mirroring all `topic_deliveries` columns: `topic: String`, `created_at: u64`, `completed_at: Option<u64>`, `status: String`, `github_issue: Option<i64>`, `total_sessions: i64`, `total_tool_calls: i64`, `total_duration_secs: i64`, `phases_completed: Option<String>`.
2. FR-04.2: `Store::upsert_topic_delivery(&self, record: &TopicDeliveryRecord) -> Result<()>` — inserts a new row or replaces an existing row (INSERT OR REPLACE semantics).
3. FR-04.3: `Store::get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>>` — returns `None` for nonexistent topics, `Some(record)` otherwise.
4. FR-04.4: `Store::update_topic_delivery_counters(&self, topic: &str, sessions_delta: i64, tool_calls_delta: i64, duration_delta: i64) -> Result<()>` — atomically increments `total_sessions`, `total_tool_calls`, and `total_duration_secs` using SQL `SET col = col + ?` on an existing row. Returns an error if the topic does not exist.
5. FR-04.5: `Store::list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>>` — returns all rows, ordered by `created_at DESC`.

### FR-05: `QueryLogRecord` Struct and Store API

1. FR-05.1: Define `QueryLogRecord` struct in a new module `crates/unimatrix-store/src/query_log.rs` with fields: `query_id: i64` (0 on insert, populated on read), `session_id: String`, `query_text: String`, `ts: u64`, `result_count: i64`, `result_entry_ids: String` (JSON array), `similarity_scores: String` (JSON array), `retrieval_mode: String`, `source: String`.
2. FR-05.2: `Store::insert_query_log(&self, record: &QueryLogRecord) -> Result<()>` — inserts a single row. The `query_id` field is ignored on insert (AUTOINCREMENT allocates it).
3. FR-05.3: `Store::scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>>` — returns all `query_log` rows for the given `session_id`, ordered by `ts ASC`.

### FR-06: Search Pipeline Integration (UDS Path)

1. FR-06.1: In `handle_context_search` (UDS listener), after the injection_log batch write, insert a fire-and-forget `query_log` row via `spawn_blocking_fire_and_forget`.
2. FR-06.2: The `query_log` row must capture: `session_id` from the UDS request, `query_text` from the search query, `ts` as current unix timestamp, `result_count` as the number of results returned, `result_entry_ids` as a JSON array of entry ID integers, `similarity_scores` as a JSON array of f64 similarity values (parallel to `result_entry_ids`), `retrieval_mode` as `"strict"`, `source` as `"uds"`.
3. FR-06.3: If the `query_log` write fails, log a warning and continue. The search response must not be affected. [Ref: SR-05]

### FR-07: Search Pipeline Integration (MCP Path)

1. FR-07.1: In the MCP tool handler that invokes `SearchService::search`, after the search completes, insert a fire-and-forget `query_log` row via `spawn_blocking` (matching the existing audit event pattern).
2. FR-07.2: The `query_log` row must capture identical fields as FR-06.2, except: `retrieval_mode` is `"flexible"`, `source` is `"mcp"`.
3. FR-07.3: If the `query_log` write fails, log a warning and continue. The search response must not be affected. [Ref: SR-05]

### FR-08: Shared QueryLogRecord Construction [Ref: SR-07]

1. FR-08.1: Both UDS and MCP paths must construct `QueryLogRecord` using a shared builder function or constructor that takes `(session_id, query_text, results, retrieval_mode, source)` as parameters. This ensures field parity between paths and prevents divergent field population.

---

## Non-Functional Requirements

### NFR-01: Migration Performance

The v10-to-v11 migration must complete in under 500ms for databases with up to 1,000 attributed sessions. The migration is additive (CREATE TABLE + INSERT) and runs within the existing main migration transaction.

### NFR-02: Query Log Write Latency

`query_log` writes must not add observable latency to search responses. Both paths use fire-and-forget writes (spawn_blocking), matching the injection_log precedent. The search response is sent before the write completes.

### NFR-03: Query Log Capacity

The `query_log` table must perform acceptably at 30,000 rows/year (5x the initial 6K estimate, per SR-06). The `session_id` and `ts` indexes ensure efficient scan queries at this scale. SQLite handles this comfortably.

### NFR-04: Migration Idempotency

Running the v10-to-v11 migration on a database already at v11 must be a no-op. The `current_version >= CURRENT_SCHEMA_VERSION` guard ensures this. Running `create_tables` on a v11 database must also be a no-op (`IF NOT EXISTS` on all DDL).

### NFR-05: Backward Compatibility

Opening a v11 database with older server code (expecting v10) must not crash. SQLite ignores unknown tables. The schema_version counter mismatch causes the migration guard to skip, and existing code does not reference the new tables.

### NFR-06: JSON Encoding Consistency

`result_entry_ids` and `similarity_scores` must use `serde_json::to_string` for serialization, consistent with other JSON columns (`signal_queue.entry_ids`, `agent_registry.capabilities`).

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `topic_deliveries` table exists in schema v11 with all 9 columns (topic PK, created_at, completed_at, status, github_issue, total_sessions, total_tool_calls, total_duration_secs, phases_completed) | Unit test: open v11 db, `pragma_table_info('topic_deliveries')` returns 9 columns with correct types |
| AC-02 | `query_log` table exists in schema v11 with all 9 columns (query_id PK AUTOINCREMENT, session_id, query_text, ts, result_count, result_entry_ids, similarity_scores, retrieval_mode, source) | Unit test: open v11 db, `pragma_table_info('query_log')` returns 9 columns with correct types |
| AC-03 | `query_log` has indexes on `session_id` and `ts` | Unit test: `pragma_index_list('query_log')` returns 2 indexes |
| AC-04 | Migration from v10 to v11 creates both tables and backfills `topic_deliveries` from existing attributed sessions | Integration test: create v10 db with sessions, run migration, verify topic_deliveries rows match expected aggregates |
| AC-05 | Migration is idempotent — running on a v11 database is a no-op | Integration test: run migration twice, verify no errors and no duplicate rows |
| AC-06 | Migration handles empty sessions table (no attributed sessions) without error | Integration test: create v10 db with no sessions, run migration, verify 0 topic_deliveries rows |
| AC-07 | `Store::upsert_topic_delivery` inserts a new row or replaces an existing row | Unit test: insert, verify, upsert with changed fields, verify updated |
| AC-08 | `Store::get_topic_delivery` returns None for nonexistent topics | Unit test: query nonexistent topic, assert None |
| AC-09 | `Store::update_topic_delivery_counters` atomically increments counters on an existing row | Unit test: insert row with known values, call update with deltas, verify new values are original + delta |
| AC-10 | `Store::insert_query_log` writes a row with auto-allocated query_id | Unit test: insert row with query_id=0, read back, assert query_id > 0 |
| AC-11 | `Store::scan_query_log_by_session` returns all query_log rows for a given session_id, ordered by ts ASC | Unit test: insert 3 rows for same session with different ts, verify returned in order |
| AC-12 | UDS `handle_context_search` writes a `query_log` row after every successful search (fire-and-forget; search latency unaffected) | Integration test: invoke UDS search, verify query_log row exists with source="uds" and retrieval_mode="strict" |
| AC-13 | MCP `SearchService::search` caller writes a `query_log` row after every successful search (fire-and-forget) | Integration test: invoke MCP search tool, verify query_log row exists with source="mcp" and retrieval_mode="flexible" |
| AC-14 | `query_log.result_entry_ids` is a JSON array of entry ID integers | Unit test: insert row, read back, deserialize as `Vec<u64>` |
| AC-15 | `query_log.similarity_scores` is a JSON array of f64 values, parallel to result_entry_ids | Unit test: insert row with 3 entry IDs and 3 scores, read back, verify lengths match and values are f64 |
| AC-16 | `query_log.source` is "uds" for hook-triggered searches and "mcp" for tool-invoked searches | Covered by AC-12 and AC-13 |
| AC-17 | `query_log.retrieval_mode` is "strict" or "flexible" matching the search path | Covered by AC-12 and AC-13 |
| AC-18 | Backfill computes `total_sessions` and `total_duration_secs` from existing session data | Covered by AC-04 (integration test verifies aggregate values) |
| AC-19 | Backfill sets `status = 'completed'` for historically attributed topics | Covered by AC-04 (integration test asserts status field) |
| AC-20 | All existing unit and integration tests continue to pass (no regressions) | CI: full `cargo test` passes |

---

## Domain Models

### Key Entities

**Topic** — The universal grouping concept for a body of work. A topic spans multiple sessions and aggregates activity data. Semantically equivalent to `feature_cycle` in existing tables. New tables use `topic` as the canonical column name per ASS-018. Examples: `"col-016"`, `"nxs-010"`, `"bugfix-178"`.

**TopicDelivery** (`topic_deliveries` row) — An aggregate record for a topic's lifecycle. Contains counters (total sessions, tool calls, duration), lifecycle status (`active`, `completed`), optional GitHub issue link, and phase progression. Created during session attribution or backfill. Updated incrementally by downstream features (col-020).

**QueryLogEntry** (`query_log` row) — An immutable record of a single search query execution. Captures the query text, search results (entry IDs and similarity scores), retrieval mode, and source transport. Append-only; never updated or deleted.

**Session** (`sessions` row, existing) — A single agent working session. Has optional `feature_cycle` linking to a topic. The `topic_deliveries` table aggregates data across all sessions sharing the same `feature_cycle` value.

**RetrievalMode** — Enum controlling status-aware filtering: `Strict` (UDS path, hard filter) or `Flexible` (MCP path, soft penalty). Recorded in `query_log.retrieval_mode` as `"strict"` or `"flexible"`.

**Source** — The transport that triggered a search: `"uds"` (hook-triggered via Unix domain socket) or `"mcp"` (tool-invoked via MCP stdio transport).

### Relationships

```
topic_deliveries.topic  <--  sessions.feature_cycle  (1:N, no FK enforced)
query_log.session_id    <--  sessions.session_id      (N:1, no FK enforced)
```

Foreign keys are not enforced at the database level. The `topic_deliveries.topic` to `sessions.feature_cycle` relationship is maintained at the application level. This avoids ALTER TABLE on the existing sessions table and is consistent with existing FK-free patterns in `injection_log`, `signal_queue`, and `observations`.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| topic | A body of work grouping sessions and knowledge entries. Equivalent to `feature_cycle` in existing schema. |
| topic delivery | The aggregate lifecycle record for a topic across all its sessions. |
| query log | Append-only record of search executions with result metadata. |
| fire-and-forget | A write operation spawned asynchronously where failures are logged but do not affect the caller's response. Used for injection_log, observation, and now query_log writes. |
| backfill | Populating a new table with data derived from existing tables during migration. |
| attribution | The process of associating a session with a topic (performed by col-017, not nxs-010). |

---

## User Workflows

### Workflow 1: Database Upgrade (Automatic)

1. User starts Unimatrix server with a v10 database.
2. `Store::open()` calls `migrate_if_needed()`.
3. Migration detects `schema_version = 10 < 11`.
4. Within the main migration transaction: creates `topic_deliveries` and `query_log` tables, backfills `topic_deliveries` from attributed sessions, updates `schema_version` to 11.
5. `create_tables()` runs (all `IF NOT EXISTS` — no-op for the two new tables).
6. Server starts normally. Existing tools and hooks work without changes.

### Workflow 2: Search Query Logging (UDS Path)

1. Hook fires `UserPromptSubmit` event, dispatching a search request to the UDS listener.
2. `handle_context_search` executes the search pipeline (embed, HNSW, rerank, filter).
3. Search results are formatted and sent back to the hook caller.
4. After the response: injection_log batch is written (existing), then a `query_log` row is written (new), both fire-and-forget.
5. If the `query_log` write fails, a warning is logged. The hook caller is unaffected.

### Workflow 3: Search Query Logging (MCP Path)

1. Agent invokes `context_search` MCP tool.
2. `SearchService::search` executes the full pipeline.
3. The tool handler formats and returns results to the agent.
4. After the response: a `query_log` row is written fire-and-forget via `spawn_blocking`.
5. If the write fails, a warning is logged. The agent receives results normally.

### Workflow 4: Topic Delivery Management (API Consumers)

1. col-017 (hook-side attribution) resolves a session's topic on SessionClose.
2. col-017 calls `Store::upsert_topic_delivery()` to create or update the topic delivery record.
3. col-020 (multi-session retrospective) calls `Store::update_topic_delivery_counters()` to increment aggregate counters after recomputing cross-session metrics.
4. `context_status` or future tools call `Store::list_topic_deliveries()` for status reporting.

---

## Constraints

### C-01: Schema Version Sequencing [SR-02]

`CURRENT_SCHEMA_VERSION` must be set to 11. The migration guard must be `current_version < 11`. This feature must merge strictly after col-017 (which sets v10). CI should assert schema version monotonicity across branches.

### C-02: Init Sequence — Migrate Before Create [SR-01]

The existing init sequence in `Store::open()` already runs `migrate_if_needed()` before `create_tables()`. The v10-to-v11 migration creates both new tables with `IF NOT EXISTS`. When `create_tables()` subsequently runs, the DDL is a no-op for the new tables. No init sequence change is needed — verify this with a test that opens a v10 database with nxs-010 code.

### C-03: Fire-and-Forget Failure Semantics [SR-05]

Query log write failures must be handled identically to injection_log write failures: log a warning via `tracing::warn!`, do not retry, do not propagate the error to the caller. The search response is already sent or prepared before the write is attempted.

### C-04: AUTOINCREMENT Pattern Boundary [SR-03]

`query_log` uses AUTOINCREMENT. Entity tables (`entries`, `signal_queue`, `injection_log`, `audit_log`) continue to use named counters. The decision boundary: append-only logs where non-contiguous IDs are acceptable use AUTOINCREMENT; entity tables where ID predictability matters use named counters. Document this in an ADR.

### C-05: col-017 Dependency [SR-08]

nxs-010 delivery must be gated on col-017 integration tests passing. Without col-017's session attribution, backfill produces zero `topic_deliveries` rows (which is safe but not useful). The `topic_deliveries` Store API is useful regardless of backfill content.

### C-06: Transaction Scope

The backfill runs within the main migration transaction. For databases with up to 1,000 attributed sessions, this is acceptable (estimated <100ms). The INSERT OR IGNORE ensures no duplicate rows on re-run.

### C-07: No GC Policy

`query_log` has no garbage collection mechanism. At estimated 6K-30K rows/year, this is negligible for SQLite. A GC policy will be designed separately if data volume warrants it.

---

## Dependencies

### Crate Dependencies

- **rusqlite** (existing) — SQLite access for DDL, DML, and migration.
- **serde_json** (existing) — JSON serialization of `result_entry_ids` and `similarity_scores` arrays.
- **tracing** (existing) — Warning logs for fire-and-forget write failures.
- **tokio** (existing in unimatrix-server) — `spawn_blocking` for fire-and-forget query_log writes.

### Feature Dependencies

- **col-017 (Hook-Side Topic Attribution)** — Must land before nxs-010. Populates `sessions.feature_cycle` which the backfill reads. Also sets `CURRENT_SCHEMA_VERSION` to 10.
- **col-019 (PostToolUse Response Capture)** — Wave 1 peer. No direct dependency but ships in same wave context.

### Existing Components Modified

- `crates/unimatrix-store/src/db.rs` — Add two table DDL blocks to `create_tables()`.
- `crates/unimatrix-store/src/migration.rs` — Add `current_version < 11` block, bump `CURRENT_SCHEMA_VERSION` to 11.
- `crates/unimatrix-server/src/uds/listener.rs` — Add fire-and-forget `query_log` write in `handle_context_search`.
- `crates/unimatrix-server/src/services/search.rs` or its tool-layer caller — Add fire-and-forget `query_log` write after MCP search.

### New Modules

- `crates/unimatrix-store/src/topic_deliveries.rs` — `TopicDeliveryRecord` struct + Store methods.
- `crates/unimatrix-store/src/query_log.rs` — `QueryLogRecord` struct + Store methods.

---

## NOT in Scope

- **No changes to the observations table** — `topic_signal` column was added by col-017 (already in v10).
- **No topic attribution logic** — Attribution is col-017's responsibility. nxs-010 provides storage only.
- **No multi-session retrospective computation** — That is col-020, which consumes `topic_deliveries`.
- **No knowledge effectiveness analysis** — That is crt-018, downstream of nxs-010.
- **No search quality analysis or gap detection** — That is crt-019, consuming `query_log` data.
- **No query data export pipeline** — That is col-021.
- **No GC policy for query_log** — Deferred until data volume warrants it.
- **No renaming of existing `feature_cycle` columns** — Backward compatibility maintained.
- **No changes to SearchService pipeline logic** — Only adding a write-after-search in the transport layer.
- **No `topic_deliveries.total_tool_calls` backfill from `observation_metrics`** — Left at 0; col-020 will recompute.
- **No query embedding hash capture** — Deferred per scope open question #2.
- **No foreign key enforcement between `topic_deliveries` and `sessions`** — Application-level enforcement per scope open question #3.
