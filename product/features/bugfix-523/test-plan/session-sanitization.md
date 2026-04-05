# Test Plan: `sanitize_session_id` Guard in `post_tool_use_rework_candidate` Arm (Item 4)

## Component

`dispatch_request` match arm for `post_tool_use_rework_candidate` in
`crates/unimatrix-server/src/uds/listener.rs`

## Risks Covered

| Risk | Priority | AC |
|------|----------|----|
| R-04: Guard inserted after `event.session_id` first use | Critical | AC-28 + code inspection |
| R-08: Valid events rejected by guard (regression) | High | AC-29 |

---

## Background

The `post_tool_use_rework_candidate` arm is the last `dispatch_request` arm that uses
`event.session_id` without calling `sanitize_session_id` first. Every other arm in
`dispatch_request` that consumes `session_id` has this guard. The fix inserts the identical
guard pattern used in the `RecordEvent` general arm (lines 731–738).

Existing `sanitize_session_id` unit tests (lines 3830–3886) cover the function's input
contract: valid alphanumeric IDs, dash/underscore, max 128 chars, empty, too-long,
path-traversal, special characters. The new dispatch-arm tests verify the guard is called
in the right place, not re-test the sanitization logic itself.

---

## Test Functions

### T-08: `test_dispatch_rework_candidate_invalid_session_id_rejected` (AC-28)

**File**: `uds/listener.rs` `#[cfg(test)]`
**Type**: `#[test]` or `#[tokio::test]` (match existing dispatch-arm test style)

**Arrange**:
- Construct a `HookRequest::RecordEvent { event }` where:
  - `event.event_type = "post_tool_use_rework_candidate"`
  - `event.session_id = "../../etc/passwd"` (path traversal — rejected by sanitize_session_id)
  - `event.payload` contains valid `tool_name` and `file_path` fields (so the test
    focuses on the session_id rejection, not payload extraction failure)
- Provide a mock or stub session registry / observation store (no real DB access needed).
- Ensure the mock grants `Capability::SessionWrite` so the capability check passes.

**Act**: Call `dispatch_request(request, ...)` (or the equivalent handler that exercises
the dispatch arm).

**Assert**:
- Return value is `HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: _ }`.
- The specific code value is `ERR_INVALID_PAYLOAD` (the i64 constant from `listener.rs`).
  Not a different error code — assert the exact constant.
- `session_registry.record_rework_event` is **never called** (mock records call count;
  assert count = 0).

**R-04 coverage**: This runtime test confirms the guard fires for a malformed session_id.
It does NOT alone satisfy R-04 — code inspection must additionally confirm insertion order.

---

### T-09: `test_dispatch_rework_candidate_valid_path_not_regressed` (AC-29)

**File**: `uds/listener.rs` `#[cfg(test)]`
**Type**: `#[test]` or `#[tokio::test]`

**Arrange**:
- Construct a `HookRequest::RecordEvent { event }` where:
  - `event.event_type = "post_tool_use_rework_candidate"`
  - `event.session_id = "session-abc123"` (valid: alphanumeric with dash)
  - `event.payload` contains valid `tool_name`, `file_path`, `had_failure` fields
- Mock grants `Capability::SessionWrite`.
- Session registry mock with a registered session for `"session-abc123"`.

**Act**: Call `dispatch_request(request, ...)`.

**Assert**:
- Return value is `HookResponse::Ok` or equivalent success variant.
- `session_registry.record_rework_event` is called exactly once (mock records invocations;
  assert count = 1).

**R-08 coverage**: Ensures the `sanitize_session_id` call does not reject valid session_ids.
This test must be present alongside T-08 — they are a required pair.

---

## Code Inspection Requirement (R-04 — Non-Negotiable)

The runtime test T-08 alone is insufficient for R-04 coverage. Gate 3b reviewer must
confirm via source inspection that the structural ordering in
`post_tool_use_rework_candidate` arm is:

1. Capability check (`uds_has_capability(Capability::SessionWrite)`) — existing
2. `sanitize_session_id(&event.session_id)` guard — **NEW (this change)**
3. `event.payload.get("tool_name")` extraction — existing
4. `session_registry.record_rework_event(&event.session_id, ...)` — existing
5. `record_topic_signal` / other session_id uses — existing

**No use of `event.session_id` may appear between steps 1 and 2.** A guard placed after
step 3 (even if step 3 does not touch `event.session_id` directly) violates the structural
contract and creates a maintenance trap for future edits.

The guard block must use `ERR_INVALID_PAYLOAD` as the error code, not any other constant.
The warn message must contain `"(rework_candidate)"` to identify the arm in logs.

Gate report must confirm: "Item 4 insertion order verified by code inspection: guard appears
immediately after capability check, before `event.payload.get('tool_name')`. No use of
`event.session_id` between capability check and guard. `ERR_INVALID_PAYLOAD` code used."

---

## Guard Pattern Reference

The guard inserted must be identical to the `RecordEvent` general arm pattern
(lines 731–738 in `listener.rs`):

```rust
if let Err(e) = sanitize_session_id(&event.session_id) {
    tracing::warn!(
        session_id = %event.session_id,
        error = %e,
        "UDS: RecordEvent (rework_candidate) rejected: invalid session_id"
    );
    return HookResponse::Error {
        code: ERR_INVALID_PAYLOAD,
        message: e,
    };
}
```

The only difference from the general arm is the parenthetical qualifier `(rework_candidate)`
in the warn message. The code, structure, and field references must be identical.

---

## Existing Sanitize Tests — Not Duplicated

The following existing test functions (lines 3830–3886) cover the `sanitize_session_id`
function contract. The new dispatch-arm tests (T-08, T-09) do NOT re-test these cases.

Existing tests already cover:
- Valid alphanumeric, valid with dash/underscore, valid 128-char max
- Too-long (129 chars), path characters (`/`, `..`), space, dot, exclamation
- Empty string

The dispatch-arm tests verify only that the guard is in the right place and is called at
dispatch time — the correctness of `sanitize_session_id` itself is already validated.

---

## Edge Cases

- **Session_id at exactly 128 valid chars**: Must pass the guard (T-09 style). The
  existing `sanitize_session_id_valid_128_chars` test covers this at the function level;
  the dispatch test need not repeat it.
- **Session_id = `"../../etc/passwd"`**: Path traversal is the canonical injection input.
  T-08 uses this exact value per IMPLEMENTATION-BRIEF.md and ACCEPTANCE-MAP.md.
- **Capability check fails first**: If `Capability::SessionWrite` is not granted, the arm
  returns before reaching the `sanitize_session_id` guard. T-08 must ensure the mock grants
  `SessionWrite` so the capability check passes and the guard is the one that fires.
