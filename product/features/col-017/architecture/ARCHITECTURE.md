# col-017: Hook-Side Topic Attribution — Architecture

## Overview

Push topic extraction to the hook edge, accumulate signals server-side per session, resolve on SessionClose via majority vote, and persist to `sessions.feature_cycle`. This closes the broken feedback loop where `context_retrospective` returns empty for 100% of features.

Three layers: hook-side extraction → server-side accumulation → close-time resolution.

---

## ADR-017-001: Extraction Facade over Individual Functions

**Status**: Accepted
**Context**: `extract_from_path`, `extract_feature_id_pattern`, and `extract_from_git_checkout` in `unimatrix-observe/src/attribution.rs` are private. col-017 needs them callable from `unimatrix-server/src/uds/hook.rs`. Risk R1 identifies this as a cross-crate API surface decision.

**Decision**: Expose a single public facade function:

```rust
/// Extract a topic signal from arbitrary text using the canonical priority chain.
/// Returns the first match: file path > feature ID pattern > git checkout.
pub fn extract_topic_signal(text: &str) -> Option<String> {
    extract_from_path(text)
        .or_else(|| extract_feature_id_pattern(text))
        .or_else(|| extract_from_git_checkout(text))
}
```

Individual extractors remain private. The facade encapsulates the priority ordering so callers cannot misuse it.

**Rationale**:
- Single call site in `hook.rs` — callers don't need granular access.
- Priority ordering (path > pattern > git) is a domain invariant, not a caller choice.
- If a future caller needs individual extractors, promote them then — not speculatively.
- Addresses R1: stable API surface is one function, not three.

**Consequences**:
- `unimatrix-server` depends on `unimatrix-observe` (already the case via `attribution.rs::attribute_sessions()`).
- Signature change to `extract_topic_signal` is the only cross-crate contract.

---

## ADR-017-002: HashMap Accumulator for Topic Signals

**Status**: Accepted
**Context**: `SessionState` needs to accumulate topic signals over a session's lifetime. Risk R2 identifies `Vec<String>` vs `HashMap<String, (u32, u64)>` as a design choice with memory and resolution implications.

**Decision**: Use `HashMap<String, TopicTally>` in `SessionState`:

```rust
pub struct TopicTally {
    pub count: u32,
    pub last_seen: u64,  // timestamp for tie-breaking
}

// In SessionState:
pub topic_signals: HashMap<String, TopicTally>,
```

**Rationale**:
- Memory is O(unique topics) not O(events). Typical sessions have 1-3 unique topics.
- Majority vote resolution is O(unique topics) — iterate the map, find max count.
- Tie-breaking by `last_seen` is built into the structure (no need to scan Vec for recency).
- Addresses R2: bounded memory even in pathological long sessions.

**Resolution algorithm** (on SessionClose):
1. Find entry with max `count`.
2. If tie, pick the entry with highest `last_seen` (most recent).
3. If map is empty, fall back to content-based attribution.

**Consequences**:
- Slightly more complex than Vec for tests (must construct HashMap entries).
- `TopicTally` is a small internal struct in `session.rs`, not a public API.

---

## ADR-017-003: Independent Migration Versioning (v9 → v10)

**Status**: Accepted
**Context**: col-017, col-018, and col-019 are Wave 1 parallel features. All may need schema changes. Risk R3 identifies `CURRENT_SCHEMA_VERSION` as a single-point merge conflict and asks: shared migration or independent bumps?

**Decision**: col-017 owns the v9 → v10 migration. col-018 and col-019 add their DDL to the same v10 migration function via additive statements, rebasing onto col-017's migration shell if it merges first.

Migration v9 → v10 for col-017:
```sql
ALTER TABLE observations ADD COLUMN topic_signal TEXT;
```

If col-018/col-019 merge after col-017, they append their ALTER statements to the existing `migrate_v9_to_v10()` function. If they merge before, col-017 adapts.

**Rationale**:
- Independent version bumps (v9→v10, v10→v11, v11→v12) waste version numbers and create ordering fragility — the second feature to merge must know which version it starts from.
- Shared migration with one owner is the established pattern (v5→v6 normalized multiple tables in one migration).
- The integration test `test_schema_version_is_N` catches mismatches.
- Merge conflicts in a single function are trivial to resolve (additive ALTER statements).

**Consequences**:
- col-017 should merge first or the migration shell must be coordinated.
- `CURRENT_SCHEMA_VERSION` bump to 10 happens once, not three times.

---

## Component Architecture

### 1. Hook-Side Extraction (`unimatrix-server/src/uds/hook.rs`)

**Change**: In `build_request()` and `generic_record_event()`, extract topic signal from tool inputs before constructing the `ImplantEvent`.

```
build_request(event, input)
  ├── extract text from input.extra["tool_input"] / input.prompt / etc.
  ├── call extract_topic_signal(text) → Option<String>
  └── attach to ImplantEvent.topic_signal
```

**Extraction sources by event type**:

| Event | Text source | Field path |
|-------|------------|------------|
| PreToolUse | Tool input text | `input.extra["tool_input"]` (stringified) |
| PostToolUse | Tool input text | `input.extra["tool_input"]` (stringified) |
| SubagentStart | Prompt snippet | `input.extra["prompt_snippet"]` (stringified) |
| UserPromptSubmit | Prompt text | `input.prompt` |
| SessionStart | — | No extraction (feature from `input.extra["feature_cycle"]`) |
| Stop | — | No extraction |

For `generic_record_event()`: stringify `input.extra` values and run `extract_topic_signal()` on the concatenated text.

### 2. Wire Protocol (`unimatrix-engine/src/wire.rs`)

