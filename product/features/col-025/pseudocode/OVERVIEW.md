# col-025 Pseudocode Overview — Feature Goal Signal

## Purpose

This document describes the component interaction, data flow, shared types, and
sequencing constraints for the 8 components of col-025. Implementors must read
this before any per-component file.

---

## Components and Files Affected

| Component | File(s) | Crate |
|-----------|---------|-------|
| schema-migration-v16 | `src/migration.rs`, `src/db.rs` | unimatrix-store |
| session-state-extension | `src/infra/session.rs` | unimatrix-server |
| cycle-event-handler | `src/uds/listener.rs` (fn `handle_cycle_event`) | unimatrix-server |
| mcp-cycle-wire-protocol | `src/mcp/tools.rs` (struct `CycleParams`) | unimatrix-server |
| session-resume | `src/uds/listener.rs` (`SessionRegister` arm) | unimatrix-server |
| briefing-query-derivation | `src/services/index_briefing.rs` | unimatrix-server |
| subagent-start-injection | `src/uds/hook.rs` (SubagentStart arm) | unimatrix-server |
| format-index-table-header | `src/mcp/response/briefing.rs` | unimatrix-server |

### Shared-file ownership (wave planning)

Components 3, 5, and 7 all touch `listener.rs` (or `hook.rs`):
- Component 3 (cycle-event-handler) owns `fn handle_cycle_event`.
- Component 5 (session-resume) owns the `HookRequest::SessionRegister` arm in
  `fn dispatch_request`.
- Component 7 (subagent-start-injection) owns the SubagentStart branch in
  `fn run` inside `src/uds/hook.rs` (the hook subcommand runner — NOT
  `dispatch_request` in listener.rs).

Components 6 and 8 both touch `index_briefing.rs` (Component 6) and
`response/briefing.rs` (Component 8):
- Component 6 (briefing-query-derivation) owns `derive_briefing_query` +
  `synthesize_from_session` in `src/services/index_briefing.rs`.
- Component 8 (format-index-table-header) owns `format_index_table` in
  `src/mcp/response/briefing.rs` and `CONTEXT_GET_INSTRUCTION` in
  `src/services/index_briefing.rs` (per ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md,
  and ADR-006; the constant is defined alongside `MAX_GOAL_BYTES` and re-exported
  or imported into `briefing.rs` for use in `format_index_table`).

---

## Data Flow

```
context_cycle(start, goal: "...") [MCP tool call]
         |
         v
CycleParams.goal -- trimmed + empty-normalized to None if blank
                 -- byte check > MAX_GOAL_BYTES -> CallToolResult::error (no DB write)
         |
         v (goal passes validation)
ImplantEvent payload { ..., "goal": "..." }
-- emitted by MCP handler via existing fire-and-forget hook path
         |
         v
handle_cycle_event [CYCLE_START_EVENT, uds/listener.rs]
         |
    [synchronous section]
         |-- extract goal from payload
         |-- UDS byte guard: if goal.len() > MAX_GOAL_BYTES ->
         |     truncate at UTF-8 char boundary + tracing::warn!
         |-- session_registry.set_current_goal(session_id, goal)
         |
    [fire-and-forget spawn]
         |-- store.insert_cycle_event(..., goal: Option<&str>)
         |   -- new 8th parameter, bound at last position
         |
         v
SessionState.current_goal: Option<String>
         |
    +----+------------------------------------+
    |                                         |
    v                                         v
derive_briefing_query                 SubagentStart arm [hook.rs]
(src/services/index_briefing.rs)      -- check current_goal first
step 2: synthesize_from_session       -- goal present, non-empty:
  returns state.current_goal.clone()     IndexBriefingService::index(
         |                                  query: &goal, k=20)
         v                             -- goal absent: existing
IndexBriefingService::index               transcript/RecordEvent path
         |                                 (unchanged)
         v
format_index_table [response/briefing.rs]
-- prepends CONTEXT_GET_INSTRUCTION header once
-- before table header line

Session resume path (server restart):
SessionRegister arm [listener.rs]
  if session has feature_cycle:
    store.get_cycle_start_goal(feature_cycle).await
    .unwrap_or_else(|e| { tracing::warn!(...); None })
    -> session_registry.set_current_goal(session_id, goal)
```

---

## Shared Types Modified or Introduced

### `SessionState` (src/infra/session.rs)

New field added after `current_phase`:
```
pub current_goal: Option<String>
```
Initialized to `None` in `register_session`. All struct literal construction
sites in tests must add `current_goal: None` (or use `..Default::default()`).
See pattern #3180.

### `CycleParams` (src/mcp/tools.rs)

