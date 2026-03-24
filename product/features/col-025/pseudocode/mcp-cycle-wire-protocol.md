# Component: mcp-cycle-wire-protocol

**Crate**: `unimatrix-server`
**File**: `src/mcp/tools.rs` (struct `CycleParams` + `context_cycle` handler)

---

## Purpose

Add `goal: Option<String>` to the `CycleParams` wire struct so that
`context_cycle(start, goal: "...")` carries the goal through the MCP path.
Apply validation (trim, empty normalization, byte check) in the handler.
Emit the validated goal in the `ImplantEvent` payload for the UDS listener.

---

## Modified Struct: `CycleParams`

Current struct (no `goal` field):
```
pub struct CycleParams {
    pub r#type: String,
    pub topic: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}
```

Add `goal` field after `next_phase` (before `agent_id`):
```
pub struct CycleParams {
    pub r#type: String,
    pub topic: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    /// Optional goal statement for the feature cycle (col-025).
    ///
    /// Only meaningful for type="start". Ignored for "phase-end" and "stop".
    /// Max 1024 bytes (MAX_GOAL_BYTES). Empty/whitespace normalized to None
    /// at the handler layer (FR-11). Old callers omitting this field receive None.
    pub goal: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}
```

`#[serde(default)]` is not needed — `Option<String>` deserializes as `None`
when the key is absent from JSON (serde default behavior for `Option`).

---

## Modified Handler: `context_cycle`

The current handler validates params, builds a response text, and returns.
The MCP server is described as "session-unaware" (comment on line 1800) but
does emit events to the UDS path (fire-and-forget hook path for attribution).

Looking at the existing code: the `context_cycle` tool acknowledges the
action and relies on the hook path for attribution. The `ImplantEvent` for
`CYCLE_START_EVENT` is emitted by the hook process (not the MCP handler
directly). The MCP handler accepts `CycleParams`, validates, and returns
an acknowledgment text. The hook process (`hook.rs`) is what actually sends
`ImplantEvent` with `payload` containing `feature_cycle`, `phase`, etc.

**Clarification**: After careful reading of the code flow, `context_cycle` in
`tools.rs` does NOT directly emit an `ImplantEvent`. The hook path (hook.rs)
is what creates and sends `CYCLE_START_EVENT` payloads. The MCP tool's job is
to validate and return an acknowledgment. The goal field needs to be:
1. Validated at the MCP handler (trim, normalize, byte check).
2. Included in the acknowledgment response so the agent knows what was accepted.
3. Accessible to the hook path via the wire protocol.

However, reviewing the architecture more carefully: the MCP tool `context_cycle`
runs synchronously and the hook process emits events separately. The way goal
reaches `handle_cycle_event` is through the `ImplantEvent` payload constructed
by the hook process. The MCP handler's `goal` validation is a guard only —
it does NOT create the ImplantEvent.

The actual flow for goal propagation:
- Agent calls `context_cycle(start, goal: "...")` via MCP.
- MCP handler validates the goal and returns acknowledgment.
- Claude Code (separately) fires the `CYCLE_START_EVENT` hook, which
  `hook.rs` processes, constructing an `ImplantEvent` with payload.

**Gap identified**: The MCP tool handler validates the goal but the goal needs
to reach the `ImplantEvent` payload. Currently the hook.rs `build_request`
function constructs `HookRequest` from `HookInput` (stdin). The `goal` from
the MCP params is not automatically available to the hook process.

**Resolution per architecture**: The `context_cycle` MCP handler validates
the goal for MCP callers. The goal that reaches `handle_cycle_event` comes
from the hook process's `ImplantEvent.payload["goal"]`. For the UDS path,
the goal arrives in the hook payload when the agent calls
`context_cycle(start, goal: ...)` — Claude Code captures all tool parameters
and includes them in hook event payloads. The `ImplantEvent.payload` is
`serde_json::Value` populated from the tool call parameters by Claude Code's
hook machinery.

The MCP handler's responsibility is validation and rejection only. No direct
ImplantEvent emission needed from tools.rs.

### Validation block inside `context_cycle` handler

Insert after step 3 (existing validation via `validate_cycle_params`), before
step 4 (build response). Only executes when `validated.cycle_type == CycleType::Start`:

```
// 3b. Goal validation (col-025, ADR-005): Start events only.
// For PhaseEnd and Stop, goal is silently ignored.
let validated_goal: Option<String> = if validated.cycle_type == CycleType::Start {
    match params.goal {
        None => None,
        Some(g) => {
            // Step 1: Trim whitespace
            let trimmed = g.trim().to_owned();

            // Step 2: Normalize empty / whitespace-only to None (FR-11, ADR-005)
            if trimmed.is_empty() {
                None
            } else {
                // Step 3: Byte length check (ADR-005, MAX_GOAL_BYTES = 1024)
                if trimmed.len() > MAX_GOAL_BYTES {
                    return Ok(CallToolResult::error(vec![
                        rmcp::model::Content::text(format!(
                            "goal exceeds {} bytes ({} bytes provided); \
                             shorten the goal and retry",
                            MAX_GOAL_BYTES,
                            trimmed.len()
                        ))
                    ]));
                }
                Some(trimmed)
            }
        }
    }
} else {
    None  // PhaseEnd and Stop: goal silently ignored
};
```

