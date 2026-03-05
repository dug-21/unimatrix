# Architecture: col-012 Data Path Unification

## System Overview

col-012 eliminates the dual data path (JSONL files + SQLite tables) by persisting all hook events in a new `observations` table and migrating the retrospective pipeline to read from it. The change touches three crates and four shell scripts:

```
Hook Shell Scripts                    unimatrix-server (UDS listener)
  observe-*.sh  ──────────────────►  RecordEvent / RecordEvents handler
  [JSONL write removed]               │
                                       ▼
                                   observations table (NEW, schema v7)
                                       │
                                       ▼
                                   unimatrix-observe (retrospective pipeline)
                                     [reads SQL via ObservationSource trait]
                                     [JSONL parsing removed]
```

After col-012, observation data flows exclusively through the UDS socket into SQLite. The retrospective pipeline queries SQLite instead of parsing files. All 21 detection rules are unchanged.

## Component Breakdown

### Component 1: Schema Migration (unimatrix-store)

**Responsibility**: Add `observations` table to SQLite, bump schema version to 7.

- Extend `migration.rs` with v6->v7 migration step
- Update `CURRENT_SCHEMA_VERSION` to 7
- Add `observations` table in `create_tables()` for fresh databases
- Migration is idempotent (CREATE TABLE IF NOT EXISTS)

**Schema**:
```sql
CREATE TABLE IF NOT EXISTS observations (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT    NOT NULL,
    ts_millis    INTEGER NOT NULL,
    hook         TEXT    NOT NULL,
    tool         TEXT,
    input        TEXT,
    response_size INTEGER,
    response_snippet TEXT
);
CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis);
```

**ADR-001 (session_hash decision)**: Use `INTEGER PRIMARY KEY AUTOINCREMENT` instead of `(session_hash, ts_millis)` composite key. Rationale: the proposed composite key has a collision risk when two events arrive in the same millisecond within the same session (common with PreToolUse + PostToolUse pairs). AUTOINCREMENT eliminates this entirely and is simpler. Session lookup uses the `idx_observations_session` index. See ADR-001.

### Component 2: Event Persistence (unimatrix-server)

**Responsibility**: Persist all RecordEvent/RecordEvents payloads to the observations table.

- Modify `RecordEvent` handler in `uds/listener.rs` to extract fields from `ImplantEvent.payload` and insert into observations table
- Modify `RecordEvents` handler to batch-insert in a single transaction
- Use `spawn_blocking` (fire-and-forget) -- same pattern as injection log writes
- Map `ImplantEvent` fields to observations columns:
  - `event_type` -> `hook` (the hook event name, e.g., "PreToolUse")
  - `session_id` -> `session_id`
  - `timestamp` -> `ts_millis` (convert seconds to milliseconds: `timestamp * 1000`)
  - `payload.tool_name` -> `tool`
  - `payload.tool_input` -> `input` (JSON-serialized string)
  - `payload.response_size` -> `response_size` (PostToolUse only)
  - `payload.response_snippet` -> `response_snippet` (PostToolUse only)
  - For SubagentStart: `payload.agent_type` -> `tool`, `payload.prompt_snippet` -> `input`

### Component 3: Observation Source Trait (unimatrix-observe)

**Responsibility**: Abstract the data source so detection rules remain decoupled from storage.

Preserves ADR-001 (unimatrix-observe independence from unimatrix-store). The crate defines the trait; unimatrix-server provides the implementation.

```rust
// In unimatrix-observe::source (new module)
pub trait ObservationSource {
    /// Load observation records for a given feature cycle.
    /// Returns records sorted by timestamp, attributed to the feature.
    fn load_feature_observations(
        &self,
        feature_cycle: &str,
    ) -> Result<Vec<ObservationRecord>>;

    /// Discover session IDs associated with a feature cycle.
    fn discover_sessions_for_feature(
        &self,
        feature_cycle: &str,
    ) -> Result<Vec<String>>;

    /// Get aggregate observation statistics.
    fn observation_stats(&self) -> Result<ObservationStats>;
}
```

**ADR-002**: The trait lives in unimatrix-observe, not unimatrix-store. unimatrix-server implements it using Store's connection. This inverts the dependency -- observe defines the contract, server fulfills it. See ADR-002.

### Component 4: SQL Implementation (unimatrix-server)

**Responsibility**: Implement `ObservationSource` against the SQLite observations table.

```rust
// In unimatrix-server::services (new struct)
pub struct SqlObservationSource {
    store: Arc<Store>,
}

impl ObservationSource for SqlObservationSource {
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>> {
        // 1. Query SESSIONS for session_ids WHERE feature_cycle = ?
        // 2. Query observations WHERE session_id IN (...)
        // 3. Map rows to ObservationRecord
        // 4. Sort by ts_millis
    }

    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>> {
        // Query SESSIONS WHERE feature_cycle = ?
    }

    fn observation_stats(&self) -> Result<ObservationStats> {
        // SELECT COUNT(*), COUNT(DISTINCT session_id), MIN(ts_millis), MAX(ts_millis)
        // FROM observations
    }
}
```