New field added after `format`:
```
pub goal: Option<String>
```
Only processed when `CycleType::Start`. Silently ignored for `PhaseEnd` and
`Stop`. Backward compatible: callers omitting `goal` receive `None`.

### Constants

```
// src/services/index_briefing.rs
pub const MAX_GOAL_BYTES: usize = 1024;

// src/services/index_briefing.rs  (per ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, ADR-006)
pub const CONTEXT_GET_INSTRUCTION: &str =
    "Use context_get with the entry ID for full content when relevant.";
```

`MAX_GOAL_BYTES` placement: defined in `src/services/index_briefing.rs`
alongside `CONTEXT_GET_INSTRUCTION`. Import it into `src/mcp/tools.rs` for
the MCP handler validation and into `src/uds/listener.rs` for the UDS
byte guard.

### `Store` interface additions (src/db.rs)

```
// New function:
pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>

// Modified function (new 8th parameter):
pub async fn insert_cycle_event(
    &self,
    cycle_id: &str,
    seq: i64,
    event_type: &str,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
    goal: Option<&str>,   // NEW — last position
) -> Result<()>
```

### `SessionRegistry` addition (src/infra/session.rs)

```
// New method:
pub fn set_current_goal(&self, session_id: &str, goal: Option<String>)
```

---

## Sequencing Constraints (Build Order)

Wave 1 — foundation (no inter-component dependencies):
1. `schema-migration-v16` — store changes; all server components depend on
   the `insert_cycle_event` signature and `get_cycle_start_goal` API.
2. `session-state-extension` — adds `SessionState.current_goal` and
   `set_current_goal`; all listener components depend on this.
3. `format-index-table-header` — adds `CONTEXT_GET_INSTRUCTION` constant to
   `src/services/index_briefing.rs` and updates `format_index_table` in
   `src/mcp/response/briefing.rs`; independent of session state changes.

Wave 2 — server integration (depends on Wave 1):
4. `mcp-cycle-wire-protocol` — adds `goal` to `CycleParams`; depends on
   `MAX_GOAL_BYTES` constant from schema-migration-v16 or hook.rs constants.
5. `briefing-query-derivation` — replaces `synthesize_from_session`; depends
   on `SessionState.current_goal` from session-state-extension.

Wave 3 — listener integration (depends on Wave 1 + 2):
6. `cycle-event-handler` — modifies `handle_cycle_event`; depends on
   `set_current_goal` (session-state-extension) and updated
   `insert_cycle_event` signature (schema-migration-v16).
7. `session-resume` — modifies `SessionRegister` arm; depends on
   `get_cycle_start_goal` (schema-migration-v16) and `set_current_goal`
   (session-state-extension).
8. `subagent-start-injection` — modifies SubagentStart arm in hook.rs;
   depends on `SessionState.current_goal` (session-state-extension) and
   `IndexBriefingService` (already wired in hook.rs path via dispatch_request).

---

## OQ Resolutions from IMPLEMENTATION-BRIEF.md

### OQ-01: insert_cycle_event call site count
Verified: exactly ONE call site in `listener.rs` at line 2302.
The spawn inside `handle_cycle_event` Step 5. No other callers.

### OQ-02: tracing::warn! severity for non-fatal resume errors
Confirmed: `listener.rs` uses `tracing::warn!` for all non-fatal DB failures
in session paths (e.g., line 2268: "col-022: feature_cycle persist failed").
Using `tracing::warn!` on the resume path is consistent.

### OQ-03: CONTEXT_GET_INSTRUCTION exact wording
Settled per ADR-006 §Decision:
`"Use context_get with the entry ID for full content when relevant."`
This is the authoritative value; do not deviate.

### OQ-04: session_id availability in SubagentStart before CYCLE_START_EVENT
Resolution: The SubagentStart arm in `hook.rs` is the hook-side process
(not dispatch_request). It uses `hook_input.session_id` which is always
present in any hook payload. The session_id does NOT need a prior
CYCLE_START_EVENT — it is available from the moment Claude Code fires any
hook. The goal check reads `session_registry.get_state(session_id)` in
dispatch_request (server side); if the session is not yet registered (no
SessionRegister fired), `get_state` returns `None` and the goal-present
branch never triggers (falls through to transcript path). This is correct
and safe — see subagent-start-injection.md for full flow.

**Critical note**: The SubagentStart injection goal path lives in
`src/uds/hook.rs` (the hook subprocess), NOT in `src/uds/listener.rs`
dispatch_request. The hook.rs SubagentStart arm currently constructs a
`HookRequest::ContextSearch` and sends it to the server. The goal-present
branch must instead construct a `HookRequest::CompactPayload`-equivalent
that the server handles as a briefing. See subagent-start-injection.md for
the exact approach.
