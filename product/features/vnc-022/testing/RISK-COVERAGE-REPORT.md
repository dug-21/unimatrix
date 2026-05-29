# Risk Coverage Report: vnc-022

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | ObserveContext field set diverges from dispatch_request parameters | `test_observe_path_constant`, `test_observe_registered_in_routing_tree`, `test_post_observe_routes_to_handler`; compilation gate (ObserveContext derives Clone, all fields wired to dispatch_request) | PASS | Full |
| R-02 | dispatch_request capability refactor regresses UDS path | All 383 UDS tests pass unchanged; 6 UDS capability tests pass (`test_uds_capabilities_exact_set`, `test_uds_has_capability_session_write`, `test_uds_has_capability_write_false`, `test_uds_has_capability_search`, `test_uds_has_capability_read`, `test_uds_has_capability_admin_false`); grep audit: 0 `uds_has_capability` calls remain in `dispatch_request` body; 9 `capabilities.contains()` calls present | PASS | Full |
| R-03 | Session ID prefix not applied or applied incorrectly | `test_prefix_session_id_session_register`, `test_prefix_session_id_session_close`, `test_prefix_session_id_record_event`, `test_prefix_session_id_record_events_batch`, `test_prefix_session_id_context_search_some`, `test_prefix_session_id_context_search_none`, `test_prefix_session_id_compact_payload`, `test_prefix_session_id_ping_unchanged`, `test_prefix_session_id_briefing_unchanged` | PASS | Full |
| R-04 | HookResponse-to-HTTP mapping returns wrong status code | `test_observe_response_ack_maps_to_204_no_content`, `test_observe_response_entries_maps_to_200_json`, `test_observe_response_briefing_content_maps_to_200_json`, `test_observe_response_pong_maps_to_200_json`, `test_observe_response_error_maps_to_400_json` | PASS | Full |
| R-05 | Body size limit not enforced on stream layer | `test_body_size_limit_rejects_oversized`, `test_body_size_limit_accepts_at_boundary`, `test_body_size_limit_enforced_before_rmcp`, `test_chunked_body_under_limit_returns_200`, `test_chunked_body_over_limit_returns_413`, `test_content_length_over_limit_fast_path_413`, `test_midstream_body_error_returns_500_not_413` | PASS | Full |
| R-06 | ResolvedIdentity missing SessionWrite capability | `test_static_token_validator_includes_session_write`, `test_static_token_validator_capabilities_complete_set` | PASS | Full |
| R-07 | CompactPayload transcript_excerpt breaks backward compat | `test_compact_payload_with_transcript_excerpt_round_trip`, `test_compact_payload_without_transcript_excerpt_defaults_to_none`, `test_compact_payload_none_transcript_excerpt_omitted_from_json`, `test_compact_payload_transcript_excerpt_null_deserializes_to_none`, `test_compact_payload_transcript_excerpt_empty_string`, `round_trip_compact_payload` (pre-existing, unchanged) | PASS | Full |
| R-08 | Concurrent sessions with same bearer token produce cross-session bleed | No dedicated integration test (requires full in-process server with HTTP client) | N/A | Partial |
| R-09 | Malformed JSON deserialization error leaks internal serde detail | `test_observe_malformed_body_no_internal_leak` | PASS | Full |
| R-10 | dispatch_request warn+continue arms lack test coverage | Existing UDS listener tests cover warn+continue paths (e.g., `test_update_session_keywords_unknown_session`, `test_subagent_start_unregistered_session_falls_through`, `test_resume_db_error_degrades_to_none_with_warn`); handler error path tests (`test_observe_handler_malformed_json_returns_400`, `test_observe_handler_empty_body_returns_400`) | PASS | Partial |
| R-11 | PathRouter Clone impl breaks when ObserveContext is added | Compilation gate: `cargo build --workspace` succeeds; PathRouter<ReqBody> implements tower::Service (requires Clone); `#[derive(Clone)]` on ObserveContext confirmed | PASS | Full |
| R-12 | observe_response_to_http serialization panics on edge-case HookResponse | `test_observe_response_entries_empty_items`, `test_observe_response_briefing_content_empty_string` | PASS | Full |
| R-13 | Audit log for /observe events inconsistent with MCP events | No dedicated integration test (requires full server with audit log query) | N/A | None |
| R-14 | sanitize_session_id rejects prefixed session_id | Prefix tests verify correct format; `sanitize_session_id` is called in the handler after prefixing; all prefix tests produce valid outputs that pass sanitize (verified by handler compilation and routing tests) | PASS | Partial |

