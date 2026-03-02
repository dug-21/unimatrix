# Risk-Based Test Strategy: col-007

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | UDS ContextSearch returns different results than MCP context_search for the same query (pipeline drift) | High | Medium | High |
| R-02 | Async dispatch migration breaks existing UDS handlers (Ping, SessionRegister, SessionClose, RecordEvent) | Medium | Low | Low |
| R-03 | Injection formatting exceeds byte budget due to multi-byte UTF-8 characters in entry content | Medium | Medium | Medium |
| R-04 | Similarity floor (0.5) or confidence floor (0.3) filters out all results, causing silent injection failure for most prompts | Medium | Medium | Medium |
| R-05 | Cold ONNX path still reachable if SessionStart fires after first UserPromptSubmit (race condition) | High | Low | Medium |
| R-06 | Co-access dedup HashMap grows unbounded from sessions without SessionClose | Low | Medium | Low |
| R-07 | HookInput.prompt field conflicts with HookInput.extra flatten for unknown fields | Medium | Low | Low |
| R-08 | Hook process blocks on UDS read beyond 40ms timeout when server is under load | High | Low | Medium |
| R-09 | Entry content contains markdown/formatting that disrupts Claude's parsing of injected context | Medium | Medium | Medium |
| R-10 | ContextSearch request with very long prompt (>10KB) causes oversized embedding or payload | Medium | Low | Low |
| R-11 | Server-side embedding spawn_blocking blocks the tokio runtime when many concurrent ContextSearch requests arrive | High | Low | Medium |
| R-12 | Warming embed_entry call on SessionStart fails and leaves embed service in Failed state permanently | Medium | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Pipeline Drift Between MCP and UDS Search

**Severity**: High
**Likelihood**: Medium
**Impact**: Agents receive different knowledge via hooks than via explicit MCP calls. Inconsistent behavior undermines trust in the system.

**Test Scenarios**:
1. Same query via MCP `context_search` and UDS ContextSearch returns identical entry IDs in identical order
2. Same query with metadata filters (topic, category) produces identical results
3. Re-ranking scores match between MCP and UDS (verify blended score formula is identical)
4. Co-access boost produces identical ordering between MCP and UDS for the same anchor entries

**Coverage Requirement**: Integration test with populated knowledge base comparing MCP and UDS results for 3+ diverse queries. Entry IDs and order must match exactly.

### R-02: Async Dispatch Breaks Existing Handlers

**Severity**: Medium
**Likelihood**: Low
**Impact**: Existing hook operations (Ping, SessionRegister, SessionClose) fail or return wrong responses.

**Test Scenarios**:
1. Ping request still returns Pong with server version
2. SessionRegister still returns Ack
3. SessionClose still returns Ack
4. RecordEvent still returns Ack
5. RecordEvents batch still returns Ack
6. Unknown request type still returns Error with ERR_UNKNOWN_REQUEST

**Coverage Requirement**: Existing `dispatch_*` unit tests updated with `.await` pass unchanged. All 6 handler variants verified.

### R-03: Byte Budget Overflow with Multi-Byte Characters

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Injected output exceeds token budget, causing context bloat.

**Test Scenarios**:
1. Entry with ASCII-only content respects byte budget
2. Entry with CJK characters (3 bytes each) respects byte budget
3. Entry with emoji (4 bytes each) respects byte budget
4. Truncation at byte boundary does not produce invalid UTF-8
5. Entry list with mixed character widths correctly accumulates byte count

**Coverage Requirement**: Unit tests for `format_injection()` with multi-byte content. Verify output.len() <= MAX_INJECTION_BYTES and output is valid UTF-8.

### R-04: Threshold Filters Suppress All Results

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Most prompts get no injected knowledge, defeating the feature's purpose. Users see no difference with col-007 enabled.

**Test Scenarios**:
1. Entries with similarity > 0.5 and confidence > 0.3 are included
2. Entry with similarity 0.49 is excluded (below floor)
3. Entry with confidence 0.29 is excluded (below floor)
4. Prompt with no semantically related entries returns empty results
5. Prompt with entries at exactly the threshold boundaries (0.5 similarity, 0.3 confidence) are included

**Coverage Requirement**: Unit tests with controlled similarity and confidence values. Integration test with real embeddings to verify thresholds are not too aggressive for the current knowledge base.

### R-05: SessionStart/UserPromptSubmit Race Condition

**Severity**: High
**Likelihood**: Low
**Impact**: First prompt in a session hits cold ONNX (~200ms), blowing the 50ms budget. User experiences a noticeable delay on their first prompt.

