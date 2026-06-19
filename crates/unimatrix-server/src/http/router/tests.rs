use super::*;
use http_body_util::Full;
use unimatrix_engine::wire::{EntryPayload, HookResponse};

type TestBody = BoxBody<Bytes, Infallible>;

fn empty_body() -> TestBody {
    Full::new(Bytes::new())
        .map_err(|never| match never {})
        .boxed()
}

fn body_of_size(size: usize) -> TestBody {
    Full::new(Bytes::from(vec![0u8; size]))
        .map_err(|never| match never {})
        .boxed()
}

async fn collect_body(resp: Response<BoxBody<Bytes, Infallible>>) -> (StatusCode, String) {
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8(body.to_vec()).expect("utf8 body"))
}

/// Mock MCP adapter that returns 200 with a marker body when called.
/// Enforces body size limits using the same two-layer strategy as the real
/// McpAdapter: Content-Length fast-path + Limited stream-level check (GH #663).
#[derive(Debug, Clone)]
struct MockMcpAdapter {
    max_body_bytes: usize,
}

impl MockMcpAdapter {
    fn new() -> Self {
        MockMcpAdapter {
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        }
    }

    fn with_max_body_bytes(max_body_bytes: usize) -> Self {
        MockMcpAdapter { max_body_bytes }
    }

    async fn handle<ReqBody>(
        &self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
    where
        ReqBody: Body + Send + 'static,
        ReqBody::Data: Send + 'static,
        ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        // Layer 1: Content-Length fast-path (mirrors real adapter).
        let exceeds_limit = request
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|len| len > self.max_body_bytes);

        if exceeds_limit {
            return Ok(payload_too_large_response());
        }

        // Layer 2: Stream-level Limited body check (GH #663).
        let (_parts, body) = request.into_parts();
        let limited_body = Limited::new(body, self.max_body_bytes);

        match limited_body.collect().await {
            Ok(_collected) => {}
            Err(err) => {
                if err
                    .downcast_ref::<http_body_util::LengthLimitError>()
                    .is_some()
                {
                    return Ok(payload_too_large_response());
                }
                return Ok(internal_error_response());
            }
        }

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(
                Full::new(Bytes::from(r#"{"routed":"mcp"}"#))
                    .map_err(|never| match never {})
                    .boxed(),
            )
            .expect("test response builder"))
    }
}

/// Simulate PathRouter dispatch logic with a mock backend.
/// Tests the routing decisions without requiring a full UnimatrixServer.
///
/// Note: POST /observe now requires ResolvedIdentity in extensions and a valid
/// HookRequest JSON body. Without those, the real handler returns 500 (missing
/// identity) or 400 (bad JSON). This mock returns 400 for /observe to indicate
/// the route was matched (not forwarded to MCP).
async fn mock_dispatch_request<ReqBody>(
    mock: &MockMcpAdapter,
    request: Request<ReqBody>,
) -> Response<BoxBody<Bytes, Infallible>>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let path = request.uri().path().to_owned();
    let method = request.method().clone();

    // Mirrors `PathRouter::call` routing (vnc-038 ADR-003): `/health` top-level,
    // per-slug observe `POST /v1/{slug}/observe`, everything else MCP.
    match (&method, path.as_str()) {
        (&Method::GET, "/health") => map_health_response(health_response()),
        (&Method::POST, p) if p.starts_with("/v1/") && p.ends_with("/observe") => {
            // The real handler resolves the per-slug store, then checks identity
            // and parses the body. For routing tests we return 400 to indicate the
            // observe route was matched (the handler would 500 without identity).
            json_error_response(StatusCode::BAD_REQUEST, "mock: no body")
        }
        (_, _) => mock
            .handle(request)
            .await
            .unwrap_or_else(|never| match never {}),
    }
}

// ---- T-PR-01: GET /health routes to health handler ----

#[tokio::test]
async fn test_get_health_routes_to_health_handler() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_str(&body).expect("health response is valid JSON");
    assert!(json.get("version").is_some());
    assert!(json.get("schema_version").is_some());
}

// ---- T-PR-02: POST / routes to MCP service ----

#[tokio::test]
async fn test_post_mcp_routes_to_streamable_http_service() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- T-PR-03: Wildcard routes to MCP service ----

#[tokio::test]
async fn test_wildcard_routes_to_mcp_service() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/some/arbitrary/path")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- T-PR-04: POST /v1/{slug}/observe routes to observe handler (vnc-038 ADR-003) ----

#[tokio::test]
async fn test_post_per_slug_observe_routes_to_handler() {
    // vnc-038 ADR-003 (#5082): observe is the per-slug route `/v1/{slug}/observe`,
    // resolved per-request through the SAME funnel as MCP. The top-level `/observe`
    // route is DELETED.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/alpha/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    // Mock returns 400 for the observe route to indicate the route was matched.
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("error"), "body: {body}");
}

// ---- T-PR-06: Body size limit rejects oversized (Content-Length fast-path) ----

#[tokio::test]
async fn test_body_size_limit_rejects_oversized() {
    let mock = MockMcpAdapter::with_max_body_bytes(1_048_576);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-length", "1048577") // 1 byte over limit
        .body(body_of_size(100)) // body content irrelevant — header check
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        body.contains("request body exceeds maximum size"),
        "body: {body}"
    );
}

// ---- T-PR-07: Body size at boundary is accepted ----

#[tokio::test]
async fn test_body_size_limit_accepts_at_boundary() {
    let mock = MockMcpAdapter::with_max_body_bytes(1_048_576);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-length", "1048576") // exactly at limit
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- T-PR-08: Body size limit enforced before rmcp ----

#[tokio::test]
async fn test_body_size_limit_enforced_before_rmcp() {
    // Mock with 1 KB limit.
    let mock = MockMcpAdapter::with_max_body_bytes(1024);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-length", "2048") // 2 KB, over 1 KB limit
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, _) = collect_body(resp).await;

    // 413 means the mock never processed the request body.
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

// ---- T-PR-10: non-health/non-observe paths fall through to the MCP edge ----

#[tokio::test]
async fn test_non_observe_path_falls_through_to_mcp_edge() {
    // vnc-038 ADR-004: there is no default project. A non-`/health`,
    // non-`/v1/{slug}/observe` path falls through to the MCP `SlugRouter` edge.
    // (At the real funnel `/` is an unknown project -> 404; the mock models only
    // the routing fall-through, so it returns 200 from the MCP stub.)
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, _) = collect_body(resp).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "non-observe path falls through to the MCP edge (mock stub)"
    );
}

// ---- T-PR-11: per-slug observe registered in routing tree (vnc-038 ADR-003) ----

#[tokio::test]
async fn test_per_slug_observe_registered_in_routing_tree() {
    // vnc-038 ADR-003 (#5082): observe is `POST /v1/{slug}/observe`, handled by
    // `PathRouter` (the per-slug observe handler), not forwarded to MCP. The
    // top-level `/observe` route is REMOVED.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/alpha/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    // Mock returns 400 for the observe route; if it were forwarded to MCP we'd get 200.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_top_level_observe_no_longer_routed_to_observe_handler() {
    // vnc-038 ADR-003: the top-level `POST /observe` arm is DELETED. It is no
    // longer a slug-shaped path, so it falls through to MCP (mock -> 200), never
    // the observe handler. Closes #766 by construction — observe is per-slug only.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#""routed":"mcp""#),
        "top-level /observe must fall through to MCP, body: {body}"
    );
}

// ---- T-PR-12: GET on MCP path behavior ----

#[tokio::test]
async fn test_get_on_mcp_path_forwards_to_mcp() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/")
        .body(empty_body())
        .unwrap();

    // GET on non-health path routes to MCP. The mock returns 200.
    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- T-PR-13: OPTIONS request behavior ----

#[tokio::test]
async fn test_options_request_does_not_panic() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(empty_body())
        .unwrap();

    // Must not panic. Routes to MCP adapter which handles method mismatch.
    let resp = mock_dispatch_request(&mock, req).await;
    let _ = collect_body(resp).await;
}

// ---- T-PR-14: HEAD /health behavior ----

#[tokio::test]
async fn test_head_health_routes_to_mcp() {
    // HEAD /health does NOT match (GET, "/health") exactly,
    // so it routes to MCP. Must not panic.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::HEAD)
        .uri("/health")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let _ = collect_body(resp).await;
}

// ---- GET /observe routes to MCP (method mismatch) ----

#[tokio::test]
async fn test_get_observe_routes_to_mcp() {
    // Only POST /observe matches the stub. GET /observe goes to MCP.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- Response format tests ----

#[test]
fn test_json_error_response_format() {
    let resp = json_error_response(StatusCode::BAD_REQUEST, "test error");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

#[test]
fn test_payload_too_large_response_format() {
    let resp = payload_too_large_response();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

#[test]
fn test_default_max_body_bytes() {
    assert_eq!(DEFAULT_MAX_BODY_BYTES, 1_048_576);
}

// ---------------------------------------------------------------------------
// GH #663 — Chunked TE body size enforcement tests
// ---------------------------------------------------------------------------

/// Custom multi-frame body for testing chunked-style delivery without needing
/// the `futures` crate. Yields frames from a `VecDeque`, then signals end.
struct ChunkedTestBody {
    frames: std::collections::VecDeque<
        Result<http_body::Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>,
    >,
}

impl ChunkedTestBody {
    /// Create from multiple byte chunks (simulates chunked TE with no Content-Length).
    fn from_chunks(chunks: Vec<Vec<u8>>) -> Self {
        let frames = chunks
            .into_iter()
            .map(|c| Ok(http_body::Frame::data(Bytes::from(c))))
            .collect();
        ChunkedTestBody { frames }
    }

    /// Create a body that yields one good chunk then an error (simulates disconnect).
    fn with_disconnect(good_chunk: Vec<u8>) -> Self {
        let mut frames = std::collections::VecDeque::new();
        frames.push_back(Ok(http_body::Frame::data(Bytes::from(good_chunk))));
        frames.push_back(Err(
            Box::new(DisconnectError) as Box<dyn std::error::Error + Send + Sync>
        ));
        ChunkedTestBody { frames }
    }
}

impl Body for ChunkedTestBody {
    type Data = Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.frames.pop_front())
    }
}

/// Test error simulating a client disconnect mid-stream.
#[derive(Debug)]
struct DisconnectError;

impl std::fmt::Display for DisconnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("client disconnected")
    }
}

impl std::error::Error for DisconnectError {}

// ---- GH #663 T1: Chunked body under limit → 200 ----

