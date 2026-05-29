use super::*;
use http::Request;
use http_body_util::BodyExt;
use std::convert::Infallible;

/// Mock inner service that returns 200 OK and captures the request extensions.
#[derive(Clone)]
struct MockInnerService;

impl<B: Send + 'static> Service<Request<B>> for MockInnerService {
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let has_identity = req.extensions().get::<ResolvedIdentity>().is_some();
        let identity_json = if let Some(id) = req.extensions().get::<ResolvedIdentity>() {
            format!(
                r#"{{"agent_id":"{}","trust_level":"{:?}","caps":{}}}"#,
                id.agent_id,
                id.trust_level,
                id.capabilities.len()
            )
        } else {
            "null".to_string()
        };

        let body = format!(
            r#"{{"status":"ok","has_identity":{},"identity":{}}}"#,
            has_identity, identity_json
        );

        Box::pin(async move {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(
                    Full::new(Bytes::from(body))
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("test response builder"))
        })
    }
}

fn test_token_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = i as u8;
    }
    bytes
}

fn test_token_hex() -> String {
    hex::encode(test_token_bytes())
}

fn make_auth_service() -> StaticTokenAuth<MockInnerService> {
    let layer = StaticTokenAuthLayer::new(test_token_bytes());
    layer.layer(MockInnerService)
}

fn empty_body() -> BoxBody<Bytes, Infallible> {
    Full::new(Bytes::new())
        .map_err(|never| match never {})
        .boxed()
}

async fn body_to_string(resp: Response<BoxBody<Bytes, Infallible>>) -> (StatusCode, String) {
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8(body.to_vec()).expect("utf8 body"))
}

const EXPECTED_401_BODY: &str = r#"{"error":"missing or invalid authorization"}"#;

// ---- T-STA-01 ----

#[tokio::test]
async fn test_valid_bearer_token_passes_to_inner_service() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {}", test_token_hex()))
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""has_identity":true"#), "body: {body}");
}

// ---- T-STA-02 ----

#[tokio::test]
async fn test_missing_authorization_header_returns_401() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, EXPECTED_401_BODY);
}

// ---- T-STA-03 ----

#[tokio::test]
async fn test_wrong_token_returns_401() {
    let mut svc = make_auth_service();
    let wrong = hex::encode([0xffu8; 32]);
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {wrong}"))
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, EXPECTED_401_BODY);
}

// ---- T-STA-04 ----

#[tokio::test]
async fn test_non_bearer_prefix_returns_401() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", "Basic dXNlcjpwYXNz")
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- T-STA-05 ----

#[tokio::test]
async fn test_malformed_hex_token_returns_401() {
    let mut svc = make_auth_service();
    let bad_hex = "z".repeat(64);
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {bad_hex}"))
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, EXPECTED_401_BODY);
}

// ---- T-STA-06 ----

#[tokio::test]
async fn test_short_hex_token_returns_401() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", "Bearer aabb")
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, EXPECTED_401_BODY);
}

// ---- T-STA-07 ----

#[tokio::test]
async fn test_response_body_identical_for_all_rejection_paths() {
    // (a) No Authorization header
    let mut svc = make_auth_service();
    let req_a = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .body(empty_body())
        .unwrap();
    let (status_a, body_a) = body_to_string(svc.call(req_a).await.unwrap()).await;

    // (b) Wrong token
    let mut svc = make_auth_service();
    let wrong = hex::encode([0xffu8; 32]);
    let req_b = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {wrong}"))
        .body(empty_body())
        .unwrap();
    let (status_b, body_b) = body_to_string(svc.call(req_b).await.unwrap()).await;

    // (c) Malformed hex
    let mut svc = make_auth_service();
    let bad = "g".repeat(64);
    let req_c = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {bad}"))
        .body(empty_body())
        .unwrap();
    let (status_c, body_c) = body_to_string(svc.call(req_c).await.unwrap()).await;

    assert_eq!(status_a, StatusCode::UNAUTHORIZED);
    assert_eq!(status_b, StatusCode::UNAUTHORIZED);
    assert_eq!(status_c, StatusCode::UNAUTHORIZED);
    assert_eq!(body_a, EXPECTED_401_BODY);
    assert_eq!(body_b, EXPECTED_401_BODY);
    assert_eq!(body_c, EXPECTED_401_BODY);
}

// ---- T-STA-08 ----

#[tokio::test]
async fn test_valid_token_inserts_resolved_identity_into_extensions() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {}", test_token_hex()))
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""agent_id":"http-bearer""#), "body: {body}");
    assert!(
        body.contains(r#""trust_level":"Restricted""#),
        "body: {body}"
    );
    assert!(body.contains(r#""caps":3"#), "body: {body}");
}

// ---- T-STA-09 ----

#[test]
fn test_credential_type_constant_is_static_token() {
    assert_eq!(CREDENTIAL_TYPE_STATIC_TOKEN, "static_token");
}

// ---- T-STA-10 ----

#[tokio::test]
async fn test_bearer_validator_trait_valid_token() {
    let validator = StaticTokenValidator::new(test_token_bytes());
    let result = validator.validate(&test_token_hex()).await;
    let identity = result.expect("valid token should succeed");
    assert_eq!(identity.agent_id, "http-bearer");
    assert_eq!(identity.trust_level, TrustLevel::Restricted);
    assert_eq!(
        identity.capabilities,
        vec![Capability::Read, Capability::Write, Capability::Search]
    );
}

// ---- T-STA-11 ----

#[tokio::test]
async fn test_bearer_validator_trait_invalid_token() {
    let validator = StaticTokenValidator::new(test_token_bytes());
    let result = validator.validate(&hex::encode([0xffu8; 32])).await;
    assert!(result.is_err(), "wrong token should fail");
}

// ---- T-STA-12 ----

#[tokio::test]
async fn test_empty_authorization_header_returns_401() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", "")
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- T-STA-13 ----

#[tokio::test]
async fn test_bearer_with_extra_spaces_returns_401() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!(" Bearer  {}", test_token_hex()))
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- T-STA-14 ----

#[tokio::test]
async fn test_very_long_authorization_header_returns_401() {
    let mut svc = make_auth_service();
    let long_hex = "aa".repeat(500_000); // 1MB of hex chars
    let req = Request::builder()
        .uri("/mcp")
        .method(Method::POST)
        .header("authorization", format!("Bearer {long_hex}"))
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- Health bypass tests (ADR-002) ----

#[tokio::test]
async fn test_health_get_bypass_no_auth_needed() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/health")
        .method(Method::GET)
        .body(empty_body())
        .unwrap();

    let (status, body) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""status":"ok""#), "body: {body}");
}

#[tokio::test]
async fn test_health_post_no_bypass() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/health")
        .method(Method::POST)
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_healthz_no_bypass() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/healthz")
        .method(Method::GET)
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_health_trailing_slash_no_bypass() {
    let mut svc = make_auth_service();
    let req = Request::builder()
        .uri("/health/")
        .method(Method::GET)
        .body(empty_body())
        .unwrap();

    let (status, _) = body_to_string(svc.call(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[test]
fn test_health_path_constant() {
    assert_eq!(HEALTH_PATH, "/health");
}
