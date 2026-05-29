# Risk-Based Test Strategy: vnc-022

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | ObserveContext field set diverges from dispatch_request parameter list — missing or stale Arc field causes runtime panic or silent pipeline bypass | High | Med | High |
| R-02 | dispatch_request capability parameter refactor regresses UDS path — missed uds_has_capability replacement or wrong capabilities slice silently breaks local hooks | High | Med | High |
| R-03 | Session ID prefix ("http-") not applied before sanitize_session_id — unsanitized or unprefixed session_id propagates into SessionRegistry and audit context | High | Med | High |
| R-04 | HookResponse-to-HTTP mapping returns wrong status code — client fire-and-forget logic relies on 204 vs 200 distinction, wrong mapping breaks hook stdout flow | Med | Med | Med |
| R-05 | Body size limit not enforced on stream layer — Content-Length header check only, allowing chunked/streaming requests to bypass the 1MB limit | High | Low | Med |
| R-06 | ResolvedIdentity missing SessionWrite capability — dispatch_request arms silently reject session operations, returning Error instead of processing events | High | Med | High |
| R-07 | CompactPayload transcript_excerpt field breaks backward compatibility — existing UDS callers that omit the field fail deserialization if serde(default) annotation is wrong | Med | Low | Low |
| R-08 | Concurrent sessions with same bearer token produce cross-session state bleed in SessionRegistry | High | Low | Med |
| R-09 | Malformed JSON deserialization error leaks internal serde detail to client in 400 response body | Low | Med | Low |
| R-10 | dispatch_request warn+continue arms (side-effect failures) lack test coverage — failure paths masked by passing behavior, historically omitted (ref #4473) | Med | High | High |
| R-11 | PathRouter Clone impl breaks when ObserveContext is added — tower Service requires Clone, and ObserveContext must derive Clone correctly for all Arc fields | Med | Med | Med |
| R-12 | observe_response_to_http serialization panics on edge-case HookResponse variants — BriefingContent with empty string or Entries with zero items | Low | Low | Low |
| R-13 | Audit log for /observe events missing or structurally different from MCP audit events — credential_type or agent_id mismatch breaks audit trail consistency | Med | Med | Med |
| R-14 | sanitize_session_id rejects prefixed session_id — the "http-" prefix plus client UUID exceeds 128-char limit or hits unexpected character validation | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: ObserveContext field set diverges from dispatch_request parameters
**Severity**: High
**Likelihood**: Med
**Impact**: If ObserveContext is missing a field (e.g., pending_entries_analysis), the /observe handler either fails to compile (best case) or passes a zero-value/default that causes silent pipeline misbehavior (worst case if using Option wrappers).

**Test Scenarios**:
1. Integration test calls /observe with a ContextSearch request and verifies the full pipeline executes (embed, vector search, ranking) — exercises all 10 service handles end-to-end
2. Integration test calls /observe with SessionRegister and verifies session appears in SessionRegistry — exercises session_registry handle
3. Integration test calls /observe with CompactPayload and verifies BriefingContent response — exercises adapt_service and services handles

**Coverage Requirement**: At least one test per service handle must exercise the full pipeline path through /observe to confirm the handle is correctly wired.

### R-02: dispatch_request capability refactor regresses UDS path
**Severity**: High
**Likelihood**: Med
**Impact**: Local `unimatrix hook` silently breaks. Users on local deployments lose the intelligence pipeline. This is the #1 regression risk — the function has 9 capability check sites.

**Test Scenarios**:
1. All existing UDS integration tests pass unchanged after the refactor (AC-18)
2. New unit test verifies `capabilities.contains(&Capability::SessionWrite)` returns true for UDS_CAPABILITIES
3. Grep audit: zero remaining references to `uds_has_capability` inside `dispatch_request` body after refactor

**Coverage Requirement**: Existing UDS test suite must pass with zero modifications. A grep-based assertion confirms no stale `uds_has_capability` calls remain in `dispatch_request`.

### R-03: Session ID prefix not applied or applied incorrectly
**Severity**: High
**Likelihood**: Med
**Impact**: HTTP and UDS sessions share the same namespace — a remote session could collide with or read state from a local session. Per #3902, unsanitized session_id propagates into AuditContext strings and registry reads.

**Test Scenarios**:
1. Integration test sends SessionRegister via HTTP, then queries SessionRegistry and verifies the stored session_id has "http-" prefix
2. Integration test sends same client session_id via both HTTP and UDS (if test infra supports), verifies they map to different SessionRegistry entries
3. Unit test verifies prefixed session_id passes sanitize_session_id validation
4. Integration test verifies prefixed session_id appears in audit log entries

**Coverage Requirement**: Every HTTP /observe test must verify the session_id stored in SessionRegistry carries the "http-" prefix.

### R-04: HookResponse-to-HTTP mapping returns wrong status code
**Severity**: Med
**Likelihood**: Med
**Impact**: Fire-and-forget clients checking `status == 204` would see 200 and potentially try to parse an empty body. Sync clients expecting 200 with JSON would get 204 and fail on empty body.

**Test Scenarios**:
1. Test Ack response maps to 204 with empty body (fire-and-forget event)
2. Test Entries response maps to 200 with JSON body containing "type":"Entries"
3. Test BriefingContent response maps to 200 with JSON body containing "type":"BriefingContent"
4. Test Error response maps to 400 with JSON body containing "type":"Error"
5. Test Pong response maps to 200 with JSON body containing "type":"Pong"

**Coverage Requirement**: Unit test on `observe_response_to_http` covers all 5 HookResponse variants with status code and Content-Type assertions.

### R-05: Body size limit bypassed on streaming requests
**Severity**: High
**Likelihood**: Low
**Impact**: Unbounded body consumption could OOM the server. The MCP path already uses two-layer enforcement (Content-Length + Limited stream); the /observe handler must replicate both.

**Test Scenarios**:
1. Integration test sends request with Content-Length > 1MB, verifies 413 response
2. Integration test sends request without Content-Length header but with body > 1MB (chunked), verifies 413 response from Limited layer
3. Integration test sends request at exactly 1MB boundary, verifies acceptance

**Coverage Requirement**: Both Content-Length fast-path and stream-level Limited enforcement tested independently.

### R-06: ResolvedIdentity missing SessionWrite capability
**Severity**: High
**Likelihood**: Med
**Impact**: All session-mutating operations (SessionRegister, SessionClose, RecordEvent, cycle_start/stop) silently fail with HookResponse::Error. The /observe endpoint appears to work (returns 400 with error JSON) but no events are processed.

**Test Scenarios**:
1. Integration test sends SessionRegister via HTTP and verifies 204 (not 400) — proves SessionWrite is present
2. Unit test on StaticTokenValidator verifies the returned ResolvedIdentity.capabilities includes Capability::SessionWrite
3. Integration test sends RecordEvent via HTTP and verifies observation persisted to database (AC-02)

**Coverage Requirement**: At least one test verifies a session-mutating operation succeeds via HTTP (not just that the endpoint responds).

### R-07: CompactPayload transcript_excerpt breaks backward compat
**Severity**: Med
**Likelihood**: Low
**Impact**: Existing UDS callers sending CompactPayload without transcript_excerpt fail deserialization if serde(default) is missing.

**Test Scenarios**:
1. Unit test: deserialize CompactPayload JSON without transcript_excerpt field — must succeed with None
2. Unit test: deserialize CompactPayload JSON with transcript_excerpt: "text" — must succeed with Some("text")
3. Unit test: serialize CompactPayload with transcript_excerpt: None — field must be absent from JSON output (skip_serializing_if)

**Coverage Requirement**: Wire type round-trip test with and without the field (AC-09).

### R-08: Concurrent session cross-bleed
**Severity**: High
**Likelihood**: Low
**Impact**: Events from session A modify session B's state in SessionRegistry. Per #4354, shared state with concurrent sessions causes cross-session bleed.

**Test Scenarios**:
1. Integration test registers two sessions with same bearer token but different session_ids, sends events to each, verifies independent state (AC-14)
2. Integration test sends ContextSearch for session A, verifies injection history only reflects session A's prior events

**Coverage Requirement**: At least one concurrent-session test proving isolation.

### R-09: Serde error detail leaked in 400 response
**Severity**: Low
**Likelihood**: Med
**Impact**: Internal type names, field names, and enum variant structure leaked to external clients. Minor information disclosure.

**Test Scenarios**:
1. Integration test sends `{"type":"Bogus"}`, verifies 400 response body contains an error message but not internal Rust type paths
2. Integration test sends truncated JSON, verifies error message is generic

**Coverage Requirement**: At least one malformed-body test inspects the error message for internal detail leakage.

### R-10: Warn+continue failure paths lack test coverage
**Severity**: Med
**Likelihood**: High
**Impact**: Per #4473, dispatch_request arms that use warn+continue for side-effect failures are the most commonly omitted tests. The HTTP path inherits all these arms. Missing tests mean silent pipeline degradation goes undetected.

**Test Scenarios**:
1. Test RecordEvent with an invalid session_id (unregistered session) — verify warn+continue behavior: event still acknowledged (204), warning logged
2. Test ContextSearch when embed_service is unavailable — verify graceful degradation (empty entries or error, not panic)
3. Test CompactPayload when adapt_service returns error — verify BriefingContent fallback or error response

**Coverage Requirement**: At least one warn+continue failure path tested per dispatch arm category (session ops, search ops, briefing ops).

### R-11: PathRouter Clone breaks with ObserveContext
**Severity**: Med
**Likelihood**: Med
**Impact**: tower::Service requires Clone. If ObserveContext doesn't derive Clone (e.g., a field type doesn't implement Clone), PathRouter fails to compile as a tower Service. If it compiles but cloning is expensive, it degrades throughput.