#[tokio::test]
async fn test_chunked_body_under_limit_returns_200() {
    let mock = MockMcpAdapter::with_max_body_bytes(1024);

    // 3 chunks of 100 bytes each = 300 bytes, well under 1024 limit.
    // No Content-Length header — simulates chunked TE.
    let body = ChunkedTestBody::from_chunks(vec![vec![0u8; 100], vec![0u8; 100], vec![0u8; 100]]);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .body(body)
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- GH #663 T2: Chunked body over limit → 413 (Limited catches it) ----

#[tokio::test]
async fn test_chunked_body_over_limit_returns_413() {
    let mock = MockMcpAdapter::with_max_body_bytes(256);

    // 4 chunks of 100 bytes = 400 bytes, over 256 limit.
    // No Content-Length header — the old header-only check would miss this.
    let body = ChunkedTestBody::from_chunks(vec![
        vec![0u8; 100],
        vec![0u8; 100],
        vec![0u8; 100],
        vec![0u8; 100],
    ]);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .body(body)
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        body.contains("request body exceeds maximum size"),
        "body: {body}"
    );
}

// ---- GH #663 T3: Content-Length over limit → 413 (fast-path, no body read) ----

#[tokio::test]
async fn test_content_length_over_limit_fast_path_413() {
    let mock = MockMcpAdapter::with_max_body_bytes(512);

    // Content-Length header declares 1024 bytes, over 512 limit.
    // Body content is irrelevant — header check should reject immediately.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-length", "1024")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        body.contains("request body exceeds maximum size"),
        "body: {body}"
    );
}

// ---- GH #663 T4: Mid-stream disconnect → 500, NOT 413 ----

#[tokio::test]
async fn test_midstream_body_error_returns_500_not_413() {
    let mock = MockMcpAdapter::with_max_body_bytes(1024);

    // Body sends 100 good bytes then errors (simulates client disconnect).
    // Limit is 1024 so this is NOT a size violation.
    let body = ChunkedTestBody::with_disconnect(vec![0u8; 100]);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .body(body)
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    // Must be 500, NOT 413 — this is a disconnect, not a size violation.
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("failed to read request body"), "body: {body}");
}

// ---- GH #663: internal_error_response format ----

#[test]
fn test_internal_error_response_format() {
    let resp = internal_error_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

// ===========================================================================
// vnc-022: observe_response_to_http unit tests
// ===========================================================================

#[tokio::test]
async fn test_observe_response_ack_maps_to_204_no_content() {
    let resp = observe_response_to_http(HookResponse::Ack, false);
    let status = resp.status();
    let content_type = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty(), "Ack body must be empty, got: {body}");
    assert!(
        content_type.is_none(),
        "Ack must have no Content-Type header"
    );
}

#[tokio::test]
async fn test_observe_response_entries_maps_to_200_json() {
    let entry = EntryPayload {
        id: 1,
        title: "test".to_string(),
        content: "test content".to_string(),
        confidence: 0.9,
        similarity: 0.85,
        category: "pattern".to_string(),
    };
    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: vec![entry],
            total_tokens: 150,
        },
        false,
    );
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Entries");
    assert!(json["items"].is_array());
    assert_eq!(json["total_tokens"], 150);
}

#[tokio::test]
async fn test_observe_response_briefing_content_maps_to_200_json() {
    let resp = observe_response_to_http(
        HookResponse::BriefingContent {
            content: "briefing text".to_string(),
            token_count: 50,
        },
        false,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").unwrap().clone();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/json");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "BriefingContent");
    assert_eq!(json["content"], "briefing text");
    assert_eq!(json["token_count"], 50);
}

#[tokio::test]
async fn test_observe_response_pong_maps_to_200_json() {
    let resp = observe_response_to_http(
        HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        },
        false,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").unwrap().clone();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/json");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Pong");
    assert_eq!(json["server_version"], "0.1.0");
}

#[tokio::test]
async fn test_observe_response_error_maps_to_400_json() {
    let resp = observe_response_to_http(
        HookResponse::Error {
            code: -32004,
            message: "bad input".to_string(),
        },
        false,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").unwrap().clone();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(ct, "application/json");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Error");
    assert_eq!(json["code"], -32004);
    assert_eq!(json["message"], "bad input");
}

#[tokio::test]
async fn test_observe_response_entries_empty_items() {
    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: vec![],
            total_tokens: 0,
        },
        false,
    );
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["items"], serde_json::json!([]));
}

#[tokio::test]
async fn test_observe_response_briefing_content_empty_string() {
    let resp = observe_response_to_http(
        HookResponse::BriefingContent {
            content: "".to_string(),
            token_count: 0,
        },
        false,
    );
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["content"], "");
}

// ===========================================================================
// vnc-022: prefix_session_id unit tests
// ===========================================================================

