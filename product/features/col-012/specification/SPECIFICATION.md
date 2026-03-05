# Specification: col-012 Data Path Unification

## Objective

Eliminate the dual data path (JSONL files + SQLite tables) by adding an `observations` table to SQLite, persisting all hook events that RecordEvent currently discards, migrating the retrospective pipeline from JSONL file parsing to SQL queries, and removing the JSONL infrastructure. Net code reduction with all 21 detection rules unchanged.

## Functional Requirements

### FR-01: Schema Migration

- FR-01.1: Schema migration from v6 to v7 creates the `observations` table with columns: `id` (AUTOINCREMENT PK), `session_id` (TEXT NOT NULL), `ts_millis` (INTEGER NOT NULL), `hook` (TEXT NOT NULL), `tool` (TEXT), `input` (TEXT), `response_size` (INTEGER), `response_snippet` (TEXT).
- FR-01.2: Migration creates indexes `idx_observations_session` on `session_id` and `idx_observations_ts` on `ts_millis`.
- FR-01.3: Migration is idempotent (uses CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS).
- FR-01.4: Fresh databases at v7 include the observations table via `create_tables()`.
- FR-01.5: `CURRENT_SCHEMA_VERSION` updated to 7.

### FR-02: Event Persistence

- FR-02.1: `RecordEvent` handler persists all hook events (PreToolUse, PostToolUse, SubagentStart, SubagentStop) to the observations table.
- FR-02.2: `RecordEvents` handler persists all events in a single transaction (batch insert).
- FR-02.3: Event persistence uses `spawn_blocking` fire-and-forget pattern (does not block UDS response).
- FR-02.4: Field mapping from `ImplantEvent`:
  - `event_type` -> `hook`
  - `session_id` -> `session_id`
  - `timestamp * 1000` -> `ts_millis`
  - `payload.tool_name` or `payload.agent_type` -> `tool`
  - `payload.tool_input` (JSON string) or `payload.prompt_snippet` -> `input`
  - `payload.response_size` -> `response_size`
  - `payload.response_snippet` -> `response_snippet`
- FR-02.5: Missing optional fields (tool, input, response_size, response_snippet) are stored as NULL.
- FR-02.6: Events with event_type not matching a known hook type (PreToolUse, PostToolUse, SubagentStart, SubagentStop) are stored with their original event_type string in the `hook` column.

### FR-03: ObservationSource Trait

- FR-03.1: `unimatrix-observe` exports an `ObservationSource` trait with three methods: `load_feature_observations`, `discover_sessions_for_feature`, `observation_stats`.
- FR-03.2: `load_feature_observations(feature_cycle)` returns `Vec<ObservationRecord>` sorted by timestamp, containing all observations from sessions associated with the given feature.
- FR-03.3: `discover_sessions_for_feature(feature_cycle)` returns session IDs from the SESSIONS table where `feature_cycle` matches.
- FR-03.4: `observation_stats()` returns aggregate statistics: record count, distinct session count, oldest record age in days, sessions approaching 60-day cleanup.
- FR-03.5: The trait is defined in `unimatrix-observe` with no dependency on `unimatrix-store`.

### FR-04: SQL Implementation

- FR-04.1: `SqlObservationSource` struct in `unimatrix-server` implements `ObservationSource`.
- FR-04.2: `load_feature_observations` queries SESSIONS for session_ids, then queries observations for those sessions, maps rows to `ObservationRecord`.
- FR-04.3: For sessions where `feature_cycle` is NULL, those sessions are excluded from results (no fallback to content scanning).
- FR-04.4: SubagentStart/SubagentStop field normalization matches current parser.rs behavior: `agent_type` -> `tool`, `prompt_snippet` -> `input` (as String value), SubagentStop has tool=None, input=None.

### FR-05: Retrospective Pipeline Migration

- FR-05.1: `context_retrospective` MCP tool uses `ObservationSource` instead of JSONL discovery/parsing.
- FR-05.2: All 21 detection rules receive `Vec<ObservationRecord>` as input, unchanged.
- FR-05.3: MetricVector computation, baseline comparison, and report synthesis are unchanged.
- FR-05.4: Report structure (RetrospectiveReport) is unchanged.
- FR-05.5: `context_status` observation stats use `ObservationSource::observation_stats()`.

### FR-06: JSONL Removal

- FR-06.1: Shell hook scripts no longer write JSONL files (remove `echo "$RECORD" >> ...` lines).
- FR-06.2: Shell hooks continue forwarding events to UDS (`unimatrix-server hook`).
- FR-06.3: `parser.rs` JSONL parsing code is removed from unimatrix-observe (or `parse_timestamp` retained if used elsewhere).
- FR-06.4: `files.rs` session file discovery code is removed from unimatrix-observe.
- FR-06.5: `SessionFile` type removed from public API.
- FR-06.6: `lib.rs` re-exports updated to remove JSONL-specific items.

### FR-07: Retention

- FR-07.1: Observation records older than 60 days are eligible for deletion.
- FR-07.2: Retention cleanup executes `DELETE FROM observations WHERE ts_millis < ?` (60 days ago in millis).
- FR-07.3: Cleanup runs in the existing `context_status` maintain path, replacing JSONL file cleanup.

## Non-Functional Requirements

