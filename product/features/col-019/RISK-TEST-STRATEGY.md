# col-019: Risk-Test Strategy

## Risk Register

### R-01: Rework Detection Regression (from SR-01)
**Severity**: HIGH | **Likelihood**: MEDIUM | **Impact**: Rework tracking stops working, session outcomes degrade

The `post_tool_use_rework_candidate` match arm in dispatch_request() (listener.rs line 522) is the gatekeeper for rework detection. Any change to its matching logic, event routing, or payload extraction could break rework tracking silently.

**Mitigation**:
- M-01a: The rework handler match arm stays as-is. Observation persistence is added AFTER `session_registry.record_rework_event()` (line 555) AND topic signal accumulation (lines 558-563), not before.
- M-01b: All existing rework tests in hook.rs run unchanged.
- M-01c: New test verifying rework event AND observation row are both created for the same input.

**Test Coverage**:
- T-01: Existing rework tests (hook.rs) -- regression guard
- T-02: Integration test: rework event recorded AND observation row inserted for Edit PostToolUse
- T-03: Integration test: rework event recorded AND observation row inserted for Bash PostToolUse with failure

### R-02: tool_response Absent or Unexpected Shape (from SR-02)
**Severity**: MEDIUM | **Likelihood**: MEDIUM | **Impact**: response_size/snippet remain NULL for some events

Claude Code's tool_response varies by tool type. Edge cases: absent field, null value, empty object `{}`, primitive value (string, number), deeply nested object, very large response.

**Mitigation**:
- M-02a: `extract_response_fields()` uses `Option` throughout. Missing/null/absent tool_response results in (None, None).
- M-02b: Serialization via `serde_json::to_string()` handles all JSON value types.
- M-02c: Defensive snippet truncation with `.chars().take(500)` for UTF-8 boundary safety.

**Test Coverage**:
- T-04: tool_response is a normal JSON object -> size and snippet populated
- T-05: tool_response is absent -> (None, None)
- T-06: tool_response is JSON null -> (None, None)
- T-07: tool_response is empty object `{}` -> size=2, snippet="{}"
- T-08: tool_response is a string value -> size and snippet from serialized string
- T-09: tool_response serialized > 500 chars -> snippet truncated at char boundary

### R-03: col-017 Topic Signal Pipeline Disruption (from SR-05)
**Severity**: MEDIUM | **Likelihood**: LOW | **Impact**: Topic attribution stops working for rework events

The rework handler (listener.rs lines 558-563) accumulates topic signals via `session_registry.record_topic_signal()`. The observation write added by col-019 must not interfere with this synchronous path.

**Mitigation**:
- M-03a: Observation write is fire-and-forget via `spawn_blocking` -- runs asynchronously after all synchronous operations complete.
- M-03b: Observation write placement is after line 563 (topic signal accumulation), before `HookResponse::Ack`.
- M-03c: `ObservationRow.topic_signal` is populated from `event.topic_signal.clone()` in `extract_observation_fields()`, preserving the signal in the observation row.

**Test Coverage**:
- T-10: Rework candidate observation preserves topic_signal in observation row
- T-11: Topic signal accumulation still happens for rework events (existing col-017 test coverage)

### R-04: Hook Payload Size Increase (from SR-06, ADR-003)
**Severity**: LOW | **Likelihood**: LOW | **Impact**: UDS transport slower for large tool_responses

Adding tool_input and tool_response to rework candidate payloads increases UDS message size.

**Mitigation**:
- M-04a: MAX_PAYLOAD_SIZE is 1 MiB. Claude Code already truncates large responses.
- M-04b: Transport errors are caught and logged. The hook exits 0 regardless (FR-03.7).

**Test Coverage**:
- T-12: Rework payload with tool_input and tool_response verifies correct payload structure (unit test in hook.rs)

