# Risk-Based Test Strategy: col-022

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Force-set (`set_feature_force`) overwrites correct heuristic attribution when agent passes wrong topic | High | Med | High |
| R-02 | Hook validation failure silently drops cycle_start event, leaving session unattributed with no error surfaced to caller | High | Med | High |
| R-03 | `session_from_row` column index mismatch after adding `keywords` column causes all session reads to return corrupted data | High | Low | High |
| R-04 | Magic string coupling: "cycle_start"/"cycle_stop" event_type constants diverge between hook builder and listener dispatcher | Med | Med | Med |
| R-05 | Schema v12 migration fails on databases with pre-existing `keywords` column (interrupted prior migration) | Med | Low | Med |
| R-06 | Keywords JSON serialization/deserialization mismatch (e.g., hook writes bare string, listener expects JSON array) | Med | Med | Med |
| R-07 | `set_feature_force` race condition under concurrent UDS messages from parallel hook invocations in same session | Med | Low | Med |
| R-08 | MCP tool response claims `was_set: true` but hook-side attribution actually failed (fire-and-forget disconnect) | Med | High | High |
| R-09 | Hook handler fails to match tool_name containing "context_cycle" due to MCP server prefix mismatch (e.g., `mcp__unimatrix__context_cycle` vs `context_cycle`) | High | Med | High |
| R-10 | `update_session_keywords` fire-and-forget spawn_blocking panics, poisoning the runtime or losing keywords silently | Low | Low | Low |
| R-11 | `is_valid_feature_id` visibility: cross-crate dependency or duplication leads to validation divergence between observe and server crates | Med | Med | Med |
| R-12 | cycle_stop observation event not queryable by retrospective pipeline due to missing feature_cycle in observation row | Med | Med | Med |

## Risk-to-Scenario Mapping

### R-01: Force-set overwrites correct heuristic attribution with wrong topic
**Severity**: High
**Likelihood**: Med
**Impact**: Session permanently attributed to wrong feature. Retrospective returns data for wrong feature; correct feature loses observation coverage.

**Test Scenarios**:
1. Session has correct feature_cycle from eager attribution. Agent calls `context_cycle(start, "wrong-feature")`. Verify `set_feature_force` overwrites to "wrong-feature" and `SetFeatureResult::Overridden` is returned with previous value logged at warn level.
2. Session has feature_cycle from SessionStart extra field. Agent calls `context_cycle(start)` with matching topic. Verify `SetFeatureResult::AlreadyMatches` returned, no overwrite.
3. Session has feature_cycle from SessionStart. Agent calls `context_cycle(start)` with different topic. Verify overwrite occurs and warn log emitted.

**Coverage Requirement**: Unit tests for all three `SetFeatureResult` variants. Integration test confirming SQLite persistence matches in-memory state after force-set.

### R-02: Hook validation failure silently drops cycle_start
**Severity**: High
**Likelihood**: Med
**Impact**: Agent receives MCP tool success response but hook-side attribution never happened. Session remains unattributed. No error visible to the calling agent.

**Test Scenarios**:
1. Hook receives PreToolUse with malformed `tool_input` (missing `topic` field). Verify event falls through to generic RecordEvent path, no panic, warning logged.
2. Hook receives PreToolUse with `topic` exceeding 128 chars. Verify `validate_cycle_params` rejects, hook falls through gracefully.
3. Hook receives PreToolUse with `type` = "restart" (invalid). Verify validation rejects, hook does not crash.
4. Verify that when hook validation fails, the generic RecordEvent handler still records the observation (defense-in-depth).

**Coverage Requirement**: Unit tests for each invalid input variant through `validate_cycle_params`. Integration test confirming hook process exits 0 on validation failure.

### R-03: SESSION_COLUMNS / session_from_row column index mismatch
**Severity**: High
**Likelihood**: Low
**Impact**: Every session read returns wrong data in every field after the mismatch point. Could corrupt feature_cycle, outcome, duration -- cascading through retrospective, status, and observation pipelines.

