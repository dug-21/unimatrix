# col-019: Scope Risk Assessment

## SR-01: Rework Detection Regression

**Severity**: HIGH
**Likelihood**: MEDIUM

The current rework interception path (`post_tool_use_rework_candidate`) handles PostToolUse events for file-mutating tools and feeds them to session_registry for rework detection, plus accumulates topic signals (col-017). The fix must add observation persistence alongside both without breaking either.

Risk: if the observation write path is added incorrectly, it could alter rework event routing, break the rework candidate matching in `dispatch_request()`, or introduce latency that affects rework detection timing.

**Mitigation**: The rework handler must remain the primary match arm. Observation persistence should be added as a secondary fire-and-forget write within the existing rework handler block, after both `record_rework_event()` (line 555) and topic signal accumulation (line 558). All existing rework tests must pass unchanged.

## SR-02: Claude Code tool_response Schema Variability

**Severity**: MEDIUM
**Likelihood**: MEDIUM

Claude Code's `tool_response` field is tool-specific -- different tools return different JSON shapes. Some tools may return large responses (e.g., Read on large files), nested objects, arrays, or even non-JSON content. The field may be absent for some tool types or error cases.

**Mitigation**: Defensive extraction: treat `tool_response` as `Option<serde_json::Value>`. Serialize to string for size/snippet computation. Handle missing/null gracefully (leave response_size and response_snippet as NULL). Cap snippet at 500 chars with UTF-8 boundary safety. Use `serde_json::to_string()` which handles all JSON value types. Add tests for edge cases.

## SR-03: Observation Table Write Volume Increase

**Severity**: LOW
**Likelihood**: HIGH (certain)

Currently, rework-eligible PostToolUse events (Bash, Edit, Write, MultiEdit) are NOT written to the observations table. Adding observation persistence will increase write volume -- roughly doubling PostToolUse observation rows.

**Mitigation**: Observation writes are already fire-and-forget via `spawn_blocking`. SQLite handles thousands of single-row INSERTs per second. The 60-day retention in context_status maintain=true bounds growth.

## SR-04: Dual-Write Consistency for Rework Events

**Severity**: LOW
**Likelihood**: LOW

When rework-eligible PostToolUse events are recorded in session_registry AND persisted as observations, the two writes are independent. If the observation write fails, rework tracking still succeeds. This asymmetry is acceptable and matches existing patterns.

**Mitigation**: Both paths already have independent error handling. The fire-and-forget pattern for observations is established (col-012). No additional synchronization needed.

## SR-05: col-017 Topic Signal Pipeline Interaction

**Severity**: MEDIUM
**Likelihood**: LOW

col-017 added topic signal extraction in `extract_event_topic_signal()` (hook.rs line 168) and topic signal accumulation in the rework handler (listener.rs line 558), generic RecordEvent handler (line 583), and RecordEvents handler (line 614). The observation persistence addition in the rework handler must not interfere with topic signal flow.

**Mitigation**: The observation write is fire-and-forget via `spawn_blocking` -- it runs asynchronously and cannot block or interfere with the synchronous topic signal accumulation. The observation write should be placed after all synchronous operations (rework recording + topic signal accumulation) in the rework handler. The `topic_signal` field on `ObservationRow` is already populated from `event.topic_signal.clone()` in `extract_observation_fields()`.

## SR-06: Hook-Side Payload Size Increase

**Severity**: LOW
**Likelihood**: LOW

Adding `tool_input` and `tool_response` to rework candidate payloads increases UDS message size. Large Read responses could be significant.

**Mitigation**: MAX_PAYLOAD_SIZE is 1 MiB. Claude Code already truncates large responses. Transport errors are caught and logged. The hook exits 0 regardless (FR-03.7).

## Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation Strategy |
|----|------|----------|------------|-------------------|
| SR-01 | Rework detection regression | HIGH | MEDIUM | Add observation write alongside, not instead of, rework handler |
| SR-02 | tool_response schema variability | MEDIUM | MEDIUM | Defensive Option<Value> extraction, edge case tests |
| SR-03 | Observation write volume increase | LOW | HIGH | Fire-and-forget pattern handles load; monitor growth |
| SR-04 | Dual-write consistency | LOW | LOW | Independent error handling, documented non-transactional |
| SR-05 | col-017 topic signal interaction | MEDIUM | LOW | Fire-and-forget write placed after all synchronous ops |
| SR-06 | Hook payload size increase | LOW | LOW | Bounded by Claude Code response truncation and 1 MiB limit |

## Top 3 Risks for Architect Attention

1. **SR-01**: Rework detection must not regress. Architecture must ensure the rework match arm remains primary and observation write is additive, placed after both rework recording and topic signal accumulation.
2. **SR-02**: tool_response variability requires defensive extraction. Architecture must specify exact extraction logic and edge case handling.
3. **SR-05**: col-017 topic signal pipeline must continue functioning. Architecture must specify observation write placement relative to topic signal accumulation in the rework handler.
