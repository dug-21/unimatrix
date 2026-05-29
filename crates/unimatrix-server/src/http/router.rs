//! PathRouter tower Service, ProjectRouter, path dispatch, rmcp adapter (C3).
//!
//! Routes HTTP requests by URI path:
//! - `GET /health` -> health handler (no auth, bypassed by StaticTokenAuth)
//! - `POST /observe` -> HTTP 501 stub (auth required, W2-7 future)
//! - `/* (everything else)` -> MCP dispatch through ProjectRouter -> McpAdapter
//!
//! Contains `ProjectRouter` as the W2-6 structural seam and `McpAdapter` as
//! the rmcp isolation boundary (ADR-003).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tower::Service;

use crate::http::health::health_response;
use crate::server::UnimatrixServer;

/// Path for the remote telemetry endpoint (FR-24, W2-7 future).
pub(crate) const OBSERVE_PATH: &str = "/observe";

/// Maximum request body size default (1 MB). Used when no config override.
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// PathRouter — top-level path-dispatching tower Service
// ---------------------------------------------------------------------------

/// Tower service that dispatches requests by URI path.
///
/// - `GET /health` -> JSON health response
/// - `POST /observe` -> 501 stub
/// - `/* (everything else)` -> MCP via ProjectRouter
pub struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    project_router: ProjectRouter<ReqBody>,
}

impl<ReqBody> std::fmt::Debug for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathRouter")
            .field("project_router", &self.project_router)
            .finish()
    }
}

impl<ReqBody> PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Create a new PathRouter wrapping a ProjectRouter.
    pub fn new(project_router: ProjectRouter<ReqBody>) -> Self {
        PathRouter { project_router }
    }
}

impl<ReqBody> Clone for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn clone(&self) -> Self {
        PathRouter {
            project_router: self.project_router.clone(),
        }
    }
}

impl<ReqBody> Service<Request<ReqBody>> for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let path = request.uri().path().to_owned();
        let method = request.method().clone();

        match (method, path.as_str()) {
            (Method::GET, "/health") => {
                // Route to health handler. Auth already bypassed by StaticTokenAuth (ADR-002).
                let resp = map_health_response(health_response());
                Box::pin(async move { Ok(resp) })
            }
            (Method::POST, "/observe") => {
                // 501 stub (FR-20). Auth enforced by upstream StaticTokenAuth.
                let resp = observe_stub_response();
                Box::pin(async move { Ok(resp) })
            }
            (_, _) => {
                // Route everything else to MCP via ProjectRouter.
                let mut router = self.project_router.clone();
                Box::pin(async move { router.route_mcp(request).await })
            }
        }
    }
}

/// Map the health response (String body) to BoxBody for consistency.
fn map_health_response(resp: Response<String>) -> Response<BoxBody<Bytes, Infallible>> {
    let (parts, body) = resp.into_parts();
    Response::from_parts(
        parts,
        Full::new(Bytes::from(body))
            .map_err(|never| match never {})
            .boxed(),
    )
}

/// 501 Not Implemented response for `/observe` (FR-20, W2-7).
fn observe_stub_response() -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(
                r#"{"error":"Remote telemetry not yet implemented. See W2-7."}"#,
            ))
            .map_err(|never| match never {})
            .boxed(),
        )
        .expect("static response builder cannot fail")
}

// ---------------------------------------------------------------------------
// ProjectRouter — W2-6 structural seam (single-project default mode)
// ---------------------------------------------------------------------------

/// Project-aware MCP request router.
///
/// In vnc-021, operates in single-project default mode: all MCP requests
/// route to a single `McpAdapter`. W2-6 will add path-prefix extraction
/// and multi-project slug lookup via `stores: HashMap<String, Arc<...>>`.
pub struct ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    default_server: McpAdapter,
    _phantom: std::marker::PhantomData<fn(ReqBody)>,
}

impl<ReqBody> std::fmt::Debug for ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectRouter")
            .field("default_server", &self.default_server)
            .finish()
    }
}

impl<ReqBody> ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Create a new ProjectRouter in single-project default mode.
    pub fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let mcp_adapter = McpAdapter::new(server, max_body_bytes);
        ProjectRouter {
            default_server: mcp_adapter,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Route an MCP request through the default project adapter.
    ///
    /// In single-project mode (vnc-021), all requests go to default.
    /// W2-6 seam: extract slug from path prefix, lookup in stores map,
    /// fall back to default_project.
    async fn route_mcp(
        &mut self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        self.default_server.handle(request).await
    }
}

