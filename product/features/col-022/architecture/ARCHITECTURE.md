# col-022: Explicit Feature Cycle Lifecycle -- Architecture

## System Overview

col-022 introduces an explicit, authoritative mechanism for SM/coordinator agents to declare which feature cycle a session belongs to. Today, feature attribution relies on heuristic signals (file path patterns, topic extraction, eager voting). This works when signals are strong but fails for worktree-isolated subagents, single-spawn workflows, and mixed-signal sessions.

The feature adds a single MCP tool (`context_cycle`) that serves as the trigger point. The real attribution work happens in the PreToolUse hook handler, which has session identity from Claude Code. The MCP server itself remains session-unaware.

This feature touches four components: the MCP tool layer (validation + acknowledgment), the hook handler (interception + dispatch), the UDS listener (attribution + persistence), and the SQLite schema (keywords column).

## Component Breakdown

### C1: MCP Tool -- `context_cycle`

**Crate:** `unimatrix-server` (`src/mcp/tools.rs`)
**Responsibility:** Parameter validation and agent-facing response. The tool is lightweight -- it validates inputs and returns an acknowledgment. The hook-side path does the actual attribution work.

- Accepts `type` ("start" / "stop"), `topic` (feature cycle ID), `keywords` (optional, up to 5 strings)
- Validates using shared `validate_cycle_params()` function (see C5)
- Returns structured response indicating action taken
- Requires `SessionWrite` capability (same as `context_store`)

### C2: Hook Handler -- PreToolUse Interception

**Crate:** `unimatrix-server` (`src/uds/hook.rs`)
**Responsibility:** Intercept `context_cycle` MCP tool calls via the PreToolUse hook, extract parameters, and dispatch to the UDS listener for attribution.

When Claude Code fires a PreToolUse hook for `context_cycle`:
1. `build_request()` detects `tool_name` containing `context_cycle` in `input.extra`
2. Extracts `type`, `topic`, `keywords` from `input.extra["tool_input"]`
3. Validates using shared `validate_cycle_params()` (see C5)
4. Constructs a `RecordEvent` with `event_type: "cycle_start"` or `"cycle_stop"` and `feature_cycle` + `keywords` in the payload
5. Sets `topic_signal` to the `topic` value (strong signal for attribution)

This follows the existing pattern where `build_request()` maps hook events to `HookRequest` variants, and matches the resolved decision to reuse `RecordEvent` rather than adding new wire protocol variants.

### C3: UDS Listener -- Attribution Handler

**Crate:** `unimatrix-server` (`src/uds/listener.rs`)
**Responsibility:** Process `cycle_start` / `cycle_stop` events received as `RecordEvent`, set session feature_cycle, and persist keywords.

