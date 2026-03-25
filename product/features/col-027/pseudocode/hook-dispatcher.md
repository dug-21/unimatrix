# Component: hook-dispatcher

**File:** `crates/unimatrix-server/src/uds/hook.rs`
**Wave:** 2 (depends on core-constants for `hook_type::POSTTOOLUSEFAILURE` usage in tests)
**Action:** Modify two functions: `build_request()` and `extract_event_topic_signal()`

---

## Purpose

Convert a raw `PostToolUseFailure` stdin payload into a typed `HookRequest::RecordEvent` for
fire-and-forget transport. Extract `tool_name` and `topic_signal` from the failure payload fields.
Ensure the hook binary always exits 0 regardless of payload shape.

The existing wildcard arm in `build_request()` (`_ => generic_record_event(...)`) would handle
an unregistered `PostToolUseFailure` but stores records with `tool_name = None` and the wrong
`event_type`. The explicit arm is required (ADR-001, FR-03.1).

---

## New/Modified Functions

### Function: extract_event_topic_signal() — add "PostToolUseFailure" arm

**Existing function signature (unchanged):**
```
fn extract_event_topic_signal(event: &str, input: &HookInput) -> Option<String>
```

**Current match arms:** "PreToolUse", "PostToolUse", "SubagentStart", "UserPromptSubmit", wildcard.

**Change:** Add an explicit `"PostToolUseFailure"` arm before the `_` wildcard. The arm is
identical to the `"PostToolUse"` arm: reads `input.extra["tool_input"]` to extract the topic
signal. The two events share the same `tool_input` field structure (SPEC §Open Questions, Q4).
This is acceptable duplication — separate arms allow future divergence (ADR-001 consequences).

```
fn extract_event_topic_signal(event: &str, input: &HookInput) -> Option<String> {
    match event {
        "PreToolUse" => {
            // ... unchanged ...
        }
        "PostToolUse" => {
            // ... unchanged ...
        }
        "PostToolUseFailure" => {         // NEW ARM — col-027
            // Same source field as PostToolUse: tool_input contains the invocation parameters.
            // PostToolUseFailure does NOT have a tool_response, but tool_input is present.
            let text = input
                .extra
                .get("tool_input")
                .map(|v| match v {
                    // If tool_input is already a string (unusual), use it directly
                    serde_json::Value::String(s) => s.clone(),
                    // Otherwise serialize to JSON string
                    _ => serde_json::to_string(v).unwrap_or_default(),
                })
                .unwrap_or_default();
            // extract_topic_signal returns None for empty/whitespace input
            extract_topic_signal(&text)
        }
        "SubagentStart" => {
            // ... unchanged ...
        }
        "UserPromptSubmit" => {
            // ... unchanged ...
        }
        _ => {
            // ... unchanged wildcard ...
        }
    }
}
```

**Key constraint:** If `tool_input` is absent or null, `get().map(...).unwrap_or_default()` returns
an empty string, and `extract_topic_signal("")` returns `None`. No panic.

---

### Function: build_request() — add "PostToolUseFailure" arm

**Existing function signature (unchanged):**
```
fn build_request(event: &str, input: &HookInput) -> HookRequest
```

**Change:** Add an explicit `"PostToolUseFailure"` match arm. Position it immediately after the
`"PostToolUse"` arm (line ~506 in current file, before `"PreToolUse"` arm and `_ => ...` wildcard).

