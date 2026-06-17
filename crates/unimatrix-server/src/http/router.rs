//! PathRouter tower Service, the SlugRouter funnel edge, per-slug observe, and
//! the rmcp `McpAdapter` (C3).
//!
//! Routes HTTP requests by URI path (vnc-038 ADR-003/004):
//! - `GET /health` -> health handler (no auth, bypassed by StaticTokenAuth)
//! - `POST /v1/{slug}/observe` -> per-slug observe handler, resolved per-request
//!   through the SAME funnel as MCP (auth required, vnc-022/vnc-038 ADR-003)
//! - `/* (everything else)` -> MCP dispatch through the `SlugRouter` funnel ->
//!   `resolve_store` -> per-slug `McpAdapter`
//!
//! The top-level `/observe` route and the `DefaultResolver` are DELETED (vnc-038
//! ADR-003/004): there is no default store; observe is per-slug only. `McpAdapter`
//! is the rmcp isolation boundary (ADR-003).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response};
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
use crate::server::{PendingEntriesAnalysis, UnimatrixServer};
use crate::services::ServiceLayer;

/// Maximum request body size default (1 MB). Used when no config override.
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;

// ---------------------------------------------------------------------------
// ObserveContext — service handle bundle for /observe handler (ADR-001)
// ---------------------------------------------------------------------------

/// Service handle bundle for the per-slug observe handler (ADR-001/003).
///
/// Holds Arc-cloned references to the subset of `UnimatrixServer` fields
/// needed by `dispatch_request()`. Constructed once in `main.rs`, stored
/// on `PathRouter`, referenced by the `/v1/{slug}/observe` handler.
///
/// Intentionally NOT the same as `UnimatrixServer` — carries only what
/// `dispatch_request` needs, not MCP-specific state.
///
/// **vnc-038 ADR-003 (#5082)**: observe is now a per-slug route on the SAME
/// per-request funnel as MCP. This context holds the `Arc<dyn StoreResolver>`
/// (the SAME resolver `SlugRouter` holds) instead of a pre-resolved single store.
/// The store is resolved PER CALL from the transport-derived `ProjectKey::Slug`;
/// the boot-bound `resolve_store(&ProjectKey::Default)` and the
/// `store`/`entry_store` fixed handles are DELETED (the #4974 ceremonial-funnel
/// guard — no boot-bound or parallel observe store path).
#[derive(Clone)]
pub struct ObserveContext {
    /// The store-resolution funnel — the SAME `Arc<dyn StoreResolver>` the
    /// `SlugRouter` holds (one funnel, two entry handlers). The per-request store
    /// is resolved from it on each observe call (ADR-003 #5082); both
    /// `dispatch_request` `store` and `entry_store` params get that one resolved
    /// handle (the boot pairing preserved per-request).
    pub resolver: Arc<dyn StoreResolver>,
    /// Embedding service handle (dispatch_request param 3: `embed_service`).
    pub embed_service: Arc<EmbedServiceHandle>,
    /// Async vector store (dispatch_request param 4: `vector_store`).
    pub vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
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
/// - `/* (everything else)` -> MCP via the `SlugRouter` single-funnel seam
///
/// vnc-034 (ADR-003): the MCP fall-through arm dispatches through `SlugRouter`
/// (the per-request `parse_project_key -> resolve_store -> dispatch` funnel),
/// NOT `ProjectRouter` directly — the store reaches MCP only THROUGH the seam
/// (FR-X5, no bypass). Wave 2 swaps the injected resolver at the same call site
/// (R-01 sc.2).
pub struct PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    slug_router: SlugRouter,
    observe_ctx: ObserveContext,
    _phantom: std::marker::PhantomData<fn(ReqBody)>,
}

