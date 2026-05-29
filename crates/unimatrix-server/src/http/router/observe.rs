//! /observe handler helpers: response mapping, session ID prefixing, error responses (vnc-022).

use std::convert::Infallible;

use bytes::Bytes;
use http::{Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use unimatrix_engine::wire::{HookRequest, HookResponse};

use super::internal_error_response;

/// Map a HookResponse to an HTTP response (ADR-004).
///
/// - Ack -> 204 No Content (empty body)
/// - Entries/BriefingContent/Pong -> 200 OK + JSON body
/// - Error -> 400 Bad Request + JSON body
pub(crate) fn observe_response_to_http(resp: HookResponse) -> Response<BoxBody<Bytes, Infallible>> {
    match resp {
        HookResponse::Ack => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(
                Full::new(Bytes::new())
                    .map_err(|never| match never {})
                    .boxed(),
            )
            .expect("static response builder cannot fail"),

        HookResponse::Entries { .. }
        | HookResponse::BriefingContent { .. }
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
