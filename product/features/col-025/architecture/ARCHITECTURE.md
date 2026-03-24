# col-025: Feature Goal Signal — Architecture

## System Overview

col-025 introduces a `goal` field to the feature cycle lifecycle. When an agent
starts a cycle with `context_cycle(type: "start", goal: "...")`, the intent of
that feature is now a first-class, durable field — stored in `cycle_events`,
cached in `SessionState`, and used as the query signal for both `context_briefing`
and UDS hook injection. This replaces the weak topic-ID fallback that previously
drove retrieval when no explicit `task` was provided.

The change spans four layers:

1. **Schema** — `cycle_events` gains a `goal TEXT` column (v15 → v16).
2. **Store interface** — `insert_cycle_event` signature gains `goal: Option<&str>`.
3. **Session state** — `SessionState` gains `current_goal: Option<String>` with two population paths (start and resume).
4. **Query derivation** — `derive_briefing_query` step 2 returns `current_goal` instead of synthesizing from topic signals; the `SubagentStart` injection path adds an explicit goal-as-fallback branch.

All other scoring, ranking, and embedding pipeline behaviour is unchanged.

---

## Component Breakdown

### Component 1: Schema Migration (v15 → v16)

**Crate**: `unimatrix-store`
**Files**: `src/migration.rs`, `src/db.rs`

Responsibilities:
- Add `goal TEXT` column to `cycle_events` via idempotent `ALTER TABLE` with `pragma_table_info` pre-check.
- Bump `CURRENT_SCHEMA_VERSION` from 15 to 16.
- Update `insert_cycle_event` to accept and bind `goal: Option<&str>`.
- Provide `get_cycle_start_goal(cycle_id: &str) -> Result<Option<String>>` read helper for the session resume path.

### Component 2: SessionState Extension

**Crate**: `unimatrix-server`
**Files**: `src/infra/session.rs`

Responsibilities:
- Add `current_goal: Option<String>` to `SessionState`.
- Initialize to `None` in `register_session` (existing call site).
- Provide `set_current_goal(&self, session_id: &str, goal: Option<String>)` on `SessionRegistry`.

### Component 3: Cycle Event Handler

**Crate**: `unimatrix-server`
**Files**: `src/uds/listener.rs` (fn `handle_cycle_event`)

Responsibilities:
- Extract `goal` from `CYCLE_START_EVENT` payload.
- Set `state.current_goal` synchronously in the in-memory registry (same section as `set_feature_force` and `set_current_phase`).
- Pass `goal` into the fire-and-forget `insert_cycle_event` spawn (no DB read required on the start path).
- `CYCLE_PHASE_END` and `CYCLE_STOP` events do not read or modify `current_goal`.

### Component 4: MCP context_cycle Wire Protocol

**Crate**: `unimatrix-server`
**Files**: `src/mcp/tools.rs` (struct `CycleParams`)

Responsibilities:
- Add `goal: Option<String>` to `CycleParams`.
- On `CycleType::Start`: extract goal, emit it in the fire-and-forget `ImplantEvent` payload so the UDS listener receives it via the hook path.
- Backward compatible: callers omitting `goal` receive `None`.

### Component 5: Session Resume

**Crate**: `unimatrix-server`
**Files**: `src/uds/listener.rs` (fn `dispatch_request`, `SessionRegister` arm)

Responsibilities:
- On `SessionRegister`, if the session has a `feature_cycle` set: call `store.get_cycle_start_goal(feature_cycle).await` and populate `state.current_goal`.
- Degrade gracefully: any DB error or `None` result → `current_goal = None`.
- This is the only async/fallible path in the entire feature (SR-05).

### Component 6: Briefing Query Derivation

**Crate**: `unimatrix-server`
**Files**: `src/services/index_briefing.rs` (fn `derive_briefing_query`, fn `synthesize_from_session`)

Responsibilities:
- Replace `synthesize_from_session` body: return `state.current_goal.clone()` directly.
- When `Some`, this string wins step 2 over the current topic-signals synthesis.
- When `None` (no goal, legacy cycle), step 3 topic-ID fallback runs unchanged.
- Shared by both `handle_compact_payload` (UDS) and `context_briefing` (MCP) — both benefit from the change with no additional wiring.

### Component 7: SubagentStart Injection Precedence

**Crate**: `unimatrix-server`
**Files**: `src/uds/listener.rs` (SubagentStart hook arm, steps 5b)

Responsibilities:
- After extracting transcript query (existing step 5b), if transcript query is `None` OR empty:
  check `session_registry.get_state(session_id)?.current_goal` as fallback before topic-ID.
- Precedence: `transcript_query (non-empty)` → `current_goal` → topic.
- This is the only injection path not served automatically by `derive_briefing_query`; requires explicit branching (SR-03).

---

## Component Interactions

