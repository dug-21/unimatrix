use super::*;
use http_body_util::Full;

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
async fn dispatch_request<ReqBody>(
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
        (Method::POST, "/observe") => observe_stub_response(),
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- T-PR-04: POST /observe returns 501 ----

#[tokio::test]
async fn test_post_observe_returns_501_stub() {
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(
        body,
        r#"{"error":"Remote telemetry not yet implemented. See W2-7."}"#
    );
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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
    // /observe is handled by PathRouter (501 stub), not a 404.
    let mock = MockMcpAdapter::new();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/observe")
        .body(empty_body())
        .unwrap();

    let resp = dispatch_request(&mock, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
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
    let resp = dispatch_request(&mock, req).await;
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
    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
    let (status, body) = collect_body(resp).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""routed":"mcp""#), "body: {body}");
}

// ---- Response format tests ----

#[test]
fn test_observe_stub_response_format() {
    let resp = observe_stub_response();
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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

    let resp = dispatch_request(&mock, req).await;
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
