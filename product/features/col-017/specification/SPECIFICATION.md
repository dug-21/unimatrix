# Specification: col-017 Hook-Side Topic Attribution

## Domain Model

### Core Types

#### TopicSignal

A per-event topic identifier extracted from hook input content. Short string matching a feature ID pattern (e.g., "col-017", "nxs-002").

| Field | Type | Description |
|-------|------|-------------|
| value | `String` | Feature ID extracted from event content |

Extraction priority chain (highest to lowest confidence):
1. **File path**: `product/features/{id}/` pattern via `extract_from_path()`
2. **Feature ID pattern**: Word-boundary `alpha-digits` match via `extract_feature_id_pattern()`
3. **Git branch**: `feature/{id}` branch name via `extract_from_git_checkout()`

First match wins. Returns `Option<String>` — None when no signal found.

#### TopicAccumulator

Per-session in-memory tally of topic signals. Lives in `SessionState`.

| Field | Type | Description |
|-------|------|-------------|
| counts | `HashMap<String, u32>` | Topic → occurrence count |
| last_seen | `HashMap<String, u64>` | Topic → timestamp of most recent signal |

Design choice: `HashMap<String, u32>` over `Vec<String>` per R2 risk assessment. O(unique topics) memory, O(1) vote resolution. Typical sessions have 1-3 unique topics.

#### MajorityVoteResult

Outcome of topic resolution at SessionClose.

| Variant | Description |
|---------|-------------|
| `Resolved(String)` | Clear winner or tie broken by recency |
| `NoSignals` | No topic signals accumulated; triggers fallback |

#### ObservationRow (extended)

Existing struct in `listener.rs:1579`. Extended with one field.

| Field | Type | Description |
|-------|------|-------------|
| session_id | `String` | Session identifier |
| ts_millis | `i64` | Event timestamp in milliseconds |
| hook | `String` | Hook event type |
| tool | `Option<String>` | Tool name (PreToolUse/PostToolUse) |
| input | `Option<String>` | Tool input or prompt text |
| response_size | `Option<i64>` | Response size (PostToolUse) |
| response_snippet | `Option<String>` | Response snippet (PostToolUse) |
| **topic_signal** | **`Option<String>`** | **NEW: Extracted topic signal for this event** |

#### ImplantEvent (extended)

Existing struct in `wire.rs:177`. Extended with one field.

| Field | Type | Description |
|-------|------|-------------|
| event_type | `String` | Hook event type |
| session_id | `String` | Session identifier |
| timestamp | `u64` | Unix timestamp (seconds) |
| payload | `Value` | Event-specific data |
| **topic_signal** | **`Option<String>`** | **NEW: Topic signal extracted by hook process** |

Uses `#[serde(default)]` for backward compatibility (R5).

#### SessionState (extended)

Existing struct in `session.rs:83`. Extended with accumulator fields.

| Existing field | Type |
|---------------|------|
| session_id | `String` |
| role | `Option<String>` |
| feature | `Option<String>` |
| injection_history | `Vec<InjectionRecord>` |
| coaccess_seen | `HashSet<Vec<u64>>` |
| compaction_count | `u32` |
| signaled_entries | `HashSet<u64>` |
| rework_events | `Vec<ReworkEvent>` |
| agent_actions | `Vec<SessionAction>` |
| last_activity_at | `u64` |

| **New field** | **Type** | **Description** |
|--------------|----------|-----------------|
| **topic_counts** | **`HashMap<String, u32>`** | **Topic → signal count** |
| **topic_last_seen** | **`HashMap<String, u64>`** | **Topic → most recent timestamp** |

### Schema Changes

#### observations table (extended)

```sql
ALTER TABLE observations ADD COLUMN topic_signal TEXT;
```

Nullable. Existing rows unaffected. No index needed (not queried by topic_signal directly).

#### sessions table (no DDL change)

`sessions.feature_cycle` already exists as nullable TEXT. This feature populates it.

### Migration

Schema version: v9 → v10 (R3: col-017 owns migration shell; col-018/col-019 add their DDL to the same function).

Migration steps:
1. Backup database file
2. `ALTER TABLE observations ADD COLUMN topic_signal TEXT`
3. Backfill: run content-based attribution on all sessions where `feature_cycle IS NULL` and `status != 'active'` (closed sessions only). Update `sessions.feature_cycle` with results.
4. Bump schema_version to 10

Backfill constraint: ~100 sessions typical, content scan is cheap, runs synchronously in migration (R7).

## Functional Requirements

### FR-01: Topic Signal Extraction Facade

