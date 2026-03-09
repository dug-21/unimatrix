# col-017: Hook-Side Topic Attribution — Implementation Brief

## Problem

`context_retrospective` returns empty for 100% of features. `sessions.feature_cycle` is never populated because: (1) Claude Code doesn't send `feature_cycle` in hook input, (2) content-based attribution runs only at retrospective time and never persists results, (3) retrospective queries `sessions WHERE feature_cycle IS NOT NULL` — zero rows.

## Solution

Push topic extraction to hook edge, accumulate signals server-side per session, resolve on SessionClose via majority vote, persist to `sessions.feature_cycle`.

## Architecture Decisions

| ADR | Decision | Rationale |
|-----|----------|-----------|
| ADR-017-001 | Single `extract_topic_signal()` facade; individual extractors stay private | Priority ordering (path > pattern > git) is a domain invariant, not a caller choice. One cross-crate contract. |
| ADR-017-002 | `HashMap<String, TopicTally>` accumulator with `TopicTally { count: u32, last_seen: u64 }` | O(unique topics) memory, O(1) vote resolution. Spec's two-HashMap variant superseded — use single struct per ADR. |
| ADR-017-003 | col-017 owns v9→v10 migration shell; col-018/col-019 append DDL | Single shared migration avoids version fragility. Established pattern (v5→v6 precedent). |

### Discrepancy Resolutions

1. **TopicTally struct vs two HashMaps**: Spec FR-05.1 defines `topic_counts: HashMap<String, u32>` and `topic_last_seen: HashMap<String, u64>`. Architecture ADR-017-002 defines `TopicTally { count, last_seen }` with single `HashMap<String, TopicTally>`. **Follow ADR** — single struct, one lookup per signal.

2. **FR-01.2 pub vs ADR facade-only**: Spec FR-01.2 says make individual extractors `pub`. ADR-017-001 says keep them private, only facade is public. **Follow ADR** — facade-only public.

---

## Components

### C1: Extraction Facade
**Crate**: `unimatrix-observe` | **File**: `src/attribution.rs`

Add `pub fn extract_topic_signal(text: &str) -> Option<String>` that chains: `extract_from_path` → `extract_feature_id_pattern` → `extract_from_git_checkout`. First match wins. Individual extractors stay private (ADR-017-001). No other visibility changes.

**FR**: FR-01.1, FR-01.3 | **AC**: AC-01–AC-05, AC-22

### C2: Wire Protocol Extension
**Crate**: `unimatrix-engine` | **File**: `src/wire.rs`

