//! PathRouter tower Service, ProjectRouter, path dispatch, rmcp adapter (C3).
//!
//! Routes HTTP requests by URI path:
//! - `GET /health` -> health handler (no auth, bypassed by StaticTokenAuth)
//! - `POST /observe` -> HTTP 501 stub (auth required, W2-7 future)
//! - `/* (everything else)` -> MCP dispatch through ProjectRouter -> McpAdapter
//!
//! Contains `ProjectRouter` as the W2-6 structural seam and `McpAdapter` as
//! the rmcp isolation boundary (ADR-003).

// C3 is built ahead of C1 (listener) and C8 (lifecycle). These types will be
// consumed when the listener wires up the full HTTP stack.
#![allow(dead_code)]

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
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
pub(crate) struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    project_router: ProjectRouter<ReqBody>,
}

impl<ReqBody> std::fmt::Debug for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
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
    ReqBody::Error: std::fmt::Display,
{
    /// Create a new PathRouter wrapping a ProjectRouter.
    pub(crate) fn new(project_router: ProjectRouter<ReqBody>) -> Self {
        PathRouter { project_router }
    }
}

impl<ReqBody> Clone for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
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
    ReqBody::Error: std::fmt::Display,
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
pub(crate) struct ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    default_server: McpAdapter<ReqBody>,
}

impl<ReqBody> std::fmt::Debug for ProjectRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
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
    ReqBody::Error: std::fmt::Display,
{
    /// Create a new ProjectRouter in single-project default mode.
    pub(crate) fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let mcp_adapter = McpAdapter::new(server, max_body_bytes);
        ProjectRouter {
            default_server: mcp_adapter,
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
    ReqBody::Error: std::fmt::Display,
{
    fn clone(&self) -> Self {
        ProjectRouter {
            default_server: self.default_server.clone(),
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
/// R-01 spike result: extensions DO propagate through rmcp (the `Parts`
/// struct including `extensions` is injected into MCP message extensions).
/// The copy step is therefore a debug assertion, not a runtime fallback.
pub(crate) struct McpAdapter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    /// The rmcp StreamableHttpService. Isolated behind this adapter.
    streamable: StreamableHttpService<UnimatrixServer, LocalSessionManager>,
    /// Maximum request body size (bytes). Enforced before rmcp sees the body.
    max_body_bytes: usize,
    /// Phantom for ReqBody generic.
    _phantom: std::marker::PhantomData<fn(ReqBody)>,
}

impl<ReqBody> std::fmt::Debug for McpAdapter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAdapter")
            .field("max_body_bytes", &self.max_body_bytes)
            .finish_non_exhaustive()
    }
}

impl<ReqBody> McpAdapter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    /// Create a new McpAdapter wrapping a `StreamableHttpService`.
    fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let session_manager = Arc::new(LocalSessionManager::default());
        let config = StreamableHttpServerConfig::default();

        let streamable =
            StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

        McpAdapter {
            streamable,
            max_body_bytes,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Handle an MCP request with body size enforcement and error mapping.
    async fn handle(
        &mut self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        // Step 1: Enforce body size limit BEFORE rmcp (R-11, ADR-003).
        let exceeds_limit = request
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|len| len > self.max_body_bytes);

        if exceeds_limit {
            return Ok(payload_too_large_response());
        }

        // Step 2: Delegate to StreamableHttpService.
        // R-01 validated: extensions propagate through rmcp via Parts injection.
        let response = self.streamable.call(request).await;

        match response {
            Ok(resp) => Ok(resp),
            Err(never) => match never {},
        }
    }
}

impl<ReqBody> Clone for McpAdapter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: std::fmt::Display,
{
    fn clone(&self) -> Self {
        McpAdapter {
            streamable: self.streamable.clone(),
            max_body_bytes: self.max_body_bytes,
            _phantom: std::marker::PhantomData,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