**Test Scenarios**:
1. Compilation test: PathRouter with ObserveContext compiles and implements tower::Service
2. Integration test: multiple concurrent /observe requests succeed (proves Clone works at runtime)

**Coverage Requirement**: Compilation is the primary gate. Concurrent request test provides runtime confirmation.

### R-13: Audit log inconsistency for /observe events
**Severity**: Med
**Likelihood**: Med
**Impact**: Audit events from /observe are structurally different from MCP audit events — breaks audit queries, compliance reporting, log analysis.

**Test Scenarios**:
1. Integration test sends RecordEvent via HTTP, queries audit log, verifies credential_type = "static_token"
2. Integration test verifies audit entry agent_id = "http-bearer"
3. Integration test compares audit entry structure from /observe event with an MCP tool call audit entry — same fields present

**Coverage Requirement**: At least one audit-trail assertion per /observe integration test.

### R-14: sanitize_session_id rejects prefixed session_id
**Severity**: Med
**Likelihood**: Low
**Impact**: All HTTP sessions rejected at the sanitize_session_id gate because the "http-" prefix plus client session_id exceeds 128 chars or the hyphen is somehow invalid.

**Test Scenarios**:
1. Unit test: "http-" + 36-char UUID passes sanitize_session_id (41 chars total, well under 128)
2. Unit test: "http-" + 123-char string (max remaining) passes sanitize_session_id (128 chars total)
3. Unit test: "http-" + 124-char string (129 total) fails sanitize_session_id

