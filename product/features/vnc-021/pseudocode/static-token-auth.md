# static-token-auth (C2) -- `src/http/auth.rs`

## Purpose

Tower middleware that validates `Authorization: Bearer <token>` headers using constant-time comparison (ADR-001). Produces a `ResolvedIdentity` on success and inserts it into request extensions. Bypasses auth for `GET /health` (ADR-002). Defines the `BearerValidator` trait for W2-3 extensibility (FR-14).

## Constants

```
CREDENTIAL_TYPE_STATIC_TOKEN: &str = "static_token"
HEALTH_PATH: &str = "/health"
AUTH_BYPASS_PATHS: &[(&str, http::Method)] = &[("/health", Method::GET)]
```

## Types

### `AuthError`

```
enum AuthError:
    MissingHeader,
    InvalidFormat,       // not "Bearer <hex>"
    InvalidToken,        // constant-time comparison failed
    Internal(String),    // unexpected failures
```

### `BearerValidator` trait (FR-14)

```
trait BearerValidator: Send + Sync + 'static:
    /// Validate a bearer token string and return a ResolvedIdentity on success.
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>
```

### `StaticTokenValidator`

The concrete implementation of `BearerValidator` for static tokens.

```
struct StaticTokenValidator:
    /// Raw token bytes (32 bytes) for constant-time comparison
    token_bytes: [u8; 32]

impl BearerValidator for StaticTokenValidator:
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>:
        // Step 1: Hex-decode the presented token
        // This is an early-return -- acceptable because it does NOT leak
        // information about the stored token value (ADR-001)
        let presented_bytes = hex::decode(token)
            .map_err(|_| AuthError::InvalidToken)?

        // Step 2: Length check before ConstantTimeEq
        // ConstantTimeEq requires equal-length slices. If length differs,
        // we still must not reveal length of valid token -- but our token
        // is always 32 bytes (public knowledge), so length mismatch is
        // an immediate reject without timing leak.
        if presented_bytes.len() != 32:
            return Err(AuthError::InvalidToken)

        // Step 3: Constant-time comparison (ADR-001)
        // subtle::ConstantTimeEq compares all 32 bytes regardless of content
        use subtle::ConstantTimeEq
        let is_valid: bool = self.token_bytes.ct_eq(&presented_bytes).into()

        if !is_valid:
            return Err(AuthError::InvalidToken)

        // Step 4: Construct ResolvedIdentity for HTTP bearer callers
        return Ok(ResolvedIdentity {
            agent_id: "http-bearer".to_string(),
            trust_level: TrustLevel::Standard,
            capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
        })
```

### `StaticTokenAuthLayer`

Tower `Layer` that wraps any inner service with auth middleware.

```
struct StaticTokenAuthLayer:
    validator: Arc<dyn BearerValidator>

impl StaticTokenAuthLayer:
    fn new(token_bytes: [u8; 32]) -> Self:
        StaticTokenAuthLayer {
            validator: Arc::new(StaticTokenValidator { token_bytes })
        }

impl<S> Layer<S> for StaticTokenAuthLayer:
    type Service = StaticTokenAuth<S>

    fn layer(&self, inner: S) -> StaticTokenAuth<S>:
        StaticTokenAuth {
            inner,
            validator: Arc::clone(&self.validator),
        }
```

### `StaticTokenAuth<S>` -- Tower Service