**Change**: Add `topic_signal` to `ImplantEvent`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplantEvent {
    pub event_type: String,
    pub session_id: String,
    pub timestamp: u64,
    pub payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_signal: Option<String>,
}
```

`serde(default)` ensures backward compatibility in both directions (R5).

### 3. Server-Side Accumulation (`unimatrix-server/src/infra/session.rs`)

**Change**: Add `topic_signals: HashMap<String, TopicTally>` to `SessionState`.

**New method on SessionState**:
```rust
pub fn record_topic_signal(&mut self, signal: String, timestamp: u64) {
    let tally = self.topic_signals.entry(signal).or_insert(TopicTally { count: 0, last_seen: 0 });
    tally.count += 1;
    tally.last_seen = timestamp;
}
```

### 4. Dispatch Integration (`unimatrix-server/src/uds/listener.rs`)

**RecordEvent/RecordEvents handler changes**:
- After `extract_observation_fields()`, if `event.topic_signal.is_some()`, call `session_state.record_topic_signal()`.
- Add `topic_signal` field to `ObservationRow` and `insert_observation()` SQL.

**SessionClose handler changes** (in `process_session_close()`):
- Before drain, resolve topic from `session_state.topic_signals`:
  1. Find max-count entry (tie-break by `last_seen`).
  2. If resolved and `session_state.feature` is None, set it.
  3. If no signals, fall back: load observations → `attribute_sessions()` → persist.
- `UPDATE sessions SET feature_cycle = ? WHERE session_id = ?` (fire-and-forget via `spawn_blocking`).

### 5. Observation Schema (`unimatrix-store/src/migration.rs`)

**v9 → v10 migration**:
```sql
ALTER TABLE observations ADD COLUMN topic_signal TEXT;
```

**Backfill** (in same migration):
- SELECT sessions WHERE feature_cycle IS NULL.
- For each, load observations, run `attribute_sessions()`, persist result.
- Bounded by existing session count (~100 typical).

### 6. Attribution Visibility (`unimatrix-observe/src/attribution.rs`)

**Change**: Add `pub fn extract_topic_signal(text: &str) -> Option<String>` as described in ADR-017-001. No other visibility changes.

---

## Data Flow (Complete)

```
Claude Code Hook Event (stdin JSON)
        │
        ▼
  build_request()
  ├── extract text from input fields
  ├── extract_topic_signal(text) → Option<String>
  └── ImplantEvent { ..., topic_signal }
        │
        ▼
  HookRequest::RecordEvent { event }
        │
  ──── UDS wire ────
        │
        ▼
  dispatch_request()
  ├── extract_observation_fields(event) → ObservationRow (+ topic_signal)
  ├── insert_observation(store, obs)     // persists topic_signal column
  └── if event.topic_signal.is_some():
      session_state.record_topic_signal(signal, timestamp)
        │
        ▼
  SessionClose
  ├── resolve_topic(session_state.topic_signals)
  │   ├── max count, tie-break by last_seen
  │   └── or: fallback content-based attribution
  ├── UPDATE sessions SET feature_cycle = topic
  └── (write_auto_outcome_entry deleted — GH #430; SESSIONS holds session telemetry)
        │
        ▼
  context_retrospective("col-017")
  └── SELECT sessions WHERE feature_cycle = 'col-017'  ← now returns rows
```

---

## Integration Surfaces

| Surface | Crate boundary | Change type | Risk |
|---------|---------------|-------------|------|
| `extract_topic_signal()` | observe → server | New pub fn | Low (facade, stable) |
| `ImplantEvent.topic_signal` | engine → server | New field | Low (serde default) |
| `SessionState.topic_signals` | server internal | New field | Low (additive) |
| `ObservationRow.topic_signal` | server internal | New field | Low (additive) |
| `observations.topic_signal` | store migration | New column | Low (nullable, additive) |
| `sessions.feature_cycle` | store (existing) | Populated | None (column exists) |

**Cross-crate dependencies** (no new crate deps):
- `unimatrix-server` already depends on `unimatrix-observe` and `unimatrix-engine`
- `unimatrix-store` already depends on `unimatrix-engine`

---

## Testing Strategy

| Layer | Test type | Location |
|-------|-----------|----------|
| `extract_topic_signal()` facade | Unit | `attribution.rs` (extend existing 20+ tests) |
| `TopicTally` + `record_topic_signal()` | Unit | `session.rs` |
| Majority vote resolution | Unit | `listener.rs` or `session.rs` |
| Wire protocol backward compat | Unit | `wire.rs` (deserialize with/without field) |
| `ObservationRow` + `insert_observation` with topic_signal | Unit | `listener.rs` |
| Migration v9→v10 | Integration | `migration.rs` existing migration test suite |
| End-to-end: hook event → session.feature_cycle populated | Integration | `listener.rs` integration tests |
| Backfill correctness | Integration | Migration test with pre-existing unattributed sessions |

---

## Risk Mitigations Summary

| Risk | Mitigation in Architecture |
|------|---------------------------|
| R1: Cross-crate API | ADR-017-001: Single facade, individual extractors stay private |
| R2: Memory growth | ADR-017-002: HashMap<String, TopicTally> bounds to O(unique topics) |
| R3: Migration coordination | ADR-017-003: col-017 owns v9→v10, others append additively |
| R4: False positives | Priority chain in facade + majority vote + `is_valid_feature_id` filter |
| R5: Wire compat | `serde(default)` + `skip_serializing_if` on `ImplantEvent.topic_signal` |
| R6: SessionClose race | UDS preserves ordering; stale threshold provides buffer |
| R7: Fallback perf | Content-based fallback only when no hook signals; ~100ms typical |