Add to `ImplantEvent`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub topic_signal: Option<String>,
```

**FR**: FR-02.1–FR-02.4 | **AC**: AC-06, AC-07

### C3: Hook-Side Extraction
**Crate**: `unimatrix-server` | **File**: `src/uds/hook.rs`

In `build_request()`, extract topic signal from tool inputs per event type:

| Event | Text source |
|-------|------------|
| PreToolUse | `input.extra["tool_input"]` stringified |
| PostToolUse (non-rework) | `input.extra["tool_input"]` stringified |
| SubagentStart | `input.extra["prompt_snippet"]` stringified |
| UserPromptSubmit (record path) | `input.prompt` |
| Other (generic_record_event) | `serde_json::to_string(&input.extra)` |

Call `extract_topic_signal(text)` → set on `ImplantEvent.topic_signal`. Must be cheap: string scanning only, no I/O.

**FR**: FR-03.1–FR-03.4 | **AC**: AC-08–AC-10

### C4: Session Accumulation
**Crate**: `unimatrix-server` | **Files**: `src/infra/session.rs`, `src/uds/listener.rs`

Add to `SessionState`:
```rust
pub topic_signals: HashMap<String, TopicTally>,
```

```rust
pub struct TopicTally {
    pub count: u32,
    pub last_seen: u64,
}
```

Add `SessionState::record_topic_signal(&mut self, signal: String, timestamp: u64)` — increments count, updates `last_seen` if timestamp is newer.

In RecordEvent dispatch: if `event.topic_signal.is_some()`, call `session_state.record_topic_signal()`.

**FR**: FR-05.1–FR-05.3 | **AC**: AC-12

### C5: Observation Persistence
**Crate**: `unimatrix-server` | **File**: `src/uds/listener.rs`

- Add `topic_signal: Option<String>` to `ObservationRow`
- Update `extract_observation_fields()` to propagate `event.topic_signal`
- Update `insert_observation()` SQL: 7→8 positional parameters
- Update `insert_observations_batch()` likewise

**Critical risk SR-1**: Column count mismatch between struct fields, SQL columns, and parameter bindings. Must verify roundtrip in integration test.

**FR**: FR-04.1–FR-04.4 | **AC**: AC-11

### C6: SessionClose Resolution
**Crate**: `unimatrix-server` | **File**: `src/uds/listener.rs`

Add `fn majority_vote(signals: &HashMap<String, TopicTally>) -> Option<String>`:
1. If empty → `None`
2. Find max count. Single winner → return it.
3. Tie → highest `last_seen`. Still tied → lexicographic smallest.

In SessionClose dispatch:
1. Retrieve `SessionState` topic_signals
2. `majority_vote()` → if `Some(topic)`, fire-and-forget `UPDATE sessions SET feature_cycle = topic`
3. If `None` → fallback: load observations, run `attribute_sessions()`, persist result
4. DB write via `spawn_blocking` — must not block hook response

Add `Store::update_session_feature_cycle(session_id: &str, topic: &str) -> Result<()>` if not existing.

**FR**: FR-06.1–FR-06.4, FR-07.1–FR-07.2 | **AC**: AC-13–AC-18

### C7: Schema Migration v9→v10
**Crate**: `unimatrix-store` | **File**: `src/migration.rs`

1. `fn migrate_v9_to_v10(conn, db_path)`:
   - `ALTER TABLE observations ADD COLUMN topic_signal TEXT`
   - Backfill: `SELECT session_id FROM sessions WHERE feature_cycle IS NULL AND ended_at IS NOT NULL` → load observations → `attribute_sessions()` → `UPDATE sessions SET feature_cycle`
2. Bump `CURRENT_SCHEMA_VERSION` to 10
3. Wire into `migrate_if_needed()` dispatch

**Backfill safety (AR-3)**: Only closed sessions (`ended_at IS NOT NULL`). Active sessions must NOT be backfilled.

**FR**: FR-08.1–FR-08.6 | **AC**: AC-19, AC-20

---

## Implementation Order

```
C1 (facade) ──┐
C2 (wire)  ───┼── C3 (hook extraction) ── C5 (observation persist) ── C4 (accumulation) ── C6 (resolution) ── C7 (migration)
              │
              └── Independent, no ordering constraint between C1/C2
```

**Recommended sequence**:
1. **C1 + C2** in parallel — no dependencies, foundational
2. **C7** (migration) — schema must exist before integration tests
3. **C5** (observation persistence) — depends on C2 (ImplantEvent field)
4. **C3** (hook extraction) — depends on C1 (facade) and C2 (wire field)
5. **C4** (accumulation) — depends on C2
6. **C6** (resolution) — depends on C4, C5, and C3

## Constraints

- C-01: Hook extraction = pure string scanning. No I/O, no network, no disk.
- C-02: `ImplantEvent.topic_signal` uses `#[serde(default)]` for wire compat.
- C-03: `observations.topic_signal` = nullable TEXT. No NOT NULL.
- C-04: Accumulator = `HashMap<String, TopicTally>` (ADR-017-002).
- C-05: SessionClose persistence = fire-and-forget via `spawn_blocking`.
- C-06: col-017 owns migration. col-018/col-019 append to same function.
- C-07: No Claude Code hook schema changes.
- C-08: `topic` = `feature_cycle`. New code uses `topic`; schemas keep `feature_cycle`.
- C-09: Extend existing test infrastructure. No new scaffolding.

## Non-Functional Requirements

| NFR | Target |
|-----|--------|
| Hook extraction latency | < 1ms added to hook process time |
| Per-session memory | O(unique topics), < 1KB typical |
| Wire backward compat | Old↔new in both directions via serde(default) |
| SessionClose DB write | Fire-and-forget, non-blocking |

## Key Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| SR-1: INSERT column count mismatch | P0 | Integration test: roundtrip observation with/without topic_signal |
| AR-1: Facade priority inversion | P1 | Unit test: input with multiple signal types returns highest-priority |
| AR-2: Tie-breaking determinism | P1 | Unit test: tied counts + same timestamp → lexicographic fallback |
| AR-3: Backfill touches active sessions | P1 | Migration WHERE clause: `ended_at IS NOT NULL` |
| SR-2: generic_record_event over-extraction | P1 | Unit test: JSON with false-positive patterns rejected |