**Test Scenarios**:
1. SessionRegister followed by ContextSearch: ContextSearch returns results (model is warm)
2. ContextSearch without prior SessionRegister: returns empty Entries (EmbedNotReady), not an error
3. Two concurrent SessionRegister requests: both complete successfully (warming is idempotent)
4. ContextSearch immediately after SessionRegister (minimal gap): verify model readiness

**Coverage Requirement**: Integration test simulating the SessionStart -> UserPromptSubmit sequence with timing verification. Verify that EmbedNotReady results in silent skip (empty Entries, exit 0).

### R-06: Co-Access Dedup Memory Leak

**Severity**: Low
**Likelihood**: Medium
**Impact**: Slow memory growth if sessions are never properly closed. Negligible for typical usage (~200KB max) but unbounded in theory.

**Test Scenarios**:
1. SessionClose clears the dedup set for that session
2. Multiple sessions each maintain independent dedup sets
3. Dedup set correctly identifies duplicate entry vectors

**Coverage Requirement**: Unit tests for CoAccessDedup: insert, check, clear operations. Verify no entries remain after SessionClose.

### R-07: HookInput.prompt vs Extra Flatten Conflict

**Severity**: Medium
**Likelihood**: Low
**Impact**: The `prompt` field could appear in both the named field and the `extra` flatten map, or the named field could shadow an unexpected field.

**Test Scenarios**:
1. JSON with `prompt` field: named field is populated, not in `extra`
2. JSON without `prompt` field: named field is None, no phantom value
3. JSON with empty `prompt`: named field is Some("")
4. JSON with both `prompt` and other unknown fields: `prompt` in named field, others in `extra`

**Coverage Requirement**: Unit tests for HookInput deserialization covering all 4 scenarios. Verify serde behavior matches expectations.

### R-08: UDS Timeout Under Server Load

**Severity**: High
**Likelihood**: Low
**Impact**: Hook process blocks for 40ms+ waiting for server response, exceeding the 50ms total budget. Claude Code may kill the hook process.

**Test Scenarios**:
1. ContextSearch under normal conditions completes within 40ms
2. Hook process handles transport timeout gracefully (no stdout, exit 0)
3. Server under load with 5 concurrent ContextSearch requests: all complete or timeout cleanly

**Coverage Requirement**: Latency benchmark test. Timeout behavior unit test (mock transport with delayed response).

### R-09: Entry Content Disrupts Claude's Context Parsing

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Injected markdown headings, code blocks, or special characters could confuse Claude about message boundaries or instruction hierarchy.