**File**: `crates/unimatrix-observe/src/attribution.rs`

FR-01.1: Add `pub fn extract_topic_signal(text: &str) -> Option<String>` that encapsulates the priority chain: `extract_from_path` → `extract_feature_id_pattern` → `extract_from_git_checkout`. Returns first match or None.

FR-01.2: Make `extract_from_path`, `extract_feature_id_pattern`, and `extract_from_git_checkout` `pub` (currently private). The facade is the preferred entry point, but individual functions remain available for callers needing specific extraction (R1: cross-crate API surface).

FR-01.3: Existing unit tests (20+) must pass unchanged.

### FR-02: Wire Protocol Extension

**File**: `crates/unimatrix-engine/src/wire.rs`

FR-02.1: Add `topic_signal: Option<String>` to `ImplantEvent` with `#[serde(default)]`.

FR-02.2: Deserialization of payloads without `topic_signal` field must produce `None` (backward compat — R5).

FR-02.3: Deserialization of payloads with `topic_signal: "col-017"` must produce `Some("col-017".to_string())`.

FR-02.4: Serialization of `ImplantEvent` with `topic_signal: Some(...)` must include the field. Serialization with `None` may omit or include null (both acceptable).

### FR-03: Hook-Side Extraction in build_request

**File**: `crates/unimatrix-server/src/uds/hook.rs`

FR-03.1: In `build_request()`, after constructing the `ImplantEvent` payload, call `extract_topic_signal()` on the relevant text content. Set `topic_signal` on the `ImplantEvent`.

FR-03.2: Extraction sources per event type:

| Event | Text source for extraction |
|-------|---------------------------|
| PreToolUse | `input.extra["tool_input"]` as string |
| PostToolUse (non-rework) | `input.extra["tool_input"]` as string |
| SubagentStart | `input.extra["prompt_snippet"]` as string |
| UserPromptSubmit (record path) | `input.prompt` |
| Other events | `serde_json::to_string(&input.extra)` (best-effort scan of payload) |

FR-03.3: In `generic_record_event()`, extract topic signal from `serde_json::to_string(&input.extra)` and set on `ImplantEvent.topic_signal`.

FR-03.4: Extraction must be cheap: string scanning only, no I/O, no allocation beyond the result string. The existing attribution functions satisfy this constraint.

### FR-04: Observation Persistence with Topic Signal

**File**: `crates/unimatrix-server/src/uds/listener.rs`

FR-04.1: Add `topic_signal: Option<String>` to `ObservationRow`.

FR-04.2: In `extract_observation_fields()`, propagate `event.topic_signal` to `ObservationRow.topic_signal`.

FR-04.3: In `insert_observation()`, include `topic_signal` in the INSERT statement: `INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)`.

FR-04.4: In `insert_observations_batch()`, same extension for batch inserts.

### FR-05: Server-Side Topic Accumulation

**File**: `crates/unimatrix-server/src/infra/session.rs` and `crates/unimatrix-server/src/uds/listener.rs`

FR-05.1: Add `topic_counts: HashMap<String, u32>` and `topic_last_seen: HashMap<String, u64>` to `SessionState`. Default to empty HashMaps.

FR-05.2: Add method `SessionState::record_topic_signal(&mut self, signal: &str, timestamp: u64)` that increments the count for the signal and updates last_seen if the timestamp is newer.

FR-05.3: In the RecordEvent dispatch path (listener.rs), after inserting the observation, if `event.topic_signal` is `Some(signal)`, call `session_state.record_topic_signal(&signal, event.timestamp)`. This is fire-and-forget (no error propagation needed for the signal tally).

### FR-06: SessionClose Topic Resolution

**File**: `crates/unimatrix-server/src/uds/listener.rs`

FR-06.1: Add `fn majority_vote(counts: &HashMap<String, u32>, last_seen: &HashMap<String, u64>) -> Option<String>`:
- If `counts` is empty, return `None`.
- Find the maximum count. If one topic has it, return that topic.
- If tied, among tied topics, return the one with the highest `last_seen` timestamp.
- If still tied (same timestamp), return any (deterministic: lexicographic smallest).

FR-06.2: In the SessionClose dispatch path, before existing session close logic:
1. Retrieve `SessionState` for the session.
2. Call `majority_vote(&state.topic_counts, &state.topic_last_seen)`.
3. If `Some(topic)`: update `sessions.feature_cycle = topic` via `store.update_session_feature_cycle(session_id, &topic)`.
4. If `None`: fall back to content-based attribution — load observations for this session from DB, run `attribute_sessions()`, persist result.