**Coverage Requirement**: Boundary test on prefixed session_id length.

## Integration Risks

1. **ObserveContext construction in main.rs** — The context is built from UnimatrixServer fields before rmcp wrapping. If the server construction order changes (e.g., a field is initialized lazily after rmcp takes ownership), ObserveContext holds a stale or uninitialized handle. Test: end-to-end integration test that boots the server and calls /observe.

2. **dispatch_request call site divergence** — Two call sites (UDS line ~478, HTTP handler) must pass parameters in identical order. A reordering in one site silently compiles if types happen to match (e.g., both `store` and `entry_store` are `Arc<Store>`). Test: type-distinct parameter assertions or a context struct that eliminates positional risk (ADR-001 mitigates this).

3. **StaticTokenAuth middleware ordering** — The /observe handler extracts ResolvedIdentity from request extensions. If middleware ordering changes and StaticTokenAuth runs after PathRouter (or not at all for /observe), the extension is missing and the handler panics on unwrap. Test: integration test with valid token verifies 204 (not 500). Integration test without token verifies 401 (not panic).

4. **Body collection race with tower Service poll_ready** — PathRouter implements tower::Service manually. If body collection (via Limited + collect) is done outside the future returned by call(), it may violate tower's readiness contract. Test: concurrent requests under load.