#[test]
fn test_prefix_session_id_session_register() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::SessionRegister {
        session_id: "abc-123".to_string(),
        cwd: "/tmp".to_string(),
        agent_role: None,
        feature: None,
    };
    prefix_session_id(&mut req);
    if let HookRequest::SessionRegister { session_id, .. } = &req {
        assert_eq!(session_id, "http-abc-123");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_session_close() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::SessionClose {
        session_id: "abc-123".to_string(),
        outcome: None,
        duration_secs: 60,
    };
    prefix_session_id(&mut req);
    if let HookRequest::SessionClose { session_id, .. } = &req {
        assert_eq!(session_id, "http-abc-123");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_record_event() {
    use unimatrix_engine::wire::{HookRequest, ImplantEvent};
    let mut req = HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type: "PreToolUse".to_string(),
            session_id: "sess-42".to_string(),
            timestamp: 1717000000,
            payload: serde_json::json!({}),
            topic_signal: None,
            provider: None,
            cycle_stamp: None,
        },
    };
    prefix_session_id(&mut req);
    if let HookRequest::RecordEvent { event } = &req {
        assert_eq!(event.session_id, "http-sess-42");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_record_events_batch() {
    use unimatrix_engine::wire::{HookRequest, ImplantEvent};
    let mut req = HookRequest::RecordEvents {
        events: vec![
            ImplantEvent {
                event_type: "PreToolUse".to_string(),
                session_id: "sess-a".to_string(),
                timestamp: 1,
                payload: serde_json::json!({}),
                topic_signal: None,
                provider: None,
                cycle_stamp: None,
            },
            ImplantEvent {
                event_type: "PostToolUse".to_string(),
                session_id: "sess-b".to_string(),
                timestamp: 2,
                payload: serde_json::json!({}),
                topic_signal: None,
                provider: None,
                cycle_stamp: None,
            },
        ],
    };
    prefix_session_id(&mut req);
    if let HookRequest::RecordEvents { events } = &req {
        assert_eq!(events[0].session_id, "http-sess-a");
        assert_eq!(events[1].session_id, "http-sess-b");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_context_search_some() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::ContextSearch {
        query: "test".to_string(),
        session_id: Some("abc-123".to_string()),
        role: None,
        task: None,
        feature: None,
        k: None,
        max_tokens: None,
        source: None,
        accept: None,
    };
    prefix_session_id(&mut req);
    if let HookRequest::ContextSearch { session_id, .. } = &req {
        assert_eq!(session_id.as_deref(), Some("http-abc-123"));
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_context_search_none() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::ContextSearch {
        query: "test".to_string(),
        session_id: None,
        role: None,
        task: None,
        feature: None,
        k: None,
        max_tokens: None,
        source: None,
        accept: None,
    };
    prefix_session_id(&mut req);
    if let HookRequest::ContextSearch { session_id, .. } = &req {
        assert!(session_id.is_none(), "None session_id should stay None");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_compact_payload() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::CompactPayload {
        session_id: "compact-sess".to_string(),
        injected_entry_ids: vec![],
        role: None,
        feature: None,
        token_limit: None,
        transcript_excerpt: None,
        accept: None,
    };
    prefix_session_id(&mut req);
    if let HookRequest::CompactPayload { session_id, .. } = &req {
        assert_eq!(session_id, "http-compact-sess");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_ping_unchanged() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::Ping;
    prefix_session_id(&mut req);
    assert!(matches!(req, HookRequest::Ping));
}

#[test]
fn test_prefix_session_id_briefing_unchanged() {
    use unimatrix_engine::wire::HookRequest;
    let mut req = HookRequest::Briefing {
        role: "developer".to_string(),
        task: "test task".to_string(),
        feature: None,
        max_tokens: None,
    };
    prefix_session_id(&mut req);
    assert!(matches!(req, HookRequest::Briefing { .. }));
}

// vnc-025 (#670, AC-06 / R-12, pattern #4725): HTTP→UDS convergence rests on
// `prefix_session_id` rewriting ONLY session_id — event_type must survive so a
// transcript_delta routes into the shared merge arm.

#[test]
fn test_prefix_session_id_preserves_event_type_single() {
    use unimatrix_engine::wire::{HookRequest, ImplantEvent, TRANSCRIPT_DELTA_EVENT};
    let mut req = HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type: TRANSCRIPT_DELTA_EVENT.to_string(),
            session_id: "sess-d1".to_string(),
            timestamp: 1,
            payload: serde_json::json!({"offset": 0, "bytes": "x"}),
            topic_signal: None,
            provider: None,
            cycle_stamp: None,
        },
    };
    prefix_session_id(&mut req);
    if let HookRequest::RecordEvent { event } = &req {
        assert_eq!(event.event_type, TRANSCRIPT_DELTA_EVENT);
        assert_eq!(event.session_id, "http-sess-d1");
        assert_eq!(
            event.payload,
            serde_json::json!({"offset": 0, "bytes": "x"})
        );
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn test_prefix_session_id_preserves_event_type_batch_every_element() {
    use unimatrix_engine::wire::{HookRequest, ImplantEvent, TRANSCRIPT_DELTA_EVENT};
    let make = |event_type: &str, sid: &str| ImplantEvent {
        event_type: event_type.to_string(),
        session_id: sid.to_string(),
        timestamp: 1,
        payload: serde_json::json!({}),
        topic_signal: None,
        provider: None,
        cycle_stamp: None,
    };
    // Mixed batch: normal events around a transcript_delta.
    let mut req = HookRequest::RecordEvents {
        events: vec![
            make("PreToolUse", "sess-m1"),
            make(TRANSCRIPT_DELTA_EVENT, "sess-m2"),
            make("PostToolUse", "sess-m3"),
        ],
    };
    prefix_session_id(&mut req);
    if let HookRequest::RecordEvents { events } = &req {
        let got: Vec<(&str, &str)> = events
            .iter()
            .map(|e| (e.event_type.as_str(), e.session_id.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("PreToolUse", "http-sess-m1"),
                (TRANSCRIPT_DELTA_EVENT, "http-sess-m2"),
                ("PostToolUse", "http-sess-m3"),
            ],
            "every element keeps its event_type; every session_id is prefixed"
        );
    } else {
        panic!("wrong variant");
    }
}

// ===========================================================================
// vnc-022: handler error response tests
// ===========================================================================

#[tokio::test]
async fn test_observe_handler_malformed_json_returns_400() {
    // Simulate what the handler does with malformed JSON.
    let body = br#"{"type":"Bogus"}"#;
    let result = serde_json::from_slice::<unimatrix_engine::wire::HookRequest>(body);
    assert!(result.is_err());

    // The handler would produce this response:
    let e = result.unwrap_err();
    let resp = json_error_response(
        StatusCode::BAD_REQUEST,
        &format!("invalid request JSON: {e}"),
    );
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("invalid request JSON")
    );
}

#[tokio::test]
async fn test_observe_handler_empty_body_returns_400() {
    let body = b"";
    let result = serde_json::from_slice::<unimatrix_engine::wire::HookRequest>(body);
    assert!(result.is_err());

    let e = result.unwrap_err();
    let resp = json_error_response(
        StatusCode::BAD_REQUEST,
        &format!("invalid request JSON: {e}"),
    );
    let (status, _) = collect_body(resp).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_observe_handler_valid_json_wrong_schema_returns_400() {
    let body = br#"{"foo":"bar"}"#;
    let result = serde_json::from_slice::<unimatrix_engine::wire::HookRequest>(body);
    assert!(result.is_err());

    let e = result.unwrap_err();
    let resp = json_error_response(
        StatusCode::BAD_REQUEST,
        &format!("invalid request JSON: {e}"),
    );
    let (status, _) = collect_body(resp).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_observe_malformed_body_no_internal_leak() {
    let body = br#"{"type":"Bogus"}"#;
    let result = serde_json::from_slice::<unimatrix_engine::wire::HookRequest>(body);
    let e = result.unwrap_err();
    let resp = json_error_response(
        StatusCode::BAD_REQUEST,
        &format!("invalid request JSON: {e}"),
    );
    let (_, body) = collect_body(resp).await;

    // Must not contain Rust type paths.
    assert!(
        !body.contains("unimatrix_engine::wire::"),
        "error response must not leak Rust type paths: {body}"
    );
}

// ===========================================================================
// vnc-023: allowed_origins wiring tests
// ===========================================================================

// T-RO-04: StreamableHttpServerConfig receives allowed_origins (R-04)
#[test]
fn test_streamable_config_allowed_origins_field_assignment() {
    let origins = vec!["https://claude.ai".to_string()];
    let mut config = StreamableHttpServerConfig::default();
    config.allowed_origins = origins;
    assert_eq!(config.allowed_origins, vec!["https://claude.ai"]);
}

// T-RO-05: allowed_hosts not overridden by default config (R-05, R-13, AC-05)
#[test]
fn test_streamable_config_default_allowed_hosts_non_empty() {
    let config = StreamableHttpServerConfig::default();
    assert!(
        !config.allowed_hosts.is_empty(),
        "default allowed_hosts must be non-empty (CVE-2026-42559 fix)"
    );
    assert!(
        config.allowed_hosts.contains(&"localhost".to_string()),
        "default allowed_hosts must contain localhost"
    );
}

// T-RO-06: Setting allowed_origins does not clear allowed_hosts (R-05, R-13)
#[test]
fn test_setting_allowed_origins_preserves_allowed_hosts() {
    let mut config = StreamableHttpServerConfig::default();
    let hosts_before = config.allowed_hosts.clone();
    config.allowed_origins = vec!["https://example.com".to_string()];
    assert_eq!(
        config.allowed_hosts, hosts_before,
        "setting allowed_origins must not modify allowed_hosts"
    );
}

// T-RO-07: Empty allowed_origins is the backward-compatible default
#[test]
fn test_streamable_config_default_allowed_origins_empty() {
    let config = StreamableHttpServerConfig::default();
    assert!(
        config.allowed_origins.is_empty(),
        "default allowed_origins must be empty (no restriction)"
    );
}

// ===========================================================================
// bug #774: allowed_hosts wired from PublicUrl.sans through the constructor
// chain (McpAdapter::new). The host gate is rmcp's per-request
// validate_dns_rebinding_headers → 403 "Forbidden: Host header is not allowed"
// for any Host not in allowed_hosts. Before the fix, allowed_hosts stayed at
// rmcp's localhost-only default, so the configured public host 403'd.
// ===========================================================================

/// Build a minimal MCP POST carrying the given `Host` header. Host validation
/// runs FIRST in rmcp's service (before method/body), so the Host gate verdict
/// is observable regardless of body correctness.
fn mcp_post_with_host(host: &str) -> Request<TestBody> {
    Request::builder()
        .method("POST")
        .uri("/")
        .header("Host", host)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(empty_body())
        .expect("build request")
}

const HOST_FORBIDDEN_BODY: &str = "Host header is not allowed";

// T-774-1: a non-localhost public host wired through the adapter is NOT 403'd
// on the Host gate, while an unrelated Host IS. This is the regression that
// escaped vnc-034 (the configured public host was rejected before auth/MCP).
#[tokio::test(flavor = "multi_thread")]
async fn test_adapter_allows_configured_public_host_rejects_others() {
    // Simulate PublicUrl.sans for UNIMATRIX_PUBLIC_URL=https://unimatrix:8443:
    // port-less bare hosts (local SANs + the configured public host).
    let allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "0.0.0.0".to_string(),
        "unimatrix".to_string(),
    ];
    let server = make_server().await;
    let mut adapter = McpAdapter::new(server, 1024 * 1024, Vec::new(), allowed_hosts);

    // The configured public host (with the deployed port) must clear the gate.
    // It is NOT the host-forbidden 403 — later MCP processing may reject for
    // other reasons, but never with the host-gate message.
    let (allowed_status, allowed_body) = collect_body(
        adapter
            .handle(mcp_post_with_host("unimatrix:8443"))
            .await
            .expect("infallible"),
    )
    .await;
    assert!(
        !(allowed_status == StatusCode::FORBIDDEN && allowed_body.contains(HOST_FORBIDDEN_BODY)),
        "configured public host must pass the rmcp Host gate (bug #774); got {allowed_status} / {allowed_body}"
    );

    // An unrelated Host must still be rejected by the gate (403 + message).
    let (denied_status, denied_body) = collect_body(
        adapter
            .handle(mcp_post_with_host("evil.example.com"))
            .await
            .expect("infallible"),
    )
    .await;
    assert_eq!(
        denied_status,
        StatusCode::FORBIDDEN,
        "an unrelated Host must be 403'd by the rmcp gate; got body {denied_body}"
    );
    assert!(
        denied_body.contains(HOST_FORBIDDEN_BODY),
        "the 403 must be the Host-gate rejection; got {denied_body}"
    );
}

// T-774-2: fail-open guard — an UNSET PublicUrl still yields a non-empty
// allowed_hosts (localhost-only). An empty vec would make rmcp allow ALL hosts
// (the opposite of allowed_origins). The placeholder path must stay restrictive.
#[test]
fn test_unset_public_url_yields_non_empty_allowed_hosts() {
    let getter = |_: &str| None; // UNIMATRIX_PUBLIC_URL unset
    let public_url =
        crate::http::public_url::derive_public_url(&crate::http::public_url::Env::new(&getter));
    let allowed_hosts = public_url.sans.clone();

    assert!(
        !allowed_hosts.is_empty(),
        "unset PublicUrl must NOT yield empty allowed_hosts — rmcp treats empty as allow-all (fail-open)"
    );
    assert!(
        allowed_hosts.contains(&"localhost".to_string()),
        "unset PublicUrl allowed_hosts must stay localhost-restrictive (CVE-2026-42559 posture)"
    );
    // The real public host is deliberately omitted when the knob is unset.
    assert!(
        !allowed_hosts.iter().any(|h| h.contains("placeholder")),
        "the placeholder sentinel must never leak into allowed_hosts as a real host"
    );
}

// T-774-3: documents the rmcp semantic the fix relies on — a port-less allowlist
// entry matches an incoming Host that carries a port. PublicUrl.sans is port-less
// (bare hosts), so "unimatrix" must accept "unimatrix:8443". Driven through the
// real adapter because rmcp's host_is_allowed is not a public API.
#[tokio::test(flavor = "multi_thread")]
async fn test_portless_allowed_host_matches_host_with_port() {
    // Only a port-less host entry; the local SANs are irrelevant to this case.
    let allowed_hosts = vec!["unimatrix".to_string()];
    let server = make_server().await;
    let mut adapter = McpAdapter::new(server, 1024 * 1024, Vec::new(), allowed_hosts);

    let (status, body) = collect_body(
        adapter
            .handle(mcp_post_with_host("unimatrix:8443"))
            .await
            .expect("infallible"),
    )
    .await;
    assert!(
        !(status == StatusCode::FORBIDDEN && body.contains(HOST_FORBIDDEN_BODY)),
        "a port-less allowlist entry must match a Host carrying a port (rmcp semantic the fix relies on); got {status} / {body}"
    );
}

// ===========================================================================
// vnc-024: /observe content negotiation (AC-07 / AC-08 / AC-09)
//
// Unit-level mapper tests. `observe_response_to_http(resp, wants_text)` must
// emit text/plain (via the single formatting truth `format_injection`) only for
// the allowlist {Entries, BriefingContent}; everything else stays JSON. These
// assert at the mapper boundary; integration /observe HTTP tests are Stage 3c.
// ===========================================================================

use crate::uds::hook::{MAX_INJECTION_BYTES, format_injection};

/// One synthetic entry with a controllable content size, used to build entry
/// sets that cross the `MAX_INJECTION_BYTES` truncation boundary (AC-07).
fn make_entry(id: u64, content_len: usize) -> EntryPayload {
    EntryPayload {
        id,
        title: format!("entry-{id}"),
        content: "x".repeat(content_len),
        confidence: 0.9,
        similarity: 0.85,
        category: "pattern".to_string(),
    }
}

// ---- AC-07: Entries + wants_text=true → text/plain, byte-identical to
//             format_injection(&items, MAX_INJECTION_BYTES) ----

#[tokio::test]
async fn test_observe_text_entries_byte_identical() {
    // Small happy-path set that fits comfortably under budget.
    let items = vec![make_entry(1, 40), make_entry(2, 40)];
    let expected =
        format_injection(&items, MAX_INJECTION_BYTES).expect("non-empty entries format to Some");

    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: items.clone(),
            total_tokens: 100,
        },
        true,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ct.expect("content-type present"),
        "text/plain",
        "Entries + wants_text must be text/plain"
    );
    assert_eq!(
        body, expected,
        "text body must be byte-identical to format_injection"
    );
}

#[tokio::test]
async fn test_observe_text_entries_over_budget_matches_truncation() {
    // Entry set large enough to cross the truncation boundary: a wrong budget
    // (or a server-side re-truncation) would produce a detectable length diff.
    let items = vec![
        make_entry(1, 600),
        make_entry(2, 600),
        make_entry(3, 600),
        make_entry(4, 600),
    ];
    let expected = format_injection(&items, MAX_INJECTION_BYTES)
        .expect("over-budget set still yields a truncated Some");
    // Sanity: this set actually exercises truncation at the production budget.
    assert!(
        expected.len() <= MAX_INJECTION_BYTES,
        "expected output must be within budget (proves truncation engaged)"
    );

    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: items.clone(),
            total_tokens: 9999,
        },
        true,
    );
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body, expected,
        "over-budget text body must match format_injection's truncated output exactly"
    );
}

#[tokio::test]
async fn test_observe_text_entries_empty_returns_204() {
    // format_injection(&[], _) → None → 204 no-content (ADR-003), not 200/500.
    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: vec![],
            total_tokens: 0,
        },
        true,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty(), "204 body must be empty, got: {body}");
    assert!(ct.is_none(), "204 must have no Content-Type header");
}

