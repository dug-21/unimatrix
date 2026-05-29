# observe-handler: Replace 501 Stub with Real /observe Handler

## Purpose

Replace `observe_stub_response()` in `PathRouter::call()` with a real async handler that: collects and size-limits the body, deserializes `HookRequest`, extracts `ResolvedIdentity` from extensions, prefixes the session_id, calls `dispatch_request`, and maps `HookResponse` to HTTP status codes.

## File: `crates/unimatrix-server/src/http/router.rs`

### Required New Imports

```
use crate::infra::registry::Capability;
use crate::mcp::identity::ResolvedIdentity;
use crate::uds::listener::dispatch_request;
use unimatrix_engine::wire::{HookRequest, HookResponse};
```

Note: `Limited`, `BodyExt`, `Full`, `Bytes`, `BoxBody`, `Infallible`, `StatusCode` are already imported. Verify each import against what already exists.

### Modified: PathRouter::call() -- POST /observe arm

**Current** (lines 115-119):
```
(Method::POST, "/observe") => {
    // 501 stub (FR-20). Auth enforced by upstream StaticTokenAuth.
    let resp = observe_stub_response();
    Box::pin(async move { Ok(resp) })
}
```

**After** -- pseudocode for the replacement:

```
(Method::POST, "/observe") => {
    // vnc-022: Real /observe handler. Auth enforced by upstream StaticTokenAuth.
    let observe_ctx = self.observe_ctx.clone();
    Box::pin(async move {
        // Step 1: Extract ResolvedIdentity from request extensions.
        // StaticTokenAuth inserts this for all authenticated requests.
        // If missing, the middleware failed to run -- should not happen
        // in normal operation.
        let identity = match request.extensions().get::<ResolvedIdentity>() {
            Some(id) => id.clone(),
            None => {
                // Defensive: middleware should have rejected unauthenticated requests.
                // Log at error level -- indicates middleware misconfiguration.
                tracing::error!("POST /observe: ResolvedIdentity missing from extensions");
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
        // Applied to whichever HookRequest variant carries a session_id.
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
```

### Remove: observe_stub_response Function

Delete the entire `observe_stub_response()` function (lines 141-153). It is replaced by the inline handler above. No other code references it.

### New Function: observe_response_to_http

Maps `HookResponse` variants to HTTP responses per ADR-004.

```
/// Map a HookResponse to an HTTP response (ADR-004).
///
/// - Ack -> 204 No Content (empty body)
/// - Entries/BriefingContent/Pong -> 200 OK + JSON body
/// - Error -> 400 Bad Request + JSON body
fn observe_response_to_http(resp: HookResponse) -> Response<BoxBody<Bytes, Infallible>> {
    match resp {
        HookResponse::Ack => {
            // 204 No Content, empty body. Fire-and-forget events.
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(
                    Full::new(Bytes::new())
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("static response builder cannot fail")
        }

        HookResponse::Entries { .. }
        | HookResponse::BriefingContent { .. }
        | HookResponse::Pong { .. } => {
            // 200 OK + JSON serialized HookResponse.
            // serde_json::to_vec serializes with the #[serde(tag = "type")] discriminator.
            match serde_json::to_vec(&resp) {
                Ok(body) => {
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .body(
                            Full::new(Bytes::from(body))
                                .map_err(|never| match never {})
                                .boxed(),
                        )
                        .expect("response builder cannot fail")
                }
                Err(e) => {
                    // Serialization failure is a server bug -- should never happen
                    // for well-formed HookResponse variants.
                    tracing::error!(error = %e, "failed to serialize HookResponse");
                    internal_error_response()
                }
            }
        }

        HookResponse::Error { .. } => {
            // 400 Bad Request + JSON serialized HookResponse::Error.
            match serde_json::to_vec(&resp) {
                Ok(body) => {
                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(
                            Full::new(Bytes::from(body))
                                .map_err(|never| match never {})
                                .boxed(),
                        )
                        .expect("response builder cannot fail")
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to serialize HookResponse::Error");
                    internal_error_response()
                }
            }
        }
    }
}
```

### New Function: prefix_session_id

Mutates the session_id field of applicable HookRequest variants by prepending "http-".

```
/// Prefix client-supplied session_id with "http-" for transport scoping (ADR-003).
///
/// Day 1: constant "http-" prefix for all HTTP callers.
/// W2-3 evolution: "http-{subject_hash}-" for per-user isolation under OAuth.
fn prefix_session_id(request: &mut HookRequest) {
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
            // ContextSearch session_id is Option<String>
            if let Some(sid) = session_id {
                *sid = format!("http-{sid}");
            }
        }
        HookRequest::Ping | HookRequest::Briefing { .. } => {
            // No session_id to prefix.
        }
    }
}
```

**Important**: Verify the session_id field types for each HookRequest variant:
- SessionRegister, SessionClose, CompactPayload: `session_id: String`
- RecordEvent: `event: ImplantEvent` where ImplantEvent has `pub session_id: String`
- RecordEvents: `events: Vec<ImplantEvent>`
- ContextSearch: `session_id: Option<String>`
- Ping: no session_id
- Briefing: no session_id