```
fn build_request(event: &str, input: &HookInput) -> HookRequest {
    // ... session_id and cwd resolution unchanged ...

    match event {
        "SessionStart" => { /* ... unchanged ... */ }
        "Stop" | "TaskCompleted" => { /* ... unchanged ... */ }
        "Ping" => { /* ... unchanged ... */ }
        "UserPromptSubmit" => { /* ... unchanged ... */ }
        "PreCompact" => { /* ... unchanged ... */ }
        "PostToolUse" => { /* ... unchanged, rework logic etc ... */ }

        // col-027: Explicit arm for PostToolUseFailure -- must NOT fall to wildcard (ADR-001)
        "PostToolUseFailure" => {
            // Step 1: Extract tool_name from extra["tool_name"]
            // Defensive: absent or non-string tool_name produces ""
            // (not None -- RecordEvent carries tool_name as part of payload,
            //  extraction into ObservationRow happens in listener.rs)
            let tool_name = input
                .extra
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Step 2: Compute topic_signal from tool_input (same source as PostToolUse)
            // Returns None if tool_input absent or produces no signal
            let topic_signal = extract_event_topic_signal(event, input);

            // Step 3: Return fire-and-forget RecordEvent
            // payload = input.extra.clone() carries tool_name, error, tool_input, is_interrupt
            // listener.rs extract_observation_fields() reads from this payload
            //
            // MUST NOT enter rework logic: failure events are not rework candidates
            // MUST NOT call extract_response_fields(): error field is handled in listener.rs
            // MUST route to HookRequest::RecordEvent (not ContextSearch, not SessionRegister)
            HookRequest::RecordEvent {
                event: ImplantEvent {
                    event_type: event.to_string(),   // "PostToolUseFailure" verbatim
                    session_id,
                    timestamp: now_secs(),
                    payload: input.extra.clone(),    // carries all payload fields
                    topic_signal,
                },
            }
        }

        "PreToolUse" => build_cycle_event_or_fallthrough(event, session_id, input),
        "SubagentStart" => { /* ... unchanged ... */ }
        _ => generic_record_event(event, session_id, input),
    }
}
```

**Why NOT call extract_response_fields() here:** That function reads `payload["tool_response"]`
(a JSON object). `PostToolUseFailure` payloads have no `tool_response`. Calling it returns
`(None, None)` silently. The error field is extracted in `listener.rs` via the new
`extract_error_field()` function, which is the correct place for storage-layer extraction.

**Why pass `input.extra.clone()` as payload:** The listener reads `payload["tool_name"]`,
`payload["tool_input"]`, and `payload["error"]` directly from the stored payload. Passing
`input.extra` unchanged (as done for all other RecordEvent paths) preserves these fields.

**Defensive parsing guarantee (NFR-02):**
- `input.extra.get("tool_name").and_then(|v| v.as_str()).unwrap_or("")` — if absent, `""`
- `extract_event_topic_signal(event, input)` — returns `None` for absent tool_input
- `input.extra.clone()` — clones the entire extra value; if extra is `Value::Null` (malformed
  input), the clone is `Value::Null` and downstream get() calls return None safely
- `now_secs()` — pure time function, no I/O

---

## State Machine

No state. `build_request()` and `extract_event_topic_signal()` are pure functions with no
side effects and no shared state. The hook binary is stateless per-invocation.

---

## Initialization Sequence

No initialization. `build_request()` is called once per hook invocation from `run()` (Step 5).
The `run()` function flow is unchanged:

```
run("PostToolUseFailure", project_dir):
  1. read_stdin()
  2. parse_hook_input(&stdin_content)         -- defensive JSON parse
  3. resolve_cwd()                            -- unchanged
  4. detect_project_root()                   -- unchanged
  5. build_request("PostToolUseFailure", &hook_input)  -- NEW explicit arm fires here
  5b. SubagentStart fallback -- skipped (event != "SubagentStart")
  5c. req_source extraction -- None (RecordEvent carries no source)
  5d. CompactPayload transcript extraction -- skipped (not CompactPayload)
  6. is_fire_and_forget = true (RecordEvent matches the fire-and-forget check)
  7. transport.connect() -- unchanged, 40ms timeout
  8. send RecordEvent -- fire-and-forget, no response processing
  9. exit 0
```

---

## Data Flow

**Inputs:**
- `event: &str` = `"PostToolUseFailure"` (from argv)
- `input: &HookInput` with:
  - `extra["tool_name"]: String` — the tool that failed
  - `extra["tool_input"]: Object` — the invocation parameters
  - `extra["error"]: String` — the error message (plain string, not object)
  - `extra["is_interrupt"]: bool?` — optional, absent if not user-interrupted

**Output:** `HookRequest::RecordEvent { event: ImplantEvent { ... } }`

**Type transformations:**
```
input.extra["tool_name"].as_str()  ->  String  (via .unwrap_or("").to_string())
extract_event_topic_signal()       ->  Option<String>
event.to_string()                  ->  "PostToolUseFailure" (verbatim)
input.extra.clone()                ->  serde_json::Value (passed as-is)
now_secs()                         ->  u64 (Unix timestamp in seconds)
```

**Payload contract (listener.rs reads):**
```
payload["tool_name"] -- String: used by listener to populate obs.tool
payload["tool_input"] -- Object: used by listener to populate obs.input
payload["error"]     -- String: used by extract_error_field() for obs.response_snippet
payload["is_interrupt"] -- bool?: ignored by listener in col-027, captured for future use
```

