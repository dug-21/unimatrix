# Test Plan: observe-handler

Component: `crates/unimatrix-server/src/http/router.rs` — /observe HTTP handler, observe_response_to_http mapper

Covers: AC-01 through AC-08, AC-10, AC-14, AC-15, AC-17, R-01, R-03, R-04, R-05, R-08, R-09, R-10, R-12, R-13, R-14

This is the largest component. Tests are organized by category.

## Unit Tests: Response Mapping (observe_response_to_http)

Location: `crates/unimatrix-server/src/http/router/tests.rs`

### test_observe_response_ack_maps_to_204_no_content

Arrange: `HookResponse::Ack`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 204
- Body is empty (0 bytes)
- No Content-Type header

### test_observe_response_entries_maps_to_200_json

Arrange: `HookResponse::Entries { items: vec![entry_payload], total_tokens: 150 }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 200
- Content-Type == "application/json"
- Body deserializes as JSON with `"type":"Entries"` and `"items"` array

### test_observe_response_briefing_content_maps_to_200_json

Arrange: `HookResponse::BriefingContent { content: "briefing".to_string(), token_count: 50 }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 200
- Content-Type == "application/json"
- Body contains `"type":"BriefingContent"`

### test_observe_response_pong_maps_to_200_json

Arrange: `HookResponse::Pong { server_version: "0.1.0".to_string() }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 200
- Content-Type == "application/json"
- Body contains `"type":"Pong"` and `"server_version"`

### test_observe_response_error_maps_to_400_json

Arrange: `HookResponse::Error { code: -32004, message: "bad input".to_string() }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 400
- Content-Type == "application/json"
- Body contains `"type":"Error"` and `"code":-32004`

### test_observe_response_entries_empty_items (R-12)

