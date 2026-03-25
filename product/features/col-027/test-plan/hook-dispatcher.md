# Test Plan: hook-dispatcher (hook.rs)

**File:** `crates/unimatrix-server/src/uds/hook.rs`
**Risks covered:** R-05 (wildcard in `build_request()`), R-08 (non-zero exit on malformed payload),
R-09 (`extract_event_topic_signal()` falls through)

---

## Unit Test Expectations

Tests live in the existing `#[cfg(test)] mod tests` block in `hook.rs`. Several `build_request_*`
tests already exist; new tests follow the same naming pattern.

All tests use the `test_input()` helper (already present) and extend it with `extra` fields as
needed.

---

### T-HD-01: `test_build_request_posttoolusefailure_explicit_arm` (AC-11)
**AC:** AC-11
**Risk:** R-05

Arrange: construct `HookInput` with `extra = json!({"tool_name": "Bash", "error": "permission denied", "tool_input": {}})`.
Act: call `build_request("PostToolUseFailure", &input)`.
Assert:
- Returns `HookRequest::RecordEvent { event }` (not any other variant)
- `event.event_type == "PostToolUseFailure"` (explicit arm ran, not wildcard)
- `event.topic_signal` is populated (not None) — verifies `extract_event_topic_signal` arm ran

**Why**: R-05 — if the wildcard arm handles this event, `event_type` will be whatever the wildcard
produces (likely still `"PostToolUseFailure"` in some implementations, but `tool_name` may not be
extracted and the rework logic might be entered). This test pins the exact discriminant.

---

### T-HD-02: `test_build_request_posttoolusefailure_empty_extra` (AC-12 unit part)
**AC:** AC-12
**Risk:** R-08

Arrange: `HookInput` with `extra = json!({})` (all optional fields absent).
Act: call `build_request("PostToolUseFailure", &input)`.
Assert:
- Does not panic
- Returns `HookRequest::RecordEvent { .. }` (a valid request)
- No observable assertion on `topic_signal` (may be None)

---

### T-HD-03: `test_build_request_posttoolusefailure_missing_tool_name` (AC-12 unit part)
**AC:** AC-12
**Risk:** R-08

Arrange: `HookInput` with `extra = json!({"error": "something went wrong", "tool_input": {}})`.
Act: call `build_request("PostToolUseFailure", &input)`.
Assert:
- Does not panic
- Returns `HookRequest::RecordEvent { .. }`

**Note:** `tool_name` absent is a realistic scenario (defensive parsing requirement FR-03.6).

---

### T-HD-04: `test_build_request_posttoolusefailure_null_error` (AC-12 unit part)
**AC:** AC-12
**Risk:** R-08

Arrange: `HookInput` with `extra = json!({"tool_name": "Read", "error": null, "tool_input": {}})`.
Act: call `build_request("PostToolUseFailure", &input)`.
Assert:
- Does not panic
- Returns `HookRequest::RecordEvent { .. }`

**Note:** `error` being JSON null rather than a string must not cause a `.unwrap()` panic.
`as_str()` on a null JSON value returns `None` safely.

---

### T-HD-05: `test_build_request_posttoolusefailure_does_not_enter_rework_logic`
**AC:** AC-11
**Risk:** R-05 (secondary)

Arrange: `HookInput` with `extra = json!({"tool_name": "Write", "error": "permission denied", "tool_input": {}})`.
Act: call `build_request("PostToolUseFailure", &input)`.
Assert:
- Returns `HookRequest::RecordEvent { .. }` — NOT `HookRequest::RecordEvents { .. }` (the multi-edit path)
- `event.event_type == "PostToolUseFailure"` — not `"post_tool_use_rework_candidate"`

**Why**: The `PostToolUse` arm contains rework detection logic. The `PostToolUseFailure` arm must
be entirely separate and must never produce rework-tagged records.

---

### T-HD-06: `test_extract_event_topic_signal_posttoolusefailure` (R-09)
**AC:** AC-11 (partial)
**Risk:** R-09

Arrange: `HookInput` with `extra = json!({"tool_name": "Bash", "tool_input": {"command": "ls /tmp"}, "error": "no such file"})`.
Act: call `extract_event_topic_signal("PostToolUseFailure", &input)`.
Assert:
- Returns `Some(s)` where `s` is derived from `tool_input`, not from the full `extra` blob
- Specifically: `s` must not contain `"error"` as a key (i.e., it is not the full `extra` serialized)
- Optionally: `s` contains the `"command"` value `"ls /tmp"` if `tool_input` stringification
  matches `PostToolUse` path

**Why**: R-09 — if the wildcard arm runs, `topic_signal` gets the entire `extra` blob including
`"error"`. This is semantically wrong (error string contaminates the topic signal). The explicit
arm restricts to `tool_input`.

---

## Code Inspection Assertions (AC-11a)

During Stage 3c, before running tests:

```bash
grep -n '"PostToolUseFailure"' crates/unimatrix-server/src/uds/hook.rs
```

Assert the output contains at least two lines:
1. One line in `build_request()` — the match arm
2. One line in `extract_event_topic_signal()` — the match arm

If either is absent, R-05 and R-09 are uncovered regardless of test results.

---

## Integration Test Expectations (AC-12 — binary exit-code)

These tests require the compiled binary (`cargo build --release` first):

```bash
# Empty JSON payload — most common malformed case
echo '{}' | unimatrix hook PostToolUseFailure
assert exit code 0

# Malformed JSON (not valid JSON at all)
echo 'not-json' | unimatrix hook PostToolUseFailure
assert exit code 0

# Empty stdin
echo '' | unimatrix hook PostToolUseFailure
assert exit code 0
```

These are shell-level tests run during Stage 3c. They verify FR-03.7 (the hook never fails)
for the `PostToolUseFailure` path specifically.

**Note:** These tests may require the Unimatrix server to be running (or not — the hook exits 0
even if the server is unavailable, per the fire-and-forget graceful degradation path). Stage 3c
executor should run them with the server running to verify the full path.

---

## Edge Cases

- `extra` key present but value is `[]` (JSON array): `as_str()` returns `None`, no panic
- `tool_input` field absent: `topic_signal` may be `None`; no panic
- `is_interrupt: true` present in payload: must be included in forwarded payload but must not
  affect the returned `HookRequest` variant or `event_type`
- Very long `error` string (10KB): must not cause the hook to exceed the 40ms transport budget;
  the truncation to 500 chars happens in `listener.rs`, not `hook.rs` — `hook.rs` forwards the raw
  payload. This is acceptable; the 40ms budget is for the channel send, not for the downstream
  DB write.
