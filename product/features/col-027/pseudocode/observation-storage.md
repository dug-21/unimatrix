# Component: observation-storage

**File:** `crates/unimatrix-server/src/uds/listener.rs`
**Wave:** 2 (depends on core-constants for `hook_type::POSTTOOLUSEFAILURE`)
**Action:** Modify `extract_observation_fields()` + add new `extract_error_field()`

---

## Purpose

Convert an `ImplantEvent` with `event_type = "PostToolUseFailure"` into an `ObservationRow` for
SQLite insertion. Extract `tool_name`, `tool_input`, and error string from the payload. Store
`hook = "PostToolUseFailure"` verbatim (no normalization).

The critical risk (R-01): the `"PostToolUseFailure"` arm must call `extract_error_field()`, NOT
`extract_response_fields()`. The existing `extract_response_fields()` reads `payload["tool_response"]`
(a JSON object) which does not exist on failure payloads — it would silently return `(None, None)`.

---

## New Functions

### Function: extract_error_field() — NEW sibling to extract_response_fields()

**Signature (from ARCHITECTURE.md Integration Surface):**
```
fn extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)
```

**Location:** Define immediately after `extract_response_fields()` in listener.rs (maintaining
the sibling relationship for code readability and to make the separation explicit to future readers).

**Purpose:** Read `payload["error"]` as a plain string and return it as a truncated snippet.
`response_size` is always `None` for failure events (ADR-002, FR-04.4) — error strings are small
and measuring byte length provides no analytical value.

**Algorithm:**

```
fn extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>) {
    // Step 1: Access payload["error"] as a string.
    //   - payload.get("error") returns None if key absent
    //   - .as_str() returns None if value is not a JSON string (e.g., null, object, array)
    //   - This guard rejects any non-string "error" field without panic (security: R-01)
    let error_str = match payload.get("error").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        // absent, null, non-string, or empty string -> (None, None)
        _ => return (None, None),
    };

    // Step 2: Truncate to 500 chars at valid UTF-8 char boundary.
    //   Uses existing truncate_at_utf8_boundary(s, max_bytes) helper.
    //   500-char limit is consistent with extract_response_fields() snippet budget.
    let snippet = truncate_at_utf8_boundary(error_str, 500);

    // Step 3: response_size is always None for failure events.
    //   Error strings are small; no analytical value in measuring them.
    (None, Some(snippet))
}
```

**Notes on truncate_at_utf8_boundary:**
- Existing function in listener.rs at line ~91 (read from file)
- Signature: `fn truncate_at_utf8_boundary(s: &str, max_bytes: usize) -> String`
- Takes `&str` and returns `String`
- `error_str` is `&str` from `.as_str()` — passes directly

**Edge cases:**
- `payload["error"]` absent → `get()` returns `None` → returns `(None, None)`
- `payload["error"]` is JSON `null` → `as_str()` returns `None` → returns `(None, None)`
- `payload["error"]` is a non-string (e.g., array or object) → `as_str()` returns `None` → `(None, None)`
- `payload["error"]` is `""` (empty string) → `Some(s) if !s.is_empty()` fails → `(None, None)`
- `payload["error"]` is exactly 500 chars → `truncate_at_utf8_boundary` returns unchanged
- `payload["error"]` is 501+ chars → truncated at char boundary ≤ 500 bytes
- Multi-byte UTF-8 in error string → `truncate_at_utf8_boundary` walks back to char boundary

---

## Modified Functions

### Function: extract_observation_fields() — add "PostToolUseFailure" arm

**Existing signature (unchanged):**
```
fn extract_observation_fields(event: &unimatrix_engine::wire::ImplantEvent) -> ObservationRow
```

**Current match arms:** "PreToolUse", "PostToolUse" | "post_tool_use_rework_candidate",
"SubagentStart", "SubagentStop" | _ (wildcard).

**Change:** Add explicit `"PostToolUseFailure"` arm BEFORE the `"SubagentStop" | _` wildcard.
The arm must be explicit (ADR-001, FR-04.1) — the wildcard stores `tool = None` (R-03).

