//! Per-slug observe request handler and the shared HTTP error-response builders.
//!
//! Extracted from `router.rs` (vnc-038 CI-2 / AC-12) to keep that file under
//! the 500-line guideline. Pure relocation — no behavior change. The items are
//! re-exported through `router.rs` so existing call sites (and the `super::*`
//! test module) are unaffected.
//!
//! - [`route_observe`] is the per-slug observe handler on the per-request funnel
//!   (vnc-038 ADR-003 #5082): it resolves the per-request `Arc<Store>` from the
//!   transport-derived `ProjectKey::Slug` through the SAME `resolve_store` funnel
//!   as MCP — no boot-bound or parallel observe store.
//! - [`map_health_response`], [`payload_too_large_response`], and
//!   [`internal_error_response`] are the shared response builders.

use std::convert::Infallible;

use bytes::Bytes;
use http::{Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};

use http::Request;

use crate::mcp::identity::ResolvedIdentity;
use crate::uds::listener::dispatch_request;

use super::observe::{json_error_response, observe_response_to_http, prefix_session_id};
use super::{DEFAULT_MAX_BODY_BYTES, ObserveContext, RouteError, seam};

use unimatrix_engine::wire::HookRequest;

/// Per-slug observe handler on the per-request funnel (vnc-038 ADR-003 #5082).
///
/// Resolves the per-request `Arc<Store>` from the transport-derived
/// `ProjectKey::Slug` through the SAME `resolve_store` funnel as MCP — there is no
/// boot-bound or parallel observe store (the #4974 ceremonial-funnel guard). A
/// no-slug / unregistered / invalid-slug observe is a loud error, never a default
/// store (R-09/R-10).
pub(crate) async fn route_observe<ReqBody>(
    observe_ctx: ObserveContext,
    request: Request<ReqBody>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // Step 0: transport-derived identity via the SAME grammar as MCP, then resolve
    // the per-request store through the ONE funnel (ADR-003 — no boot-bound handle).
    let key = match seam::parse_project_key(request.uri().path()) {
        Ok(k) => k,
        Err(RouteError::InvalidSlug(_)) => {
            return Ok(json_error_response(
                StatusCode::BAD_REQUEST,
                "invalid project slug",
            ));
        }
        Err(RouteError::UnknownProject) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "unknown project",
            ));
        }
    };
    let store = match observe_ctx.resolver.resolve_store(&key) {
        Ok(s) => s,
        Err(RouteError::UnknownProject) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "unknown project",
            ));
        }
        Err(RouteError::InvalidSlug(_)) => {
            return Ok(json_error_response(
                StatusCode::BAD_REQUEST,
                "invalid project slug",
            ));
        }
    };

    // Step 0b: Resolve the per-slug observe state from the SAME funnel + SAME
    // `key` the store resolved from (vnc-046 ADR-001, FR-5/FR-6). registry and
    // pending are the #930 split-brain fix; services is the P2 read-leak fix
    // (R-09). A post-`resolve_store` `Err` here is a boot-wiring contradiction
    // (foreclosed by ADR-003's boot assertion) → 500, NEVER 404, never panic
    // (R-14). The genuine unregistered-slug 404 already fired at `resolve_store`.
    let registry = match observe_ctx.resolver.registry_for(&key) {
        Ok(r) => r,
        Err(_) => return Ok(internal_error_response()),
    };
    let pending = match observe_ctx.resolver.pending_for(&key) {
        Ok(p) => p,
        Err(_) => return Ok(internal_error_response()),
    };
    let services = match observe_ctx.resolver.services_for(&key) {
        Ok(s) => s,
        Err(_) => return Ok(internal_error_response()),
    };

    // Step 1: Extract ResolvedIdentity from request extensions.
    let identity = match request.extensions().get::<ResolvedIdentity>() {
        Some(id) => id.clone(),
        None => {
            tracing::error!("observe: ResolvedIdentity missing from extensions");
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

    // Step 2b: Read Accept header for content negotiation (ADR-003, vnc-024).
    // MUST be read here, before `into_parts()` consumes the request — a late
    // read silently loses the header and falls back to JSON (Constraint 2 / R-07).
    // `wants_text` is true iff the Accept value contains "text/plain".
    let wants_text = request
        .headers()
        .get(http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/plain"));

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

    // Step 6: Call dispatch_request with HTTP capabilities. The per-request
    // `store` serves BOTH the `store` and `entry_store` params (the boot pairing
    // preserved per-request, ADR-003). registry/pending/services are the per-slug
    // handles resolved at Step 0b (vnc-046) — NOT global `ObserveContext` fields.
    let response = dispatch_request(
        hook_request,
        &store,
        &observe_ctx.embed_service,
        &store,
        &observe_ctx.server_version,
        &registry,
        &pending,
        &services,
        &identity.capabilities,
    )
    .await;

    // Step 7: Map HookResponse to HTTP response (content negotiation, ADR-003).
    Ok(observe_response_to_http(response, wants_text))
}

/// Map the health response (String body) to BoxBody for consistency.
pub(crate) fn map_health_response(resp: Response<String>) -> Response<BoxBody<Bytes, Infallible>> {
    let (parts, body) = resp.into_parts();
    Response::from_parts(
        parts,
        Full::new(Bytes::from(body))
            .map_err(|never| match never {})
            .boxed(),
    )
}

/// 413 Payload Too Large response for oversized request bodies.
pub(crate) fn payload_too_large_response() -> Response<BoxBody<Bytes, Infallible>> {
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
pub(crate) fn internal_error_response() -> Response<BoxBody<Bytes, Infallible>> {
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
