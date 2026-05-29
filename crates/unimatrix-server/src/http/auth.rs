//! StaticTokenAuth tower Layer/Service, BearerValidator trait, constant-time validation.
//!
//! Validates `Authorization: Bearer <hex>` headers using constant-time comparison
//! (ADR-001). Bypasses auth for `GET /health` (ADR-002). Defines the `BearerValidator`
//! trait for future extensibility (FR-14, W2-3 bridge to JWT/OAuth).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use subtle::ConstantTimeEq;
use tower::{Layer, Service};

use crate::infra::registry::{Capability, TrustLevel};
use crate::mcp::identity::ResolvedIdentity;

/// Credential type written to audit logs for HTTP bearer-token callers (ADR-006).
pub(crate) const CREDENTIAL_TYPE_STATIC_TOKEN: &str = "static_token";

/// Path that bypasses authentication (ADR-002).
pub(crate) const HEALTH_PATH: &str = "/health";

/// (path, method) pairs that bypass authentication entirely.
/// Exact match only -- no prefix matching (R-07).
const AUTH_BYPASS_PATHS: &[(&str, &Method)] = &[(HEALTH_PATH, &Method::GET)];

/// Auth-specific error type.
#[derive(Debug)]
pub(crate) enum AuthError {
    /// No Authorization header present.
    MissingHeader,
    /// Header present but not in `Bearer <hex>` format.
    InvalidFormat,
    /// Constant-time comparison failed (wrong token, bad hex, wrong length).
    InvalidToken,
    /// Unexpected internal failure.
    Internal(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingHeader => write!(f, "missing authorization header"),
            AuthError::InvalidFormat => write!(f, "invalid authorization format"),
            AuthError::InvalidToken => write!(f, "invalid token"),
            AuthError::Internal(msg) => write!(f, "internal auth error: {msg}"),
        }
    }
}

/// Abstraction for bearer token validation (FR-14).
///
/// Enables future extension to JWT/OAuth validators (W2-3) without
/// changing the middleware.
pub(crate) trait BearerValidator: Send + Sync + 'static {
    /// Validate a bearer token string and return a `ResolvedIdentity` on success.
    fn validate(
        &self,
        token: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedIdentity, AuthError>> + Send + '_>>;
}

/// Concrete `BearerValidator` for static 256-bit tokens.
///
/// Compares presented tokens using `subtle::ConstantTimeEq` (ADR-001).
#[derive(Debug)]
pub(crate) struct StaticTokenValidator {
    /// Raw token bytes (32 bytes) for constant-time comparison.
    token_bytes: [u8; 32],
}

impl StaticTokenValidator {
    /// Create a new validator from raw 32-byte token.
    pub(crate) fn new(token_bytes: [u8; 32]) -> Self {
        Self { token_bytes }
    }
}

impl BearerValidator for StaticTokenValidator {
    fn validate(
        &self,
        token: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedIdentity, AuthError>> + Send + '_>> {
        // Capture result synchronously -- no actual async work needed for static tokens.
        let result = self.validate_sync(token);
        Box::pin(async move { result })
    }
}

impl StaticTokenValidator {
    /// Synchronous validation logic. Separated for clarity and testability.
    fn validate_sync(&self, token: &str) -> Result<ResolvedIdentity, AuthError> {
        // Step 1: Hex-decode the presented token.
        // Early return on non-hex is acceptable -- reveals nothing about stored token (ADR-001).
        let presented_bytes = hex::decode(token).map_err(|_| AuthError::InvalidToken)?;

        // Step 2: Length check. Our token is always 32 bytes (public knowledge).
        // Length mismatch is an immediate reject without timing leak.
        if presented_bytes.len() != 32 {
            return Err(AuthError::InvalidToken);
        }

        // Step 3: Constant-time comparison (ADR-001).
        // subtle::ConstantTimeEq compares all 32 bytes regardless of content.
        // NO early returns between hex-decode and this comparison.
        let is_valid: bool = self.token_bytes.ct_eq(&presented_bytes).into();

        if !is_valid {
            return Err(AuthError::InvalidToken);
        }

        // Step 4: Construct ResolvedIdentity for HTTP bearer callers.
        Ok(ResolvedIdentity {
            agent_id: "http-bearer".to_string(),
            trust_level: TrustLevel::Restricted,
            capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
        })
    }
}