```
fn extract_observation_fields(event: &ImplantEvent) -> ObservationRow {
    let session_id = event.session_id.clone();
    let ts_millis = (event.timestamp as i64).saturating_mul(1000);
    let hook = event.event_type.clone();

    let (tool, input, response_size, response_snippet) = match hook.as_str() {
        "PreToolUse" => {
            // ... unchanged ...
        }
        "PostToolUse" | "post_tool_use_rework_candidate" => {
            // ... unchanged, calls extract_response_fields() ...
        }
        "SubagentStart" => {
            // ... unchanged ...
        }

        // col-027: Explicit arm for PostToolUseFailure
        // MUST be before the wildcard to avoid R-03 (tool = None storage)
        // MUST call extract_error_field(), NOT extract_response_fields() (ADR-002, R-01)
        "PostToolUseFailure" => {
            // Field: tool_name -- same field as PostToolUse (FR-04.2)
            let tool = event
                .payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Field: tool_input -- serialize as JSON string, same as PreToolUse (FR-04.6)
            let input = event
                .payload
                .get("tool_input")
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            // Field: response_size, response_snippet -- from extract_error_field()
            // CRITICAL: Must call extract_error_field(), not extract_response_fields()
            // extract_response_fields() reads payload["tool_response"] which is absent
            // on failure payloads -- returns (None, None) silently if called here.
            let (response_size, response_snippet) = extract_error_field(&event.payload);

            (tool, input, response_size, response_snippet)
        }

        "SubagentStop" | _ => (None, None, None, None),
    };

    // Normalization block: UNCHANGED.
    // "post_tool_use_rework_candidate" -> "PostToolUse" (col-019)
    // "PostToolUseFailure" is NOT normalized here (ADR-003, FR-04.5)
    let hook = if hook == "post_tool_use_rework_candidate" {
        "PostToolUse".to_string()
    } else {
        hook
    };

    ObservationRow {
        session_id,
        ts_millis,
        hook,      // "PostToolUseFailure" stored verbatim
        tool,      // Some(tool_name) or None if tool_name absent in payload
        input,     // Some(serialized_tool_input) or None if tool_input absent
        response_size,    // None (always for PostToolUseFailure)
        response_snippet, // Some(error_string[:500]) or None if error absent
        topic_signal: event.topic_signal.clone(),
    }
}
```

**Result for a typical PostToolUseFailure event:**
```
ObservationRow {
    session_id: "sess-abc",
    ts_millis: 1234567890000,
    hook: "PostToolUseFailure",      // verbatim, not normalized
    tool: Some("Bash"),
    input: Some("{\"command\":\"ls /restricted\"}"),
    response_size: None,             // always None
    response_snippet: Some("permission denied: /restricted"),
    topic_signal: Some("ls /restricted"),
}
```

---

## State Machine

No state. `extract_observation_fields()` and `extract_error_field()` are pure functions.
The `insert_observation()` function that calls `extract_observation_fields()` runs in a
`spawn_blocking` context as part of the existing `RecordEvent` handling path. No changes to
that calling context.

---

## Initialization Sequence

No initialization. Both functions are called from the existing `RecordEvent` handler in listener.rs.
The dispatch path is:

```
handle_record_event(event: ImplantEvent):
  obs = extract_observation_fields(&event)  // NEW arm fires for PostToolUseFailure
  insert_observation(store, &obs)           // unchanged SQL insert
```

---

## Data Flow

**Inputs to extract_error_field():**
- `payload: &serde_json::Value` — the `input.extra.clone()` from hook.rs
- Reads: `payload["error"]` (plain string)

**Outputs from extract_error_field():**
- `(Option<i64>, Option<String>)` = `(None, Some(snippet))` or `(None, None)`

**Inputs to extract_observation_fields() PostToolUseFailure arm:**
- `event.payload["tool_name"]` → `obs.tool`
- `event.payload["tool_input"]` → `obs.input` (serialized)
- `extract_error_field(&event.payload)` → `(obs.response_size, obs.response_snippet)`
- `event.topic_signal` → `obs.topic_signal` (pass-through)
- `event.event_type` → `obs.hook` (= "PostToolUseFailure", normalized block does NOT touch it)

**SQLite column mapping (unchanged schema):**
```
obs.session_id     -> observations.session_id TEXT
obs.ts_millis      -> observations.ts_millis  INTEGER
obs.hook           -> observations.hook        TEXT  = "PostToolUseFailure"
obs.tool           -> observations.tool        TEXT? = tool_name or NULL
obs.input          -> observations.input       TEXT? = serialized tool_input or NULL
obs.response_size  -> observations.response_size INTEGER? = NULL always
obs.response_snippet -> observations.response_snippet TEXT? = error string[:500] or NULL
obs.topic_signal   -> observations.topic_signal TEXT?
```

---

## Error Handling

| Condition | Handling |
|-----------|----------|
| `payload["error"]` absent | `extract_error_field()` returns `(None, None)`; `response_snippet = None` |
| `payload["error"]` is null JSON | `as_str()` returns None; `(None, None)` |
| `payload["error"]` is non-string | `as_str()` returns None; `(None, None)` |
| `payload["error"]` is empty string | `!s.is_empty()` guard triggers; `(None, None)` |
| `payload["tool_name"]` absent | `obs.tool = None`; ToolFailureRule skips None-tool records gracefully |
| `payload["tool_input"]` absent | `obs.input = None`; no panic |
| `serde_json::to_string()` fails on tool_input | `.unwrap_or_default()` returns `""`; stored as empty string (acceptable) |
| SQL insertion fails | Propagated as `StoreError`; observation not stored; hook has already exited 0 |

---

## Key Test Scenarios

