# Agent Report: vnc-021-agent-7-path-router

## Task
Implement Path-Dispatching Router (C3) — `crates/unimatrix-server/src/http/router.rs`

## Files Modified
- `crates/unimatrix-server/src/http/router.rs` — full implementation replacing placeholder
- `crates/unimatrix-server/src/http/router/tests.rs` — 17 unit tests per test plan
- `crates/unimatrix-server/Cargo.toml` — added `http-body = "1"` direct dependency
- `crates/unimatrix-server/src/http/tls.rs` — cargo fmt import reorder (no logic change)
- `Cargo.lock` — updated for http-body addition

## R-01 Spike Result: PASS

Extensions DO propagate through rmcp's `StreamableHttpService`. Analysis of rmcp 0.16.0 source:

1. `tower.rs:296` — `let (part, body) = request.into_parts();` captures the `Parts` struct (which contains `http::Extensions`)
2. `tower.rs:324-331, 384, 463` — `part` is injected into MCP message extensions via `req.request.extensions_mut().insert(part);`
3. `server.rs:1064` — existing code already reads `context.extensions.get::<http::request::Parts>()` to extract session info

Therefore: `ResolvedIdentity` inserted into `http::Extensions` by StaticTokenAuth middleware is accessible as `parts.extensions.get::<ResolvedIdentity>()` inside ServerHandler. No ADR-003 fallback needed — the copy step is a no-op.

## Implementation Summary

### PathRouter
- Tower `Service<Request<ReqBody>>` dispatching on URI path
- `GET /health` -> `health_response()` (auth bypassed upstream)
- `POST /observe` -> 501 stub with exact JSON body per FR-20
- `/* (everything else)` -> MCP via ProjectRouter

### ProjectRouter (W2-6 seam)
- Single-project default mode: all MCP requests route to one `McpAdapter`
- W2-6 will add path-prefix extraction and multi-project slug lookup

### McpAdapter (ADR-003)
- Thin wrapper around `StreamableHttpService<UnimatrixServer, LocalSessionManager>`
- Enforces `max_request_body_bytes` via Content-Length header check BEFORE rmcp
- Uses `StreamableHttpService::new(|| Ok(server.clone()), session_manager, config)` factory pattern
- Error type is `Infallible` — all errors become HTTP responses

### Dependency Addition
- Added `http-body = "1"` as direct dependency (was transitive only) — needed for `Body` trait bound on generic router types

## Tests: 17 passed, 0 failed

| Test | Status | Covers |
|------|--------|--------|
| test_get_health_routes_to_health_handler | PASS | T-PR-01 |
| test_post_mcp_routes_to_streamable_http_service | PASS | T-PR-02 |
| test_wildcard_routes_to_mcp_service | PASS | T-PR-03 |
| test_post_observe_returns_501_stub | PASS | T-PR-04 |
| test_body_size_limit_rejects_oversized | PASS | T-PR-06 |
| test_body_size_limit_accepts_at_boundary | PASS | T-PR-07 |
| test_body_size_limit_enforced_before_rmcp | PASS | T-PR-08 |
| test_project_router_default_project_mode | PASS | T-PR-10 |
| test_observe_path_constant | PASS | T-PR-11 |
| test_observe_registered_in_routing_tree | PASS | T-PR-11 |
| test_get_on_mcp_path_forwards_to_mcp | PASS | T-PR-12 |
| test_options_request_does_not_panic | PASS | T-PR-13 |
| test_head_health_routes_to_mcp | PASS | T-PR-14 |
| test_get_observe_routes_to_mcp | PASS | edge case |
| test_observe_stub_response_format | PASS | format |
| test_payload_too_large_response_format | PASS | format |
| test_default_max_body_bytes | PASS | constant |

### Tests Not Implemented
- T-PR-05 (POST /observe requires auth) — requires full stack integration (StaticTokenAuth + PathRouter); tested at integration level
- T-PR-09 (ProjectRouter wraps MCP service) — structural guarantee; ProjectRouter.route_mcp always delegates to McpAdapter

## Issues
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-003 (#4667) on rmcp adapter boundary, rmcp dependency pattern (#4661), and rmcp implementation constraints (#4367). Applied ADR-003 thin adapter pattern.
- Queried: mcp__unimatrix__context_search -- no additional patterns for tower routing found beyond what briefing surfaced.
- Stored: nothing novel to store -- the R-01 extension propagation finding confirms the expected case documented in ADR-003. The rmcp `Parts` injection mechanism is visible in source and already used by existing code (server.rs:1064).
