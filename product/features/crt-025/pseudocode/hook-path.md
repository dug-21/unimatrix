# Component 3: Hook Path
## File: `crates/unimatrix-server/src/uds/hook.rs`

---

## Purpose

The hook intercepts `context_cycle` PreToolUse events before the tool executes, builds a specialized `HookRequest::RecordEvent` with the correct `event_type`, and emits it for the UDS listener to handle. This component updates `build_cycle_event_or_fallthrough` to extract `phase`, `outcome`, `next_phase` from `tool_input`, pass them to the updated `validate_cycle_params`, map `"phase-end"` to `CYCLE_PHASE_END_EVENT`, and include the new fields in the event payload. Keywords extraction and serialization are removed.

---

## Import Changes

```
// BEFORE:
use crate::infra::validation::{CYCLE_START_EVENT, CYCLE_STOP_EVENT, CycleType, validate_cycle_params};

// AFTER:
use crate::infra::validation::{
    CYCLE_PHASE_END_EVENT,   // NEW
    CYCLE_START_EVENT,
    CYCLE_STOP_EVENT,
    CycleType,
    validate_cycle_params,
};
```

---

## Modified Function: `build_cycle_event_or_fallthrough`

### Current flow (what exists)

1. Check if tool_name contains "context_cycle"
2. Extract `type_str` and `topic_str` from `tool_input`
3. Extract `keywords_opt` from `tool_input`
4. Call `validate_cycle_params(type_str, topic_str, keywords_opt)`
5. On validation failure: log warning, fall through to `generic_record_event`
6. Map `cycle_type` → `event_type` constant
7. Build payload with `feature_cycle` and optional `keywords` JSON
8. Return `HookRequest::RecordEvent { event }`

### Updated flow

```
FUNCTION build_cycle_event_or_fallthrough(event, session_id, input) -> HookRequest:

    // Step 1: Check if context_cycle tool (UNCHANGED)
    tool_name = extract tool_name from input
    IF tool_name does not contain "context_cycle":
        return generic_record_event(event, session_id, input)
    IF tool_name != "context_cycle" AND NOT contains "unimatrix":
        return generic_record_event(event, session_id, input)

    // Step 2: Extract tool_input from event (UNCHANGED)
    tool_input = event.input fields (tool_input JSON object)
    IF tool_input missing: log warning, return generic_record_event(...)

    // Step 3: Extract type and topic (UNCHANGED)
    type_str  = tool_input.get("type").as_str() or ""
    topic_str = tool_input.get("topic").as_str() or ""

    // Step 4: Extract new optional fields (NEW)
    phase_opt     = tool_input.get("phase").and_then(as_str)        // Option<&str>
    outcome_opt   = tool_input.get("outcome").and_then(as_str)      // Option<&str>
    next_phase_opt = tool_input.get("next_phase").and_then(as_str)  // Option<&str>

    // Step 5: Remove keywords extraction (REMOVED)
    // BEFORE: keywords_opt = tool_input.get("keywords") ...
    // AFTER:  nothing — keywords are no longer extracted

    // Step 6: Validate (CHANGED: new signature)
    validated = match validate_cycle_params(
        type_str,
        topic_str,
        phase_opt,
        outcome_opt,
        next_phase_opt,
    ):
        Err(msg) →
            // FR-03.7: Log warning, fall through to generic path — do NOT hard-fail
            eprintln!("unimatrix: context_cycle validation failed in hook: {msg} (tool_name={tool_name})")
            return generic_record_event(event, session_id, input)
        Ok(v) → v

    // Step 7: Map cycle_type to event_type constant (CHANGED: add PhaseEnd)
    event_type = match validated.cycle_type:
        CycleType::Start    → CYCLE_START_EVENT.to_string()
        CycleType::PhaseEnd → CYCLE_PHASE_END_EVENT.to_string()   // NEW
        CycleType::Stop     → CYCLE_STOP_EVENT.to_string()

    // Step 8: Build payload (CHANGED: add phase/outcome/next_phase, remove keywords)
    payload = {
        "feature_cycle": validated.topic,
    }

    // Conditionally include optional fields when present (avoid null noise in payload)
    IF validated.phase is Some(p):
        payload["phase"] = p
    IF validated.outcome is Some(o):
        payload["outcome"] = o
    IF validated.next_phase is Some(np):
        payload["next_phase"] = np

    // REMOVED: keywords serialization block

    // Step 9: Build and return RecordEvent (UNCHANGED structure)
    return HookRequest::RecordEvent {
        event: ImplantEvent {
            session_id: session_id,
            event_type: event_type,
            payload: payload,
            // ... other ImplantEvent fields from original hook event ...
        }
    }
```