**Test Scenarios**:
1. Round-trip test: insert a session with all fields populated (including keywords), read it back, verify every field matches.
2. Insert a session without keywords (NULL), read back, verify `keywords` is `None` and all other fields are correct.
3. Verify `SESSION_COLUMNS` string count matches `session_from_row` column index accesses.

**Coverage Requirement**: Integration test with full round-trip. Compile-time or test-time assertion that SESSION_COLUMNS token count matches session_from_row expectations.

### R-04: Magic string event_type constant divergence
**Severity**: Med
**Likelihood**: Med
**Impact**: Hook sends "cycle_start" but listener matches on "cycle_begin" (or vice versa). Cycle events fall through to generic handler, losing force-set semantics. Attribution works via #198 fallback (set_feature_if_absent) but loses the explicit-wins guarantee.

**Test Scenarios**:
1. Integration test: hook builds a cycle_start RecordEvent, listener dispatches it. Verify the cycle_start-specific handler runs (not the generic handler).
2. Verify event_type constants are defined in a shared location and both hook and listener reference the same constants.

**Coverage Requirement**: End-to-end integration test from hook event construction through listener dispatch. Shared constant definition verified by compilation.

### R-05: Schema v12 migration on corrupted/partial state
**Severity**: Med
**Likelihood**: Low
**Impact**: Server fails to start. Existing data inaccessible until manual intervention.

**Test Scenarios**:
1. Run migration on a fresh v11 database. Verify v12 schema with keywords column present.
2. Run migration on a database that already has a keywords column (idempotency check or graceful error).
3. Verify existing session rows have `keywords = NULL` after migration.