---

## Error Handling

| Condition | Handling |
|-----------|----------|
| `input.extra` is `Value::Null` or missing fields | `.get().and_then().unwrap_or("")` returns `""` or `None`; no panic |
| `tool_name` absent from payload | `""` stored; listener extracts `None` from `payload["tool_name"].as_str()` returning None |
| `tool_input` absent from payload | `extract_event_topic_signal()` returns `None`; `topic_signal: None` stored |
| `error` field not accessed here | Error field is left in `input.extra` and extracted by listener |
| `is_interrupt` absent | Not accessed in dispatcher; passes through in `payload` unchanged |
| Transport unavailable | Existing enqueue path fires; hook exits 0 (unchanged behavior) |
| Malformed JSON on stdin | `parse_hook_input()` returns a default `HookInput` with null extra; arm runs safely |

The hook binary must exit 0 in all cases (FR-03.7). The `run()` function signature returns
`Result<(), Box<dyn Error>>` and the main function maps any Err to exit 0.

---

## Key Test Scenarios

### T-HD-01: build_request() PostToolUseFailure returns RecordEvent with correct event_type (AC-11, R-05)

```
test build_request_posttoolusefailure_returns_record_event:
  input = HookInput with:
    session_id = Some("sess-1")
    extra = json!({ "tool_name": "Bash", "error": "permission denied", "tool_input": {} })
  req = build_request("PostToolUseFailure", &input)
  assert matches!(req, HookRequest::RecordEvent { .. })
  if let HookRequest::RecordEvent { event } = req:
    assert_eq!(event.event_type, "PostToolUseFailure")
    // tool_name is in the payload, not a top-level field on ImplantEvent
    assert_eq!(event.payload["tool_name"], "Bash")
```

### T-HD-02: PostToolUseFailure arm does NOT enter rework logic (structural/code inspection)

Verified by: the arm returns `HookRequest::RecordEvent` directly — no call to
`is_rework_eligible_tool()`, no `post_tool_use_rework_candidate` event_type used.

### T-HD-03: Missing tool_name does not panic (AC-12, R-08)

```
test build_request_posttoolusefailure_missing_tool_name:
  input = HookInput with:
    session_id = Some("sess-1")
    extra = json!({ "error": "some error" })  // tool_name absent
  // Must not panic
  req = build_request("PostToolUseFailure", &input)
  assert matches!(req, HookRequest::RecordEvent { .. })
```

### T-HD-04: Empty extra (null payload) does not panic (AC-12, R-08)

```
test build_request_posttoolusefailure_null_extra:
  input = HookInput with:
    session_id = Some("sess-1")
    extra = Value::Null
  req = build_request("PostToolUseFailure", &input)
  assert matches!(req, HookRequest::RecordEvent { .. })
```

### T-HD-05: extract_event_topic_signal returns from tool_input, not error blob (R-09)

```
test extract_event_topic_signal_posttoolusefailure:
  input = HookInput with:
    extra = json!({ "tool_input": {"path": "/foo/bar"}, "error": "file not found" })
  result = extract_event_topic_signal("PostToolUseFailure", &input)
  // result is derived from tool_input, not from raw extra blob
  // (exact content depends on extract_topic_signal internals, but it should not be
  //  the full serialized extra JSON)
  // Verify it's NOT the full extra blob:
  if let Some(signal) = result:
    assert !signal.contains("\"error\":")  // error key not in topic signal
```

### T-HD-06: Integration — build_request event_type matches settings.json key (R-14 cross-check)

The event name passed to `build_request()` is the same string as the hook key in settings.json.
Both are `"PostToolUseFailure"`. This is verified by T-HD-01 asserting `event.event_type == "PostToolUseFailure"`.

---

## Anti-Patterns to Avoid

- Do NOT call `extract_response_fields()` anywhere in the `"PostToolUseFailure"` arm
- Do NOT route `"PostToolUseFailure"` through `is_rework_eligible_tool()` or rework logic
- Do NOT use the wildcard arm for `"PostToolUseFailure"` — explicit arm is mandatory (FR-03.1)
- Do NOT produce stdout output for this event type (FR-03.7, C-05)
- Do NOT add synchronous DB writes to this path (NFR-01, C-03)
