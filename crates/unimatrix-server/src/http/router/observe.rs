//! /observe handler helpers: response mapping, session ID prefixing, error responses (vnc-022).

use std::convert::Infallible;

use bytes::Bytes;
use http::{Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use unimatrix_engine::wire::{HookRequest, HookResponse};

use super::internal_error_response;
use crate::uds::hook::{MAX_INJECTION_BYTES, format_injection};

/// Single formatting truth shared by the HTTP `/observe` text/plain path and the UDS
/// `Text` branch (`listener.rs::handle_connection`) — vnc-027 ADR-001 §5.
///
/// Returns `Some(text)` for the two injection-bearing variants, `None` otherwise (the
/// caller maps `None` -> 204 / `Ack`). The allowlist is exactly `{Entries, BriefingContent}`;
/// `Pong`/`Ack`/`Error` never convert. Because BOTH transports call this one function,
/// UDS `Text` bodies and HTTP `text/plain` bodies are byte-identical by construction
/// (R-09 s3, parity-by-construction — vnc-025 ADR-005 lesson).
///
/// - `Entries`   -> `format_injection(items, MAX_INJECTION_BYTES)` (includes the load-bearing
///   `--- Unimatrix Context ---\n` header; `None` when empty / over-budget).
/// - `BriefingContent` -> `content` verbatim (NO header prepended).
pub(crate) fn response_injection_text(resp: &HookResponse) -> Option<String> {
    match resp {
        HookResponse::Entries { items, .. } => format_injection(items, MAX_INJECTION_BYTES),
        HookResponse::BriefingContent { content, .. } => Some(content.clone()),
        _ => None,
    }
}

/// Map a HookResponse to an HTTP response (ADR-004; ADR-003 content negotiation, vnc-024).
///
/// JSON path (`wants_text == false`, or non-injection response):
/// - Ack -> 204 No Content (empty body)
/// - Entries/BriefingContent/Pong -> 200 OK + JSON body
/// - Error -> 400 Bad Request + JSON body
///
/// Text path (`wants_text == true`, HTTP-only): the two injection-bearing responses are
/// formatted as `text/plain` via the single formatting truth `response_injection_text`
/// (-> `format_injection`, Constraint 4 / AC-07 byte-identity, shared with the UDS `Text`
/// branch). `Pong`/`Ack`/`Error` fall through to JSON regardless of `Accept` (the allowlist
/// is exactly `{Entries, BriefingContent}`, R-06).
pub(crate) fn observe_response_to_http(
    resp: HookResponse,
    wants_text: bool,
) -> Response<BoxBody<Bytes, Infallible>> {
    // Content-negotiated text path: Entries / BriefingContent only (ADR-003), via the
    // shared injection-text core so HTTP and UDS bodies are byte-identical (ADR-001 §5).
    if wants_text {
        if let Some(text) = response_injection_text(&resp) {
            return http_200_text_plain(text);
        }
        // Entries that formatted to None (empty / over-budget) -> 204, not 200/500.
        // Pong / Ack / Error fall through to the unchanged JSON envelope below
        // (Pong.server_version is parsed structured, R-06).
        if matches!(resp, HookResponse::Entries { .. }) {
            return http_204_no_content();
        }
    }

    match resp {
        HookResponse::Ack => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(
                Full::new(Bytes::new())
                    .map_err(|never| match never {})
                    .boxed(),
            )
            .expect("static response builder cannot fail"),

        // `Text` is a UDS-only variant (vnc-027 ADR-001 §3): it is constructed in the
        // listener post-dispatch, never returned by the HTTP dispatch path. Handled here
        // only to keep the match exhaustive; falls into the JSON envelope harmlessly.
        HookResponse::Entries { .. }
        | HookResponse::BriefingContent { .. }
        | HookResponse::Text { .. }
        | HookResponse::Pong { .. } => match serde_json::to_vec(&resp) {
            Ok(body) => Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(
                    Full::new(Bytes::from(body))
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("response builder cannot fail"),
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize HookResponse");
                internal_error_response()
            }
        },

        HookResponse::Error { .. } => match serde_json::to_vec(&resp) {
            Ok(body) => Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("content-type", "application/json")
                .body(
                    Full::new(Bytes::from(body))
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("response builder cannot fail"),
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize HookResponse::Error");
                internal_error_response()
            }
        },
    }
}

/// 200 OK with a `text/plain` body (content-negotiated injection text, ADR-003).
fn http_200_text_plain(body: String) -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain")
        .body(
            Full::new(Bytes::from(body))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("static response builder cannot fail")
}

/// 204 No Content with an empty body (empty/over-budget Entries text path, ADR-003).
fn http_204_no_content() -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(
            Full::new(Bytes::new())
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("static response builder cannot fail")
}

/// Prefix client-supplied session_id with "http-" for transport scoping (ADR-003).
///
/// Day 1: constant "http-" prefix for all HTTP callers.
/// W2-3 evolution: "http-{subject_hash}-" for per-user isolation under OAuth.
pub(crate) fn prefix_session_id(request: &mut HookRequest) {
    match request {
        HookRequest::SessionRegister { session_id, .. }
        | HookRequest::SessionClose { session_id, .. }
        | HookRequest::CompactPayload { session_id, .. } => {
            *session_id = format!("http-{session_id}");
        }
        HookRequest::RecordEvent { event } => {
            event.session_id = format!("http-{}", event.session_id);
        }
        HookRequest::RecordEvents { events } => {
            for event in events.iter_mut() {
                event.session_id = format!("http-{}", event.session_id);
            }
        }
        HookRequest::ContextSearch { session_id, .. } => {
            if let Some(sid) = session_id {
                *sid = format!("http-{sid}");
            }
        }
        HookRequest::Ping | HookRequest::Briefing { .. } => {
            // No session_id to prefix.
        }
    }
}

/// JSON error response for handler-level errors (not from dispatch_request).
///
/// Body format: `{"error":"<message>"}` -- distinct from `HookResponse::Error` which
/// has `{"type":"Error","code":N,"message":"..."}`.
pub(crate) fn json_error_response(
    status: StatusCode,
    message: &str,
) -> Response<BoxBody<Bytes, Infallible>> {
    let body = serde_json::json!({"error": message}).to_string();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(body))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("response builder cannot fail")
}