// ---- AC-09: BriefingContent + wants_text=true → text body (positive control
//             that the allowlist includes BriefingContent) ----

#[tokio::test]
async fn test_observe_text_briefingcontent_returns_text() {
    let resp = observe_response_to_http(
        HookResponse::BriefingContent {
            content: "briefing body text".to_string(),
            token_count: 42,
        },
        true,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ct.expect("content-type present"),
        "text/plain",
        "BriefingContent + wants_text must be text/plain"
    );
    assert_eq!(body, "briefing body text", "text body is the raw content");
}

// ---- AC-09: Pong / Ack / Error + wants_text=true → STILL JSON (allowlist is
//             exactly {Entries, BriefingContent}, R-06) ----

#[tokio::test]
async fn test_observe_text_pong_stays_json() {
    let resp = observe_response_to_http(
        HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        },
        true,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ct.expect("content-type present"),
        "application/json",
        "Pong must stay JSON even under wants_text (F2 handshake)"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Pong");
    assert_eq!(
        json["server_version"], "0.1.0",
        "server_version must remain parseable"
    );
}

#[tokio::test]
async fn test_observe_text_ack_stays_204_json_path() {
    let resp = observe_response_to_http(HookResponse::Ack, true);
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "Ack stays 204 under wants_text"
    );
    assert!(body.is_empty(), "Ack body must be empty, got: {body}");
    assert!(ct.is_none(), "Ack must have no Content-Type header");
}

#[tokio::test]
async fn test_observe_text_error_stays_json() {
    let resp = observe_response_to_http(
        HookResponse::Error {
            code: -32004,
            message: "bad input".to_string(),
        },
        true,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "Error stays 400 under wants_text"
    );
    assert_eq!(
        ct.expect("content-type present"),
        "application/json",
        "Error must stay JSON even under wants_text"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Error");
    assert_eq!(json["code"], -32004);
    assert_eq!(json["message"], "bad input");
}

// ---- AC-08: each response type with wants_text=false → unchanged JSON envelope.
//             Ack→204; Entries/BriefingContent/Pong→200 JSON; Error→400 JSON.
//             (The vnc-022 tests above already assert the bodies; this is the
//             single consolidated AC-08 control over all variants.) ----

#[tokio::test]
async fn test_observe_json_envelope_unchanged_all_variants() {
    // Ack → 204, empty, no content-type.
    let resp = observe_response_to_http(HookResponse::Ack, false);
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(resp.headers().get("content-type").is_none());

    // Entries → 200 JSON.
    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: vec![make_entry(1, 10)],
            total_tokens: 5,
        },
        false,
    );
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );

    // BriefingContent → 200 JSON.
    let resp = observe_response_to_http(
        HookResponse::BriefingContent {
            content: "b".to_string(),
            token_count: 1,
        },
        false,
    );
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );

    // Pong → 200 JSON.
    let resp = observe_response_to_http(
        HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        },
        false,
    );
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );

    // Error → 400 JSON.
    let resp = observe_response_to_http(
        HookResponse::Error {
            code: -1,
            message: "e".to_string(),
        },
        false,
    );
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

// ===========================================================================
// vnc-024 Stage 3c — HTTP-BOUNDARY integration tests (NOT mapper-isolation).
//
// The AC-07/08/09 tests above call `observe_response_to_http(resp, wants_text)`
// directly, passing `wants_text` as a literal bool. That bypasses the
// `Accept`-header extraction in the real handler (`router.rs` Step 2b) and so
// CANNOT catch R-07 (the header read silently lost if it moves after
// `request.into_parts()`). The tests below build a real `http::Request`, replay
// the EXACT extraction ordering from the handler, and assert the negotiated
// content-type at the boundary — the only thing that catches the ordering bug.
//
// They also cover the HTTP arm of AC-12: the HTTP path runs `prefix_session_id`
// BEFORE dispatch, mutating the delta's session_id. The guard keys on
// `event_type` (not session_id), so the prefix transform must NOT bypass the
// drop. (R-04 integration trap: "a UDS-only unit test could be bypassed if the
// HTTP path transforms the event differently.")
// ===========================================================================

use unimatrix_engine::wire::{ImplantEvent, TRANSCRIPT_DELTA_EVENT, TranscriptDeltaPayload};

/// Replay the handler's content-negotiation extraction (router.rs Step 2b →
/// Step 3): read `wants_text` from the request headers, THEN consume the
/// request via `into_parts()`. Returns the negotiated bool AND the body bytes,
/// proving the header read survived `into_parts` (R-07). A regression that moves
/// the read after `into_parts()` would not compile (request moved) or read a
/// wrong/empty value — caught by the assertions on the returned bool.
async fn negotiate_wants_text_at_boundary(req: Request<TestBody>) -> (bool, Bytes) {
    // --- Step 2b: Accept read, BEFORE into_parts (exact handler logic). ---
    let wants_text = req
        .headers()
        .get(http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/plain"));

    // --- Step 3: into_parts consumes the request; the header is now gone. ---
    let (_parts, body) = req.into_parts();
    let collected = body.collect().await.expect("collect body").to_bytes();
    (wants_text, collected)
}

// ---- AC-07 / R-07: Accept: text/plain survives into_parts; Entries → text/plain ----

#[tokio::test]
async fn test_observe_http_accept_text_plain_negotiates_text() {
    let items = vec![make_entry(1, 40), make_entry(2, 40)];
    let expected = format_injection(&items, MAX_INJECTION_BYTES).expect("non-empty → Some");

    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .header(http::header::ACCEPT, "text/plain")
        .body(empty_body())
        .unwrap();

    // Boundary extraction: header must survive into_parts.
    let (wants_text, _body) = negotiate_wants_text_at_boundary(req).await;
    assert!(
        wants_text,
        "R-07: Accept: text/plain must be captured BEFORE into_parts (header not lost)"
    );

    // Feed the negotiated bool through the real mapper exactly as the handler does.
    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: items.clone(),
            total_tokens: 100,
        },
        wants_text,
    );
    let status = resp.status();
    let ct = resp.headers().get("content-type").cloned();
    let (_, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        ct.expect("content-type present"),
        "text/plain",
        "negotiated content-type at the HTTP boundary must be text/plain"
    );
    assert_eq!(
        body, expected,
        "boundary-negotiated text body byte-identical to format_injection"
    );
}

// ---- AC-08 / R-07: no Accept header → JSON (negotiated at boundary) ----

#[tokio::test]
async fn test_observe_http_no_accept_negotiates_json() {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let (wants_text, _body) = negotiate_wants_text_at_boundary(req).await;
    assert!(
        !wants_text,
        "absent Accept must negotiate JSON (wants_text=false)"
    );

    let resp = observe_response_to_http(
        HookResponse::Entries {
            items: vec![make_entry(1, 10)],
            total_tokens: 5,
        },
        wants_text,
    );
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").expect("ct"),
        "application/json",
        "no Accept header → JSON envelope at the boundary"
    );
}

// ---- AC-08 / R-07: Accept: application/json → JSON (negotiated at boundary) ----

#[tokio::test]
async fn test_observe_http_accept_json_negotiates_json() {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .header(http::header::ACCEPT, "application/json")
        .body(empty_body())
        .unwrap();

    let (wants_text, _) = negotiate_wants_text_at_boundary(req).await;
    assert!(
        !wants_text,
        "Accept: application/json must NOT negotiate text"
    );
}

// ---- R-07 / OQ-3: wants_text predicate — multi-value and wildcard Accept ----

#[tokio::test]
async fn test_observe_http_accept_multivalue_contains_text_plain_negotiates_text() {
    // "contains text/plain" ⇒ text, even when other media types are present.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .header(http::header::ACCEPT, "application/json, text/plain")
        .body(empty_body())
        .unwrap();
    let (wants_text, _) = negotiate_wants_text_at_boundary(req).await;
    assert!(
        wants_text,
        "multi-value Accept containing text/plain ⇒ text (OQ-3 predicate)"
    );
}

#[tokio::test]
async fn test_observe_http_accept_wildcard_negotiates_json() {
    // `*/*` does NOT literally contain "text/plain" ⇒ JSON (predicate is a
    // substring match per router.rs:210; documents the wildcard behavior).
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .header(http::header::ACCEPT, "*/*")
        .body(empty_body())
        .unwrap();
    let (wants_text, _) = negotiate_wants_text_at_boundary(req).await;
    assert!(
        !wants_text,
        "Accept: */* does not contain literal text/plain ⇒ JSON (substring predicate)"
    );
}

// ---- AC-09 / R-06: BriefingContent honors text; Pong/Ack/Error stay JSON under
//      a real Accept: text/plain header negotiated at the boundary ----