/// Tower `Layer` that wraps any inner service with bearer token authentication.
#[derive(Debug, Clone)]
pub(crate) struct StaticTokenAuthLayer {
    validator: Arc<dyn BearerValidator>,
}

impl StaticTokenAuthLayer {
    /// Create a new auth layer from raw 32-byte token.
    pub(crate) fn new(token_bytes: [u8; 32]) -> Self {
        Self {
            validator: Arc::new(StaticTokenValidator::new(token_bytes)),
        }
    }
}

impl<S> Layer<S> for StaticTokenAuthLayer {
    type Service = StaticTokenAuth<S>;

    fn layer(&self, inner: S) -> StaticTokenAuth<S> {
        StaticTokenAuth {
            inner,
            validator: Arc::clone(&self.validator),
        }
    }
}

/// Tower `Service` that validates bearer tokens before forwarding requests.
///
/// Generic over `S` (inner service) and `ReqBody` (request body type).
/// Produces `Response<BoxBody<Bytes, Infallible>>` to match rmcp's
/// `StreamableHttpService` response type.
#[derive(Debug, Clone)]
pub(crate) struct StaticTokenAuth<S> {
    inner: S,
    validator: Arc<dyn BearerValidator>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for StaticTokenAuth<S>
where
    S: Service<
            Request<ReqBody>,
            Response = Response<BoxBody<Bytes, Infallible>>,
            Error = Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send,
    ReqBody: Send + 'static,
{
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        // --- AUTH BYPASS CHECK (ADR-002) ---
        // Exact path match + method check. NEVER use starts_with (R-07).
        let path = request.uri().path();
        let method = request.method();

        let is_bypassed = AUTH_BYPASS_PATHS
            .iter()
            .any(|(p, m)| path == *p && method == *m);

        if is_bypassed {
            let future = self.inner.call(request);
            return Box::pin(future);
        }

        // --- EXTRACT AUTHORIZATION HEADER ---
        let auth_header = match request.headers().get(http::header::AUTHORIZATION) {
            Some(value) => value.clone(),
            None => {
                // Early return: missing header. Does not leak token info (ADR-001).
                return Box::pin(async { Ok(unauthorized_response()) });
            }
        };

        // Parse header value to string.
        let auth_str = match auth_header.to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => {
                // Non-ASCII header value.
                return Box::pin(async { Ok(unauthorized_response()) });
            }
        };

        // --- EXTRACT BEARER PREFIX ---
        // Early return on wrong prefix is acceptable (ADR-001).
        let token = match auth_str.strip_prefix("Bearer ") {
            Some(t) => t.to_owned(),
            None => {
                return Box::pin(async { Ok(unauthorized_response()) });
            }
        };

        // --- VALIDATE TOKEN ---
        let validator = Arc::clone(&self.validator);
        // Clone inner service per tower convention (call consumes readiness).
        let mut inner = self.inner.clone();
        // Swap so self retains the clone and the moved `inner` is the ready one.
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            match validator.validate(&token).await {
                Ok(identity) => {
                    // Insert ResolvedIdentity into request extensions.
                    // Consumed by rmcp -> build_context_with_external_identity.
                    let mut request = request;
                    request.extensions_mut().insert(identity);
                    inner.call(request).await
                }
                Err(_) => {
                    // ALL auth failures produce identical response (FR-10, FR-11).
                    Ok(unauthorized_response())
                }
            }
        })
    }
}

/// Identical 401 response for all rejection paths (R-02 mitigation).
///
/// Returns JSON body with generic error message to prevent information leakage
/// about which stage of validation failed.
fn unauthorized_response() -> Response<BoxBody<Bytes, Infallible>> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("content-type", "application/json")
        .body(
            Full::new(Bytes::from(
                r#"{"error":"missing or invalid authorization"}"#,
            ))
            .map_err(|never| match never {})
            .boxed(),
        )
        .expect("static response builder cannot fail")
}

// Make `Arc<dyn BearerValidator>` debuggable for `#[derive(Debug)]` on
// `StaticTokenAuthLayer` and `StaticTokenAuth`.
impl std::fmt::Debug for dyn BearerValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("dyn BearerValidator")
    }
}

#[cfg(test)]
mod tests;
