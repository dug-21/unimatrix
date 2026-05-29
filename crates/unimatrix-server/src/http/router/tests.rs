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

    match (method, path.as_str()) {
        (Method::GET, "/health") => map_health_response(health_response()),
        (Method::POST, "/observe") => {
            // The real handler would check identity then parse body.
            // Without ResolvedIdentity, it returns 500. For routing tests,
            // we return 400 to indicate the route was matched.
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

// ---- T-PR-04: POST /observe routes to observe handler (vnc-022) ----

#[tokio::test]
async fn test_post_observe_routes_to_handler() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    // Mock returns 400 for /observe to indicate the route was matched.
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

// ---- T-PR-10: ProjectRouter default project mode ----

#[tokio::test]
async fn test_project_router_default_project_mode() {
    // Sending to root path should route to default project, not 404.
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
        "root path routes to default project"
    );
}

// ---- T-PR-11: /observe registered in routing tree ----

#[test]
fn test_observe_path_constant() {
    assert_eq!(OBSERVE_PATH, "/observe");
}

#[tokio::test]
async fn test_observe_registered_in_routing_tree() {
    // /observe is handled by PathRouter (observe handler), not forwarded to MCP.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = mock_dispatch_request(&mock, req).await;
    // Mock returns 400 for /observe; if it were forwarded to MCP we'd get 200.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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
    let resp = observe_response_to_http(HookResponse::Ack);
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
    let resp = observe_response_to_http(HookResponse::Entries {
        items: vec![entry],
        total_tokens: 150,
    });
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["type"], "Entries");
    assert!(json["items"].is_array());
    assert_eq!(json["total_tokens"], 150);
}

#[tokio::test]
async fn test_observe_response_briefing_content_maps_to_200_json() {
    let resp = observe_response_to_http(HookResponse::BriefingContent {
        content: "briefing text".to_string(),
        token_count: 50,
    });
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
    let resp = observe_response_to_http(HookResponse::Pong {
        server_version: "0.1.0".to_string(),
    });
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
    let resp = observe_response_to_http(HookResponse::Error {
        code: -32004,
        message: "bad input".to_string(),
    });
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
    let resp = observe_response_to_http(HookResponse::Entries {
        items: vec![],
        total_tokens: 0,
    });
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["items"], serde_json::json!([]));
}

#[tokio::test]
async fn test_observe_response_briefing_content_empty_string() {
    let resp = observe_response_to_http(HookResponse::BriefingContent {
        content: "".to_string(),
        token_count: 0,
    });
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
            },
            ImplantEvent {
                event_type: "PostToolUse".to_string(),
                session_id: "sess-b".to_string(),
                timestamp: 2,
                payload: serde_json::json!({}),
                topic_signal: None,
                provider: None,
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