impl<ReqBody> Clone for ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn clone(&self) -> Self {
        ProjectRouter {
            default_server: self.default_server.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// McpAdapter — thin rmcp isolation boundary (ADR-003)
// ---------------------------------------------------------------------------

/// Thin adapter isolating `StreamableHttpService` from the rest of the HTTP
/// stack (ADR-003). Enforces body size limits before rmcp sees the request,
/// maps rmcp errors to consistent JSON responses, and provides a workaround
/// seam for extension propagation issues.
///
/// Body size enforcement uses a two-layer strategy:
/// 1. **Fast-path**: Content-Length header check rejects oversized requests
///    without reading any body bytes (zero-cost for well-behaved clients).
/// 2. **Stream-level**: `http_body_util::Limited` wraps the body stream to
///    enforce the limit regardless of transfer encoding (chunked TE fix,
///    GH #663). The body is fully collected before passing to rmcp.
///
/// R-01 spike result: extensions DO propagate through rmcp (the `Parts`
/// struct including `extensions` is injected into MCP message extensions).
/// The copy step is therefore a debug assertion, not a runtime fallback.
#[derive(Clone)]
pub(crate) struct McpAdapter {
    /// The rmcp StreamableHttpService. Isolated behind this adapter.
    streamable: StreamableHttpService<UnimatrixServer, LocalSessionManager>,
    /// Maximum request body size (bytes). Enforced before rmcp sees the body.
    max_body_bytes: usize,
}

impl std::fmt::Debug for McpAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAdapter")
            .field("max_body_bytes", &self.max_body_bytes)
            .finish_non_exhaustive()
    }
}

impl McpAdapter {
    /// Create a new McpAdapter wrapping a `StreamableHttpService`.
    fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let session_manager = Arc::new(LocalSessionManager::default());
        let config = StreamableHttpServerConfig::default();

        let streamable =
            StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

        McpAdapter {
            streamable,
            max_body_bytes,
        }
    }

    /// Handle an MCP request with body size enforcement and error mapping.
    ///
    /// Generic over the incoming body type so callers can pass any
    /// `Body` (e.g., `hyper::body::Incoming`). The body is consumed here;
    /// rmcp always receives `Request<Full<Bytes>>`.
    async fn handle<ReqBody>(
        &mut self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
    where
        ReqBody: Body + Send + 'static,
        ReqBody::Data: Send + 'static,
        ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        // Step 1: Fast-path Content-Length header check (zero-cost rejection).
        // Rejects obviously oversized requests without reading any body bytes.
        let exceeds_limit = request
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|len| len > self.max_body_bytes);

        if exceeds_limit {
            return Ok(payload_too_large_response());
        }

        // Step 2: Stream-level body collection with size limit (GH #663).
        // Limited wraps the body stream and returns LengthLimitError if the
        // accumulated data exceeds max_body_bytes. This catches chunked TE
        // bodies that omit Content-Length.
        let (parts, body) = request.into_parts();
        let limited_body = Limited::new(body, self.max_body_bytes);

        let collected = match limited_body.collect().await {
            Ok(collected) => collected,
            Err(err) => {
                // Distinguish size-limit errors from other body read errors
                // (e.g., client disconnect). Only return 413 for LengthLimitError.
                if err
                    .downcast_ref::<http_body_util::LengthLimitError>()
                    .is_some()
                {
                    return Ok(payload_too_large_response());
                }
                // Other body read errors (disconnect, malformed chunks) → 500.
                return Ok(internal_error_response());
            }
        };

        // Step 3: Reconstruct request with fully-collected body for rmcp.
        // R-01 validated: extensions propagate through rmcp via Parts injection.
        let full_request = Request::from_parts(parts, Full::new(collected.to_bytes()));
        let response = self.streamable.call(full_request).await;

        match response {
            Ok(resp) => Ok(resp),
            Err(never) => match never {},
        }
    }
}

// ---------------------------------------------------------------------------
// Error response helpers
// ---------------------------------------------------------------------------

/// 413 Payload Too Large response for oversized request bodies.
fn payload_too_large_response() -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(
                r#"{"error":"request body exceeds maximum size"}"#,
            ))
            .map_err(|never| match never {})
            .boxed(),
        )
        .expect("static response builder cannot fail")
}

/// 500 Internal Server Error response for body read failures (e.g., client
/// disconnect, malformed chunks). Distinct from 413 to avoid masking
/// non-size-related errors (GH #663).
fn internal_error_response() -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(r#"{"error":"failed to read request body"}"#))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("static response builder cannot fail")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
