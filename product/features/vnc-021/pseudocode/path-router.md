# path-router (C3) -- `src/http/router.rs`

## Purpose

Path-dispatching tower Service that routes HTTP requests by URI path to the correct handler: `/health` to the health handler, `/observe` to a 501 stub, and everything else (`/*`) to `StreamableHttpService<UnimatrixServer>` via the rmcp adapter boundary (ADR-003). Contains `ProjectRouter` as the W2-6 structural seam.

This component also contains the **R-01 spike test** pseudocode for validating rmcp extension propagation.

## R-01 Spike Test (MUST EXECUTE FIRST)

Before building the full router, validate that `http::Extensions` survive `StreamableHttpService` processing. This is the single highest-risk integration point.

```
#[cfg(test)]
mod spike_tests:
    /// R-01: Validate that ResolvedIdentity inserted into request extensions
    /// by upstream middleware survives rmcp's StreamableHttpService processing
    /// and is accessible in build_context_with_external_identity.
    #[tokio::test]
    async fn spike_rmcp_extension_propagation():
        // 1. Build a minimal UnimatrixServer (use existing make_server() fixture)
        let server = make_server().await

        // 2. Build StreamableHttpService wrapping the server
        let streamable = StreamableHttpService::new(server.clone())
        // NOTE: Check rmcp 0.16 API -- StreamableHttpService::new may need
        //       additional parameters (session config, etc.)

        // 3. Construct a mock HTTP request with ResolvedIdentity in extensions
        let identity = ResolvedIdentity {
            agent_id: "spike-test".to_string(),
            trust_level: TrustLevel::Standard,
            capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
        }
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/mcp")  // or whatever path rmcp expects
            .header("content-type", "application/json")
            .body(/* MCP initialize JSON-RPC body */)
            .unwrap()
        request.extensions_mut().insert(identity.clone())

        // 4. Call the service
        let response = streamable.call(request).await

        // 5. OUTCOME DETERMINES ARCHITECTURE PATH:
        //    - If identity survives in extensions -> primary path works
        //    - If identity is dropped -> activate ADR-003 adapter fallback
        //
        // The spike test may need to verify this indirectly:
        // a. Send an MCP initialize + tool call sequence
        // b. Check that build_context_with_external_identity received Some(&identity)
        // c. Or check audit_log for credential_type = "static_token"
        //
        // If direct verification is not possible via HTTP round-trip,
        // add a test-only hook in build_context_with_external_identity that
        // records whether external_identity was Some or None.
```

**Spike outcome determines McpAdapter behavior**: If extensions propagate (expected case), the copy step in McpAdapter is a debug assertion. If extensions are dropped, McpAdapter must use a task-local or side-channel to inject the identity.

## Types

### `PathRouter`

```
struct PathRouter:
    project_router: ProjectRouter
    health_handler: fn() -> Response<Body>  // from health.rs

impl Service<Request<Body>> for PathRouter:
    type Response = Response<Body>
    type Error = Infallible  // all errors become HTTP responses
    type Future = Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>>

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>>:
        Poll::Ready(Ok(()))

    fn call(&mut self, request: Request<Body>) -> Self::Future:
        let path = request.uri().path()
        let method = request.method().clone()

        match (method, path):
            (Method::GET, "/health") =>
                // Route to health handler (auth already bypassed by StaticTokenAuth)
                let response = health_response()
                Box::pin(async move { Ok(response) })

            (Method::POST, "/observe") =>
                // 501 stub (FR-20). Auth is required -- enforced by upstream StaticTokenAuth.
                let response = observe_stub_response()
                Box::pin(async move { Ok(response) })

            (_, _) =>
                // Route everything else to MCP via ProjectRouter
                let fut = self.project_router.route_mcp(request)
                Box::pin(fut)
```

### `observe_stub_response()`

```
fn observe_stub_response() -> Response<Body>:
    Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)  // 501
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"error":"Remote telemetry not yet implemented. See W2-7."}"#
        ))
        .unwrap()
```

### `ProjectRouter` (W2-6 seam)

```
struct ProjectRouter:
    /// Single-project default: one server instance serving all requests.
    /// W2-6 will add: stores: HashMap<String, Arc<ProjectContext>>
    default_server: McpAdapter

impl ProjectRouter:
    /// Create a new ProjectRouter in single-project default mode.
    fn new(server: UnimatrixServer, config: &HttpConfig) -> Self:
        let mcp_adapter = McpAdapter::new(server, config.max_request_body_bytes)
        ProjectRouter {
            default_server: mcp_adapter,
        }

    /// Route an MCP request. In single-project mode, all requests go to default.
    /// W2-6 will add path-prefix extraction and project slug lookup.
    async fn route_mcp(&mut self, request: Request<Body>) -> Result<Response<Body>, Infallible>:
        // In vnc-021: no slug extraction. Direct delegation.
        // W2-6 seam: extract slug from path prefix, lookup in stores map,
        // fall back to default_project.
        self.default_server.handle(request).await
```