**Coverage Requirement**: Migration integration test per established pattern (#681, #836).

### R-06: Keywords JSON serialization mismatch
**Severity**: Med
**Likelihood**: Med
**Impact**: Keywords stored but unreadable by future injection pipeline. Silent data corruption -- stored as string "keyword1" instead of `["keyword1"]`.

**Test Scenarios**:
1. Store keywords `["a", "b"]` via cycle_start path. Read session record. Deserialize keywords field as `Vec<String>`. Verify round-trip fidelity.
2. Store empty keywords `[]`. Verify stored as `"[]"` not NULL.
3. Store keywords with special characters (quotes, backslashes, unicode). Verify JSON escaping is correct.
4. Verify `Option<String>` field with NULL deserializes to `None`, not `Some("null")`.

**Coverage Requirement**: Unit test for serialization/deserialization. Integration test through full hook-to-persistence path.

### R-07: Concurrent set_feature_force race
**Severity**: Med
**Likelihood**: Low
**Impact**: Two concurrent cycle_start events for different topics on the same session. Last writer wins at the SessionRegistry level (in-memory), but SQLite persistence may store a different value if spawn_blocking tasks execute out of order.

**Test Scenarios**:
1. Call `set_feature_force` twice concurrently with different topics on the same session. Verify final in-memory state is deterministic (last call wins).
2. Verify SQLite persistence matches in-memory state after concurrent writes settle.

**Coverage Requirement**: Unit test with sequential calls (concurrent UDS is hard to test deterministically). Document the accepted race window.

### R-08: MCP tool response disconnected from hook-side reality
**Severity**: Med
**Likelihood**: High
**Impact**: Agent believes attribution succeeded (`was_set: true`) but the hook path failed (UDS timeout, server down). Agent proceeds without realizing session is unattributed.

**Test Scenarios**:
1. MCP tool returns success response. Verify response text explicitly states that attribution is best-effort and confirmed via retrospective, not the tool response.
2. Verify FR-19 `was_set` field semantics: the MCP tool has no session identity, so `was_set` cannot reflect actual attribution state. Verify the response does not claim definitive attribution.

**Coverage Requirement**: Unit test for response format. Documentation in tool description that response is acknowledgment only.

### R-09: Hook tool_name matching fails due to MCP prefix
**Severity**: High
**Likelihood**: Med
**Impact**: Hook never intercepts context_cycle calls. All cycle_start events are missed. Feature is completely non-functional despite MCP tool working.

**Test Scenarios**:
1. Simulate PreToolUse with `tool_name: "mcp__unimatrix__context_cycle"`. Verify hook detects and processes it.
2. Simulate PreToolUse with `tool_name: "context_cycle"` (no prefix). Verify hook detects and processes it.
3. Simulate PreToolUse with `tool_name: "other_server__context_cycle"`. Verify hook does NOT process it (wrong server).

**Coverage Requirement**: Unit test for tool_name matching logic with all known prefix patterns.

### R-10: Keywords spawn_blocking panic
**Severity**: Low
**Likelihood**: Low
**Impact**: Keywords lost for one session. Feature_cycle attribution unaffected (independent write).

**Test Scenarios**:
1. Pass malformed keywords JSON to `update_session_keywords`. Verify error is caught, not propagated as panic.

**Coverage Requirement**: Unit test for error path in persistence helper.

### R-11: is_valid_feature_id validation divergence
**Severity**: Med
**Likelihood**: Med
**Impact**: Hook accepts feature IDs that the observe crate rejects (or vice versa). Downstream processing fails on IDs that passed validation.

**Test Scenarios**:
1. If re-exported: verify `validate_cycle_params` calls the same `is_valid_feature_id` from `unimatrix-observe`.
2. If duplicated: verify both copies produce identical results for a set of edge-case inputs (empty, special chars, max length, valid patterns).

**Coverage Requirement**: Unit test with shared test vectors exercised against both validation paths.

### R-12: cycle_stop observation not queryable by retrospective
**Severity**: Med
**Likelihood**: Med
**Impact**: Retrospective cannot find cycle boundaries. The "stop" signal is recorded but invisible to downstream analysis.

**Test Scenarios**:
1. Record cycle_stop observation. Query observations by session_id and event_type "cycle_end". Verify the row exists with correct feature_cycle and timestamp.
2. Run `context_retrospective(feature_cycle: "X")` after cycle_start + observations + cycle_stop. Verify cycle_stop appears in the observation set.

**Coverage Requirement**: Integration test through retrospective pipeline.

## Integration Risks

- **Hook-to-Listener event_type contract**: The hook constructs events with string event_type values that the listener must match exactly. No compile-time enforcement exists. A typo or rename on either side silently breaks the cycle-specific code path while the generic fallback masks the failure.
- **MCP tool / hook timing assumption**: The architecture assumes PreToolUse fires before MCP server processes the call. If Claude Code changes this ordering (or processes them in parallel), the hook-side attribution and MCP-side response become inconsistent.
- **Cross-crate SessionRecord field addition**: Adding `keywords` to `SessionRecord` in `unimatrix-store` must be synchronized with `session_from_row` column indexing, `SESSION_COLUMNS` constant, `update_session` closure, and any `INSERT INTO sessions` statements. Missing one site causes silent data corruption.
- **Fire-and-forget observation gap**: Both `update_session_feature_cycle` and `update_session_keywords` are fire-and-forget. If the server is under load, spawn_blocking tasks may queue behind other work, creating a window where the session record is partially updated (feature_cycle set, keywords not yet written).

## Edge Cases

- **Empty keywords array**: `keywords: []` should be stored as `"[]"`, not NULL. Distinguish from omitted keywords (NULL).
- **Keywords with JSON-special characters**: Strings containing `"`, `\`, or unicode must survive JSON serialization round-trip.
- **Topic at exactly 128 characters**: Boundary -- must be accepted. 129 characters must be rejected.
- **Individual keyword at exactly 64 characters**: Boundary -- must be accepted. 65 characters must be truncated to 64.
- **Exactly 5 keywords**: Accepted as-is. 6 keywords truncated to first 5.
- **cycle_start on completed/timed-out session**: `set_feature_force` targets SessionRegistry (in-memory). If session already closed, it may not be in the registry. Verify graceful handling.
- **cycle_stop without prior cycle_start**: Should record boundary event regardless. No dependency on prior start.
- **Duplicate cycle_start calls**: Second call with same topic returns `AlreadyMatches`. Second call with different topic overwrites (ADR-002).
- **NULL/missing tool_input in PreToolUse**: Hook must not panic when `tool_input` JSON is absent or malformed.

## Security Risks

- **Untrusted input: `topic` parameter**: Accepts arbitrary string from agent. Mitigated by `sanitize_metadata_field()` (strips control chars, enforces length). Blast radius if bypassed: arbitrary strings stored in sessions table feature_cycle column, potentially breaking retrospective queries or causing injection in future SQL queries that interpolate the value.
- **Untrusted input: `keywords` array**: Up to 5 strings of 64 chars each. Stored as JSON in TEXT column. Blast radius if validation bypassed: oversized JSON blob in sessions table. Mitigated by truncation in `validate_cycle_params()`.
- **Untrusted input: `type` parameter**: Enum-validated to "start"/"stop". No injection surface beyond validation bypass causing unexpected code paths.
- **Path traversal / injection**: Not applicable -- no file paths or SQL interpolation. All persistence uses parameterized queries.
- **Capability check**: `context_cycle` requires `SessionWrite` capability. Verify this is enforced. An agent without `SessionWrite` must not be able to set feature attribution.
- **Force-set as attribution hijack**: `set_feature_force` allows any caller with SessionWrite to overwrite attribution. In the current architecture, only hook connections have SessionWrite, so MCP-connected agents cannot force-set directly. Verify this boundary holds.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| UDS transport timeout on cycle_start | Hook exits 0. Attribution lost. Eager attribution takes over as fallback. Agent sees MCP success response (misleading). |
| Server unavailable when hook fires | Same as timeout. Fire-and-forget semantics accepted. |
| Schema migration v12 fails at startup | Server refuses to start. Standard migration error propagation. No data loss (ALTER TABLE is atomic in SQLite). |
| `set_feature_force` on unknown session_id | Returns gracefully (session not in registry). Observation still recorded. Keywords not persisted (no session record to update). |
| Keywords persistence fails (spawn_blocking) | Warning logged. Feature_cycle attribution succeeds independently. Keywords NULL for this session. |
| Malformed tool_input JSON in PreToolUse | Hook falls through to generic RecordEvent path. Observation recorded without cycle-specific handling. Warning logged. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-01 | ADR-002 introduces `set_feature_force` so explicit signal always wins over heuristic. Race condition eliminated. Risk shifts to wrong-topic overwrite (R-01). |
| SR-02 | -- | Addressed by architecture: cycle path is a single fire-and-forget UDS write with no response wait. <5ms marginal cost within 50ms budget. Verify via NFR-01 latency test. |
| SR-03 | R-04 | ADR-001 reuses RecordEvent. Backward compat maintained: unknown event_type falls through to generic handler. Risk shifts to magic-string coupling (R-04). |
| SR-04 | -- | Specification defines follow-up issue creation as definition of done. Out of architecture scope. Accepted. |
| SR-05 | R-06 | ADR-003 chose JSON column. Storage supports future injection reads. Risk shifts to serialization fidelity (R-06). |
| SR-06 | R-12 | cycle_stop records observation event. Architecture relies on retrospective querying observations by session. Risk: boundary event may not be queryable (R-12). |
| SR-07 | R-04, R-11 | ADR-004 creates shared `validate_cycle_params()`. Validation divergence eliminated. Residual risk: event_type string constants (R-04) and `is_valid_feature_id` visibility (R-11). |
| SR-08 | R-08 | Architecture accepts MCP tool cannot confirm attribution (no session identity). Response is acknowledgment only. Risk: agent misinterprets response (R-08). |
| SR-09 | R-03, R-05 | ADR-005 uses minimal ALTER TABLE migration. SESSION_COLUMNS/session_from_row coupling is the residual risk (R-03). Migration idempotency risk (R-05). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-02, R-08, R-09) | 12 scenarios |
| Medium | 6 (R-04, R-05, R-06, R-07, R-11, R-12) | 13 scenarios |
| Low | 2 (R-03, R-10) | 4 scenarios |
| **Total** | **12** | **29 scenarios** |