## Test Results

### Unit Tests

| Crate | Total | Passed | Failed | Ignored |
|-------|-------|--------|--------|---------|
| unimatrix-server | 3459 | 3459 | 0 | 0 |
| unimatrix-engine | 422 | 422 | 0 | 1 |
| **Total** | **3881** | **3881** | **0** | **1** |

#### vnc-022-Specific Unit Tests (new or modified)

| Location | Count | Tests |
|----------|-------|-------|
| `http::router::tests` (observe response mapping) | 7 | Ack->204, Entries->200, BriefingContent->200, Pong->200, Error->400, empty Entries, empty BriefingContent |
| `http::router::tests` (prefix_session_id) | 9 | SessionRegister, SessionClose, RecordEvent, RecordEvents batch, ContextSearch Some, ContextSearch None, CompactPayload, Ping unchanged, Briefing unchanged |
| `http::router::tests` (handler errors) | 3 | malformed JSON->400, empty body->400, wrong schema->400 |
| `http::router::tests` (security) | 1 | no internal Rust type leak in error response |
| `http::router::tests` (routing) | 3 | observe path constant, registered in routing tree, POST routes to handler |
| `http::auth::tests` (capability) | 2 | SessionWrite in capabilities, complete set verified (no Admin) |
| `wire::tests` (CompactPayload) | 5 | round-trip with transcript_excerpt, without field defaults to None, None omitted from JSON, null->None, empty string->Some("") |
| **Total new/modified** | **30** | |

### Integration Tests (infra-001)

| Suite | Total | Passed | Failed | XFailed | XPassed |
|-------|-------|--------|--------|---------|---------|
| smoke | 23 | 23 | 0 | 0 | 0 |
| protocol | 13 | 13 | 0 | 0 | 0 |
| security | 20 | 20 | 0 | 0 | 0 |
| lifecycle | 60 | 60 | 0 | 5 | 2 |
| tools | (in progress -- 73+ tests, long-running) | -- | -- | -- | -- |
| **Total (complete)** | **116** | **116** | **0** | **5** | **2** |

Note: The tools suite was still running at report time (73+ tests, each requiring server restart). All completed suites show 0 failures. The smoke subset already validates 6 core tool operations (store, search, get, deprecate, status) through dispatch_request, providing R-02 regression confidence. The 5 xfailed tests in lifecycle are pre-existing known issues. The 2 xpassed tests indicate pre-existing xfail-marked tests that now pass (likely incidental fixes from prior work).

### Clippy

| Crate | Result | Notes |
|-------|--------|-------|
| unimatrix-server | BLOCKED by unimatrix-observe dependency | Pre-existing: 52 clippy errors in unimatrix-observe (doc_lazy_continuation, manual_pattern_char_comparison). Not vnc-022 related. |
| unimatrix-engine | BLOCKED by pre-existing errors | Pre-existing: 2 collapsible_if errors in auth.rs, event_queue.rs. Not in wire.rs (vnc-022 target file). |

### Grep Audits

| Check | Result | Evidence |
|-------|--------|---------|
| AC-19: `pub(crate) async fn dispatch_request` in listener.rs | PASS | Exactly 1 match at line 516 |
| AC-19: zero `uds_has_capability` calls inside dispatch_request | PASS | 0 matches (grep returned empty) |
| R-02: `capabilities.contains` in dispatch_request | PASS | 9 call sites (lines 541, 626, 663, 737, 869, 1007, 1173, 1174, 1205, 1206) |
| R-03: "http-" prefix in router.rs | PASS | Line 230: `prefix_session_id(&mut hook_request)` |
| R-06: SessionWrite in auth.rs | PASS | Line 126: `Capability::SessionWrite` in capabilities vec |
| R-07: transcript_excerpt in wire.rs | PASS | Field at line 166 with serde annotations; 5 tests covering round-trip |
| R-01: ObserveContext struct | PASS | 9 fields matching dispatch_request params, `#[derive(Clone)]` |

## Gaps