### Response text update

Update the response text to acknowledge the goal when provided:

```
let response_text = if let Some(ref g) = validated_goal {
    format!(
        "Acknowledged: {} for topic '{}' with goal: '{}'. \
         Attribution is applied via the hook path (fire-and-forget). \
         Use context_cycle_review to confirm session attribution.",
        action, validated.topic, g
    )
} else {
    format!(
        "Acknowledged: {} for topic '{}'. \
         Attribution is applied via the hook path (fire-and-forget). \
         Use context_cycle_review to confirm session attribution.",
        action, validated.topic
    )
};
```

### Audit log update

Update the audit detail to include the goal presence:
```
detail: format!("{} topic={}{}", action, validated.topic,
    if validated_goal.is_some() { " goal=present" } else { "" }),
```

---

## Constants

`MAX_GOAL_BYTES` must be imported into `tools.rs`. It is defined adjacent
to `MAX_INJECTION_BYTES` in `hook.rs`. Either:
- Import from `crate::uds::hook::MAX_GOAL_BYTES` (requires `pub(crate)`)
- Or re-declare in a shared constants module

The same constant value (1024) MUST be used by both `tools.rs` (MCP reject)
and `hook.rs` (UDS truncate). Do not duplicate the value inline.

---

## Data Flow

Input: `CycleParams.goal: Option<String>` from MCP wire (serde JSON)
Validation:
1. Only processed when `CycleType::Start`
2. Trimmed: `g.trim().to_owned()`
3. Empty/whitespace normalized to `None`
4. If `len() > MAX_GOAL_BYTES`: return `CallToolResult::error(...)` immediately
5. Otherwise: `Some(trimmed)` or `None`

Output: `CallToolResult` (success or error). The validated goal is returned
as part of the acknowledgment text. The goal reaches the UDS listener via
`ImplantEvent.payload["goal"]` — populated by Claude Code's hook machinery
from the original tool call parameters.

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `goal.len() > MAX_GOAL_BYTES` | `CallToolResult::error(...)` with byte count; no DB write |
| `goal` empty/whitespace | Normalized to `None`; treated as no goal |
| `goal` absent from JSON | Serde default `None`; treated as no goal |
| `goal` present on PhaseEnd/Stop | Silently ignored (`None` used) |

---

## Backward Compatibility

Callers that do not include `goal` in `context_cycle(start)` JSON receive
`goal = None` via serde default. Their behavior is identical to pre-col-025.
The `goal` field is optional; no change to existing callers (NFR-02, NFR-03).

---

## Key Test Scenarios

### T-MCP-01: Goal field on CycleParams deserializes correctly
```
act:   deserialize `{"type": "start", "topic": "col-025", "goal": "test goal"}`
assert: params.goal == Some("test goal")
```

### T-MCP-02: Goal absent → None (backward compat, AC-02)
```
act:   deserialize `{"type": "start", "topic": "col-025"}`
assert: params.goal == None
```

### T-MCP-03: Goal exceeds MAX_GOAL_BYTES → error, no write (AC-13a)
```
setup: goal = "a".repeat(MAX_GOAL_BYTES + 1)  // 1025 bytes
act:   call context_cycle handler
assert: returns CallToolResult with error content
assert: error text mentions MAX_GOAL_BYTES bytes and actual byte count
assert: no cycle_start event written (no DB row)
```

### T-MCP-04: Goal exactly MAX_GOAL_BYTES → accepted (AC-13a boundary)
```
setup: goal = "a".repeat(MAX_GOAL_BYTES)  // exactly 1024 bytes
act:   call context_cycle handler
assert: returns success acknowledgment
assert: validated_goal == Some(goal_text)
```

### T-MCP-05: Whitespace-only goal normalized to None (AC-17)
```
act:   call context_cycle handler with goal = "   "
assert: returns success acknowledgment (not error)
assert: validated_goal == None
```

### T-MCP-06: Empty string goal normalized to None (AC-17)
```
act:   call context_cycle handler with goal = ""
assert: returns success acknowledgment
assert: validated_goal == None
```

### T-MCP-07: Goal on PhaseEnd is silently ignored (FR-01)
```
act:   call context_cycle handler with type="phase-end", goal="something"
assert: returns success acknowledgment
assert: validated_goal == None (goal ignored on non-Start events)
```

### T-MCP-08: Existing CycleParams tests still pass (AC-10, NFR-02)
```
// All existing test_cycle_params_* tests in tools.rs must pass without
// modification after adding the goal field.
```