```
context_cycle(start, goal: "...") [MCP]
         │
         ▼
    CycleParams.goal  ──► ImplantEvent payload (hook path)
                                   │
                                   ▼
                         handle_cycle_event [UDS listener]
                           │                       │
                    [sync] set_current_goal     [async spawn]
                           │                   insert_cycle_event
                           ▼                   (goal bound)
                    SessionState.current_goal
                           │
              ┌────────────┴────────────────────┐
              ▼                                 ▼
   derive_briefing_query                 SubagentStart arm
   step 2: returns current_goal          explicit branch:
              │                          transcript → goal → topic
              ▼
   IndexBriefingService.index(query)

Session resume (server restart):
   SessionRegister ──► get_cycle_start_goal(feature_cycle) [async DB]
                                   │
                              current_goal (or None on error)
                                   │
                         set_current_goal [registry]
```

---

## Technology Decisions

See individual ADR files:
- **ADR-001**: Goal stored on `cycle_events`, not `sessions` (durability tier decision)
- **ADR-002**: `synthesize_from_session` returns `current_goal` directly, replacing topic-signal synthesis
- **ADR-003**: SubagentStart injection uses explicit goal branch (not routed through `derive_briefing_query`)
- **ADR-004**: Session resume DB failure degrades to `None`, not error
- **ADR-005**: Goal byte-length guard at the tool handler layer (4096 bytes)

---

## Integration Points

### Existing interfaces modified

| Component | Change |
|-----------|--------|
| `Store::insert_cycle_event` | +1 parameter: `goal: Option<&str>`, bound at the last bind position |
| `CycleParams` (MCP wire) | +1 field: `goal: Option<String>` |
| `SessionState` | +1 field: `current_goal: Option<String>` |
| `derive_briefing_query` / `synthesize_from_session` | Body change only, signature unchanged |

### New interfaces

| Interface | Signature | Source |
|-----------|-----------|--------|
| `Store::get_cycle_start_goal` | `async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` | `unimatrix-store/src/db.rs` |
| `SessionRegistry::set_current_goal` | `fn set_current_goal(&self, session_id: &str, goal: Option<String>)` | `unimatrix-server/src/infra/session.rs` |

### Schema change

`cycle_events` gains `goal TEXT` (nullable). v16 migration:

```sql
SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'
-- if 0:
ALTER TABLE cycle_events ADD COLUMN goal TEXT;
```

### Columns explicitly out of scope

`sessions.keywords TEXT` — present but dead since crt-025 WA-1. Must not be touched by this feature (SR-04).

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `CycleParams::goal` | `Option<String>` | `src/mcp/tools.rs` |
| `SessionState::current_goal` | `Option<String>` | `src/infra/session.rs` |
| `SessionRegistry::set_current_goal` | `fn(&self, &str, Option<String>)` | `src/infra/session.rs` |
| `Store::insert_cycle_event` (updated) | `+goal: Option<&str>` at param position 8 | `unimatrix-store/src/db.rs` |
| `Store::get_cycle_start_goal` | `async fn(&self, &str) -> Result<Option<String>>` | `unimatrix-store/src/db.rs` |
| `derive_briefing_query` | signature unchanged; step 2 semantics changed | `src/services/index_briefing.rs` |
| SubagentStart fallback branch | `Option<String>` from `session_registry.get_state(sid)?.current_goal` | `src/uds/listener.rs` |
| DB query (resume) | `SELECT goal FROM cycle_events WHERE cycle_id = ?1 AND event_type = 'cycle_start' LIMIT 1` | `unimatrix-store/src/db.rs` |

---

## Migration Test Cascade (SR-01)

The following test files assert `schema_version` and must be updated to expect v16:

| File | Current assertion |
|------|------------------|
| `crates/unimatrix-store/tests/migration_v14_to_v15.rs` | asserts v15 after migration |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | may assert CURRENT_SCHEMA_VERSION |
| `crates/unimatrix-store/tests/sqlite_parity_specialized.rs` | may assert CURRENT_SCHEMA_VERSION |

Delivery must audit these files and add `migration_v15_to_v16.rs` as a new test.

---

## Open Questions

1. **`insert_cycle_event` call sites**: The function is called from `handle_cycle_event` in `listener.rs`. There is one call site. Delivery must verify there are no other callers before changing the signature.

2. **`make_session_state` in tests**: `src/services/index_briefing.rs` has a `make_session_state` test helper that constructs `SessionState` directly. Adding `current_goal` field will require updating this helper. Delivery should check all `SessionState { .. }` struct literals in tests.

3. **SubagentStart session lookup**: The SubagentStart arm in hook.rs (process side) constructs a `HookRequest::ContextSearch` with `session_id` from `hook_input.session_id`. The server-side listener already holds the `SessionRegistry`; the goal lookup at `handle_context_search` time requires passing session state through or resolving it inside the handler. Delivery must confirm the session_id is reliably available in the SubagentStart arm of `dispatch_request`.