```
struct StaticTokenAuth<S>:
    inner: S
    validator: Arc<dyn BearerValidator>

impl<S> Service<Request<Body>> for StaticTokenAuth<S>
where S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
      S::Future: Send:

    type Response = Response<Body>
    type Error = S::Error  // or Infallible if we handle all errors
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>> + Send>>

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>:
        self.inner.poll_ready(cx)

    fn call(&mut self, request: Request<Body>) -> Self::Future:
        // --- AUTH BYPASS CHECK (ADR-002) ---
        // Exact path match + method check. NEVER use starts_with (R-07).
        let path = request.uri().path()
        let method = request.method()

        let is_bypassed = AUTH_BYPASS_PATHS.iter()
            .any(|(p, m)| path == *p && method == *m)

        if is_bypassed:
            // Forward directly -- no auth check
            let future = self.inner.call(request)
            return Box::pin(future)

        // --- EXTRACT AUTHORIZATION HEADER ---
        let auth_header = match request.headers().get(http::header::AUTHORIZATION):
            Some(value) => value,
            None =>
                // Early return: missing header. Does not leak token info (ADR-001).
                return Box::pin(async { Ok(unauthorized_response()) })

        // Parse header value to string
        let auth_str = match auth_header.to_str():
            Ok(s) => s,
            Err(_) =>
                // Non-ASCII header value
                return Box::pin(async { Ok(unauthorized_response()) })

        // --- EXTRACT BEARER PREFIX ---
        // Early return on wrong prefix is acceptable (ADR-001)
        let token = match auth_str.strip_prefix("Bearer "):
            Some(t) => t.to_string(),
            None =>
                return Box::pin(async { Ok(unauthorized_response()) })

        // --- VALIDATE TOKEN ---
        let validator = Arc::clone(&self.validator)
        let mut inner = self.inner.clone()

        Box::pin(async move {
            match validator.validate(&token).await:
                Ok(identity) =>
                    // Insert ResolvedIdentity into request extensions
                    // This is consumed by rmcp -> build_context_with_external_identity
                    let mut request = request
                    request.extensions_mut().insert(identity)
                    inner.call(request).await

                Err(_) =>
                    // ALL auth failures produce identical response (FR-10, FR-11)
                    Ok(unauthorized_response())
        })
```

### `unauthorized_response()` helper

```
fn unauthorized_response() -> Response<Body>:
    // Identical response for all rejection paths (R-02 mitigation)
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"error":"missing or invalid authorization"}"#))
        .unwrap()  // static builder -- cannot fail
```

## State Machine

StaticTokenAuth is stateless per-request. The validator holds the token in memory for the process lifetime. No state transitions.

## Error Handling

| Error Case | HTTP Response | Notes |
|-----------|--------------|-------|
| Missing Authorization header | 401 + JSON body | Early return (ADR-001 permits) |
| Non-"Bearer" prefix | 401 + JSON body | Early return (ADR-001 permits) |
| Invalid hex in token | 401 + JSON body | Reaches validator; no timing leak |
| Wrong token length | 401 + JSON body | Reaches validator; no timing leak |
| Wrong token value | 401 + JSON body | Constant-time comparison (ADR-001) |
| /health GET | bypass auth entirely | Exact match only (ADR-002, R-07) |
| /health POST | 401 required | Not in bypass list |
| /healthz GET | 401 required | Not in bypass list (R-07) |

## credential_type Integration (ADR-006)

The `CREDENTIAL_TYPE_STATIC_TOKEN` constant is defined here but consumed downstream. The `ResolvedIdentity` injected by this middleware flows through rmcp extensions to `build_context_with_external_identity` in server.rs, which builds the `AuditContext`. The credential_type value is set in the audit emission path when the identity source is `external_identity: Some(...)`.

The implementation agent must verify that the credential_type field in AuditContext/AuditEvent is populated correctly. The existing `build_context_with_external_identity` does NOT currently set credential_type -- this needs to be wired. The constant is exported for use in the audit emission path.

**Gap**: `build_context_with_external_identity` in server.rs builds `AuditSource::Mcp` but does not set a `credential_type` field. The AuditEvent emission path must use `CREDENTIAL_TYPE_STATIC_TOKEN` when the source identity was external. The implementation agent should check how `credential_type` flows through AuditContext -> AuditEvent -> audit_log INSERT. If `credential_type` is a field on `AuditContext`, add it there. If it is derived from the presence of `external_identity`, the derivation must be added.

## Key Test Scenarios

1. **Valid token**: Request with correct `Authorization: Bearer <hex>`. Verify inner service called with `ResolvedIdentity` in extensions.
2. **Wrong token**: Request with incorrect hex. Verify 401 response, identical body to missing-header case.
3. **Missing header**: Request with no Authorization. Verify 401.
4. **Wrong prefix**: `Authorization: Basic <token>`. Verify 401.
5. **Non-hex token**: `Authorization: Bearer gggggggg...`. Verify 401.
6. **Health bypass**: `GET /health` with no auth. Verify forwarded to inner service.
7. **Health POST no bypass**: `POST /health` with no auth. Verify 401.
8. **Health prefix no bypass**: `GET /healthz` with no auth. Verify 401 (R-07 critical).
9. **Health trailing slash**: `GET /health/` with no auth. Verify 401 (R-07).
10. **Constant-time**: Code review -- verify `subtle::ConstantTimeEq` used, no early returns between hex decode and comparison.