5. **HookRequest serde tag discrimination** — The `#[serde(tag = "type")]` discriminator must match exactly. If a client sends `"type": "sessionRegister"` (wrong case) or `"type": "session_register"` (snake_case), deserialization fails with a confusing error. Test: case-sensitivity test for each wire type variant name.

## Edge Cases

1. **Empty request body** — POST /observe with Content-Length: 0 or empty body. Should return 400 (deserialization failure), not 500.
2. **Valid JSON but wrong schema** — `{"foo": "bar"}` — valid JSON but not a HookRequest variant. Should return 400.
3. **RecordEvent with empty payload** — `{"type": "RecordEvent", "event_type": "PreToolUse", "session_id": "x", "timestamp": 0, "payload": {}}`. Should process without panic.
4. **ContextSearch with empty query** — Should return Entries with zero items or Error, not panic.
5. **CompactPayload with empty injected_entry_ids** — `[]` array. Should return BriefingContent (possibly minimal).
6. **Session ID at exactly 128 chars after prefix** — Boundary: "http-" (5) + 123 chars = 128. Must pass validation.
7. **Session ID with only prefix** — "http-" with empty client portion. Should fail validation (empty client ID is meaningless).
8. **Multiple rapid SessionRegister for same session_id** — Duplicate registration. Should be idempotent or return error, not corrupt registry state.
9. **SessionClose for unregistered session** — Should handle gracefully (warn+continue per existing dispatch behavior).
10. **Request with Content-Type other than application/json** — e.g., text/plain. Should still attempt deserialization (wire format is JSON regardless of Content-Type) or reject explicitly.

## Security Risks

### Untrusted Input Assessment

| Input Surface | Untrusted Data | Damage Potential | Blast Radius |
|---------------|---------------|------------------|-------------- |
| Request body (HookRequest JSON) | Full JSON payload from authenticated client | Malformed payloads could trigger panic in deserialization or pipeline processing | dispatch_request and all downstream services |
| session_id field | Client-generated string | Session hijacking (cross-user), registry pollution, audit log injection via unsanitized strings | SessionRegistry, AuditContext |
| query field (ContextSearch) | Arbitrary text | SQL injection via SQLite FTS if not parameterized; excessive embedding computation | Store, VectorStore, EmbedService |
| payload field (RecordEvent) | Arbitrary JSON object | Stored verbatim — could contain oversized data within the 1MB body limit | Store (observation persistence) |
| transcript_excerpt (CompactPayload) | Optional string | Could contain arbitrary large text within body limit; stored or processed without separate size validation | AdaptationService |
| Bearer token | Hex string in Authorization header | Brute-force enumeration if no rate limiting on /observe | Auth middleware, all downstream |

### Specific Security Scenarios

1. **Path traversal in cwd field** (SessionRegister): The `cwd` field is a file path string. If stored and later used in file operations without sanitization, it could enable path traversal. Verify `cwd` is treated as opaque metadata, never used for file access.