- NFR-01: Event persistence latency must not exceed the existing 50ms hook budget. The `spawn_blocking` pattern ensures the UDS response returns immediately.
- NFR-02: Batch insert for `RecordEvents` must use a single SQLite transaction for atomicity and performance.
- NFR-03: Schema migration must complete in under 1 second (table creation only, no data migration).
- NFR-04: Observation queries for a feature with 10,000 records must complete in under 500ms.
- NFR-05: Net code change must be a reduction (lines removed from JSONL infrastructure > lines added for SQL path).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Schema migration v6->v7 creates observations table with indexes | Integration test: open v6 DB, verify table exists after migration |
| AC-02 | RecordEvent handler persists all hook events to observations table | Integration test: send RecordEvent, query observations table |
| AC-03 | RecordEvents batch handler persists all events in single transaction | Integration test: send batch, verify all rows inserted |
| AC-04 | Retrospective pipeline reads from SQLite instead of JSONL | Integration test: populate observations + sessions, run retrospective |
| AC-05 | Session discovery uses SESSIONS table | Unit test: mock ObservationSource, verify discover_sessions_for_feature called |
| AC-06 | Feature attribution uses SESSIONS.feature_cycle | Integration test: sessions with feature_cycle, verify correct attribution |
| AC-07 | All 21 detection rules produce findings from SQL-sourced data | Integration test: seed observations matching each rule's trigger conditions |
| AC-08 | JSONL write path removed from shell hooks | Manual verification: grep for JSONL writes in hook scripts |
| AC-09 | JSONL parsing code removed from unimatrix-observe | Compilation: parser.rs removed or gutted, files.rs removed |
| AC-10 | context_retrospective produces valid report structure | Integration test: full pipeline produces RetrospectiveReport |
| AC-11 | context_status observation stats from observations table | Integration test: verify stats reflect table contents |
| AC-12 | All existing tests pass; new tests cover SQL path | CI: cargo test --workspace passes |
| AC-13 | Net code reduction | Manual: diff stat shows net negative |

## Domain Models

### ObservationRecord (existing, unchanged)

```
ObservationRecord {
    ts: u64                          // Unix epoch milliseconds
    hook: HookType                   // PreToolUse | PostToolUse | SubagentStart | SubagentStop
    session_id: String               // Claude Code session ID
    tool: Option<String>             // Tool name or agent type
    input: Option<serde_json::Value> // Tool input or prompt snippet
    response_size: Option<u64>       // PostToolUse only
    response_snippet: Option<String> // PostToolUse only
}
```

### observations table row (new)

```
observations {
    id: i64              // AUTOINCREMENT PK
    session_id: String   // FK-like to sessions.session_id
    ts_millis: i64       // Unix epoch milliseconds
    hook: String         // "PreToolUse" | "PostToolUse" | "SubagentStart" | "SubagentStop"
    tool: Option<String>
    input: Option<String>  // JSON-serialized tool input
    response_size: Option<i64>
    response_snippet: Option<String>
}
```

### Mapping: observations row -> ObservationRecord

- `ts_millis` -> `ts` (direct, both are epoch millis)
- `hook` string -> `HookType` enum (parse)
- `session_id` -> `session_id` (direct)
- `tool` -> `tool` (direct)
- `input` JSON string -> `input` (`serde_json::from_str` for tool inputs, `Value::String` for prompt snippets)
- `response_size` -> `response_size` (i64 -> u64 cast)
- `response_snippet` -> `response_snippet` (direct)

### ObservationStats (revised)

```
ObservationStats {
    record_count: u64                 // was: file_count
    session_count: u64                // NEW: COUNT(DISTINCT session_id)
    total_size_bytes: u64             // REMOVED (not meaningful for table rows)
    oldest_record_age_days: u64       // was: oldest_file_age_days
    approaching_cleanup: Vec<String>  // Session IDs with records 45-59 days old
}
```

## User Workflows

### Hook Event Flow (after col-012)

1. Claude Code fires hook event (e.g., PreToolUse)
2. Shell script reads stdin, forwards to `unimatrix-server hook`
3. Hook CLI builds `HookRequest::RecordEvent` (or specialized variant)
4. UDS listener receives request
5. `RecordEvent` handler extracts fields and inserts into observations table via spawn_blocking
6. Hook CLI receives `Ack`, exits 0

### Retrospective Analysis Flow (after col-012)

1. Agent calls `context_retrospective` with feature_cycle
2. Tool creates `SqlObservationSource` from `Arc<Store>`
3. `load_feature_observations(feature_cycle)` queries SESSIONS -> observations
4. Returns `Vec<ObservationRecord>` (same type as before)
5. Detection rules, metrics, baseline, report assembly -- all unchanged
6. Returns `RetrospectiveReport`

## Constraints

- Schema migration must use existing infrastructure in `crates/unimatrix-store/src/migration.rs`
- unimatrix-observe must not gain a dependency on unimatrix-store (ADR-002)
- Fire-and-forget write pattern (spawn_blocking) -- must not block hook response
- All 21 detection rules unchanged (same Rust code, same input type)
- SQLite WAL mode already configured via PRAGMAs (busy_timeout = 5000ms)

## Dependencies

| Dependency | Status | Purpose |
|-----------|--------|---------|
| col-010/010b | Complete | SESSIONS table, feature_cycle column |
| nxs-008 | Complete | Schema v6 normalization (SQLite backend) |
| col-002/002b | Complete | 21 detection rules, retrospective pipeline |
| rusqlite | Existing | SQLite access |
| serde_json | Existing | JSON serialization for input field |

## NOT in Scope

- New detection rules or extraction capabilities (col-013)
- Neural models or passive knowledge acquisition (crt-007+)
- Background maintenance tick (col-013)
- Converting Python test infrastructure to Rust
- Multi-project daemon mode
- Changing context_retrospective MCP tool interface
- Historical JSONL data migration (clean break accepted)