---

## Fallthrough Behavior on Validation Failure

This is a load-bearing safety property (FR-03.7, R-09). The hook MUST NOT hard-fail on validation errors — it falls through to `generic_record_event`, which produces a normal observation record. The tool call is not blocked.

```
// Fallthrough contract:
ON validate_cycle_params returns Err(msg):
    log warning message (eprintln! matches existing pattern in hook.rs)
    return generic_record_event(event, session_id, input)
    // No panic, no error returned to transport
```

---

## Removed Logic

```
// REMOVE: keywords_opt extraction:
//   let keywords_opt: Option<Vec<String>> = tool_input.get("keywords")...

// REMOVE: keywords payload insertion:
//   if !validated.keywords.is_empty() {
//       let keywords_json = serde_json::to_string(&validated.keywords)...
//       payload["keywords"] = serde_json::Value::String(keywords_json);
//   }
```

---

## Data Flow

```
PreToolUse event (context_cycle) arrives at hook
    tool_input = { "type": "phase-end", "topic": "crt-025", "phase": "design",
                   "outcome": "no variances", "next_phase": "implementation" }
    ↓
    phase_opt = Some("design")
    next_phase_opt = Some("implementation")
    outcome_opt = Some("no variances")
    ↓
    validate_cycle_params(...) → Ok(ValidatedCycleParams {
        cycle_type: PhaseEnd,
        topic: "crt-025",
        phase: Some("design"),
        outcome: Some("no variances"),
        next_phase: Some("implementation"),
    })
    ↓
    event_type = "cycle_phase_end"
    payload = {
        "feature_cycle": "crt-025",
        "phase": "design",
        "outcome": "no variances",
        "next_phase": "implementation",
    }
    ↓
    HookRequest::RecordEvent { event }
```

---

## Error Handling

| Situation | Behavior |
|-----------|----------|
| Validation failure (`phase` has space, etc.) | Warn + fallthrough to generic path |
| Missing `type` or `topic` in `tool_input` | Existing behavior: fallthrough with empty string passed to validate → validation fails → fallthrough |
| `tool_input` missing entirely | Existing behavior: eprintln + fallthrough |

---

## Key Test Scenarios

1. `tool_input = { "type": "phase-end", "phase": "scope", "next_phase": "design", "topic": "crt-025" }` → `RecordEvent { event_type: "cycle_phase_end", payload.phase: "scope" }`
2. `tool_input = { "type": "phase-end", "phase": "scope review", "topic": "crt-025" }` → warning logged, generic_record_event returned (no crash)
3. `tool_input = { "type": "start", "next_phase": "scope", "topic": "crt-025" }` → `RecordEvent { event_type: "cycle_start", payload.next_phase: "scope" }`
4. `tool_input = { "type": "stop", "topic": "crt-025" }` → `RecordEvent { event_type: "cycle_stop" }`, no phase fields in payload
5. `tool_input = { "type": "phase-end", "topic": "crt-025" }` (no phase/outcome/next_phase) → `RecordEvent { event_type: "cycle_phase_end" }`, payload has only feature_cycle
6. `tool_input = { "type": "phase-end", "phase": "Design", "topic": "crt-025" }` → normalized `"design"` in payload
7. Old caller passing `keywords` → keywords absent from payload (no field extracted)
8. Validation failure → fallthrough → result has `event_type != "cycle_phase_end"` (generic observation)