For `event_type == "cycle_start"`:
1. Extract `feature_cycle` from `event.payload` (existing #198 code path already does this)
2. Call `set_feature_if_absent()` on `SessionRegistry` (existing)
3. Persist via `update_session_feature_cycle()` (existing, fire-and-forget)
4. Extract `keywords` from `event.payload` and persist via `update_session_keywords()` (new, fire-and-forget)

For `event_type == "cycle_stop"`:
1. Record the observation event (existing generic path handles this)
2. No session state changes -- the session remains attributed to the original feature

The critical insight: the existing `RecordEvent` handler at listener.rs:598-618 already extracts `feature_cycle` from event payloads and calls `set_feature_if_absent`. The `cycle_start` event flows through this exact code path with zero changes to the attribution logic. The only new listener code is keywords persistence.

### C4: Schema -- Keywords Column

**Crate:** `unimatrix-store` (`src/sessions.rs`, `src/migration.rs`)
**Responsibility:** Add `keywords` TEXT column to the `sessions` table (JSON array, nullable).

- Schema v11 -> v12: `ALTER TABLE sessions ADD COLUMN keywords TEXT`
- `SessionRecord` gains `keywords: Option<String>` field (JSON-serialized `Vec<String>`)
- `SESSION_COLUMNS` updated to include `keywords`
- `session_from_row` updated to read the new column
- `update_session` write path updated to persist keywords

### C5: Shared Validation -- `validate_cycle_params()`

**Crate:** `unimatrix-server` (`src/infra/validation.rs`)
**Responsibility:** Single validation function used by both the MCP tool (C1) and the hook handler (C2) to prevent validation divergence (SR-07).

Validates:
- `type`: must be "start" or "stop"
- `topic`: non-empty, max 128 chars, passes `sanitize_metadata_field()` + `is_valid_feature_id()` structural check
- `keywords`: if present, at most 5 strings, each max 64 chars, truncate excess to 5 silently

Returns a `Result<ValidatedCycleParams, String>` with the cleaned values.

## Component Interactions

```
Agent calls context_cycle(type:"start", topic:"col-022", keywords:[...])
    |
    v
Claude Code fires PreToolUse hook
    |
    v
[C2: Hook Handler]  build_request("PreToolUse", input)
    |  detects tool_name == "context_cycle" in input.extra
    |  extracts type, topic, keywords from input.extra["tool_input"]
    |  calls validate_cycle_params() [C5]
    |  builds RecordEvent { event_type: "cycle_start",
    |                        payload: { feature_cycle: "col-022", keywords: [...] },
    |                        topic_signal: Some("col-022") }
    v
UDS transport (fire-and-forget, 40ms timeout)
    |
    v
[C3: Listener] dispatch_request(RecordEvent { event })
    |  existing #198 path: extracts feature_cycle from payload
    |  calls set_feature_if_absent() -> true (first writer wins)
    |  fire-and-forget: update_session_feature_cycle()
    |  NEW: extracts keywords from payload
    |  fire-and-forget: update_session_keywords()
    |  persists observation row (existing)
    v
HookResponse::Ack (hook discards, fire-and-forget)

    ... meanwhile ...

Claude Code sends MCP tool call to server
    |
    v
[C1: MCP Tool] context_cycle(type:"start", topic:"col-022")
    |  validates params via validate_cycle_params() [C5]
    |  returns acknowledgment: { status: "ok", action: "cycle_started" }
    v
Agent receives confirmation
```

### Data Flow: cycle_stop

```
Agent calls context_cycle(type:"stop", topic:"col-022")
    |
    v
[C2: Hook Handler]  builds RecordEvent { event_type: "cycle_stop", ... }
    |
    v
[C3: Listener]  records observation event (standard path)
    |  NO session state changes
    |  NO feature_cycle clearing
    v
[C1: MCP Tool]  returns { status: "ok", action: "cycle_stopped" }
```

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| Reuse RecordEvent for wire protocol | ADR-001 | Lower churn, established precedent from #198 payload extraction |
| Force-set semantic for explicit cycle_start | ADR-002 | Explicit signal must win over heuristic; resolves SR-01 race condition |
| JSON column for keywords | ADR-003 | Follows ADR-007 (nxs-008) pattern; keywords are stored but not queried by element |
| Shared validation function | ADR-004 | Single source of truth prevents split-brain between MCP tool and hook handler (SR-07) |
| Schema v12 migration | ADR-005 | New column via ALTER TABLE, no data migration needed |

## Integration Points

### Existing Interfaces Used

- `SessionRegistry::set_feature_if_absent(&self, session_id: &str, feature: &str) -> bool` -- existing, needs replacement with force-set variant (ADR-002)
- `update_session_feature_cycle(store, session_id, feature_cycle) -> Result<()>` -- existing, unchanged
- `sanitize_metadata_field(s: &str) -> String` -- existing, reused for topic validation
- `is_valid_feature_id(s: &str) -> bool` -- existing in `unimatrix-observe::attribution`, needs re-export or duplication in validation module
- `extract_event_topic_signal(event, input) -> Option<String>` -- existing, cycle events bypass this (topic_signal set directly)
- `generic_record_event(event, session_id, input) -> HookRequest` -- existing pattern, cycle events use specialized construction

### New Interfaces Introduced

- `validate_cycle_params(type_str, topic, keywords) -> Result<ValidatedCycleParams, String>` -- shared validation
- `SessionRegistry::set_feature_force(&self, session_id: &str, feature: &str) -> SetFeatureResult` -- new force-set method (ADR-002)
- `update_session_keywords(store, session_id, keywords_json) -> Result<()>` -- new persistence helper
- `SessionRecord.keywords: Option<String>` -- new field on session record

### Dependencies

- `unimatrix-observe` crate: `is_valid_feature_id` function (currently `pub(crate)`, needs `pub` export or validation duplicated)
- `unimatrix-store` crate: schema migration v11->v12, SessionRecord field addition
- `unimatrix-server` crate: MCP tool registration, hook handler extension, listener handler extension

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `context_cycle` MCP tool | `fn context_cycle(&self, CycleParams) -> Result<CallToolResult>` | `unimatrix-server/src/mcp/tools.rs` (new) |
| `CycleParams` struct | `{ r#type: String, topic: String, keywords: Option<Vec<String>> }` with `JsonSchema` derive | `unimatrix-server/src/mcp/tools.rs` (new) |
| `validate_cycle_params` | `fn validate_cycle_params(type_str: &str, topic: &str, keywords: Option<&[String]>) -> Result<ValidatedCycleParams, String>` | `unimatrix-server/src/infra/validation.rs` (new) |
| `ValidatedCycleParams` | `struct { cycle_type: CycleType, topic: String, keywords: Vec<String> }` | `unimatrix-server/src/infra/validation.rs` (new) |
| `CycleType` enum | `enum CycleType { Start, Stop }` | `unimatrix-server/src/infra/validation.rs` (new) |
| `SessionRegistry::set_feature_force` | `pub fn set_feature_force(&self, session_id: &str, feature: &str) -> SetFeatureResult` | `unimatrix-server/src/infra/session.rs` (new) |
| `SetFeatureResult` enum | `enum SetFeatureResult { Set, AlreadyMatches, Overridden { previous: String } }` | `unimatrix-server/src/infra/session.rs` (new) |
| `update_session_keywords` | `fn update_session_keywords(store: &Store, session_id: &str, keywords_json: &str) -> Result<()>` | `unimatrix-server/src/uds/listener.rs` (new) |
| `SessionRecord.keywords` | `pub keywords: Option<String>` (JSON array string) | `unimatrix-store/src/sessions.rs` (modified) |
| `sessions` table | `keywords TEXT` column added | `unimatrix-store/src/migration.rs` (v11->v12) |
| `CURRENT_SCHEMA_VERSION` | `11 -> 12` | `unimatrix-store/src/migration.rs` (modified) |
| `SESSION_COLUMNS` | Updated to include `keywords` | `unimatrix-store/src/sessions.rs` (modified) |
| Hook `build_request` | New match arm for PreToolUse with `tool_name` containing `context_cycle` | `unimatrix-server/src/uds/hook.rs` (modified) |
| Listener `dispatch_request` | New match arm for `event_type == "cycle_start"` before generic RecordEvent handler | `unimatrix-server/src/uds/listener.rs` (modified) |

## Error Boundaries

| Error Origin | Propagation | Handling |
|-------------|-------------|----------|
| MCP tool validation failure | Returned to agent as tool error | `validate_cycle_params` returns descriptive error string |
| Hook validation failure | Silently drops event (hook must never fail) | Log warning, return empty stdout (no injection) |
| UDS transport timeout | Hook process exits with code 0 | Fire-and-forget: attribution lost, eager attribution takes over as fallback |
| `set_feature_force` on unregistered session | Returns `false` / no-op | Log warning, event still persisted as observation |
| Keywords persistence failure | Fire-and-forget spawn_blocking | Log warning, feature_cycle attribution still succeeds independently |
| Schema migration failure | Server startup failure | Standard migration error path (store returns error) |

## Open Questions

1. **`is_valid_feature_id` visibility**: Currently `pub(crate)` in `unimatrix-observe`. The validation module in `unimatrix-server` needs it. Options: (a) re-export as `pub`, (b) duplicate the check in validation.rs. Re-export is cleaner but adds a cross-crate dependency path for validation. Implementer should choose based on whether `unimatrix-server` already depends on `unimatrix-observe`.

2. **MCP tool response content**: The MCP server does not have session_id in the tool call context, so the response cannot indicate whether attribution actually succeeded (that happens on the hook side). The tool response should acknowledge parameter acceptance only. Whether the hook-side attribution succeeded is observable only through `context_retrospective` or session queries.