Arrange: `HookResponse::Entries { items: vec![], total_tokens: 0 }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 200
- Body contains `"items":[]` — no panic on empty collection

### test_observe_response_briefing_content_empty_string (R-12)

Arrange: `HookResponse::BriefingContent { content: "".to_string(), token_count: 0 }`
Act: `observe_response_to_http(resp)`
Assert:
- Status == 200
- Body contains `"content":""`

## Unit Tests: Session ID Prefix

### test_session_id_prefix_applied

Assert: The handler applies `format!("http-{}", client_session_id)` before calling dispatch_request.
This is verified at integration level, but a unit-level assertion on the prefix logic:
- Input: `"abc-123"` -> Output: `"http-abc-123"`

### test_prefixed_session_id_passes_sanitize (R-14)

Arrange: `sanitize_session_id("http-550e8400-e29b-41d4-a716-446655440000")`
Assert: `Ok(())` — 41 chars, well under 128, all valid chars

### test_prefixed_session_id_at_max_length (R-14)

Arrange: `sanitize_session_id(&format!("http-{}", "a".repeat(123)))` — 128 chars total
Assert: `Ok(())`

### test_prefixed_session_id_over_max_length (R-14)

Arrange: `sanitize_session_id(&format!("http-{}", "a".repeat(124)))` — 129 chars total
Assert: `Err(...)` — exceeds 128 char limit

## Unit Tests: Handler Error Responses

### test_observe_handler_malformed_json_returns_400

Arrange: Simulate request body `{"type":"Bogus"}`
Act: `serde_json::from_slice::<HookRequest>(body)` fails
Assert: Handler returns 400 with JSON body containing "error" key

### test_observe_handler_empty_body_returns_400

Arrange: Empty request body (0 bytes)
Act: Deserialization attempt
Assert: 400 response, not 500

### test_observe_handler_valid_json_wrong_schema_returns_400

Arrange: `{"foo":"bar"}` — valid JSON, not a HookRequest
Act: Deserialization attempt
Assert: 400 response

## Integration Tests: Event Handling (AC-01 through AC-05, AC-15)

These tests require a running server with ObserveContext. If constructing a full in-process server is impractical, they can be structured as handler-level tests that invoke the /observe match arm logic directly.

### test_observe_session_register_returns_204 (AC-01)

Arrange: POST /observe with `{"type":"SessionRegister","session_id":"test-sess-1","cwd":"/tmp"}`
Act: Send request with valid bearer token
Assert:
- HTTP 204
- Session exists in SessionRegistry with key `"http-test-sess-1"` (R-03)

### test_observe_record_event_returns_204 (AC-02)

Arrange: Register session first. POST /observe with `{"type":"RecordEvent","event_type":"PreToolUse","session_id":"test-sess-1","timestamp":1717000000,"payload":{}}`
Act: Send request with valid bearer token
Assert:
- HTTP 204
- Observation row persisted in database

### test_observe_context_search_returns_200_entries (AC-03)

Arrange: POST /observe with `{"type":"ContextSearch","query":"test query","session_id":"test-sess-1"}`
Act: Send request with valid bearer token
Assert:
- HTTP 200
- Content-Type: application/json
- Body parses as JSON with `"type":"Entries"` and `"items"` array

### test_observe_compact_payload_returns_200_briefing (AC-04)

Arrange: POST /observe with `{"type":"CompactPayload","session_id":"test-sess-1","injected_entry_ids":[]}`
Act: Send request with valid bearer token
Assert:
- HTTP 200
- Content-Type: application/json
- Body contains `"type":"BriefingContent"` and `"content"` field (AC-10: no transcript markers)

### test_observe_context_search_subagent_start (AC-05)

Arrange: POST /observe with `{"type":"ContextSearch","query":"subagent task","session_id":"test-sess-1","source":"SubagentStart"}`
Act: Send request with valid bearer token
Assert:
- HTTP 200
- Body contains `"type":"Entries"`

### test_observe_session_close_returns_204 (AC-15)

Arrange: Register session first. POST /observe with `{"type":"SessionClose","session_id":"test-sess-1","outcome":"success","duration_secs":60}`
Act: Send request with valid bearer token
Assert: HTTP 204

### test_observe_all_critical_event_types (AC-15)

Assert: Integration tests collectively cover:
- SessionRegister (204) -- AC-01
- SessionClose (204) -- above
- RecordEvent/PreToolUse (204) -- AC-02
- RecordEvent/PostToolUse (204)
- ContextSearch (200+Entries) -- AC-03
- CompactPayload (200+BriefingContent) -- AC-04
- ContextSearch+source (200+Entries) -- AC-05

## Integration Tests: Auth and Error Paths (AC-06, AC-07, AC-08)

### test_observe_no_auth_returns_401 (AC-06)

Arrange: POST /observe without Authorization header
Assert: HTTP 401, body `{"error":"missing or invalid authorization"}`

### test_observe_invalid_token_returns_401 (AC-06)

Arrange: POST /observe with `Authorization: Bearer <wrong_hex>`
Assert: HTTP 401

### test_observe_malformed_body_returns_400 (AC-07)

Arrange: POST /observe with valid token, body `{"type":"Bogus"}`
Assert: HTTP 400 with JSON error body

### test_observe_oversized_body_returns_413 (AC-08, R-05)

Arrange: POST /observe with valid token, body > 1MB
Assert: HTTP 413 with `{"error":"request body exceeds maximum size"}`

### test_observe_oversized_body_content_length_fast_path (R-05)

Arrange: POST /observe with Content-Length header > 1MB
Assert: HTTP 413 (fast-path, no body read)

### test_observe_chunked_oversized_body_returns_413 (R-05)

Arrange: POST /observe without Content-Length, chunked body > 1MB
Assert: HTTP 413 from Limited layer

### test_observe_body_at_1mb_boundary_accepted (R-05)

Arrange: POST /observe with exactly 1MB body containing valid HookRequest JSON (padded)
Assert: Not 413 (body accepted)

## Integration Tests: Session Isolation (AC-14, R-08)

### test_observe_concurrent_sessions_isolated (AC-14)

Arrange:
1. Register session A: `session_id: "sess-a"`
2. Register session B: `session_id: "sess-b"`
3. Send RecordEvent to session A
4. Send RecordEvent to session B
Act: Query SessionRegistry for both sessions
Assert:
- `"http-sess-a"` and `"http-sess-b"` are distinct entries
- Events from A do not appear in B's state

## Integration Tests: Warn+Continue Paths (R-10)

### test_observe_record_event_unregistered_session_still_204

Arrange: POST /observe with RecordEvent referencing a session_id that was never registered
Assert:
- HTTP 204 (Ack) — event is acknowledged
- No panic, no 500

### test_observe_session_close_unregistered_session_still_204

Arrange: POST /observe with SessionClose for an unregistered session
Assert: HTTP 204 (graceful, warn+continue)

## Integration Tests: Security (R-09, R-13)

### test_observe_malformed_body_no_internal_leak (R-09)

Arrange: POST /observe with `{"type":"Bogus"}`
Act: Examine 400 response body
Assert:
- Body contains an error message
- Body does NOT contain Rust type paths (e.g., `unimatrix_engine::wire::`)
- Body does NOT contain full serde error internals

### test_observe_audit_log_consistency (R-13)

Arrange: Send RecordEvent via HTTP /observe
Act: Query audit log
Assert:
- `credential_type == "static_token"`
- `agent_id == "http-bearer"`
- Same field structure as MCP audit events

## Existing Test Updates

### T-PR-04: test_post_observe_returns_501_stub

This test MUST be updated: the 501 stub is replaced by the real handler. After vnc-022:
- If the test sends a request without auth context (no ResolvedIdentity in extensions), the handler behavior depends on middleware. In the mock-based test dispatch function, update the /observe arm to call the real handler logic or return a different status.
- The simplest update: change the assertion from 501 to whatever the handler returns for the test input (e.g., 400 for empty body without valid HookRequest).

### T-STA-08: test_valid_token_inserts_resolved_identity_into_extensions

Update `"caps":3` assertion to `"caps":4` after SessionWrite addition.

### T-STA-10: test_bearer_validator_trait_valid_token

Update `capabilities` assertion to include `Capability::SessionWrite`.

## Risk Trace

| Risk | Scenario | Test(s) |
|------|----------|---------|
| R-01 | ObserveContext missing handle | test_observe_context_search, test_observe_session_register, test_observe_compact_payload |
| R-03 | Prefix not applied | test_observe_session_register (verify stored key has "http-" prefix) |
| R-04 | Wrong status code | All observe_response_to_http unit tests |
| R-05 | Body limit bypass | test_observe_oversized_*, test_observe_chunked_* |
| R-08 | Cross-session bleed | test_observe_concurrent_sessions_isolated |
| R-09 | Error detail leak | test_observe_malformed_body_no_internal_leak |
| R-10 | Warn+continue untested | test_observe_record_event_unregistered_session_still_204 |
| R-12 | Empty collection serialization | test_observe_response_entries_empty_items, test_observe_response_briefing_content_empty_string |
| R-13 | Audit inconsistency | test_observe_audit_log_consistency |
| R-14 | Prefix rejected by sanitize | test_prefixed_session_id_* |