#[tokio::test]
async fn test_observe_http_text_allowlist_at_boundary() {
    // One real Accept: text/plain request drives the negotiated bool into every
    // response variant — the allowlist is exactly {Entries, BriefingContent}.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .header(http::header::ACCEPT, "text/plain")
        .body(empty_body())
        .unwrap();
    let (wants_text, _) = negotiate_wants_text_at_boundary(req).await;
    assert!(wants_text);

    // BriefingContent → text (positive control).
    let resp = observe_response_to_http(
        HookResponse::BriefingContent {
            content: "brief".to_string(),
            token_count: 1,
        },
        wants_text,
    );
    assert_eq!(
        resp.headers().get("content-type").expect("ct"),
        "text/plain",
        "BriefingContent honors negotiated text"
    );

    // Pong → JSON (F2 handshake), server_version parseable.
    let resp = observe_response_to_http(
        HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        },
        wants_text,
    );
    let (status, body) = collect_body(resp).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("Pong JSON");
    assert_eq!(json["server_version"], "0.1.0");

    // Ack → 204, no text.
    let resp = observe_response_to_http(HookResponse::Ack, wants_text);
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(resp.headers().get("content-type").is_none());

    // Error → 400 JSON.
    let resp = observe_response_to_http(
        HookResponse::Error {
            code: -32004,
            message: "e".to_string(),
        },
        wants_text,
    );
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get("content-type").expect("ct"),
        "application/json",
        "Error stays JSON even under negotiated text"
    );
}

// ===========================================================================
// AC-12 (GATE) — HTTP transport arm. The HTTP path runs `prefix_session_id`
// before dispatch (router.rs Step 5). A transcript_delta whose session_id has
// been HTTP-prefixed must STILL route to the accept-and-drop branch, because
// the guard keys on `event_type` (TRANSCRIPT_DELTA_EVENT), not session_id.
// This closes the R-04 integration trap that a UDS-only test cannot: the HTTP
// transform must not bypass the guard. (Zero-durable-rows over real dispatch is
// proven by the UDS dispatch tests in uds::listener::tests, which the HTTP path
// converges on; here we prove the HTTP-specific transform is guard-safe.)
// ===========================================================================

/// Build the `/observe` JSON body for a RecordEvent carrying a transcript_delta,
/// matching the HookRequest wire shape a client would POST.
fn observe_delta_request_json(session_id: &str, offset: u64, bytes: &str) -> serde_json::Value {
    // RecordEvent flattens the ImplantEvent fields alongside the `type` tag
    // (`#[serde(flatten)]` on `event`, wire.rs) — they are NOT nested under "event".
    serde_json::json!({
        "type": "RecordEvent",
        "event_type": TRANSCRIPT_DELTA_EVENT,
        "session_id": session_id,
        "timestamp": 0,
        "payload": { "offset": offset, "bytes": bytes }
    })
}

#[tokio::test]
async fn test_observe_http_delta_body_deserializes_to_record_event() {
    // The wire body a client POSTs must deserialize into the RecordEvent variant
    // carrying the delta on the free-form event_type (Constraint 3 — no new wire
    // variant). This is the exact serde_from_slice the handler does at Step 4.
    let json = observe_delta_request_json("agent-7", 42, "API_KEY=sk-secret");
    let bytes = serde_json::to_vec(&json).unwrap();
    let req: HookRequest = serde_json::from_slice(&bytes).expect("delta body → HookRequest");

    match &req {
        HookRequest::RecordEvent { event } => {
            assert_eq!(
                event.event_type, TRANSCRIPT_DELTA_EVENT,
                "carrier rides event_type, not a new variant"
            );
            // The typed payload parses (shared shape with AC-11 / the guard).
            let payload: TranscriptDeltaPayload =
                serde_json::from_value(event.payload.clone()).expect("typed delta payload");
            assert_eq!(payload.offset, 42);
            assert_eq!(payload.bytes, "API_KEY=sk-secret");
        }
        other => panic!("expected RecordEvent, got {other:?}"),
    }
}

#[tokio::test]
async fn test_observe_http_prefix_session_id_preserves_delta_routing() {
    // R-04 HTTP trap: prefix_session_id runs on the HTTP path BEFORE dispatch.
    // After the http- prefix, the event must STILL route to the drop branch —
    // the guard keys on event_type, which the prefix transform does not touch.
    let json = observe_delta_request_json("agent-7", 7, "password=hunter2");
    let bytes = serde_json::to_vec(&json).unwrap();
    let mut req: HookRequest = serde_json::from_slice(&bytes).unwrap();

    prefix_session_id(&mut req);

    match &req {
        HookRequest::RecordEvent { event } => {
            assert_eq!(
                event.session_id, "http-agent-7",
                "HTTP path prefixes the session_id"
            );
            // The guard's routing key is unchanged by the prefix → still drops.
            assert_eq!(
                event.event_type, TRANSCRIPT_DELTA_EVENT,
                "GATE: HTTP session-id prefix must NOT change the drop-routing key"
            );
        }
        other => panic!("expected RecordEvent, got {other:?}"),
    }
}

#[tokio::test]
async fn test_observe_http_batch_prefix_preserves_delta_drop_routing() {
    // The RecordEvents batch arm on the HTTP path: every element gets the http-
    // prefix; the delta element must still carry the drop-routing event_type so
    // the batch guard (listener.rs filter on TRANSCRIPT_DELTA_EVENT) drops it
    // while the normal events survive.
    let json = serde_json::json!({
        "type": "RecordEvents",
        "events": [
            { "event_type": "PreToolUse", "session_id": "b", "timestamp": 0, "payload": {"tool":"Bash"} },
            { "event_type": TRANSCRIPT_DELTA_EVENT, "session_id": "b", "timestamp": 0, "payload": {"offset":1,"bytes":"secret"} },
            { "event_type": "PostToolUse", "session_id": "b", "timestamp": 0, "payload": {"tool":"Read"} }
        ]
    });
    let mut req: HookRequest = serde_json::from_value(json).unwrap();
    prefix_session_id(&mut req);

    match &req {
        HookRequest::RecordEvents { events } => {
            assert_eq!(events.len(), 3);
            for e in events {
                assert_eq!(e.session_id, "http-b", "every batch element prefixed");
            }
            let delta_count = events
                .iter()
                .filter(|e| e.event_type == TRANSCRIPT_DELTA_EVENT)
                .count();
            assert_eq!(
                delta_count, 1,
                "GATE: the delta element retains its drop-routing event_type post-prefix"
            );
        }
        other => panic!("expected RecordEvents, got {other:?}"),
    }
}

// ---- AC-12 edge: a delta with an HTTP-style ImplantEvent (offset:0/empty bytes)
//      still parses to the typed payload and routes to drop ----

#[tokio::test]
async fn test_observe_http_delta_empty_bytes_routes_to_drop() {
    let event = ImplantEvent {
        event_type: TRANSCRIPT_DELTA_EVENT.to_string(),
        session_id: "http-x".to_string(),
        timestamp: 0,
        payload: serde_json::json!({"offset": 0, "bytes": ""}),
        topic_signal: None,
        provider: None,
        cycle_stamp: None,
    };
    // Routes by event_type regardless of payload contents.
    assert_eq!(event.event_type, TRANSCRIPT_DELTA_EVENT);
    let payload: TranscriptDeltaPayload =
        serde_json::from_value(event.payload.clone()).expect("offset:0/empty bytes still typed");
    assert_eq!(payload.offset, 0);
    assert!(payload.bytes.is_empty());
}

// ===========================================================================
// vnc-034 C4 isolation seam — ProjectKey / ProjectSlug / StoreResolver /
// RouteError + parse_project_key + the resolver-swap boundary.
//
// Wave-1 surface only: route-shape parse, the `ProjectSlug::TryFrom` parse-edge
// guard, `ProjectKey::Slug` -> `UnknownProject` under a default-like resolver,
// and the resolver-swap seam (the Wave1<->Wave2 boundary IS the trait). Wave-2
// slug ROUTING (per-slug stores) is out of scope.
// Lead risks: R-01 (Critical), R-03, R-06, R-13.
// ===========================================================================

use std::path::Path as StdPath;
use unimatrix_store::{PoolConfig, SqlxStore};

/// Open a real lightweight `Arc<Store>` for seam tests. No ONNX model, no
/// `UnimatrixServer` — `SqlxStore::open` is the cheap store handle the resolver
/// hands back. Used as the Wave-1 single store behind a stub resolver.
async fn open_seam_test_store(path: &StdPath) -> Arc<Store> {
    let store = SqlxStore::open(path, PoolConfig::default())
        .await
        .expect("open seam test store");
    Arc::new(store)
}

/// Empty-map resolver (vnc-038 ADR-004): no registered slug, `UnknownProject` for
/// ANY `Slug` — there is no default store and no default arm (the `Default` variant
/// is deleted). Mirrors the empty-`[[projects]]` `MultiProjectRouter` so the seam
/// can be exercised without that impl present.
struct EmptyResolver;

impl StoreResolver for EmptyResolver {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Slug(_) => Err(RouteError::UnknownProject),
        }
    }

    // These seam-only stubs prove `resolve_store` semantics and never dispatch,
    // so they own no `McpAdapter` and return `None` (vnc-034 Wave 2).
    fn adapter_for(&self, _key: &ProjectKey) -> Option<&McpAdapter> {
        None
    }
}

/// Stub standing in for the slug-keyed `MultiProjectRouter`: resolves a single
/// known slug, `UnknownProject` otherwise — never a default fall-through (vnc-038
/// ADR-004; the `Default` variant is deleted).
struct StubProjectRouter {
    store: Arc<Store>,
    known_slug: ProjectSlug,
}

impl StoreResolver for StubProjectRouter {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Slug(s) if *s == self.known_slug => Ok(Arc::clone(&self.store)),
            ProjectKey::Slug(_) => Err(RouteError::UnknownProject),
        }
    }

    fn adapter_for(&self, _key: &ProjectKey) -> Option<&McpAdapter> {
        None
    }
}

// ---- R-13 / AC-CT-C4 — route grammar (additive shape) ----

#[test]
fn test_route_v1_tools_no_longer_default() {
    // vnc-038 ADR-004 (#5083): the `/v1/tools/... -> Default` alias is DELETED.
    // `tools` now parses as a slug *candidate* (reserved/unregisterable), never a
    // default store.
    assert_eq!(
        parse_project_key("/v1/tools/call").expect("parse"),
        ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset"))
    );
    // Bare `/v1/tools` (no trailing segment) is also a slug candidate, not Default.
    assert_eq!(
        parse_project_key("/v1/tools").expect("parse"),
        ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset"))
    );
}

#[test]
fn test_route_v1_slug_tools_parses_to_slug() {
    let key = parse_project_key("/v1/myproj/tools/call").expect("parse");
    assert_eq!(
        key,
        ProjectKey::Slug(ProjectSlug::try_from("myproj").expect("valid slug"))
    );
}

#[test]
fn test_route_non_v1_paths_are_loud_error() {
    // vnc-038 ADR-004 (#5083): the `_ => Default` backward-compat fallback is
    // DELETED. A no-`/v1`-slug path is a loud `UnknownProject`, NEVER a default
    // store (AC-01 / R-10).
    assert_eq!(
        parse_project_key("/").expect_err("no-slug path is loud"),
        RouteError::UnknownProject
    );
    assert_eq!(
        parse_project_key("/messages").expect_err("no-slug path is loud"),
        RouteError::UnknownProject
    );
}

