# Test Plan: Lifecycle Integration (`src/infra/shutdown.rs` modifications + cross-component integration)

Covers: C8 — LifecycleHandles extension, graceful shutdown, plus full-stack integration tests spanning R-01, R-03, R-08, R-10, R-16, R-17
Risks: R-01 (extension propagation), R-03 (identity injection first activation), R-08 (shutdown), R-10 (audit credential_type), R-16 (stdio mode), R-17 (rate limit)

## Full-Stack Integration Tests

These tests start a real UnimatrixServer with HTTP listener and exercise the complete request chain. They are the most critical tests for vnc-021.

### T-LI-01: test_http_mcp_tool_call_end_to_end (R-01 — CRITICAL)
- **Risk**: R-01
- **Arrange**: Start server with HTTP enabled, TLS disabled, port 0. Load or generate token.
- **Act**: Send authenticated HTTP POST with MCP initialize + tool call (e.g., `context_status`).
- **Assert**: Receive valid MCP JSON-RPC response with tool result. This proves: token auth works, PathRouter routes to MCP, rmcp StreamableHttpService processes the request, UnimatrixServer executes the tool, response returns through the stack.

### T-LI-02: test_http_extension_propagation_to_identity (R-01 — CRITICAL)
- **Risk**: R-01
- **Arrange**: Start server with HTTP enabled. Send authenticated MCP tool call.
- **Act**: After tool execution, query `audit_log` table directly.
- **Assert**: `credential_type = "static_token"` in the audit row. This proves ResolvedIdentity was propagated from StaticTokenAuth through rmcp to build_context_with_external_identity. If credential_type is "none", extensions were dropped.

### T-LI-03: test_http_extension_propagation_fallback (R-01)
- **Risk**: R-01
- **Arrange**: If T-LI-01 fails because rmcp drops extensions, verify ADR-003 adapter fallback is implemented.
- **Act**: Re-run T-LI-01 with adapter fallback active.
- **Assert**: Tool call succeeds with correct identity. This test may be skipped if T-LI-01 passes (primary path works).

### T-LI-04: test_http_identity_chain_produces_correct_audit (R-03, R-10)
- **Risk**: R-03, R-10
- **Arrange**: Start server with HTTP enabled. Send authenticated MCP initialize (with `clientInfo.name = "test-client"`) + tool call.
- **Act**: Query `audit_log` table.
- **Assert**: Row exists with: `credential_type = "static_token"`, `agent_attribution = "test-client"`, audit source agent_id = "http-bearer".

### T-LI-05: test_uds_identity_chain_unchanged (R-03, R-10)
- **Risk**: R-03, R-10
- **Arrange**: Start server. Connect via UDS (existing path). Execute same tool as T-LI-04.
- **Act**: Query `audit_log` table.
- **Assert**: Row exists with: `credential_type = "none"`. UDS path uses resolve_agent, not build_context_with_external_identity. This proves the two identity paths are distinct.

### T-LI-06: test_http_bearer_capabilities_correct (R-03)
- **Risk**: R-03
- **Arrange**: Start server with HTTP. Authenticate.
- **Act**: Execute a tool that requires `Capability::Write` (e.g., `context_store`).
- **Assert**: Tool succeeds. ResolvedIdentity for HTTP bearer includes Write capability.

### T-LI-07: test_http_bearer_agent_id_is_http_bearer (R-03)
- **Risk**: R-03
- **Arrange**: Start server with HTTP. Authenticate.
- **Act**: Execute a tool. Query audit_log.
- **Assert**: Audit source contains `agent_id = "http-bearer"` (not empty, not "unknown").

### T-LI-08: test_graceful_shutdown_completes_inflight_request (R-08)
- **Risk**: R-08
- **Arrange**: Start server with HTTP. Open connection, begin MCP tool call.
- **Act**: Trigger CancellationToken (shutdown) while tool call is in-flight.
- **Assert**: In-flight request completes with valid response before connection closes.

### T-LI-09: test_graceful_shutdown_rejects_new_connections (R-08)
- **Risk**: R-08
- **Arrange**: Start server with HTTP. Trigger shutdown.
- **Act**: Attempt new TCP connection after shutdown initiated.
- **Assert**: New connection is rejected.

### T-LI-10: test_shutdown_sequence_ordering (R-08)
- **Risk**: R-08
- **Arrange**: Start server with both UDS and HTTP.
- **Act**: Trigger shutdown. Monitor acceptor close order.
- **Assert**: HTTP acceptor is aborted in shutdown sequence between MCP acceptor (Step 0) and hook IPC (Step 0b) per ARCHITECTURE.md.

### T-LI-11: test_http_not_started_in_stdio_mode (R-16)
- **Risk**: R-16
- **Arrange**: Config with `[http] enabled = true`. Start server in stdio mode.
- **Act**: Check whether HTTP listener was started (no bind address in LifecycleHandles).
- **Assert**: `http_listener_addr` is `None`. HTTP listener is NOT started in stdio mode.

### T-LI-12: test_http_bearer_not_exempt_from_rate_limit (R-17)
- **Risk**: R-17
- **Code Review**: Verify `CallerId::HttpBearer` match arm in rate limiter does NOT return the UdsSession exemption path. The compiler enforces the match arm exists (exhaustive), but the semantic behavior must be verified.
- **Arrange**: If rate limiting is testable: configure a low rate limit, send rapid HTTP tool calls.
- **Act**: Send tool calls faster than the rate limit allows.
- **Assert**: Rate limiting is applied (requests rejected after limit exceeded).

## LifecycleHandles Unit Tests

### T-LI-13: test_lifecycle_handles_has_http_fields
- **Arrange**: Construct `LifecycleHandles` with `http_acceptor_handle = None` and `http_listener_addr = None`.
- **Act**: Access the fields.
- **Assert**: Fields exist and are `None`. This is a compile-time structural test.

### T-LI-14: test_lifecycle_handles_stores_http_join_handle
- **Arrange**: Spawn a dummy async task, get JoinHandle. Construct LifecycleHandles with `http_acceptor_handle = Some(handle)`.
- **Act**: Access `http_acceptor_handle`.
- **Assert**: Field is `Some`. Abort the handle.

## Required Edge-Case Tests

### T-LI-15: test_concurrent_http_sessions_isolated
- **Arrange**: Start server with HTTP. Open two concurrent authenticated sessions.
- **Act**: Execute different tool calls on each session simultaneously.
- **Assert**: Results are correct for each session. No cross-session state leakage.

### T-LI-16: test_http_disabled_server_starts_normally
- **Arrange**: Config with `[http] enabled = false` (default).
- **Act**: Start server.
- **Assert**: Server starts successfully with UDS only. No HTTP listener. No startup error related to HTTP.

### T-LI-17: test_startup_log_when_http_available_but_disabled
- **Arrange**: Config with `[http] enabled = false`.
- **Act**: Start server, capture startup log output.
- **Assert**: Log contains message like `"HTTP transport available -- set [http] enabled = true in config.toml"` (per human review note #2).

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-01 | T-LI-01 |
| AC-02 | Token generation tested in token-manager.md; stdout output in T-LI-01 setup |
| AC-06 | T-LI-01, T-LI-06 |
| AC-07 | T-LI-02, T-LI-04 |
| AC-08 | T-LI-04 |
| AC-16 | Full cargo test suite pass (no modification to existing tests) |
| AC-17 | T-LI-01 (foreground mode) |
| AC-18 | T-LI-01 variant with daemon mode |
| AC-19 | T-LI-01 (requests flow through ProjectRouter) |