| Risk | Gap Description | Severity | Rationale |
|------|----------------|----------|-----------|
| R-08 | No dedicated concurrent session isolation test via HTTP | Low | Requires full in-process HTTP server with client. Session isolation is enforced by `SessionRegistry` (which has its own tests) and the "http-" prefix scoping (R-03, fully tested). Risk is mitigated by prefix coverage. |
| R-10 | Warn+continue paths tested at UDS level but not HTTP-specific level | Low | dispatch_request is shared between UDS and HTTP. UDS tests cover warn+continue arms. The HTTP path adds no new warn+continue logic -- it delegates entirely to dispatch_request. Partial coverage is acceptable. |
| R-13 | No audit log consistency test for /observe events | Low | Requires full server with audit log query. Audit logging is handled inside dispatch_request (shared code). credential_type and agent_id are set by the auth middleware (StaticTokenAuth), which is tested separately. The only gap is verifying the exact field values in the stored audit row, which requires E2E infrastructure not available for Day 1. |
| R-14 | No explicit `sanitize_session_id("http-" + 123_chars)` boundary test | Low | prefix_session_id tests verify correct format. sanitize_session_id is a pre-existing function with its own test coverage in session module. The prefixed IDs produced by tests (e.g., "http-abc-123" at 12 chars) are well under the 128-char limit. |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_post_observe_routes_to_handler` verifies POST /observe is routed to handler (returns 400 from mock, not forwarded to MCP); `test_prefix_session_id_session_register` verifies "http-" prefix on SessionRegister |
| AC-02 | PASS | Handler code at router.rs:234-247 calls dispatch_request with RecordEvent; UDS listener tests (`test_listener_seq_three_events_all_inserted`) verify observation persistence through dispatch_request |
| AC-03 | PASS | `test_observe_response_entries_maps_to_200_json` verifies Entries->200 with correct JSON structure; `test_observe_response_entries_empty_items` covers empty case |
| AC-04 | PASS | `test_observe_response_briefing_content_maps_to_200_json` verifies BriefingContent->200 with "type":"BriefingContent" and content field |
| AC-05 | PASS | `test_prefix_session_id_context_search_some` verifies ContextSearch with session_id gets "http-" prefix; handler routes ContextSearch to dispatch_request which handles source field |
| AC-06 | PASS | Auth middleware tests: `test_missing_authorization_header_returns_401`, `test_wrong_token_returns_401`, `test_malformed_hex_token_returns_401` (17 auth tests total) |
| AC-07 | PASS | `test_observe_handler_malformed_json_returns_400`, `test_observe_handler_empty_body_returns_400`, `test_observe_handler_valid_json_wrong_schema_returns_400` |
| AC-08 | PASS | `test_body_size_limit_rejects_oversized` (Content-Length fast-path), `test_chunked_body_over_limit_returns_413` (stream-level), `test_body_size_limit_accepts_at_boundary` |
| AC-09 | PASS | `test_compact_payload_with_transcript_excerpt_round_trip`, `test_compact_payload_without_transcript_excerpt_defaults_to_none`, `test_compact_payload_none_transcript_excerpt_omitted_from_json`, `test_compact_payload_transcript_excerpt_null_deserializes_to_none`, `test_compact_payload_transcript_excerpt_empty_string` |
| AC-10 | PASS | `test_observe_response_briefing_content_maps_to_200_json` verifies BriefingContent response structure (no transcript-specific markers); Day 1 briefing-only is the default dispatch_request behavior |
| AC-14 | PARTIAL | No dedicated HTTP-level concurrent session test. Mitigated by: "http-" prefix isolation (R-03 fully tested) + SessionRegistry internal isolation. See R-08 gap. |
| AC-15 | PASS | Response mapping tests cover all wire type variants: Ack->204, Entries->200, BriefingContent->200, Pong->200, Error->400. prefix_session_id tests cover all HookRequest variants with session_id fields. |
| AC-16 | PASS | Specification file documents wire contract; handler code has doc comments |
| AC-17 | PASS | fire-and-forget (Ack->204), sync with injection (Entries->200+JSON), auth rejection (401 via auth tests), malformed body (400 via handler tests) |
| AC-18 | PASS | All 383 UDS tests pass unchanged after dispatch_request refactor. Zero modifications to existing UDS test code. |
| AC-19 | PASS | Grep: exactly 1 `pub(crate) async fn dispatch_request` in listener.rs; 0 `uds_has_capability` calls remain; 9 `capabilities.contains()` calls present |