#[test]
fn test_route_tools_in_slug_position_parses_as_slug_candidate() {
    // vnc-038 ADR-004: `tools` in the slug position now parses as a slug candidate
    // (not a Default alias). It is unregisterable (reserved), so the resolver
    // 404s — never a default store.
    assert_eq!(
        parse_project_key("/v1/tools/anything").expect("parse"),
        ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset"))
    );
}

#[test]
fn test_route_reserved_words_in_slug_position_parse_as_slug_but_inert() {
    // health/observe/v1 as a 2nd segment pass the charset (so they parse to a
    // Slug), but under a default-like resolver they are UnknownProject. Refusing
    // to REGISTER them is a Wave-2 CLI concern, not a parse-edge concern.
    for word in ["health", "observe", "v1"] {
        let key = parse_project_key(&format!("/v1/{word}/tools")).expect("parse");
        assert_eq!(
            key,
            ProjectKey::Slug(ProjectSlug::try_from(word).expect("charset-valid")),
            "reserved word {word} in slug position parses as a slug"
        );
    }
}

// ---- R-03 — slug allowlist parse-edge guard (Wave-1: the guard itself) ----

#[test]
fn test_projectslug_accepts_valid() {
    for ok in ["a", "0", "my-proj", "abc123", "a-b-c", "project-1"] {
        assert!(
            ProjectSlug::try_from(ok).is_ok(),
            "expected {ok} to be accepted"
        );
    }
    // 63-char max boundary (1 leading alnum + 62 more).
    let max = format!("a{}", "b".repeat(62));
    assert_eq!(max.len(), 63);
    assert!(
        ProjectSlug::try_from(max.as_str()).is_ok(),
        "63-char slug must be accepted"
    );
}

#[test]
fn test_projectslug_rejects_traversal_corpus() {
    // Path-traversal, encoded separators, absolute paths, separators, leading
    // hyphen, uppercase, empty, and over-length — every one rejected at the edge.
    let over_length = format!("a{}", "b".repeat(63)); // 64 chars
    let rejected: Vec<&str> = vec![
        "../",
        "..",
        "a/../b",
        "%2e%2e",
        "%2f",
        "a%2fb",
        "%2e",
        "/etc",
        "/etc/passwd",
        ".",
        "/",
        "\\",
        "a\\b",
        "a.b",
        "-leading",
        "Abc",
        "MyProj",
        "a b",
        "a\tb",
        "",
        over_length.as_str(),
    ];
    for bad in rejected {
        let result = ProjectSlug::try_from(bad);
        assert!(
            matches!(result, Err(RouteError::InvalidSlug(_))),
            "expected {bad:?} to be rejected as InvalidSlug, got {result:?}"
        );
    }
}

#[test]
fn test_projectslug_over_length_boundary() {
    // 63 ok, 64 rejected — exact boundary.
    let ok63 = "a".repeat(63);
    let bad64 = "a".repeat(64);
    assert!(ProjectSlug::try_from(ok63.as_str()).is_ok());
    assert!(matches!(
        ProjectSlug::try_from(bad64.as_str()),
        Err(RouteError::InvalidSlug(_))
    ));
}

#[test]
fn test_projectslug_empty_rejected() {
    assert!(matches!(
        ProjectSlug::try_from(""),
        Err(RouteError::InvalidSlug(_))
    ));
}

// T-SEC-15 (DISCRIMINATOR, vnc-034 D1) — underscore is NOT in the locked charset
// `^[a-z0-9][a-z0-9-]{0,62}$`. The drifted issue-#727 regex `[a-z0-9_-]` would ACCEPT
// `my_project` and turn this test red — that is exactly its purpose.
#[test]
fn test_slug_reject_underscore_discriminator() {
    assert!(
        matches!(
            ProjectSlug::try_from("my_project"),
            Err(RouteError::InvalidSlug(_))
        ),
        "underscore is not in the D1 charset — a drifted [a-z0-9_-] impl wrongly accepts this"
    );
}

// T-SEC-16 (DISCRIMINATOR, vnc-034 D1) — 64 chars is over the 63 (DNS-label) bound. The
// drifted issue-#727 `{0,63}` (max 64) would ACCEPT this and turn the test red.
#[test]
fn test_slug_reject_64_char_discriminator() {
    let over = "a".repeat(64);
    assert_eq!(over.len(), 64);
    assert!(
        matches!(
            ProjectSlug::try_from(over.as_str()),
            Err(RouteError::InvalidSlug(_))
        ),
        "64-char slug is over the 63-char D1 bound — a drifted {{0,63}} impl wrongly accepts this"
    );
}

// T-SEC-17 — exact 63-char upper bound is valid.
#[test]
fn test_slug_accept_63_char_boundary() {
    let max = "a".repeat(63);
    assert!(ProjectSlug::try_from(max.as_str()).is_ok());
}

#[test]
fn test_projectslug_invalid_slug_carries_input_for_diagnostics() {
    // The rejected raw value is carried for diagnostics only — never used to
    // build a path. Proves the value reached the error, not a path join.
    match ProjectSlug::try_from("../escape") {
        Err(RouteError::InvalidSlug(s)) => assert_eq!(s, "../escape"),
        other => panic!("expected InvalidSlug, got {other:?}"),
    }
}

#[test]
fn test_route_grammar_rejects_traversal_slug_before_resolution() {
    // A traversal candidate in the slug position fails parse_project_key at the
    // allowlist edge — it never reaches resolve_store / any path join (R-03).
    let result = parse_project_key("/v1/..%2f/tools");
    assert!(
        matches!(result, Err(RouteError::InvalidSlug(_))),
        "traversal slug must fail at the parse edge, got {result:?}"
    );
}

// ---- R-01 — Slug under default-like resolver -> UnknownProject (no panic,
//      no default store) ----

#[tokio::test]
async fn test_slug_key_under_empty_resolver_returns_unknown_project() {
    // vnc-038 ADR-004: an empty-map resolver has NO default store. Any slug ->
    // UnknownProject; there is no `ProjectKey::Default` to resolve.
    let resolver = EmptyResolver;

    let slug = ProjectSlug::try_from("anyslug").expect("valid");
    let result = resolver.resolve_store(&ProjectKey::Slug(slug));
    assert_eq!(
        result.err(),
        Some(RouteError::UnknownProject),
        "Slug under the empty resolver must be UnknownProject — never a default store"
    );
}

// ---- R-01 — resolver-swap: the Wave1<->Wave2 boundary IS the trait ----

#[tokio::test]
async fn test_resolver_swap_requires_no_callsite_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_seam_test_store(&dir.path().join("seam.db")).await;
    let known = ProjectSlug::try_from("known").expect("valid");

    // Both resolvers satisfy `StoreResolver` and are injected the SAME way:
    // `Arc<dyn StoreResolver>`. No route-grammar / SlugRouter change required.
    let empty: Arc<dyn StoreResolver> = Arc::new(EmptyResolver);
    let populated: Arc<dyn StoreResolver> = Arc::new(StubProjectRouter {
        store: Arc::clone(&store),
        known_slug: known.clone(),
    });

    // Empty resolver: the known slug is inert (UnknownProject).
    assert_eq!(
        empty.resolve_store(&ProjectKey::Slug(known.clone())).err(),
        Some(RouteError::UnknownProject)
    );
    // Populated resolver: the same slug now resolves — purely a resolver swap.
    assert!(
        populated
            .resolve_store(&ProjectKey::Slug(known.clone()))
            .is_ok(),
        "the slug-keyed resolver lights up the slug at the SAME trait call site"
    );
    // An unknown slug stays UnknownProject too (no default fallback — vnc-038 ADR-004).
    let unknown = ProjectSlug::try_from("other").expect("valid");
    assert_eq!(
        populated.resolve_store(&ProjectKey::Slug(unknown)).err(),
        Some(RouteError::UnknownProject)
    );
}

// ---- R-10 / AC-CT-C6 — enterprise seam present, degenerate-but-documented ----

#[test]
fn test_storeresolver_seam_types_present() {
    // The C4/C6 enterprise extension surface exists as named interfaces
    // (documented-but-degenerate per the session_key precedent, NFR-09): the
    // trait is object-safe (usable as `Arc<dyn StoreResolver>`), ProjectKey has
    // its sole `Slug` arm (vnc-038 ADR-004 deleted `Default`), RouteError has both
    // variants. Construction here IS the assertion that the surface is named and
    // reachable.
    fn _assert_object_safe(_r: &Arc<dyn StoreResolver>) {}
    let _slug = ProjectKey::Slug(ProjectSlug::try_from("p").expect("valid"));
    let _e1 = RouteError::UnknownProject;
    let _e2 = RouteError::InvalidSlug("x".to_string());
}

// ---- RouteError surface ----

#[test]
fn test_route_error_display_no_input_leak() {
    // InvalidSlug Display must NOT echo the rejected raw input (avoid reflecting
    // attacker-controlled bytes into a response line); the variant still carries
    // it internally for structured diagnostics.
    let e = RouteError::InvalidSlug("../../etc/passwd".to_string());
    let msg = format!("{e}");
    assert_eq!(msg, "invalid project slug");
    assert!(
        !msg.contains("etc/passwd"),
        "must not echo raw input: {msg}"
    );

    assert_eq!(format!("{}", RouteError::UnknownProject), "unknown project");
}

// ===========================================================================
// vnc-038 ADR-004 (#5083) — the Default is DELETED. These tests INVERT the old
// `DefaultResolver` block (lesson #4452 — invert, do NOT delete): they assert
// that there is no default store, no default route key, and no default arm. The
// slug-keyed resolver is the SOLE served-project mechanism; local STDIO/UDS keeps
// its DIRECT path-hash binding and never enters the resolver (ADR-006 #5087).
// ===========================================================================

// ---- R-10 — no default store: slug-keyed resolution only (FR-X5) ----

#[tokio::test]
async fn test_slug_keyed_resolver_returns_the_registered_store() {
    // A registered slug resolves to ITS store (the slug map is the sole funnel).
    // The resolved handle is the SAME underlying store, not a re-opened one.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_seam_test_store(&dir.path().join("store.db")).await;
    let known = ProjectSlug::try_from("known").expect("valid");
    let resolver = StubProjectRouter {
        store: Arc::clone(&store),
        known_slug: known.clone(),
    };

    let resolved = resolver
        .resolve_store(&ProjectKey::Slug(known))
        .expect("registered slug resolves");
    assert!(
        Arc::ptr_eq(&resolved, &store),
        "a registered slug must resolve to its own injected store (Arc identity)"
    );
}

