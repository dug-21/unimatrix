# Pseudocode: session-sanitization (Item 4)

## Purpose

Insert a `sanitize_session_id` guard in the `post_tool_use_rework_candidate` dispatch arm of
`dispatch_request` in `listener.rs`. This closes the last session injection gap: every other
dispatch arm that consumes `event.session_id` already calls `sanitize_session_id` (pattern
#3921), but this arm reads `event.session_id` directly without validation. An untrusted hook
client with `SessionWrite` capability can currently inject an arbitrary string as a session
key into `session_registry.record_rework_event`.

## File

`crates/unimatrix-server/src/uds/listener.rs`

## Scope

One guard block inserted (7 lines of Rust) into the `post_tool_use_rework_candidate` arm.
No changes to the general `RecordEvent` arm, `sanitize_session_id`, or any other arm.

---

## Context: Existing RecordEvent General Arm (lines 720–741 — the reference pattern)

The general `HookRequest::RecordEvent` arm (which handles all other event types) already
has the sanitization guard at lines 731–740:

```rust
HookRequest::RecordEvent { event } => {
    if !uds_has_capability(Capability::SessionWrite) {
        return HookResponse::Error { code: -32003, ... };
    }
    // GH #519 (SEC-01): Validate session_id before any registry or DB writes.
    if let Err(e) = sanitize_session_id(&event.session_id) {
        tracing::warn!(
            session_id = %event.session_id,
            error = %e,
            "UDS: RecordEvent rejected: invalid session_id"
        );
        return HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: e,
        };
    }
    // ... event processing continues ...
}
```

Item 4 mirrors this pattern in the `post_tool_use_rework_candidate` arm with one difference
in the warn message: `"UDS: RecordEvent (rework_candidate) rejected: invalid session_id"`.

---

## Modified Arm: `post_tool_use_rework_candidate`

### Before Change (lines 656–718, verified from source)

```rust
HookRequest::RecordEvent { ref event }
    if event.event_type == "post_tool_use_rework_candidate" =>
{
    // [1] Capability check (lines 660–665)
    if !uds_has_capability(Capability::SessionWrite) {
        return HookResponse::Error {
            code: -32003,
            message: "insufficient capability: SessionWrite required".to_string(),
        };
    }
    // [--- INSERTION POINT: sanitize_session_id guard goes HERE ---]

    // [3] Payload extraction (lines 666–681)
    let tool_name = event.payload.get("tool_name")...;
    let file_path = event.payload.get("file_path")...;
    let had_failure = event.payload.get("had_failure")...;

    // [4a] Registry write (line 690)
    session_registry.record_rework_event(&event.session_id, rework_event);

    // [4b] Topic signal write (lines 693–699)
    session_registry.record_topic_signal(&event.session_id, signal.clone(), event.timestamp);

    // ... remainder unchanged ...
    HookResponse::Ack
}
```

### After Change: Structural Insertion Order (C-07 / SR-05)

1. Capability check — EXISTING, unchanged
2. `sanitize_session_id` guard — NEW, inserted here
3. Payload field extraction — EXISTING, unchanged
4. `record_rework_event` + `record_topic_signal` + observation spawn — EXISTING, unchanged

```rust
HookRequest::RecordEvent { ref event }
    if event.event_type == "post_tool_use_rework_candidate" =>
{
    // [1] Capability check — unchanged
    if !uds_has_capability(Capability::SessionWrite) {
        return HookResponse::Error {
            code: -32003,
            message: "insufficient capability: SessionWrite required".to_string(),
        };
    }

    // [2] NEW: session_id sanitization — must precede ANY use of event.session_id
    // Mirrors the RecordEvent general arm guard (lines 731–740).
    // Message qualifier "(rework_candidate)" identifies the arm in operator logs.
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

    // [3] Payload extraction — unchanged
    let tool_name = event
        .payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let file_path = event
        .payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let had_failure = event
        .payload
        .get("had_failure")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // [4] Registry calls and observation spawn — unchanged
    let rework_event = ReworkEvent { tool_name, file_path, had_failure, timestamp: event.timestamp };
    session_registry.record_rework_event(&event.session_id, rework_event);
    // ... topic_signal, spawn_blocking, HookResponse::Ack unchanged ...
}
```

### Invariant: No `event.session_id` Use Between Steps 1 and 2

The guard block must be the first consumer of `event.session_id` after the capability check.
Verify by code inspection that no statement between the closing `}` of the capability check
and the opening `if let Err(e) = sanitize_session_id(...)` references `event.session_id`.

This structural constraint is a maintenance invariant (R-04): if code is added in this
region in the future, the sanitization guard must remain the first `event.session_id` consumer.

---

## Function and Constant References

| Name | Source | Notes |
|------|--------|-------|
| `sanitize_session_id(&str) -> Result<(), String>` | `listener.rs` (local fn) | Accepts `[a-zA-Z0-9\-_]+`, length [1, 128] |
| `ERR_INVALID_PAYLOAD` | `listener.rs` (local const, i64) | Established error code for session_id failures (C-05) |
| `HookResponse::Error { code: i64, message: String }` | `listener.rs` | Existing variant |
| `event.session_id` | `HookEvent` field (String) | Referenced via `&event.session_id` |

All referenced names are local to `listener.rs`. No imports needed.

---

## Security Properties

Before this change: any hook client with `SessionWrite` capability can call
`record_rework_event` with `event.session_id = "../../etc/passwd"` or any other string.
The session key is stored verbatim in the session registry.

After this change: `sanitize_session_id("../../etc/passwd")` returns `Err(String)`.
The arm returns `HookResponse::Error { code: ERR_INVALID_PAYLOAD }`. Neither
`record_rework_event` nor `record_topic_signal` is called. The malformed session_id
does not enter the registry.

The allowlist `[a-zA-Z0-9\-_]+` with max 128 chars is unchanged — this change does not
widen the allowlist.

---

## Error Handling

`sanitize_session_id` returns `Err(String)` where the `String` is a human-readable
rejection reason. This error string is passed directly as `message` in
`HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: e }`. The caller (hook client)
receives the rejection reason. A `tracing::warn!` is emitted with both `session_id` and
`error` fields for audit visibility.

If `sanitize_session_id` returns `Ok(())`, execution continues to the payload extraction
block — no behavior change for valid session IDs.

---

## Key Test Scenarios

### AC-28: Invalid session_id rejected before registry write

```
GIVEN: dispatch_request receives HookRequest::RecordEvent where:
       event.event_type == "post_tool_use_rework_candidate"
       event.session_id == "../../etc/passwd"
       client has Capability::SessionWrite
WHEN:  dispatch_request() processes the arm
THEN:  returns HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: _ }
AND:   session_registry.record_rework_event is NOT called
AND:   a tracing::warn! is emitted (verified by code review, not test assertion)

NOTE:  code inspection must confirm no use of event.session_id appears between
       the capability check closing brace and the sanitize_session_id call.
```

Test function name: `test_dispatch_rework_candidate_invalid_session_id_rejected`

### AC-29: Valid session_id proceeds to registry (regression guard)

```
GIVEN: dispatch_request receives HookRequest::RecordEvent where:
       event.event_type == "post_tool_use_rework_candidate"
       event.session_id == "session-abc123"   (valid: matches [a-zA-Z0-9\-_]+, length <= 128)
       event.payload contains tool_name, had_failure fields
       client has Capability::SessionWrite
WHEN:  dispatch_request() processes the arm
THEN:  sanitize_session_id returns Ok(())
AND:   session_registry.record_rework_event is called with session_id="session-abc123"
AND:   returns HookResponse::Ack (or equivalent success variant)
```

Test function name: `test_dispatch_rework_candidate_valid_path_not_regressed`

---

## Risks Addressed

- R-04: Guard insertion order verified both by test (AC-28 — runtime confirmation) and
  by code inspection (structural constraint — no `event.session_id` use between steps 1 and 2).
  Both are required; the test alone is insufficient.
- R-08: AC-29 is a non-negotiable regression guard. Valid rework-candidate events must
  continue to reach `record_rework_event`.

## Knowledge Stewardship

- Entry #3921 (sanitize_session_id consistency rule): all UDS arms consuming `event.session_id`
  must call `sanitize_session_id` before first use. This pseudocode closes the last gap.
- Entry #3902 (UDS dispatch session audit lesson): guard omission pattern — rework arm was
  added after the guard was established in other arms.
- ADR-001 (entry #4143): guard placement order (capability check first, then sanitization)
  is consistent with all other arms.
- Deviations from established patterns: none.
