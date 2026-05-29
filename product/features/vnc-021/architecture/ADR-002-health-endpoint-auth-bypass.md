## ADR-002: Health Endpoint Auth Bypass via Path-Match Before Auth

### Context

The HTTP health endpoint (`GET /health`) must be accessible without authentication for external monitoring, Docker HEALTHCHECK, and load balancer probes. All other HTTP paths require bearer token authentication.

Two approaches:
1. **Path-match in auth middleware**: The `StaticTokenAuth` middleware checks the request path before token validation. If path equals `/health`, the request is forwarded without auth.
2. **Separate listener**: Run the health endpoint on a different port/listener that has no auth middleware. Adds operational complexity (two ports to configure and monitor).
3. **Auth middleware wraps only MCP paths**: The path router sits outside auth, and only MCP/observe routes are wrapped. This inverts the security model -- new paths are unauthenticated by default.

### Decision

Path-match in auth middleware (option 1). The `StaticTokenAuth` service checks `request.uri().path()` before token validation. If the path is exactly `/health` and the method is GET, the request bypasses auth and is forwarded to the inner service. All other paths require valid bearer token.

This follows the "secure by default" principle: new paths added to the router are authenticated unless explicitly exempted in the auth middleware's bypass list. The bypass list is a compile-time constant (`["/health"]`), not configurable.

### Consequences

Easier: Single auth layer wraps the entire HTTP stack. Adding new authenticated paths requires no auth wiring -- they are protected automatically. Health endpoint works with zero configuration for monitoring tools.

Harder: The bypass list must be maintained as a compile-time constant. Adding a new unauthenticated path requires a code change and review. This is intentional -- unauthenticated surface area should never grow silently.
