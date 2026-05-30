//! PathRouter tower Service, ProjectRouter, path dispatch, rmcp adapter (C3).
//!
//! Routes HTTP requests by URI path:
//! - `GET /health` -> health handler (no auth, bypassed by StaticTokenAuth)
//! - `POST /observe` -> observation handler (auth required, vnc-022)
//! - `/* (everything else)` -> MCP dispatch through ProjectRouter -> McpAdapter
//!
//! Contains `ProjectRouter` as the W2-6 structural seam and `McpAdapter` as
//! the rmcp isolation boundary (ADR-003).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tower::Service;
use unimatrix_adapt::AdaptationService;
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{Store, VectorAdapter};

use crate::http::health::health_response;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::mcp::identity::ResolvedIdentity;
use crate::server::{PendingEntriesAnalysis, UnimatrixServer};
use crate::services::ServiceLayer;
use crate::uds::listener::dispatch_request;
use unimatrix_engine::wire::HookRequest;

/// Path for the remote telemetry endpoint (FR-24, W2-7 future).
pub(crate) const OBSERVE_PATH: &str = "/observe";

/// Maximum request body size default (1 MB). Used when no config override.
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// ObserveContext — service handle bundle for /observe handler (ADR-001)
// ---------------------------------------------------------------------------

/// Service handle bundle for the `/observe` handler (ADR-001).
///
/// Holds Arc-cloned references to the subset of `UnimatrixServer` fields
/// needed by `dispatch_request()`. Constructed once in `main.rs`, stored
/// on `PathRouter`, referenced by the `/observe` handler.
///
/// Intentionally NOT the same as `UnimatrixServer` — carries only what
/// `dispatch_request` needs, not MCP-specific state.
///
/// **Risk R-01**: `store` and `entry_store` have identical types (`Arc<Store>`).
/// A positional swap compiles but corrupts the pipeline. `store` is the primary
/// knowledge store passed as `dispatch_request`'s second parameter. `entry_store`
/// is the entry-specific store passed as its fifth parameter. In the current
/// codebase, both point to the same `Arc<Store>` instance — but the field names
/// must track `dispatch_request`'s parameter names, not the backing instance.
#[derive(Clone)]
pub struct ObserveContext {
    /// Primary knowledge store (dispatch_request param 2: `store`).
    pub store: Arc<Store>,
    /// Embedding service handle (dispatch_request param 3: `embed_service`).
    pub embed_service: Arc<EmbedServiceHandle>,
    /// Async vector store (dispatch_request param 4: `vector_store`).
    pub vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    /// Entry-specific store (dispatch_request param 5: `entry_store`).
    /// Same backing instance as `store` today, but named to match the parameter.
    pub entry_store: Arc<Store>,
    /// Adaptation service (dispatch_request param 6: `adapt_service`).
    pub adapt_service: Arc<AdaptationService>,
    /// Server version string (dispatch_request param 7: `server_version`).
    pub server_version: String,
    /// Session lifecycle registry (dispatch_request param 8: `session_registry`).
    pub session_registry: Arc<SessionRegistry>,
    /// Pending entries analysis state (dispatch_request param 9).
    pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    /// Shared service layer (dispatch_request param 10: `services`).
    pub services: ServiceLayer,
}

// ---------------------------------------------------------------------------
// PathRouter — top-level path-dispatching tower Service
// ---------------------------------------------------------------------------

/// Tower service that dispatches requests by URI path.
///
/// - `GET /health` -> JSON health response
/// - `POST /observe` -> observation handler (vnc-022)
/// - `/* (everything else)` -> MCP via ProjectRouter
pub struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    project_router: ProjectRouter<ReqBody>,
    observe_ctx: ObserveContext,
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
            .field("observe_ctx", &"ObserveContext{..}")
            .finish()
    }
}

impl<ReqBody> PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Create a new PathRouter wrapping a ProjectRouter and an ObserveContext.
    pub fn new(project_router: ProjectRouter<ReqBody>, observe_ctx: ObserveContext) -> Self {
        PathRouter {
            project_router,
            observe_ctx,
        }
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
            observe_ctx: self.observe_ctx.clone(),
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
                // vnc-022: Real /observe handler. Auth enforced by upstream StaticTokenAuth.
                let observe_ctx = self.observe_ctx.clone();
                Box::pin(async move {
                    // Step 1: Extract ResolvedIdentity from request extensions.
                    let identity = match request.extensions().get::<ResolvedIdentity>() {
                        Some(id) => id.clone(),
                        None => {
                            tracing::error!(
                                "POST /observe: ResolvedIdentity missing from extensions"
                            );
                            return Ok(json_error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "internal error: identity not resolved",
                            ));
                        }
                    };

                    // Step 2: Content-Length fast-path size check (same pattern as McpAdapter).
                    let exceeds_limit = request
                        .headers()
                        .get(http::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .is_some_and(|len| len > DEFAULT_MAX_BODY_BYTES);

                    if exceeds_limit {
                        return Ok(payload_too_large_response());
                    }

                    // Step 3: Stream-level body collection with size limit (GH #663 pattern).
                    let (_parts, body) = request.into_parts();
                    let limited_body = Limited::new(body, DEFAULT_MAX_BODY_BYTES);

                    let collected = match limited_body.collect().await {
                        Ok(collected) => collected.to_bytes(),
                        Err(err) => {
                            if err
                                .downcast_ref::<http_body_util::LengthLimitError>()
                                .is_some()
                            {
                                return Ok(payload_too_large_response());
                            }
                            return Ok(internal_error_response());
                        }
                    };

                    // Step 4: Deserialize HookRequest from JSON.
                    let mut hook_request: HookRequest = match serde_json::from_slice(&collected) {
                        Ok(req) => req,
                        Err(e) => {
                            return Ok(json_error_response(
                                StatusCode::BAD_REQUEST,
                                &format!("invalid request JSON: {e}"),
                            ));
                        }
                    };

                    // Step 5: Prefix session_id with "http-" (ADR-003).
                    prefix_session_id(&mut hook_request);

                    // Step 6: Call dispatch_request with HTTP capabilities.
                    let response = dispatch_request(
                        hook_request,
                        &observe_ctx.store,
                        &observe_ctx.embed_service,
                        &observe_ctx.vector_store,
                        &observe_ctx.entry_store,
                        &observe_ctx.adapt_service,
                        &observe_ctx.server_version,
                        &observe_ctx.session_registry,
                        &observe_ctx.pending_entries_analysis,
                        &observe_ctx.services,
                        &identity.capabilities,
                    )
                    .await;

                    // Step 7: Map HookResponse to HTTP response.
                    Ok(observe_response_to_http(response))
                })
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

mod observe;
use observe::{json_error_response, observe_response_to_http, prefix_session_id};

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
    pub fn new(
        server: UnimatrixServer,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
    ) -> Self {
        let mcp_adapter = McpAdapter::new(server, max_body_bytes, allowed_origins);
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
    ///
    /// `allowed_origins` configures Origin header validation (ADR-002).
    /// Empty vec = no origin restriction (backward-compatible default).
    /// `allowed_hosts` is NOT modified — rmcp defaults it to localhost,
    /// which is the CVE-2026-42559 fix.
    fn new(server: UnimatrixServer, max_body_bytes: usize, allowed_origins: Vec<String>) -> Self {
        let session_manager = Arc::new(LocalSessionManager::default());
        let mut config = StreamableHttpServerConfig::default();
        config.allowed_origins = allowed_origins;

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
