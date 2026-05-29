# Test Plan: Path Router (`src/http/router.rs`)

Covers: C3 — PathRouter tower Service, ProjectRouter struct, path dispatch, rmcp adapter
Risks: R-11 (body size limit), R-12 (/observe auth), R-13 (ProjectRouter seam)

## Unit Tests

Tests construct a PathRouter with mock handlers and verify routing behavior.

### T-PR-01: test_get_health_routes_to_health_handler
- **Arrange**: PathRouter with health handler and mock MCP service.
- **Act**: Send `GET /health` request through PathRouter.
- **Assert**: Health handler is invoked (200 response with JSON version body).

### T-PR-02: test_post_mcp_routes_to_streamable_http_service
- **Arrange**: PathRouter with mock StreamableHttpService.
- **Act**: Send `POST /` request through PathRouter.
- **Assert**: MCP service is invoked (response comes from StreamableHttpService, not health or observe).

### T-PR-03: test_wildcard_routes_to_mcp_service
- **Arrange**: PathRouter with mock MCP service.
- **Act**: Send `POST /some/arbitrary/path` through PathRouter.
- **Assert**: MCP service handles the request (wildcard `/*` catch-all).

### T-PR-04: test_post_observe_returns_501_with_auth
- **Risk**: R-12
- **Arrange**: PathRouter. Request is authenticated (StaticTokenAuth has already inserted ResolvedIdentity).
- **Act**: Send `POST /observe`.
- **Assert**: Response status 501. Body is exactly `{"error": "Remote telemetry not yet implemented. See W2-7."}`.

### T-PR-05: test_post_observe_requires_auth
- **Risk**: R-12
- **Arrange**: Full stack (StaticTokenAuth + PathRouter). No Authorization header.
- **Act**: Send `POST /observe`.
- **Assert**: Response status 401 (auth check happens before route dispatch).

### T-PR-06: test_body_size_limit_rejects_oversized
- **Risk**: R-11
- **Arrange**: PathRouter configured with max_request_body_bytes = 1_048_576.
- **Act**: Send POST with body of 1_048_577 bytes (1 byte over limit).
- **Assert**: Response status 413. Body not fully consumed (early rejection).

### T-PR-07: test_body_size_limit_accepts_at_boundary
- **Risk**: R-11
- **Arrange**: PathRouter with default 1 MB limit.
- **Act**: Send POST with body of exactly 1_048_576 bytes.
- **Assert**: Request is accepted and forwarded to MCP service.

### T-PR-08: test_body_size_limit_enforced_before_rmcp
- **Risk**: R-11
- **Arrange**: PathRouter with 1 KB limit for test speed. Mock MCP service that panics if called.
- **Act**: Send POST with 2 KB body.
- **Assert**: Response status 413. Mock MCP service panic does NOT fire (body rejected before reaching rmcp).

### T-PR-09: test_project_router_wraps_mcp_service
- **Risk**: R-13
- **Arrange**: Construct ProjectRouter with default UnimatrixServer.
- **Act**: Send authenticated MCP request through full stack (auth + PathRouter + ProjectRouter).
- **Assert**: Request reaches UnimatrixServer through ProjectRouter (not directly). Verify via log, trace, or mock instrumentation.

### T-PR-10: test_project_router_default_project_mode
- **Risk**: R-13
- **Arrange**: Construct ProjectRouter with no slug prefix configuration.
- **Act**: Send request to root path `/`.
- **Assert**: ProjectRouter uses default_project to route (no 404 for missing slug).

### T-PR-11: test_observe_registered_in_project_router
- **Risk**: R-13
- **Arrange**: Construct ProjectRouter.
- **Act**: Verify that `/observe` is in the ProjectRouter route table.
- **Assert**: `/observe` is a registered route (grep/code inspection validates this; integration test sends POST /observe and gets 501, not 404).

## Required Edge-Case Tests

### T-PR-12: test_get_on_mcp_path_behavior
- **Arrange**: PathRouter with MCP service.
- **Act**: Send `GET /` (not POST).
- **Assert**: Behavior is defined — either forwarded to MCP (which handles method mismatch) or rejected at router level.

### T-PR-13: test_options_request_behavior
- **Arrange**: PathRouter.
- **Act**: Send `OPTIONS /`.
- **Assert**: Behavior is defined. CORS preflight handling is out of scope but must not crash.

### T-PR-14: test_head_health_behavior
- **Arrange**: PathRouter.
- **Act**: Send `HEAD /health`.
- **Assert**: Either returns 200 with no body (standard HEAD semantics) or 405. Must not panic.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-12 | T-PR-01, T-PR-02, T-PR-04 |
| AC-14 | T-PR-04, T-PR-05 |
| AC-15 | T-PR-11 (grep verification) |
| AC-19 | T-PR-09, T-PR-10 |
| AC-21 | T-PR-06, T-PR-07, T-PR-08 (body size); timeout tested in http-listener |