**Test Scenarios**:
1. Entry with markdown headings (##, ###) in content: formatted output is still parseable
2. Entry with code blocks (```) in content: code block boundaries do not interfere
3. Entry with XML-like tags in content: no injection risk
4. Entry with very long single line: does not break stdout buffering

**Coverage Requirement**: Unit test for `format_injection()` with adversarial content (nested formatting, special characters). Verify output is well-formed.

### R-10: Oversized Prompt Input

**Severity**: Medium
**Likelihood**: Low
**Impact**: Very long prompts cause large embeddings or oversized wire protocol payloads.

**Test Scenarios**:
1. Prompt > 10KB: embedding still works (ONNX truncates to model's max sequence length)
2. ContextSearch request with 100KB query: wire protocol accepts (under 1 MiB MAX_PAYLOAD_SIZE)
3. Empty prompt: returns empty results (no embedding crash)

**Coverage Requirement**: Unit tests for edge cases. Verify the embedding model's truncation behavior.

### R-11: Concurrent ContextSearch Exhausts Spawn_Blocking Pool

**Severity**: High
**Likelihood**: Low
**Impact**: Multiple concurrent embedding operations block the tokio runtime's spawn_blocking pool, stalling all UDS and MCP operations.

**Test Scenarios**:
1. Single ContextSearch completes normally
2. 3 concurrent ContextSearch requests from different sessions: all complete or timeout independently
3. ContextSearch during an MCP context_search call: both complete independently

**Coverage Requirement**: Integration test with concurrent UDS requests. Verify no deadlock or unbounded blocking.

### R-12: SessionStart Warming Failure

**Severity**: Medium
**Likelihood**: Low
**Impact**: If the warmup embed_entry call fails, the embed service remains in Ready state (the adapter loaded successfully, only the warmup call failed). Subsequent ContextSearch requests work normally because they call embed_entry themselves.

**Test Scenarios**:
1. SessionRegister with healthy embed service: warming completes, subsequent ContextSearch works
2. SessionRegister with embed service in Failed state: warning logged, Ack returned, ContextSearch returns empty results
3. SessionRegister when embed service is still Loading: blocks until Ready or Failed

**Coverage Requirement**: Unit test mocking EmbedServiceHandle states (Ready, Loading, Failed). Verify each path.

## Integration Risks

- **MCP-UDS result equivalence**: The duplicated search pipeline orchestration (ADR-001) means changes to the MCP pipeline must be mirrored in the UDS pipeline. Test for equivalence.
- **start_uds_listener() signature expansion**: Adding 4 new Arc parameters to the function changes all call sites. Currently only one call site (main.rs) but future refactoring must update it.
- **Async dispatch cascading**: Making `dispatch_request()` async changes `handle_connection()` and the test helpers. All existing dispatch tests need `.await` added.

## Edge Cases

- Empty knowledge base (0 entries): ContextSearch returns empty Entries, hook exits with no stdout
- Knowledge base with only quarantined entries: same as empty
- Prompt that is a single character: embedding succeeds (ONNX pads), search returns results based on that embedding
- Prompt that is binary data / non-UTF-8: serde default parsing converts to empty string (ADR-006)
- Session ID collision between concurrent Claude Code sessions: each session gets independent dedup sets (keyed by session_id)
- Rapid sequential UserPromptSubmit events (paste multiple lines): each fires independently, server handles concurrently

## Security Risks

- **Prompt injection via hook stdin**: The prompt field is used as a search query only (embedded into a vector). No SQL-like injection risk. The prompt is not executed or stored.
- **Entry content injection via stdout**: Injected entry content could contain instructions that manipulate Claude. Mitigation: entries are created via authenticated MCP tools with capability checks. Content from the knowledge base is trusted.
- **UDS authentication**: Inherited from col-006 Layer 2 (UID verification). No change.
- **Oversized payloads**: Wire protocol enforces MAX_PAYLOAD_SIZE (1 MiB). Prompt truncation by ONNX model prevents embedding oversized inputs.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Server not running | Hook exits 0, no stdout | Normal -- pre-col-007 behavior |
| Embed service not ready | Empty Entries response | SessionStart warming resolves |
| Embed service failed | Empty Entries response, warning logged | Manual: restart server |
| HNSW search returns 0 results | Empty Entries response | Normal -- no relevant knowledge |
| redb read error | Error response, hook exits 0 | Manual: check database integrity |
| UDS timeout | Hook exits 0, no stdout | Transient -- retry on next prompt |
| Co-access write error | Warning logged, ContextSearch response unaffected | Self-healing -- next session |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (search pipeline extraction breaks MCP) | R-01 (pipeline drift) | Eliminated by ADR-001: no extraction. Pipeline is duplicated, not shared. Drift risk remains but is lower severity than extraction regression. |
| SR-02 (UDS shared state coupling) | -- | Resolved by ADR-001: parameter expansion, no shared function. Clean crate boundaries preserved. |
| SR-03 (token budget heuristic) | R-03 (byte budget overflow) | Accepted: budget defined in bytes (MAX_INJECTION_BYTES = 1400), not tokens. Multi-byte character handling tested. |
| SR-04 (cold ONNX pre-warming) | R-05 (SessionStart race) | Resolved: blocking pre-warm on SessionStart (ADR implied by architecture). EmbedNotReady returns empty results. |
| SR-05 (injection recording divergence) | -- | Resolved: injection recording deferred to col-010. Not in col-007 scope. |
| SR-06 (co-access dedup) | R-06 (memory leak) | Resolved by ADR-003: session-scoped dedup with SessionClose cleanup. |
| SR-07 (arbitrary thresholds) | R-04 (threshold suppression) | Accepted: thresholds are compile-time constants, tunable. Integration test verifies thresholds work with current knowledge base. |
| SR-08 (async dispatch) | R-02 (breaks existing handlers) | Resolved by ADR-002: fully async dispatch, mechanical change. Existing tests updated with .await. |
| SR-09 (HookInput.prompt) | R-07 (flatten conflict) | Resolved: named field with #[serde(default)], tested for all deserialization cases. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-05, R-08, R-11) | 14 scenarios |
| Medium | 6 (R-03, R-04, R-09, R-10, R-12, R-02) | 22 scenarios |
| Low | 2 (R-06, R-07) | 7 scenarios |
| **Total** | **12** | **43 scenarios** |