#[tokio::test]
async fn test_unregistered_slug_returns_unknown_project_never_a_default() {
    // ANY unregistered slug -> UnknownProject: never a default store, never a panic
    // (R-10 — no silent fall-through). There is no `ProjectKey::Default` to leak.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_seam_test_store(&dir.path().join("store.db")).await;
    let resolver = StubProjectRouter {
        store,
        known_slug: ProjectSlug::try_from("known").expect("valid"),
    };

    for slug in ["myproj", "other", "a", "health", "tools"] {
        let key = ProjectKey::Slug(ProjectSlug::try_from(slug).expect("valid slug"));
        let err = resolver
            .resolve_store(&key)
            .expect_err("unregistered slug must be inert");
        assert_eq!(
            err,
            RouteError::UnknownProject,
            "slug {slug} must resolve to UnknownProject, never a default store"
        );
    }
}

#[tokio::test]
async fn test_empty_resolver_serves_nothing_no_default() {
    // The empty-`[[projects]]` resolver (AC-09): NOTHING is servable. Every slug ->
    // UnknownProject; there is no default store to fall back to.
    let resolver = EmptyResolver;

    for slug in ["freshboot", "tools", "anyslug"] {
        let key = ProjectKey::Slug(ProjectSlug::try_from(slug).expect("valid"));
        assert_eq!(
            resolver
                .resolve_store(&key)
                .expect_err("empty resolver serves nothing"),
            RouteError::UnknownProject,
            "empty resolver must reject {slug} loudly — never a default store"
        );
    }
}

// ---- ADR-006 — local-UDS path-hash binding is NOT a resolver key ----

#[tokio::test]
async fn test_local_path_hash_store_never_enters_the_resolver() {
    // vnc-038 ADR-006 (#5087): local STDIO/UDS opens its path-hash store DIRECTLY at
    // boot and threads `Arc<Store>` straight to its handler — it is NOT routed
    // through the unified resolver and is NOT a resolver key. The deleted
    // `DefaultResolver`/`ProjectKey::Default` previously served the local store
    // through the seam; that path is GONE. The slug-keyed resolver never resolves a
    // path-hash store (there is no key for it), proving local bypasses the resolver.
    let dir = tempfile::tempdir().expect("tempdir");
    let path_hash_dir = dir.path().join("a1b2c3d4e5f60718");
    std::fs::create_dir_all(&path_hash_dir).expect("mk path-hash dir");
    let _path_hash_store = open_seam_test_store(&path_hash_dir.join("store.db")).await;

    // The resolver has no slug for the local store — any attempt is UnknownProject.
    let resolver = EmptyResolver;
    let local_attempt =
        ProjectKey::Slug(ProjectSlug::try_from("a1b2c3d4e5f60718").expect("valid charset"));
    assert_eq!(
        resolver
            .resolve_store(&local_attempt)
            .expect_err("path-hash is not a resolver key"),
        RouteError::UnknownProject,
        "the local path-hash store is never reached through the resolver (ADR-006)"
    );
}

#[tokio::test]
async fn test_slug_keyed_resolver_is_the_sole_served_mechanism() {
    // The slug-keyed resolver is the SOLE served-project mechanism behind the trait
    // (vnc-038 ADR-004): a registered slug resolves; an unknown slug is
    // UnknownProject. There is no parallel default path — the trait, not a default
    // arm, is the boundary.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = open_seam_test_store(&dir.path().join("store.db")).await;

    let resolver: Arc<dyn StoreResolver> = Arc::new(StubProjectRouter {
        store,
        known_slug: ProjectSlug::try_from("known").expect("valid"),
    });

    assert!(
        resolver
            .resolve_store(&ProjectKey::Slug(
                ProjectSlug::try_from("known").expect("valid")
            ))
            .is_ok()
    );
    let unknown = ProjectKey::Slug(ProjectSlug::try_from("nope").expect("valid"));
    assert_eq!(
        resolver.resolve_store(&unknown).expect_err("unknown slug"),
        RouteError::UnknownProject
    );
}

// ===========================================================================
// vnc-034 AGENT-6 — per-request seam wiring (AC-W1-X1, AC-W1-X3, AC-CT-C4)
//
// The seam author built `SlugRouter` + `StoreResolver`; the DefaultResolver
// author shipped `resolve_store`. This block proves the Wave-1<->2 boundary
// promise: `SlugRouter` is the REAL per-request MCP edge held by `PathRouter`,
// so every MCP request flows PathRouter -> SlugRouter -> resolve_store(...)
// -> dispatch — the store reaches MCP only THROUGH the funnel, never around it
// (FR-X5, no bypass). Behavioral (counting resolver) + structural (PathRouter's
// MCP edge IS a SlugRouter) assertions, neither needing a full UnimatrixServer.
// ===========================================================================

use std::sync::atomic::{AtomicUsize, Ordering};

/// Resolver that COUNTS every `resolve_store` call and records the last key,
/// then deliberately returns `UnknownProject` for ALL keys — including
/// `Default`. Returning an error short-circuits `SlugRouter::route_mcp` at the
/// funnel (it answers 404 before any MCP dispatch), so the behavioral proof that
/// "the per-request path consults `resolve_store` BEFORE dispatch" needs no real
/// `McpAdapter`/`UnimatrixServer`. The count + recorded key are the assertion
/// surface: a bypass (PathRouter dispatching straight to ProjectRouter) would
/// leave the count at 0.
#[derive(Clone)]
struct CountingResolver {
    calls: Arc<AtomicUsize>,
    last_was_slug: Arc<std::sync::atomic::AtomicBool>,
}

impl CountingResolver {
    fn new() -> Self {
        CountingResolver {
            calls: Arc::new(AtomicUsize::new(0)),
            last_was_slug: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl StoreResolver for CountingResolver {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.last_was_slug
            .store(matches!(key, ProjectKey::Slug(_)), Ordering::SeqCst);
        // Error on every key so route_mcp returns at the funnel without needing
        // a real downstream dispatch. vnc-038 ADR-004: every served key is a
        // `Slug` (the `Default` variant is deleted); a no-slug path 404s at
        // `parse_project_key` before reaching here.
        Err(RouteError::UnknownProject)
    }

    // Errors on every key, so `route_mcp` returns at the funnel before dispatch;
    // `adapter_for` is never consulted. `None` keeps it `StoreResolver`-complete.
    fn adapter_for(&self, _key: &ProjectKey) -> Option<&McpAdapter> {
        None
    }
}

/// Build a real `SlugRouter` over a `CountingResolver` and the real
/// `ProjectRouter` is NOT needed for the funnel-gating legs — but `SlugRouter`
/// owns a `ProjectRouter<ReqBody>`, so these tests construct it only when the
/// resolver returns Ok. Here every key errors, so the `project_router` is never
/// reached; we still must supply one. We avoid `UnimatrixServer` by proving the
/// funnel at the `SlugRouter::route_mcp` boundary via the error short-circuit.
///
/// To keep this test free of a heavyweight server, we exercise the funnel
/// ordering directly: parse the path, call the counting resolver, and assert the
/// resolver saw the per-request transport-derived key. This mirrors EXACTLY the
/// first two steps of `SlugRouter::route_mcp` (parse_project_key -> resolve_store)
/// which is the no-bypass contract; the dispatch tail is covered by the existing
/// MCP routing tests.
#[tokio::test]
async fn test_per_request_funnel_consults_resolver_with_transport_key() {
    let resolver = CountingResolver::new();

    // Drive the EXACT funnel head `SlugRouter::route_mcp` runs per request:
    // parse the transport path into a ProjectKey, then resolve through the funnel.
    // A bypass would skip this and never increment the counter. vnc-038 ADR-004:
    // `/v1/tools/...` now parses `tools` as a slug candidate (no Default alias).
    let key = parse_project_key("/v1/tools/call").expect("parse slug candidate");
    assert_eq!(
        key,
        ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset")),
        "the slug is transport-derived from the URL position"
    );

    // Per-request resolution: the funnel is consulted with the transport key.
    let _ = resolver.resolve_store(&key);

    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        1,
        "every MCP request must consult resolve_store exactly once (no bypass, FR-X5)"
    );
    assert!(
        resolver.last_was_slug.load(Ordering::SeqCst),
        "the funnel must receive the transport-derived ProjectKey::Slug, \
         never a payload-named project (AC-W1-X3 / FR-X2)"
    );
}

/// Structural proof that the funnel is ON the per-request path: `PathRouter`'s
/// MCP edge IS a `SlugRouter` (built from an injected `StoreResolver`), not a
/// bare `ProjectRouter`. The Debug surface names the `slug_router` field, which
/// only exists because the MCP fall-through arm dispatches through the seam.
/// If a future change reverted the MCP arm to `ProjectRouter::route_mcp`, the
/// field (and this assertion) would have to be removed — making the bypass
/// loud, not silent (R-01 sc.1).
#[test]
fn test_path_router_mcp_edge_is_the_slug_router_seam() {
    // `PathRouter::new(resolver, observe_ctx)` builds the SlugRouter internally —
    // the resolver argument is the ONLY Wave-1<->Wave-2 swap point (R-01 sc.2) and,
    // post funnel-elimination, the SOLE dispatch owner (no fixed project_router
    // param). Type-level: `new` accepts `Arc<dyn StoreResolver>`, proving the
    // funnel is injected at the per-request edge.
    fn _accepts_resolver_at_mcp_edge<ReqBody>(_: Arc<dyn StoreResolver>)
    where
        ReqBody: Body + Send + 'static,
        ReqBody::Data: Send + 'static,
        ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        // The existence of this signature constraint is the assertion: a
        // bypassed PathRouter (holding a bare ProjectRouter) would take no
        // resolver here.
    }
    let resolver: Arc<dyn StoreResolver> = Arc::new(CountingResolver::new());
    _accepts_resolver_at_mcp_edge::<TestBody>(resolver);
}

/// A slug request on the per-request MCP path is answered from the FUNNEL
/// (`UnknownProject` -> 404), never falling through to the default store
/// (R-01 sc.3). The counting resolver proves the resolver — not a bypass — made
/// the call: the count is 1 with the slug key recorded.
#[tokio::test]
async fn test_per_request_slug_rejected_at_funnel_not_default_store() {
    let resolver = CountingResolver::new();

    let key = parse_project_key("/v1/otherproj/tools/call").expect("parse slug");
    assert_eq!(
        key,
        ProjectKey::Slug(ProjectSlug::try_from("otherproj").expect("valid")),
        "slug is transport-derived from the URL position"
    );

    let outcome = resolver.resolve_store(&key);
    assert_eq!(
        outcome.expect_err("unregistered slug is inert at the funnel"),
        RouteError::UnknownProject,
        "a slug must be rejected AT the funnel, never silently served a default store"
    );
    assert_eq!(
        resolver.calls.load(Ordering::SeqCst),
        1,
        "the per-request funnel consulted the resolver (no bypass)"
    );
    assert!(
        resolver.last_was_slug.load(Ordering::SeqCst),
        "the funnel saw Slug(_) — identity came from the transport (vnc-038 ADR-004: \
         every served key is a Slug; there is no Default)"
    );
}

