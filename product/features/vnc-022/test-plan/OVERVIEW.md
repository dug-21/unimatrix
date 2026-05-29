# vnc-022 Test Strategy Overview

## Test Levels

| Level | Scope | Runner |
|-------|-------|--------|
| Unit | Wire type serde, response mapping, capability checks, session ID prefix logic | `cargo test --workspace` |
| Integration (Rust) | Full /observe endpoint with mock or real service layer, dispatch_request call path | `cargo test --workspace` (in-process HTTP) |
| Integration (infra-001) | Binary-level MCP protocol suites; smoke gate + relevant suites | `pytest suites/ -v -m smoke` |

## Risk-to-Test Mapping

| Risk | Priority | Component | Test Approach |
|------|----------|-----------|---------------|
| R-01 (ObserveContext field divergence) | High | observe-context, observe-handler | Integration: call /observe with ContextSearch, SessionRegister, CompactPayload; each exercises different service handles end-to-end |
| R-02 (UDS regression from refactor) | High | dispatch-request-refactor | All existing `cargo test` UDS tests must pass unchanged; grep audit for stale `uds_has_capability` in dispatch_request |
| R-03 (Session ID prefix missing) | High | observe-handler | Integration: send SessionRegister via HTTP, verify stored session_id has "http-" prefix; unit: prefixed ID passes sanitize_session_id |
| R-04 (Wrong HTTP status mapping) | Med | observe-handler | Unit: `observe_response_to_http` tested for all 5 HookResponse variants with status + Content-Type assertions |
| R-05 (Body size limit bypass) | Med | observe-handler | Integration: Content-Length >1MB -> 413; chunked body >1MB -> 413 from Limited; body at exactly 1MB -> accepted |
| R-06 (Missing SessionWrite) | High | capability-extension | Unit: StaticTokenValidator returns capabilities including SessionWrite; Integration: SessionRegister via HTTP returns 204 not 400 |
| R-07 (CompactPayload backward compat) | Low | compact-payload-wire | Unit: round-trip serde with/without transcript_excerpt; absent field -> None; present -> Some; None serializes as absent |
| R-08 (Concurrent session bleed) | Med | observe-handler | Integration: register two sessions (same token, different IDs), send events to each, verify independent state |
| R-09 (Serde error leakage) | Low | observe-handler | Integration: send `{"type":"Bogus"}`, verify 400 response doesn't contain internal Rust type paths |
| R-10 (Warn+continue paths) | High | observe-handler, dispatch-request-refactor | Integration: RecordEvent with unregistered session -> still 204; ContextSearch with bad session -> graceful |
| R-11 (PathRouter Clone breaks) | Med | observe-context | Compilation gate + concurrent /observe requests in integration test |
| R-12 (Response serialization edge cases) | Low | observe-handler | Unit: observe_response_to_http with empty BriefingContent, zero-item Entries |
| R-13 (Audit log inconsistency) | Med | observe-handler | Integration: send RecordEvent via HTTP, verify audit log has credential_type="static_token", agent_id="http-bearer" |
| R-14 (sanitize_session_id rejects prefix) | Low | observe-handler | Unit: "http-" + 36-char UUID passes; "http-" + 123-char string passes (128 total); "http-" + 124-char fails (129 total) |

## Cross-Component Test Dependencies

1. **observe-handler depends on all others**: The handler integration tests only work if ObserveContext is correct (observe-context), dispatch_request accepts capabilities (dispatch-request-refactor), SessionWrite is in the capability set (capability-extension), and CompactPayload wire types are correct (compact-payload-wire).

2. **dispatch-request-refactor must pass before observe-handler**: UDS regression tests (R-02) gate the handler tests. If dispatch_request is broken, handler tests will also fail but for the wrong reason.

3. **capability-extension must pass before session-mutating handler tests**: Without SessionWrite, SessionRegister/RecordEvent/SessionClose all return Error instead of Ack.

## Integration Harness Plan (infra-001)

### Mandatory Gate

```bash
cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60
```

The smoke subset validates core MCP protocol, tool discovery, and basic store/search flows. These must pass to confirm the dispatch_request refactor did not break MCP functionality.

### Suite Selection

| Suite | Relevance | Rationale |
|-------|-----------|-----------|
| `protocol` | Run | dispatch_request refactor could affect MCP handshake if pub(crate) visibility causes import issues |
| `tools` | Run | Tools exercise the full server pipeline; regression detection for capability changes |
| `lifecycle` | Run | Store->search, correction chains, confidence evolution all flow through the same service handles wired into ObserveContext |
| `security` | Run | Capability enforcement tests verify the existing security model isn't broken by the capability parameterization change |

### Existing Suite Coverage vs. Feature Risks

The infra-001 suites test MCP tool calls over stdio, not the HTTP /observe endpoint. They provide UDS-path regression coverage (R-02, R-11) but do not directly test any HTTP-specific behavior (R-01, R-03, R-04, R-05, R-06, R-08, R-13).

### New Integration Tests Needed

**No new infra-001 tests are needed for Day 1.**

Rationale: The infra-001 harness exercises the MCP JSON-RPC protocol over stdio. The /observe endpoint is a separate HTTP path that does not use MCP framing. Testing /observe requires HTTP client calls (POST with bearer auth), which the current harness infrastructure does not support. All /observe testing is covered by Rust-level integration tests in `crates/unimatrix-server/src/http/router/tests.rs`.

If a future feature adds HTTP transport testing to infra-001 (e.g., a `test_observe.py` suite with an HTTP client fixture), that would be the appropriate vehicle. For vnc-022, Rust integration tests provide sufficient coverage because:
- The handler is thin (~50 lines) and delegates entirely to dispatch_request
- dispatch_request is already covered by infra-001 through the MCP/UDS path
- HTTP-specific behavior (status codes, body size, auth extraction) is unit-testable

### Gap: HTTP-Level E2E

A Rust integration test that boots a real `PathRouter` (with ObserveContext) and sends HTTP requests via `hyper::Client` would provide the highest confidence. The existing router tests use `MockMcpAdapter` for routing assertions. For vnc-022, the handler tests should either:
- (a) Test `observe_response_to_http` as a unit function (status code mapping), OR
- (b) Build a lightweight test that constructs an `ObserveContext` from a `TestHarness` and calls the handler logic directly

Option (a) is mandatory. Option (b) is recommended for high-priority risks (R-01, R-03).