### T-OS-01: PostToolUseFailure arm stores hook = "PostToolUseFailure" verbatim (AC-03, AC-04, R-03)

```
test extract_observation_fields_posttoolusefailure_hook_type:
  event = ImplantEvent {
    event_type: "PostToolUseFailure",
    session_id: "sess-1",
    timestamp: 1000,
    payload: json!({ "tool_name": "Bash", "error": "permission denied", "tool_input": {} }),
    topic_signal: None,
  }
  obs = extract_observation_fields(&event)
  // AC-03
  assert_eq!(obs.hook, "PostToolUseFailure")           // verbatim, not "PostToolUse"
  assert_eq!(obs.hook, hook_type::POSTTOOLUSEFAILURE)  // use constant, not literal (R-11)
  // AC-03: tool is Some
  assert!(obs.tool.is_some())
  assert_eq!(obs.tool.as_deref(), Some("Bash"))
  // AC-03: response_snippet contains error content
  assert_eq!(obs.response_snippet, Some("permission denied".to_string()))
  // R-10: response_size is None
  assert_eq!(obs.response_size, None)
```

### T-OS-02: extract_error_field returns snippet from error field (R-01 primary mitigation)

```
test extract_error_field_normal_error:
  payload = json!({ "tool_name": "Bash", "error": "permission denied" })
  (size, snippet) = extract_error_field(&payload)
  assert_eq!(size, None)
  assert_eq!(snippet, Some("permission denied".to_string()))
```

### T-OS-03: extract_error_field absent error returns (None, None) (R-01)

```
test extract_error_field_absent:
  payload = json!({ "tool_name": "Read" })
  (size, snippet) = extract_error_field(&payload)
  assert_eq!(size, None)
  assert_eq!(snippet, None)
```

### T-OS-04: extract_error_field long error truncated at 500 chars (R-01, NFR-02)

```
test extract_error_field_truncation:
  long_error = "x".repeat(600)
  payload = json!({ "error": long_error })
  (_, snippet) = extract_error_field(&payload)
  assert!(snippet.is_some())
  assert!(snippet.unwrap().len() <= 500)
```

### T-OS-05: extract_error_field multi-byte UTF-8 truncated at char boundary (R-01)

```
test extract_error_field_utf8_boundary:
  // each emoji is 4 bytes; 130 emojis = 520 bytes > 500 limit
  emoji_str: String = "🔥".repeat(130)
  payload = json!({ "error": emoji_str })
  (_, snippet) = extract_error_field(&payload)
  assert!(snippet.is_some())
  let s = snippet.unwrap()
  // Must be valid UTF-8 (no truncation mid-codepoint)
  assert!(std::str::from_utf8(s.as_bytes()).is_ok())
  assert!(s.len() <= 500)
```

### T-OS-06: Negative — extract_response_fields on failure payload returns (None, None) (R-01 guard)

```
test extract_response_fields_on_failure_payload_returns_none:
  // Demonstrates WHY we use extract_error_field() instead
  payload = json!({ "tool_name": "Bash", "error": "permission denied" })
  (size, snippet) = extract_response_fields(&payload)
  // tool_response field absent -> both None
  // This test documents the wrong-function risk
  assert_eq!(size, None)
  assert_eq!(snippet, None)
```

### T-OS-07: tool_name absent in payload -> obs.tool is None (R-03 guard)

```
test extract_observation_fields_posttoolusefailure_missing_tool_name:
  event = ImplantEvent {
    event_type: "PostToolUseFailure",
    payload: json!({ "error": "some error" }),   // tool_name absent
    ...
  }
  obs = extract_observation_fields(&event)
  assert_eq!(obs.hook, "PostToolUseFailure")
  assert_eq!(obs.tool, None)  // None is correct; tool_name was absent
  assert_eq!(obs.response_snippet, Some("some error".to_string()))
```

### T-OS-08: PostToolUseFailure arm does NOT normalize hook to "PostToolUse" (AC-04, ADR-003)

```
test extract_observation_fields_posttoolusefailure_not_normalized:
  event = ImplantEvent { event_type: "PostToolUseFailure", ... }
  obs = extract_observation_fields(&event)
  assert_ne!(obs.hook, "PostToolUse")    // must NOT be normalized
  assert_eq!(obs.hook, "PostToolUseFailure")
```

---

## Anti-Patterns to Avoid

- Do NOT call `extract_response_fields()` in the `"PostToolUseFailure"` arm (ADR-002, R-01)
- Do NOT normalize `"PostToolUseFailure"` to `"PostToolUse"` in the normalization block (ADR-003)
- Do NOT set `response_size` to anything other than `None` for failure records (ADR-002, FR-04.4)
- Do NOT add `"PostToolUseFailure"` to the wildcard arm — an explicit arm is required (FR-04.1)
- Do NOT modify `extract_response_fields()` to handle failure payloads (ADR-002: separate functions)
