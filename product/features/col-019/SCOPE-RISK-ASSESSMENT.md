# col-019: Scope Risk Assessment

## SR-01: Rework Detection Regression

**Severity**: HIGH
**Likelihood**: MEDIUM

The current rework interception path (`post_tool_use_rework_candidate`) handles PostToolUse events for file-mutating tools and feeds them exclusively to session_registry. The fix must add observation persistence alongside rework recording. Risk: if the observation write path is added incorrectly, it could alter the rework event routing, break the rework candidate matching in `dispatch_request()`, or introduce latency that affects rework detection timing.

**Mitigation**: The rework handler must remain the primary match arm. Observation persistence should be added as a secondary fire-and-forget write within the existing rework handler block, not by removing the rework interception. All 8 existing rework tests in hook.rs must pass unchanged.

## SR-02: Claude Code tool_response Schema Variability

**Severity**: MEDIUM
**Likelihood**: MEDIUM

Claude Code's `tool_response` field is tool-specific -- different tools return different JSON shapes. Some tools may return large responses (e.g., Read on large files), nested objects, arrays, or even non-JSON content. The `tool_response` field itself may be absent for some tool types or error cases. The field name could also change in future Claude Code versions.

**Mitigation**: Defensive extraction: treat `tool_response` as `Option<serde_json::Value>`. Serialize to string for size/snippet computation. Handle missing/null gracefully (leave response_size and response_snippet as NULL). Cap snippet at 500 chars. Use `serde_json::to_string()` which handles all JSON value types. Add tests for edge cases: missing tool_response, null value, empty object, large response, nested objects.

## SR-03: Observation Table Write Volume Increase

**Severity**: LOW
**Likelihood**: HIGH (certain)

Currently, rework-eligible PostToolUse events (Bash, Edit, Write, MultiEdit) are NOT written to the observations table. Adding observation persistence for these events will increase write volume. Based on data (5,136+ PostToolUse events), the observation table could see roughly double the PostToolUse rows.

**Mitigation**: Observation writes are already fire-and-forget via `spawn_blocking`. The SQLite write path handles batching for RecordEvents. Individual writes for RecordEvent are single-row INSERTs. The performance impact is negligible -- SQLite can handle thousands of single-row INSERTs per second. Monitor observation table growth after deployment.

## SR-04: Dual-Write Consistency for Rework Events

**Severity**: LOW
**Likelihood**: LOW

When rework-eligible PostToolUse events are both recorded in session_registry AND persisted as observations, the two writes are independent. If the observation write fails (fire-and-forget, errors logged), the rework tracking still succeeds. If the rework recording fails (in-memory, no persistence), the observation still persists. This asymmetry is acceptable but should be documented.

**Mitigation**: Both paths already have independent error handling. The fire-and-forget pattern for observations is established (col-012). No additional synchronization needed. Document in ARCHITECTURE.md that the dual-write is intentionally non-transactional.

## SR-05: Hook-Side vs Server-Side Response Processing

**Severity**: LOW
**Likelihood**: LOW

The fix could be implemented in two places: (a) in hook.rs build_request() to compute response_size/snippet before sending to the server, or (b) in listener.rs extract_observation_fields() to compute from the raw tool_response in the payload. Option (a) adds computation to the latency-constrained hook binary. Option (b) keeps the hook thin but requires the full tool_response to travel over the UDS socket.

**Mitigation**: Option (b) is preferred. The tool_response is already in the payload via `input.extra` for non-rework tools (generic_record_event passes it through). For rework tools, the hook needs to include tool_response in the payload. Server-side computation happens in spawn_blocking, outside the latency budget. The UDS payload size increase is bounded by the tool_response size, which is already bounded by Claude Code's response truncation.

## Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation Strategy |
|----|------|----------|------------|-------------------|
| SR-01 | Rework detection regression | HIGH | MEDIUM | Add observation write alongside, not instead of, rework handler |
| SR-02 | tool_response schema variability | MEDIUM | MEDIUM | Defensive Option<Value> extraction, edge case tests |
| SR-03 | Observation write volume increase | LOW | HIGH | Fire-and-forget pattern handles load; monitor growth |
| SR-04 | Dual-write consistency | LOW | LOW | Independent error handling, documented non-transactional |
| SR-05 | Hook vs server processing location | LOW | LOW | Server-side processing in spawn_blocking |

## Top 3 Risks for Architect Attention

1. **SR-01**: Rework detection must not regress. Architecture must ensure the rework match arm remains primary and observation write is additive.
2. **SR-02**: tool_response variability requires defensive extraction. Architecture must specify the exact extraction logic and edge case handling.
3. **SR-05**: Processing location decision affects the hook.rs/listener.rs split. Architecture should codify server-side processing as the approach.
