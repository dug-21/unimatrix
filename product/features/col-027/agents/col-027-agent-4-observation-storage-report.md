# Agent Report: col-027-agent-4-observation-storage

**Component:** observation-storage
**File modified:** `crates/unimatrix-server/src/uds/listener.rs`
**Wave:** 2

---

## Work Completed

### 1. New function: `extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)`

Added immediately after `extract_response_fields()` as an explicit sibling (ADR-002). Reads
`payload["error"]` as a plain string. Returns `(None, None)` for absent, null, non-string, or
empty-string error fields. Returns `(None, Some(snippet))` where snippet is truncated to 500 bytes
at a valid UTF-8 char boundary via the existing `truncate_at_utf8_boundary()` helper. Always
returns `None` for the first tuple element (response_size) per ADR-002 and FR-04.4.

### 2. Modified function: `extract_observation_fields()` — new `"PostToolUseFailure"` arm

Added an explicit `"PostToolUseFailure"` match arm before the `"SubagentStop" | _` wildcard (per
SR-07). The arm:
- Extracts `tool` from `payload["tool_name"]` (same field as PostToolUse)
- Extracts `input` from `payload["tool_input"]` serialized as JSON string
- Calls `extract_error_field(&event.payload)` for `(response_size, response_snippet)` — NOT `extract_response_fields()` (ADR-002)
- Stores `hook = "PostToolUseFailure"` verbatim; the normalization block does not touch it (ADR-003)

### 3. Unit tests: 11 new tests added

All 11 tests from the component test plan (T-OS-01 through T-OS-11) are implemented in the
existing `#[cfg(test)] mod tests` block under a `// -- col-027: ...` section marker.

| Test | ID | Coverage |
|------|----|----------|
| `test_extract_error_field_present` | T-OS-01 | Normal path, snippet populated |
| `test_extract_error_field_absent` | T-OS-02 | Absent key -> (None, None) |
| `test_extract_error_field_null` | T-OS-03 | JSON null -> (None, None) |
| `test_extract_error_field_non_string_type` | T-OS-04 | Array -> (None, None) |
| `test_extract_error_field_truncation_at_501_chars` | T-OS-05 | 501 chars truncated to 500 |
| `test_extract_error_field_exactly_500_chars` | T-OS-06 | 500 chars unchanged |
| `test_extract_error_field_empty_string` | T-OS-07 | `""` -> (None, None) |
| `test_extract_observation_fields_posttoolusefailure_full` | T-OS-08 | Full compound AC-03/AC-04/R-01/R-03/R-10 |
| `test_extract_observation_fields_posttoolusefailure_no_error_field` | T-OS-09 | Absent error, tool present |
| `test_extract_observation_fields_posttoolusefailure_tool_absent` | T-OS-10 | Absent tool_name, error present |
| `test_extract_response_fields_on_failure_payload_returns_none_none` | T-OS-11 | Negative guard / ADR-002 justification |

---

## Test Results

- `cargo test -p unimatrix-server --lib test_extract_error_field` — **7 passed, 0 failed**
- `cargo test -p unimatrix-server --lib posttoolusefailure` — **10 passed, 0 failed** (includes 3 hook.rs tests from agent-3)
- `cargo test -p unimatrix-server --lib test_extract_response_fields_on_failure` — **1 passed, 0 failed**
- `cargo build --workspace` — **0 errors**
- `cargo clippy -p unimatrix-server` — **0 errors**

Pre-existing failure in full suite: `col018_topic_signal_null_for_generic_prompt` fails under
concurrent test run due to embedding model initialization timing. Confirmed pre-existing: passes
in isolation. Not introduced by this change.

---

## ADR Compliance

- ADR-002: `extract_error_field()` is a new function, NOT a modification of `extract_response_fields()`. Call sites unchanged.
- ADR-003: `"PostToolUseFailure"` is not added to the normalization block. Stored verbatim.
- SR-07: Explicit arm added before the wildcard `"SubagentStop" | _`.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "extract_observation_fields listener PostToolUse observation record storage" -- found entry #763 (Server-Side Observation Intercept Pattern) and #3474 (ADR-002 col-027) already stored.
- Stored: nothing novel to store -- the sibling-function separation pattern is already captured in ADR-002 entry #3474. The existing `truncate_at_utf8_boundary` helper reuse is the only technique applied and it follows the established pattern from extract_response_fields() without deviation.
