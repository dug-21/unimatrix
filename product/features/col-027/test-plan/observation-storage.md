# Test Plan: observation-storage (listener.rs)

**File:** `crates/unimatrix-server/src/uds/listener.rs`
**Risks covered:** R-01 (wrong extractor), R-03 (wildcard stores `tool = None`), R-10
(`response_size` non-None)

---

## Unit Test Expectations

Tests live in the existing `#[cfg(test)] mod tests` block in `listener.rs`. The existing block
contains tests for `extract_response_fields` (lines ~4097–4170). New tests for
`extract_error_field` and the `"PostToolUseFailure"` arm of `extract_observation_fields` are added
in the same block.

---

### T-OS-01: `test_extract_error_field_present` (AC-03 partial, R-01)
**AC:** AC-03
**Risk:** R-01

Arrange: `payload = json!({"tool_name": "Bash", "error": "permission denied", "tool_input": {}})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, Some(s))` where `s == "permission denied"`
- First element is always `None` (response_size is never set for failure records)

---

### T-OS-02: `test_extract_error_field_absent` (AC-03 edge case, R-01)
**AC:** AC-03
**Risk:** R-01

Arrange: `payload = json!({"tool_name": "Bash", "tool_input": {}})` (no `"error"` key).
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, None)`
- No panic

---

### T-OS-03: `test_extract_error_field_null` (R-08 partial)
**Risk:** R-08

Arrange: `payload = json!({"error": null})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, None)`
- No panic (`as_str()` on a JSON null must return `None`, not panic)

---

### T-OS-04: `test_extract_error_field_non_string_type` (security, R-01)
**Risk:** R-01, security

Arrange: `payload = json!({"error": ["not", "a", "string"]})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, None)` — `as_str()` on a JSON array returns `None` safely
- No panic

---

### T-OS-05: `test_extract_error_field_truncation_at_501_chars` (AC-03, R-01)
**AC:** AC-03
**Risk:** R-01

Arrange: construct a 501-character ASCII string `s`. `payload = json!({"error": s})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, Some(snippet))` where `snippet.len() == 500`
- The snippet is a valid UTF-8 string (no truncation mid-codepoint)

---

### T-OS-06: `test_extract_error_field_exactly_500_chars` (edge case)
**Risk:** R-01 (boundary)

Arrange: construct exactly 500-character ASCII string `s`. `payload = json!({"error": s})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, Some(snippet))` where `snippet == s` (no truncation applied)

---

### T-OS-07: `test_extract_error_field_empty_string` (edge case)
**Risk:** R-01

Arrange: `payload = json!({"error": ""})`.
Act: call `extract_error_field(&payload)`.
Assert:
- Returns `(None, None)` OR `(None, Some(""))` — implementation must be consistent with the spec.
  The RISK-TEST-STRATEGY notes: "verify the implementation handles `\"\"` consistently."
  The Stage 3c implementer must choose one behaviour and document it; the test pins whichever is
  chosen. If `Some("")` is returned, downstream rules that check `response_snippet.is_some()` would
  match an empty-string record, which may be undesirable. `None` is the safer choice.

---

### T-OS-08: `test_extract_observation_fields_posttoolusefailure_full` (AC-03 + AC-04 + R-01 + R-03 + R-10)
**AC:** AC-03, AC-04
**Risk:** R-01, R-03, R-10

This is the primary compound assertion test. All four AC-03/AC-04 conditions plus R-10 must be
asserted in the same test function.

Arrange: construct `ImplantEvent` (or the equivalent struct passed to `extract_observation_fields`)
with:
- `event_type = "PostToolUseFailure"`
- `payload = json!({"tool_name": "Bash", "error": "some error message", "tool_input": {}})`

Act: call `extract_observation_fields(&event)` and obtain the returned `ObservationRow`.

Assert all of the following in one block:
```rust
assert_eq!(obs.hook, hook_type::POSTTOOLUSEFAILURE);       // AC-04: no normalization
assert_ne!(obs.hook, hook_type::POSTTOOLUSE);               // AC-04: NOT "PostToolUse"
assert!(obs.tool.is_some());                                 // R-03: tool not None
assert_eq!(obs.tool.as_deref(), Some("Bash"));               // R-03: correct tool name
assert_eq!(obs.response_snippet.as_deref(), Some("some error message")); // R-01: snippet populated
assert_eq!(obs.response_size, None);                         // R-10: size always None
```

Use `hook_type::POSTTOOLUSEFAILURE` (the constant), not the raw string literal `"PostToolUseFailure"`.

---

### T-OS-09: `test_extract_observation_fields_posttoolusefailure_no_error_field` (R-01, R-03)
**Risk:** R-01, R-03

Arrange: `ImplantEvent` with `event_type = "PostToolUseFailure"` and `payload` containing
`tool_name` but no `error` field.
Assert:
- `obs.hook == hook_type::POSTTOOLUSEFAILURE`
- `obs.tool.is_some()`
- `obs.response_snippet == None` (absent error → None snippet, no panic)
- `obs.response_size == None`

---

### T-OS-10: `test_extract_observation_fields_posttoolusefailure_tool_absent` (R-03)
**Risk:** R-03

Arrange: `ImplantEvent` with `event_type = "PostToolUseFailure"` and `payload = json!({"error": "boom"})` (no `tool_name`).
Assert:
- `obs.hook == hook_type::POSTTOOLUSEFAILURE`
- `obs.tool == None` (absent tool_name → None, not panic)
- `obs.response_snippet == Some("boom")`

**Why**: The arm must handle absent `tool_name` gracefully (R-08). `ToolFailureRule` skips
`None`-tool records; storing them correctly allows future rules or queries to handle them.

---

### T-OS-11: `test_extract_response_fields_on_failure_payload_returns_none_none` (R-01 negative)
**Risk:** R-01

This is the negative/guard test that demonstrates the **wrong** extractor returns nothing useful
on a failure payload — justifying the `extract_error_field()` separation (ADR-002).

Arrange: `payload = json!({"tool_name": "Bash", "error": "permission denied"})`.
Act: call `extract_response_fields(&payload)` (the existing function, NOT the new one).
Assert:
- Returns `(None, None)`

**Why**: Proves that if `extract_response_fields` were accidentally called in the
`"PostToolUseFailure"` arm, the error content would be silently lost. This test makes the guard
value of the separation visible and executable.

---

## Integration Test Expectations

No new infra-001 test is required. The storage path uses the existing `RecordEvent` fire-and-forget
path. Existing lifecycle and tools suite tests cover DB write behaviour.

The `test_extract_observation_fields_posttoolusefailure_full` unit test (T-OS-08) provides
sufficient coverage that the stored row will have the correct field values.

---

## Edge Cases

- `payload["tool_input"]` (the `input` field on `ObservationRow`): extracted and stored as
  JSON-serialized string, same as `PreToolUse` path. An absent `tool_input` should produce `None`
  for `obs.input` without panic.
- `is_interrupt: true` present in payload: ignored by `extract_observation_fields` for field
  extraction purposes but must not cause a parse failure.
- UTF-8 multi-byte error string: truncation must happen at a valid codepoint boundary. Reuse
  `truncate_at_utf8_boundary()` exactly as used for `extract_response_fields`. T-OS-05 covers this.