impl<ReqBody> std::fmt::Debug for PathRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathRouter")
            .field("slug_router", &self.slug_router)
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
    /// Create a `PathRouter` whose MCP edge is the `SlugRouter` single funnel.
    ///
    /// Takes ONLY the injected `StoreResolver` (Wave 1: `DefaultResolver`;
    /// Wave 2: `MultiProjectRouter`) and builds the `SlugRouter` internally. The
    /// resolver now OWNS per-key dispatch (`adapter_for`), so there is no longer
    /// a fixed `ProjectRouter` parameter that could service MCP as a fallback
    /// (vnc-034 Wave 2 funnel-elimination, OQ-PR-8/9). The `resolver` argument
    /// alone is the Wave 1 <-> Wave 2 swap point (R-01 sc.2).
    pub fn new(resolver: Arc<dyn StoreResolver>, observe_ctx: ObserveContext) -> Self {
        PathRouter {
            slug_router: SlugRouter::new(resolver),
            observe_ctx,
            _phantom: std::marker::PhantomData,
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
            slug_router: self.slug_router.clone(),
            observe_ctx: self.observe_ctx.clone(),
            _phantom: std::marker::PhantomData,
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

        // `/health` stays top-level (store-independent). vnc-038 ADR-003 (#5082):
        // the top-level `/observe` arm is REMOVED — observe is now a per-slug route
        // `POST /v1/{slug}/observe`, detected by suffix and dispatched through the
        // SAME funnel as MCP (`route_observe`). Everything else is MCP via the
        // `SlugRouter` (`route_mcp`).
        match (&method, path.as_str()) {
            (&Method::GET, "/health") => {
                // Route to health handler. Auth already bypassed by StaticTokenAuth (ADR-002).
                let resp = handlers::map_health_response(health_response());
                Box::pin(async move { Ok(resp) })
            }
            (&Method::POST, p) if p.starts_with("/v1/") && p.ends_with("/observe") => {
                // vnc-038 ADR-003: per-slug observe on the per-request funnel.
                // Auth enforced by upstream StaticTokenAuth.
                let observe_ctx = self.observe_ctx.clone();
                Box::pin(async move { handlers::route_observe(observe_ctx, request).await })
            }
            (_, _) => {
                // Route everything else to MCP via the SlugRouter single funnel
                // (vnc-034 ADR-003): parse_project_key -> resolve_store -> dispatch.
                // The store reaches MCP only THROUGH the seam (FR-X5, no bypass).
                let mut router = self.slug_router.clone();
                Box::pin(async move { router.route_mcp(request).await })
            }
        }
    }
}

// Per-slug observe handler and the shared HTTP error-response builders live in
// the `handlers` submodule (vnc-038 CI-2 / AC-12 — keeps router.rs under 500
// lines). Re-exported below so the `super::*` test module and existing call
// sites are unaffected (pure relocation, no behavior change).
mod handlers;
// The observe handler and its `StatusCode` / `HookRequest` dependencies moved
// into `handlers`; the `super::*` test module still references those types, so
// they are re-imported test-only here (no runtime use remains in this file).
#[cfg(test)]
use http::StatusCode;
#[cfg(test)]
use unimatrix_engine::wire::HookRequest;
// `internal_error_response` / `payload_too_large_response` are runtime builders
// reached by `McpAdapter` (this file) and `observe.rs` (via `super::`), so they
// stay re-exported in all builds. `map_health_response` is only reached via the
// `handlers::` path and the `super::*` test module, so it is test-gated.
#[cfg(test)]
pub(crate) use handlers::map_health_response;
pub(crate) use handlers::{internal_error_response, payload_too_large_response};

// `pub(crate)` so the UDS listener can reach the shared injection-text core
// (`response_injection_text`) — the single formatting truth shared by both transports
// (vnc-027 ADR-001 §5).
pub(crate) mod observe;
// Re-exported for the `super::*` test module only (the runtime callers are in
// `handlers`, which imports from `observe` directly).
#[cfg(test)]
use observe::{json_error_response, observe_response_to_http, prefix_session_id};