### R-05: UTF-8 Boundary Corruption in Snippet
**Severity**: LOW | **Likelihood**: LOW | **Impact**: Panic or garbled text in response_snippet

Byte-slicing a UTF-8 string at 500 bytes could split a multi-byte character.

**Mitigation**:
- M-05a: Use `.chars().take(500).collect::<String>()` for character-safe truncation (NFR-03).
- M-05b: response_size uses byte length of the full serialized string (not char count).

**Test Coverage**:
- T-13: tool_response containing multi-byte UTF-8 characters -> snippet truncated at char boundary, no panic

### R-06: Observation Write Volume Increase (from SR-03)
**Severity**: LOW | **Likelihood**: CERTAIN | **Impact**: Larger database, slightly more I/O

**Mitigation**:
- M-06a: Fire-and-forget pattern (existing) absorbs write cost.
- M-06b: 60-day retention bounds growth.

**Test Coverage**: No specific test. Monitored via observation_stats().

## Scope Risk Traceability

| Scope Risk | Architecture Response | Test Verification |
|-----------|----------------------|-------------------|
| SR-01: Rework detection regression | ADR-002: Additive dual-write, rework handler stays primary | T-01 (existing tests), T-02, T-03 |
| SR-02: tool_response schema variability | ADR-001: Server-side processing with defensive Option handling | T-04 through T-09, T-13 |
| SR-03: Write volume increase | Fire-and-forget pattern, 60-day retention | Monitoring |
| SR-04: Dual-write consistency | ADR-002: Intentionally non-transactional, independent error handling | T-02, T-03 |
| SR-05: col-017 topic signal interaction | ADR-002: Write placed after all synchronous ops | T-10, T-11 |
| SR-06: Hook payload size increase | ADR-003: Bounded by Claude Code truncation | T-12 |

## Test Summary

| ID | Test | Type | Covers |
|----|------|------|--------|
| T-01 | Existing rework tests | Unit (hook.rs) | R-01 regression guard |
| T-02 | Edit rework + observation dual-write | Integration | R-01, AC-02 |
| T-03 | Bash failure rework + observation dual-write | Integration | R-01, AC-06 |
| T-04 | tool_response normal object -> size/snippet | Unit (listener.rs) | R-02, AC-01 |
| T-05 | tool_response absent -> None/None | Unit (listener.rs) | R-02, AC-04 |
| T-06 | tool_response null -> None/None | Unit (listener.rs) | R-02, AC-04 |
| T-07 | tool_response empty object -> size=2 | Unit (listener.rs) | R-02 |
| T-08 | tool_response string value -> correct size | Unit (listener.rs) | R-02 |
| T-09 | tool_response > 500 chars -> truncation | Unit (listener.rs) | R-02, AC-05 |
| T-10 | Rework candidate observation preserves topic_signal | Unit (listener.rs) | R-03, AC-09 |
| T-11 | Topic signal accumulation for rework events | Existing (col-017) | R-03 |
| T-12 | Rework payload includes tool_input and tool_response | Unit (hook.rs) | R-04, AC-02 |
| T-13 | Multi-byte UTF-8 truncation safe | Unit (listener.rs) | R-05 |

## Risk Rating Summary

| Risk | Severity | Likelihood | Overall |
|------|----------|------------|---------|
| R-01 Rework regression | HIGH | MEDIUM | HIGH |
| R-02 tool_response variability | MEDIUM | MEDIUM | MEDIUM |
| R-03 col-017 topic signal disruption | MEDIUM | LOW | LOW-MEDIUM |
| R-04 Payload size increase | LOW | LOW | LOW |
| R-05 UTF-8 boundary | LOW | LOW | LOW |
| R-06 Write volume increase | LOW | CERTAIN | LOW |

**Overall feature risk**: LOW-MEDIUM. The highest risk (R-01) is well-mitigated by preserving the existing match arm structure and running all existing tests unchanged. R-03 (col-017 interaction) is mitigated by placement ordering.