### New Function: json_error_response

Utility for handler-level error responses (deserialization failure, missing identity).

```
/// JSON error response for handler-level errors (not from dispatch_request).
///
/// Body format: {"error":"<message>"} -- distinct from HookResponse::Error which
/// has {"type":"Error","code":N,"message":"..."}.
fn json_error_response(
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
```

### Update: Module Doc Comment

The module doc comment (line 1-9) references the 501 stub. Update to reflect the real handler:

```
//! PathRouter tower Service, ProjectRouter, path dispatch, rmcp adapter (C3).
//!
//! Routes HTTP requests by URI path:
//! - `GET /health` -> health handler (no auth, bypassed by StaticTokenAuth)
//! - `POST /observe` -> observation handler (auth required, vnc-022)
//! - `/* (everything else)` -> MCP dispatch through ProjectRouter -> McpAdapter
```

### Update: PathRouter Doc Comment

The PathRouter struct doc comment (line 39-43) references the 501 stub. Update:

```
/// Tower service that dispatches requests by URI path.
///
/// - `GET /health` -> JSON health response
/// - `POST /observe` -> observation handler (vnc-022)
/// - `/* (everything else)` -> MCP via ProjectRouter
```

## State Machine

No new state machines. The handler is a single async function call -- request in, response out. Session state management is internal to `dispatch_request` via `SessionRegistry` (unchanged).

## Data Flow Summary

```
Request<ReqBody>
  |-- extensions: { ResolvedIdentity }
  |-- body: JSON bytes
  v
[Extract Identity] -- failure -> 500
  v
[Content-Length check] -- exceeds -> 413
  v
[Limited body collect] -- size error -> 413, read error -> 500
  v
[serde_json::from_slice] -- parse error -> 400 {"error":"..."}
  v
[prefix_session_id] -- mutate session_id fields in-place
  v
[dispatch_request(..., &identity.capabilities)]
  v
HookResponse
  v
[observe_response_to_http]
  |-- Ack -> 204
  |-- Entries/BriefingContent/Pong -> 200 + JSON
  |-- Error -> 400 + JSON
```

## Error Handling

| Error | HTTP Status | Body | Source |
|-------|-------------|------|--------|
| Missing ResolvedIdentity | 500 | `{"error":"internal error: identity not resolved"}` | Middleware misconfiguration |
| Content-Length > 1MB | 413 | `{"error":"request body exceeds maximum size"}` | Reuse `payload_too_large_response()` |
| Stream body > 1MB | 413 | `{"error":"request body exceeds maximum size"}` | Reuse `payload_too_large_response()` |
| Body read error | 500 | `{"error":"failed to read request body"}` | Reuse `internal_error_response()` |
| JSON parse error | 400 | `{"error":"invalid request JSON: <detail>"}` | New `json_error_response()` |
| dispatch_request returns Error | 400 | `{"type":"Error","code":N,"message":"..."}` | `observe_response_to_http()` |
| HookResponse serialization failure | 500 | `{"error":"failed to read request body"}` | Reuse `internal_error_response()` |

## Key Test Scenarios

### Happy Path
1. **SessionRegister -> 204**: POST valid SessionRegister JSON. Verify 204, empty body, session in registry with "http-" prefix.
2. **RecordEvent -> 204**: POST valid RecordEvent JSON. Verify 204, observation persisted.
3. **ContextSearch -> 200 + Entries**: POST valid ContextSearch JSON. Verify 200, Content-Type application/json, body contains `"type":"Entries"`.
4. **CompactPayload -> 200 + BriefingContent**: POST valid CompactPayload JSON. Verify 200, body contains `"type":"BriefingContent"`.

### Error Path
5. **No auth -> 401**: POST without Authorization header. Verify 401 (handled by StaticTokenAuth, not this handler).
6. **Malformed JSON -> 400**: POST `{"type":"Bogus"}`. Verify 400, body contains `"error"` key.
7. **Empty body -> 400**: POST with Content-Length: 0. Verify 400 (deserialization of empty slice fails).
8. **Oversized body (CL) -> 413**: POST with Content-Length > 1MB. Verify 413.
9. **Oversized body (stream) -> 413**: POST without Content-Length, body > 1MB. Verify 413.

### Session ID Prefixing
10. **Prefix applied**: POST SessionRegister with session_id "abc-123". Query SessionRegistry. Stored key must be "http-abc-123".
11. **Prefix on ContextSearch**: POST ContextSearch with session_id "abc-123". Verify dispatch receives "http-abc-123".

### Response Mapping (unit tests on observe_response_to_http)
12. **Ack -> 204**: No body, no Content-Type header.
13. **Entries -> 200**: JSON body, Content-Type application/json.
14. **BriefingContent -> 200**: JSON body, Content-Type application/json.
15. **Pong -> 200**: JSON body with server_version.
16. **Error -> 400**: JSON body with type, code, message.

### Concurrent Sessions (R-08)
17. **Two sessions, same token**: Register session A and session B via HTTP with same bearer token. Send events to each. Verify independent state in SessionRegistry.