2. **Session ID injection** (#3902): Per historical lesson, unsanitized session_id propagates into AuditContext strings. The "http-" prefix must be applied BEFORE sanitize_session_id, and sanitize_session_id must run on the full prefixed value.

3. **Payload size within body limit**: A RecordEvent with a 900KB payload field is within the 1MB body limit but could be stored in SQLite, impacting database size and query performance. No per-field size limit exists.

4. **Serde deserialization complexity**: Deeply nested JSON or very long string fields within the 1MB limit could cause excessive allocation during deserialization. The serde_json default recursion limit (128) mitigates deep nesting, but long strings are unbounded within the body limit.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| dispatch_request panics | Tower catches panic, returns 500 Internal Server Error. Client retries (sync) or drops (fire-and-forget). Server continues serving. | Automatic — tokio task boundary isolates the panic |
| EmbedService unavailable | ContextSearch returns empty Entries or Error. Session continues. | Automatic — next request retries embed |
| SessionRegistry lock poisoned | All session operations fail with Error response. Server requires restart. | Manual restart required. Risk is low (lock poisoning requires panic while holding lock). |
| Database write failure | RecordEvent observation lost. 204 still returned (fire-and-forget semantics). | Silent data loss. Logged at WARN level. No client-visible impact. |
| Body read timeout/failure | Hyper returns error during body collection. Handler returns 500. | Automatic — connection-level failure, client retries |
| StaticTokenAuth middleware missing from chain | ResolvedIdentity not in extensions. Handler panics on unwrap/expect. | Server returns 500. Detectable only via integration test. |
| TLS handshake failure | Connection never reaches /observe handler. Client sees connection error. | Client-side — retry or fix TLS config |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (dispatch_request 10-param sprawl) | R-01, R-11 | ADR-001: ObserveContext struct bundles params. R-01 tests wiring. R-11 tests Clone derivation. |
| SR-02 (PreCompact degradation invisible) | — | Accepted for Day 1. Spec explicitly documents briefing-only. No degradation signal in response (follow-on if needed). |
| SR-03 (session_id hijacking) | R-03, R-08, R-14 | ADR-003: "http-" prefix scopes transport. R-03 tests prefix application. R-08 tests concurrent isolation. R-14 tests sanitize_session_id compatibility. |
| SR-04 (contract drift across 3 clients) | R-04 | ADR-004: Explicit status code mapping. R-04 tests all 5 response variants. Wire contract documented in spec. |
| SR-05 (nice-to-have event dependency gap) | — | Architecture explicitly analyzed event dependencies (Section "Event Dependency Analysis"). No critical-path event depends on deferred events. Accepted. |
| SR-06 (fire-and-forget data loss) | R-10 | Architecture documents fire-and-forget semantics as acceptable (Section "Fire-and-Forget Semantics"). R-10 tests warn+continue paths to ensure graceful degradation. |
| SR-07 (PathRouter cannot reach service handles) | R-01, R-11 | ADR-001: ObserveContext constructed in main.rs, stored on PathRouter. R-01 tests end-to-end wiring. R-11 tests Clone. |
| SR-08 (ResolvedIdentity missing SessionWrite) | R-06 | Architecture adds SessionWrite to HTTP capability set (C4). R-06 tests session-mutating operations succeed. |
| SR-09 (UDS regression from dispatch_request refactor) | R-02 | ADR-002: Mechanical refactor, UDS passes UDS_CAPABILITIES. R-02 tests all existing UDS tests pass unchanged. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 0 | 0 scenarios |
| High | 5 (R-01, R-02, R-03, R-06, R-10) | 15 scenarios |
| Medium | 5 (R-04, R-05, R-08, R-11, R-13) | 14 scenarios |
| Low | 4 (R-07, R-09, R-12, R-14) | 9 scenarios |