// C4 isolation seam (vnc-034 ADR-003/004/005). Extracted to a submodule to keep
// router.rs under the 500-line limit. Wave-1 MINIMAL: route grammar + trait +
// SlugRouter layer + the ProjectSlug allowlist parse edge. The Wave 1 <-> Wave 2
// boundary IS the `StoreResolver` trait — Wave 2 swaps the injected resolver at
// the SAME `SlugRouter::new` call site, no change to this layer or the grammar.
pub(crate) mod seam;
#[cfg(test)]
pub(crate) use seam::parse_project_key;
// vnc-034: the seam is WIRED — `PathRouter` holds a `SlugRouter` as its
// per-request MCP edge (above), and (Wave 2) `SlugRouter` dispatches through
// `StoreResolver::adapter_for`, so the resolver is BOTH the store funnel and the
// sole MCP dispatch route. `ProjectKey`/`ProjectSlug`/`RouteError` remain
// re-exported as the public seam surface (consumed by `main.rs` via `http/mod.rs`
// and by the resolver impls).
pub use seam::{ProjectKey, ProjectSlug, RouteError, SlugRouter, StoreResolver};

// vnc-038 ADR-004 (#5083): the Wave-1 `DefaultResolver` (single store served for
// `ProjectKey::Default`) is DELETED. `MultiProjectRouter` is the sole
// `StoreResolver`; single project is N=1 through the same slug-keyed path, no
// default store and no default arm. Local STDIO/UDS keeps its DIRECT path-hash
// binding and never enters the resolver (ADR-006 #5087).

// ---------------------------------------------------------------------------
// MultiProjectRouter — the Wave-2 `StoreResolver` (slug -> per-slug entry)
// ---------------------------------------------------------------------------
//
// vnc-034 Wave 2 funnel-elimination: the old single-project HTTP
// `ProjectRouter<ReqBody>` (a fixed `McpAdapter` behind the seam) is GONE. The
// per-key `McpAdapter` map now lives INSIDE the resolver (`MultiProjectRouter`,
// `project_resolver.rs`), and `SlugRouter` dispatches through
// `StoreResolver::adapter_for` — the SOLE dispatch route, no fixed fallback
// (ADR-003 "per-slug routing inside the seam"; OQ-PR-8/9).
//
// The resolver and its `ProjectEntry` live in the `project_resolver` submodule
// to keep this file under the 500-line limit (mirrors `default_resolver.rs`).
pub(crate) mod project_resolver;
pub use project_resolver::{MultiProjectRouter, ProjectServerInput};

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
    /// The `Arc<Store>` this adapter dispatches against (vnc-034 Wave 2).
    ///
    /// Held purely so the `SlugRouter` funnel can assert resolve/dispatch
    /// agreement (`wraps_store`, OQ-PR-4): the adapter `adapter_for(&key)`
    /// returns MUST wrap the SAME store `resolve_store(&key)` returned. Not used
    /// on the dispatch hot path — `streamable` owns the live `UnimatrixServer`.
    store: Arc<Store>,
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

        // Capture the adapter's store BEFORE `server` is moved into the rmcp
        // closure (vnc-034 Wave 2 — resolve/dispatch agreement, OQ-PR-4).
        let store = Arc::clone(&server.store);

        let streamable =
            StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

        McpAdapter {
            streamable,
            max_body_bytes,
            store,
        }
    }

    /// True iff this adapter dispatches against `store` (`Arc::ptr_eq` identity).
    ///
    /// Used only by the `SlugRouter` funnel's `debug_assert!` to prove the
    /// adapter `adapter_for(&key)` returned wraps the SAME store
    /// `resolve_store(&key)` returned — resolution and dispatch can never diverge
    /// (vnc-034 OQ-PR-4). Store has no `PartialEq`; identity is `Arc::ptr_eq`.
    pub(crate) fn wraps_store(&self, store: &Arc<Store>) -> bool {
        Arc::ptr_eq(&self.store, store)
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