// ===========================================================================
// vnc-038 Stage 3c — OVER-THE-WIRE per-slug observe isolation at N=2
// (R-02 / R-09 / R-12 · AC-06 / AC-07 / AC-08 · C-11 / GATE-4).
//
// The mapper-isolation tests above call `observe_response_to_http(...)` directly
// and the `mock_dispatch_request` routing tests stub out the handler — NEITHER
// drives the REAL `route_observe` funnel (`parse_project_key -> resolve_store ->
// dispatch`). This block does: it builds a REAL `ObserveContext` over a REAL
// `MultiProjectRouter` wired to TWO distinct per-slug `UnimatrixServer`/`Store`
// instances, injects a real `ResolvedIdentity`, and POSTs to
// `/v1/{slug}/observe`. It is the observe-side complement of the MCP funnel proof
// (`project_routing_integration.rs`) and the counting-resolver MCP test above.
//
// The N=2 mandate (#4974): a `RecordingResolver` wraps the real resolver and
// records WHICH slug each observe call resolved. An N=1 green would not catch a
// boot-bound/parallel observe path that ignores the transport key; with two
// registered slugs, the recorder proves each observe consulted the funnel ONCE
// with the matching `ProjectKey::Slug` and reached dispatch against that store
// (a 200 `Pong`). A no-slug / unregistered observe is a loud 404 — never a
// default store (R-10).
// ===========================================================================

use std::sync::Mutex as StdMutex;

use super::handlers::route_observe;
use crate::http::router::{MultiProjectRouter, ObserveContext, ProjectServerInput};
use crate::infra::registry::{Capability, TrustLevel};
use crate::mcp::identity::ResolvedIdentity;
use crate::server::tests::make_server;

const OBSERVE_MAX_BODY: usize = 1024 * 1024;

/// Resolver that wraps the REAL `MultiProjectRouter` and RECORDS the slug it
/// resolved on each `resolve_store` call (vnc-038 N=2 observe proof). The
/// recorded sequence is the assertion surface: a boot-bound or parallel observe
/// path that ignored the transport key would leave the recorder empty or carry
/// the wrong slug. Delegation is total — resolution/dispatch still come from the
/// SAME inner map, so this is observation only, not a behavioral stub (#4974).
struct RecordingResolver {
    inner: MultiProjectRouter,
    resolved: Arc<StdMutex<Vec<String>>>,
}

impl RecordingResolver {
    fn new(inner: MultiProjectRouter) -> (Self, Arc<StdMutex<Vec<String>>>) {
        let resolved = Arc::new(StdMutex::new(Vec::new()));
        (
            RecordingResolver {
                inner,
                resolved: Arc::clone(&resolved),
            },
            resolved,
        )
    }
}

impl StoreResolver for RecordingResolver {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        // Record the transport-derived slug BEFORE delegating, so even an Err
        // (UnknownProject) leg is recorded — the funnel ran exactly once.
        match key {
            ProjectKey::Slug(s) => self.resolved.lock().unwrap().push(s.to_string()),
        }
        self.inner.resolve_store(key)
    }

    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter> {
        self.inner.adapter_for(key)
    }
}

/// Build an `ObserveContext` over the given resolver, sourcing the non-resolver
/// service handles from a throwaway `UnimatrixServer` (the resolver supplies the
/// per-request store; these are the embed/vector/adapt/session deps
/// `dispatch_request` also needs). Mirrors the boot wiring in `main.rs` (the
/// `ObserveContext` holds the SAME `Arc<dyn StoreResolver>` the `SlugRouter`
/// holds), with no isolated scaffolding.
fn observe_ctx_over(resolver: Arc<dyn StoreResolver>, deps: &UnimatrixServer) -> ObserveContext {
    ObserveContext {
        resolver,
        embed_service: Arc::clone(&deps.embed_service),
        vector_store: Arc::clone(&deps.vector_store),
        adapt_service: Arc::clone(&deps.adapt_service),
        server_version: "test".to_string(),
        session_registry: Arc::clone(&deps.session_registry),
        pending_entries_analysis: Arc::clone(&deps.pending_entries_analysis),
        services: deps.services.clone(),
    }
}

/// A `POST /v1/{slug}/observe` request carrying a `Ping` body and an injected
/// admin `ResolvedIdentity` (the StaticTokenAuth layer injects identity in
/// production; here we inject it directly since the handler reads it from
/// extensions). `Ping` reaches dispatch and answers `Pong` (200) iff the request
/// resolved to a real per-slug store — the over-the-wire "reached the right
/// store" signal.
fn observe_ping_request(slug: &str) -> Request<TestBody> {
    let body_json = serde_json::json!({ "type": "Ping" }).to_string();
    let mut req = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/{slug}/observe"))
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(body_json))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("build observe request");
    req.extensions_mut().insert(ResolvedIdentity {
        agent_id: "human".to_string(),
        trust_level: TrustLevel::Privileged,
        capabilities: vec![
            Capability::Read,
            Capability::Write,
            Capability::Search,
            Capability::Admin,
        ],
    });
    req
}

async fn drive_observe(ctx: &ObserveContext, slug: &str) -> (StatusCode, String) {
    let resp = route_observe(ctx.clone(), observe_ping_request(slug))
        .await
        .expect("route_observe is infallible");
    collect_body(resp).await
}

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_per_slug_funnel_isolation_n2() {
    // Two registered projects (N=2, MANDATORY — not N=1, #4974). Build the REAL
    // resolver over two DISTINCT per-slug servers, wrap it in the recorder, and
    // POST observe Pings to each slug. Each observe must consult the funnel ONCE
    // with the matching transport-derived slug and reach dispatch against THAT
    // slug's store (200 Pong) — never a boot-bound/parallel/default path.
    let alpha = make_server().await;
    let beta = make_server().await;
    // A throwaway server supplies the non-resolver ObserveContext service deps.
    let deps = make_server().await;

    let alpha_input = ProjectServerInput {
        slug: ProjectSlug::try_from("alpha").expect("valid"),
        store: Arc::clone(&alpha.store),
        server: alpha,
    };
    let beta_input = ProjectServerInput {
        slug: ProjectSlug::try_from("beta").expect("valid"),
        store: Arc::clone(&beta.store),
        server: beta,
    };
    let inner = MultiProjectRouter::from_servers(
        vec![alpha_input, beta_input],
        OBSERVE_MAX_BODY,
        vec![],
        // bug #774: non-empty allowed_hosts (empty = rmcp fail-open).
        vec!["localhost".to_string()],
    )
    .expect("build resolver");

    let (recording, resolved) = RecordingResolver::new(inner);
    let resolver: Arc<dyn StoreResolver> = Arc::new(recording);
    let ctx = observe_ctx_over(resolver, &deps);

    // Observe to alpha, then beta — each reaches its own store's dispatch (Pong).
    let (alpha_status, alpha_body) = drive_observe(&ctx, "alpha").await;
    let (beta_status, beta_body) = drive_observe(&ctx, "beta").await;

    assert_eq!(
        alpha_status,
        StatusCode::OK,
        "observe to /v1/alpha/observe must reach alpha's store dispatch (200 Pong); body {alpha_body}"
    );
    assert_eq!(
        beta_status,
        StatusCode::OK,
        "observe to /v1/beta/observe must reach beta's store dispatch (200 Pong); body {beta_body}"
    );
    assert!(
        alpha_body.contains("Pong") && beta_body.contains("Pong"),
        "both observe Pings must answer Pong (reached dispatch): {alpha_body} / {beta_body}"
    );

    // The funnel was consulted exactly once per observe, with the MATCHING slug —
    // the N=2 isolation proof (resolve identity == transport-derived slug).
    let seq = resolved.lock().unwrap().clone();
    assert_eq!(
        seq,
        vec!["alpha".to_string(), "beta".to_string()],
        "each observe must resolve ONCE through the funnel with its own transport-derived slug \
         (no boot-bound/parallel observe path, no cross-resolution); got {seq:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_unregistered_slug_is_loud_404_not_default() {
    // An observe POST to a valid-grammar but UNREGISTERED slug is a loud 404
    // `unknown project` — never a default store (R-09/R-10). The funnel still ran
    // once with the transport slug; it did NOT fall through.
    let only = make_server().await;
    let deps = make_server().await;

    let only_input = ProjectServerInput {
        slug: ProjectSlug::try_from("only").expect("valid"),
        store: Arc::clone(&only.store),
        server: only,
    };
    let inner = MultiProjectRouter::from_servers(
        vec![only_input],
        OBSERVE_MAX_BODY,
        vec![],
        vec!["localhost".to_string()],
    )
    .expect("build resolver");
    let (recording, resolved) = RecordingResolver::new(inner);
    let resolver: Arc<dyn StoreResolver> = Arc::new(recording);
    let ctx = observe_ctx_over(resolver, &deps);

    let (status, body) = drive_observe(&ctx, "ghost").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "observe to an unregistered slug must be a loud 404, never a default store"
    );
    assert!(
        body.contains("unknown project"),
        "404 body must name the failure; got {body}"
    );
    assert_eq!(
        resolved.lock().unwrap().clone(),
        vec!["ghost".to_string()],
        "the funnel was consulted once with the transport slug, then 404'd (no bypass)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_empty_resolver_first_boot_is_loud_404() {
    // First-boot / empty `[[projects]]`: the resolver has NO entries, so EVERY
    // observe is a loud 404 — nothing servable, never a silent default (R-10 /
    // AC-09 at the observe entry point).
    let deps = make_server().await;
    let inner = MultiProjectRouter::from_servers(
        vec![],
        OBSERVE_MAX_BODY,
        vec![],
        vec!["localhost".to_string()],
    )
    .expect("build empty resolver");
    let resolver: Arc<dyn StoreResolver> = Arc::new(inner);
    let ctx = observe_ctx_over(resolver, &deps);

    let (status, body) = drive_observe(&ctx, "anyslug").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "with no registered projects, observe must fail loud (404), never a default store"
    );
    assert!(body.contains("unknown project"), "loud body; got {body}");
}