FR-06.3: The `UPDATE sessions SET feature_cycle = ?` must be fire-and-forget (spawn_blocking). Errors are logged but do not block the SessionClose response (R6, constraint 6).

FR-06.4: Add `Store::update_session_feature_cycle(session_id: &str, topic: &str) -> Result<()>` if it does not exist. Simple `UPDATE sessions SET feature_cycle = ? WHERE session_id = ?`.

### FR-07: Content-Based Attribution Persistence

**File**: `crates/unimatrix-observe/src/attribution.rs` or `crates/unimatrix-server/src/uds/listener.rs`

FR-07.1: When the content-based fallback path runs (either at SessionClose or during retrospective), persist the attributed `feature_cycle` to the sessions table. Currently `attribute_sessions()` returns results but does not persist them.

FR-07.2: After `attribute_sessions()` returns attributed sessions, for each session with a resolved topic, call `store.update_session_feature_cycle(session_id, &topic)`.

### FR-08: Schema Migration v9 → v10

**File**: `crates/unimatrix-store/src/migration.rs`

FR-08.1: Add `fn migrate_v9_to_v10(conn: &rusqlite::Connection, db_path: &Path) -> Result<()>`.

FR-08.2: Migration adds `topic_signal TEXT` column to `observations` table.

FR-08.3: Migration runs backfill: query all sessions where `feature_cycle IS NULL AND ended_at IS NOT NULL`. For each, load observations, run content-based attribution, update `feature_cycle`.

FR-08.4: Bump `CURRENT_SCHEMA_VERSION` to 10.

FR-08.5: Wire into `migrate_if_needed()` dispatch chain.

FR-08.6: col-018 and col-019 add their DDL to the same `migrate_v9_to_v10` function (R3 coordination).

## Non-Functional Requirements

### NFR-01: Hook Process Latency

Topic extraction in `build_request()` must add < 1ms to hook process time. The existing attribution functions are pure string scanning with no I/O, no allocations beyond the result string, and no regex compilation at call time.

### NFR-02: Memory Bound

Per-session topic accumulator memory must be O(unique topics), not O(events). The `HashMap<String, u32>` design ensures this (R2). Typical bound: < 1KB per session.

### NFR-03: Backward Compatibility

- Old hook binary → new server: `ImplantEvent` deserialized with `topic_signal: None`. Server falls back to content-based attribution at SessionClose.
- New hook binary → old server: Extra `topic_signal` field silently ignored by serde.
- Old observations (pre-migration): `topic_signal IS NULL`. No functional impact.

### NFR-04: Fire-and-Forget Writes

Session feature_cycle update on SessionClose must not block the hook response path. Use `spawn_blocking` for the DB write, log errors.

## Acceptance Criteria