### Component 5: Retrospective Pipeline Migration (unimatrix-server)

**Responsibility**: Rewire `context_retrospective` to use `ObservationSource` instead of JSONL.

- Replace `discover_sessions()` + `parse_session_file()` + `attribute_sessions()` with `source.load_feature_observations()`
- All downstream code (detection rules, metrics, baseline, report) receives `Vec<ObservationRecord>` unchanged
- `context_status` observation stats use `source.observation_stats()` instead of `scan_observation_stats()`

### Component 6: JSONL Removal

**Responsibility**: Remove the JSONL write path and parsing infrastructure.

**Shell hooks** (`.claude/hooks/observe-*.sh`):
- Remove JSONL file writes (`echo "$RECORD" >> ...`)
- Hooks continue forwarding to UDS (their primary purpose)
- After removal, hooks only call `unimatrix-server hook` -- JSONL lines eliminated

**unimatrix-observe**:
- Remove `parser.rs` (JSONL parsing) -- or retain `parse_timestamp()` if needed elsewhere
- Remove `files.rs` (session file discovery, cleanup, stats)
- Remove `SessionFile` type and related exports
- Update `lib.rs` to remove JSONL-specific re-exports
- Keep all detection rules, metrics, baseline, synthesis, attribution logic

**Note**: `attribution.rs` content-scanning logic is superseded by SESSIONS.feature_cycle but some attribution functions (e.g., `extract_feature_signal`) may still be useful for features that predate session registration. Retain as utility but remove from the primary pipeline path.

## Component Interactions

```
┌──────────────────┐     UDS socket      ┌──────────────────────┐
│  Hook Scripts    │────────────────────►│  UDS Listener         │
│  (bash, no JSONL)│                     │  RecordEvent handler  │
└──────────────────┘                     │  spawn_blocking write │
                                         └──────────┬───────────┘
                                                     │
                                         ┌───────────▼───────────┐
                                         │  SQLite (schema v7)   │
                                         │  observations table   │
                                         │  sessions table       │
                                         └───────────┬───────────┘
                                                     │
                                         ┌───────────▼───────────┐
                                         │  SqlObservationSource │
                                         │  (implements trait)   │
                                         └───────────┬───────────┘
                                                     │
                                         ┌───────────▼───────────┐
                                         │  unimatrix-observe    │
                                         │  ObservationSource    │
                                         │  (trait definition)   │
                                         │  Detection rules      │
                                         │  Metrics + baselines  │
                                         └───────────────────────┘
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| Primary key for observations | AUTOINCREMENT integer | Avoids timestamp collision in same-millisecond events | ADR-001 |
| Observe crate independence | Trait in observe, impl in server | Preserves ADR-001 independence; human directive | ADR-002 |
| Timestamp storage | Milliseconds (ts_millis) | Matches ObservationRecord.ts field (already millis); enables future ms-precision analysis | -- |
| Event loss when server down | Accepted (silent loss) | Hooks already silently fail when UDS is unavailable (FR-03.7: exit 0 always). Event queue provides retry. No change from current behavior. | ADR-003 |
| Retention policy | 60-day DELETE from observations WHERE ts_millis < ? | Matches current JSONL 60-day cleanup. Executed in context_status maintain path. | -- |

## Integration Points

| Integration Point | Crate | Description |
|-------------------|-------|-------------|
| `migration.rs` v6->v7 | unimatrix-store | Schema migration adds observations table |
| `create_tables()` | unimatrix-store | Fresh DB includes observations table |
| `RecordEvent` handler | unimatrix-server | Writes to observations table |
| `RecordEvents` handler | unimatrix-server | Batch writes to observations table |
| `context_retrospective` tool | unimatrix-server | Uses ObservationSource instead of JSONL |
| `context_status` tool | unimatrix-server | Uses ObservationSource for stats |
| `observe-*.sh` hooks | .claude/hooks/ | JSONL writes removed |
| `ObservationSource` trait | unimatrix-observe | New trait module |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ObservationSource` trait | `trait ObservationSource { fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>>; fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>>; fn observation_stats(&self) -> Result<ObservationStats>; }` | `unimatrix-observe::source` (new) |
| `SqlObservationSource` | `struct SqlObservationSource { store: Arc<Store> }` | `unimatrix-server::services` |
| `ImplantEvent` -> observations mapping | `event_type` -> `hook`, `timestamp*1000` -> `ts_millis`, `payload.tool_name` -> `tool`, `payload.tool_input` (JSON) -> `input`, `payload.response_size` -> `response_size`, `payload.response_snippet` -> `response_snippet` | `unimatrix-server::uds::listener` |
| `CURRENT_SCHEMA_VERSION` | `const CURRENT_SCHEMA_VERSION: u64 = 7` | `unimatrix-store::migration` |
| `ObservationStats` (revised) | Replace file-based fields with: `record_count: u64`, `session_count: u64`, `oldest_record_age_days: u64`, `approaching_cleanup: Vec<String>` | `unimatrix-observe::types` |

## Open Questions

None -- all resolved by human decisions and this architecture.