### `McpAdapter` (ADR-003 -- rmcp isolation boundary)

```
struct McpAdapter:
    /// The rmcp StreamableHttpService. Isolated behind this adapter.
    streamable: StreamableHttpService<UnimatrixServer>
    /// Maximum request body size (bytes). Enforced before rmcp sees the body.
    max_body_bytes: usize

impl McpAdapter:
    fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self:
        // Build StreamableHttpService from rmcp
        // NOTE: Check rmcp 0.16 API for exact constructor signature.
        // May need StreamableHttpServerConfig for session management.
        let streamable = StreamableHttpService::new(server)
        McpAdapter { streamable, max_body_bytes }

    async fn handle(&mut self, request: Request<Body>) -> Result<Response<Body>, Infallible>:
        // Step 1: Enforce body size limit BEFORE rmcp (R-11, ADR-003)
        if let Some(content_length) = request.headers().get("content-length"):
            if let Ok(len) = content_length.to_str().and_then(|s| s.parse::<usize>().ok()):
                if len > self.max_body_bytes:
                    return Ok(payload_too_large_response())

        // Step 2: Extract ResolvedIdentity from extensions (for ADR-003 fallback check)
        // If spike test (R-01) proved extensions propagate, this is a debug assertion.
        // If spike test proved extensions are dropped, implement fallback here.
        let identity = request.extensions().get::<ResolvedIdentity>().cloned()

        // Step 3: Delegate to StreamableHttpService
        // The streamable service implements tower::Service<Request<RequestBody>>
        // Type conversion may be needed between hyper::Body and rmcp's RequestBody
        let response = self.streamable.call(request).await

        // Step 4: If extensions were dropped (ADR-003 fallback), the adapter
        // would need to re-inject identity via a side-channel here.
        // In the expected case, this step is a no-op.

        match response:
            Ok(resp) => Ok(resp),
            Err(e) =>
                // Map rmcp errors to HTTP 500
                tracing::error!(error = %e, "rmcp StreamableHttpService error")
                Ok(internal_error_response())
```

### Helper: `payload_too_large_response()`

```
fn payload_too_large_response() -> Response<Body>:
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)  // 413
        .header("content-type", "application/json")
        .body(Body::from(r#"{"error":"request body exceeds maximum size"}"#))
        .unwrap()
```

### Helper: `internal_error_response()`

```
fn internal_error_response() -> Response<Body>:
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"error":"internal server error"}"#))
        .unwrap()
```

## ADR-003 Fallback Path

If the R-01 spike test reveals that rmcp drops extensions:

1. Before calling `streamable.call()`, extract `ResolvedIdentity` from request extensions
2. Store it in a `tokio::task_local!` or `Arc<Mutex<Option<ResolvedIdentity>>>`
3. In `build_context_with_external_identity`, if `external_identity` from extensions is `None`, check the task-local
4. This fallback is localized entirely within McpAdapter and server.rs -- no changes to auth, health, or listener

## Error Handling

| Error Case | HTTP Response | Notes |
|-----------|--------------|-------|
| Body exceeds max_request_body_bytes | 413 Payload Too Large | Checked before rmcp |
| rmcp internal error | 500 Internal Server Error | Logged, generic response |
| Unknown path | Delegated to rmcp | rmcp returns its own error for non-MCP paths |

## Key Test Scenarios

1. **R-01 Spike**: Extension propagation through StreamableHttpService. MUST PASS before full build.
2. **Health routing**: `GET /health` -> 200 JSON. Verify PathRouter dispatches correctly.
3. **Observe stub**: `POST /observe` -> 501 with exact JSON body (FR-20).
4. **MCP routing**: `POST /*` -> delegated to StreamableHttpService.
5. **Body size limit**: Request with Content-Length > 1MB -> 413 (R-11).
6. **Body size at limit**: Exactly 1MB -> accepted.
7. **ProjectRouter default**: All requests route through ProjectRouter in single-project mode (R-13).
8. **Observe in ProjectRouter tree**: `/observe` registered in ProjectRouter for W2-6 compatibility (FR-24).
9. **Method mismatch**: `GET /observe` -> routed to MCP (only POST matches the stub).