| AC | Description | Functional Requirement | Test Strategy |
|----|-------------|----------------------|---------------|
| AC-01 | `extract_topic_signal()` facade returns correct result for file path input | FR-01.1 | Unit test: input containing `product/features/col-002/SCOPE.md` returns `Some("col-002")` |
| AC-02 | `extract_topic_signal()` returns correct result for feature ID pattern | FR-01.1 | Unit test: input `"Working on col-002 design"` returns `Some("col-002")` |
| AC-03 | `extract_topic_signal()` returns correct result for git branch | FR-01.1 | Unit test: input `"git checkout -b feature/col-002"` returns `Some("col-002")` |
| AC-04 | `extract_topic_signal()` respects priority order (path > pattern > git) | FR-01.1 | Unit test: input containing both path and pattern returns path result |
| AC-05 | `extract_topic_signal()` returns None for no-signal input | FR-01.1 | Unit test: input `"regular text with no features"` returns `None` |
| AC-06 | `ImplantEvent` deserializes without `topic_signal` field → None | FR-02.2 | Unit test: deserialize JSON without topic_signal, assert field is None |
| AC-07 | `ImplantEvent` deserializes with `topic_signal` field → Some | FR-02.3 | Unit test: deserialize JSON with topic_signal, assert field is Some |
| AC-08 | `build_request()` extracts topic signal for PreToolUse with file path in tool_input | FR-03.1, FR-03.2 | Unit test: PreToolUse with tool_input containing feature path, assert topic_signal is Some |
| AC-09 | `build_request()` extracts topic signal for SubagentStart with feature ID in prompt_snippet | FR-03.2 | Unit test: SubagentStart with prompt_snippet containing feature ID |
| AC-10 | `build_request()` sets topic_signal None when no signal in input | FR-03.1 | Unit test: event with generic content, assert topic_signal is None |
| AC-11 | `ObservationRow` includes topic_signal and it is persisted to DB | FR-04.1–FR-04.3 | Integration test: insert observation with topic_signal, query back, assert column value |
| AC-12 | `SessionState::record_topic_signal` increments count and updates last_seen | FR-05.2 | Unit test: record same signal twice, assert count=2, last_seen=latest |
| AC-13 | `majority_vote` returns clear winner when one topic dominates | FR-06.1 | Unit test: counts={col-017: 5, col-018: 2}, returns Some("col-017") |
| AC-14 | `majority_vote` breaks tie by recency | FR-06.1 | Unit test: counts={a: 3, b: 3}, last_seen={a: 100, b: 200}, returns Some("b") |
| AC-15 | `majority_vote` returns None for empty counts | FR-06.1 | Unit test: empty HashMap, returns None |
| AC-16 | SessionClose resolves topic from accumulated signals and persists to sessions.feature_cycle | FR-06.2–FR-06.3 | Integration test: register session, send events with topic signals, send SessionClose, query sessions table, assert feature_cycle is populated |
| AC-17 | SessionClose falls back to content-based attribution when no signals | FR-06.2 | Integration test: register session, send events without topic signals, send SessionClose, assert feature_cycle populated via fallback |
| AC-18 | Content-based attribution results are persisted to sessions.feature_cycle | FR-07.1–FR-07.2 | Integration test: run retrospective attribution, assert sessions table updated |
| AC-19 | Migration v9→v10 adds topic_signal column to observations | FR-08.1–FR-08.2 | Migration test: run migration, assert column exists |
| AC-20 | Migration backfills feature_cycle for existing closed sessions | FR-08.3 | Migration test: create sessions with observations pre-migration, run migration, assert feature_cycle populated |
| AC-21 | `context_retrospective` returns non-empty results for features with attributed sessions | FR-06, FR-07 | End-to-end test: full pipeline through retrospective query |
| AC-22 | Existing attribution unit tests pass unchanged | FR-01.3 | `cargo test -p unimatrix-observe` |

## Acceptance Criteria Traceability

| AC | Functional Requirement | Test Location |
|----|----------------------|---------------|
| AC-01–AC-05 | FR-01 | `unimatrix-observe/src/attribution.rs` (unit tests) |
| AC-06–AC-07 | FR-02 | `unimatrix-engine/src/wire.rs` (unit tests) |
| AC-08–AC-10 | FR-03 | `unimatrix-server/src/uds/hook.rs` (unit tests) |
| AC-11 | FR-04 | `unimatrix-server` (integration test) |
| AC-12 | FR-05 | `unimatrix-server/src/infra/session.rs` (unit test) |
| AC-13–AC-15 | FR-06.1 | `unimatrix-server/src/uds/listener.rs` (unit tests) |
| AC-16–AC-17 | FR-06.2–FR-06.3 | `unimatrix-server` (integration test) |
| AC-18 | FR-07 | `unimatrix-server` or `unimatrix-observe` (integration test) |
| AC-19–AC-20 | FR-08 | `unimatrix-store/src/migration.rs` (migration test) |
| AC-21 | FR-06, FR-07 | `unimatrix-server` (end-to-end test) |
| AC-22 | FR-01.3 | `unimatrix-observe` (existing tests) |

## Constraints

- C-01: Hook-side extraction must be pure string scanning — no I/O, no network, no disk reads. Existing attribution functions satisfy this.
- C-02: `ImplantEvent.topic_signal` must use `#[serde(default)]` for wire backward compatibility (R5).
- C-03: `observations.topic_signal` column must be nullable TEXT. No NOT NULL constraint.
- C-04: `SessionState` topic accumulation must use `HashMap<String, u32>` for O(unique topics) memory bound (R2).
- C-05: SessionClose feature_cycle persistence must be fire-and-forget via `spawn_blocking` — must not add latency to hook response.
- C-06: Migration must be owned by col-017. col-018/col-019 add DDL to the same `migrate_v9_to_v10` function (R3).
- C-07: No changes to Claude Code hook schema — extraction runs entirely on our side.
- C-08: `topic` and `feature_cycle` are the same concept. New code uses `topic` naming; `feature_cycle` remains in existing schemas.
- C-09: Extend existing test infrastructure (`TestDb`, existing hook test helpers). No parallel scaffolding.
- C-10: False-positive mitigation: `is_valid_feature_id` filter rejects common noise patterns. Majority vote resolves remaining ambiguity (R4).
