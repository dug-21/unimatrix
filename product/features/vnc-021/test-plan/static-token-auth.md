# Test Plan: Static Token Auth (`src/http/auth.rs`)

Covers: C2 — StaticTokenAuth tower Layer/Service, BearerValidator trait, constant-time validation
Risks: R-02 (timing side-channel), R-07 (health bypass), R-12 (/observe auth)

## Unit Tests

Tests construct a `StaticTokenAuth<S>` wrapping a mock inner service and send crafted requests.

### T-STA-01: test_valid_bearer_token_passes_to_inner_service
- **Arrange**: Create StaticTokenAuth with known 32-byte token. Build HTTP request with `Authorization: Bearer <hex-encoded-token>`.
- **Act**: Call the service with the request.
- **Assert**: Inner service is called. Response is the inner service's response (not 401).

### T-STA-02: test_missing_authorization_header_returns_401
- **Risk**: R-02
- **Arrange**: Create StaticTokenAuth. Build HTTP request with no Authorization header.
- **Act**: Call the service.
- **Assert**: Response status is 401. Body is JSON `{"error": "missing or invalid authorization"}`. Inner service is NOT called.

### T-STA-03: test_wrong_token_returns_401
- **Risk**: R-02
- **Arrange**: Create StaticTokenAuth with known token. Build request with `Authorization: Bearer <different-hex-token>`.
- **Act**: Call the service.
- **Assert**: Response status is 401. Body is JSON `{"error": "missing or invalid authorization"}`. Inner service is NOT called.

### T-STA-04: test_non_bearer_prefix_returns_401
- **Risk**: R-02
- **Arrange**: Build request with `Authorization: Basic <base64>`.
- **Act**: Call the service.
- **Assert**: Response status is 401.

### T-STA-05: test_malformed_hex_token_returns_401
- **Risk**: R-02
- **Arrange**: Build request with `Authorization: Bearer zzzzzzzz...` (64 non-hex chars).
- **Act**: Call the service.
- **Assert**: Response status is 401. No panic. Response body identical to wrong-token case.

### T-STA-06: test_short_hex_token_returns_401
- **Risk**: R-02
- **Arrange**: Build request with `Authorization: Bearer aabb` (too short).
- **Act**: Call the service.
- **Assert**: Response status is 401. Response body identical to other rejection cases.

### T-STA-07: test_response_body_identical_for_all_rejection_paths
- **Risk**: R-02
- **Arrange**: Create three requests: (a) no Authorization header, (b) wrong token, (c) malformed hex.
- **Act**: Call the service with each.
- **Assert**: All three responses have status 401 and identical JSON body. This prevents information leakage about which stage of validation failed.

### T-STA-08: test_valid_token_inserts_resolved_identity_into_extensions
- **Arrange**: Create StaticTokenAuth. Build request with valid token. Inner service captures request extensions.
- **Act**: Call the service.
- **Assert**: `ResolvedIdentity` is present in request extensions. `agent_id == "http-bearer"`. `trust_level == TrustLevel::Standard`. Capabilities include Read, Write, Search.

### T-STA-09: test_credential_type_constant_is_static_token
- **Risk**: R-10
- **Arrange**: None.
- **Act**: Read `CREDENTIAL_TYPE_STATIC_TOKEN` constant.
- **Assert**: Value is exactly `"static_token"`.

### T-STA-10: test_bearer_validator_trait_implementation
- **Arrange**: Create `StaticTokenAuth` instance.
- **Act**: Call `validate(&self, token)` with valid token via the `BearerValidator` trait.
- **Assert**: Returns `Ok(ResolvedIdentity)` with correct fields.

### T-STA-11: test_bearer_validator_trait_invalid_token
- **Arrange**: Create `StaticTokenAuth` instance.
- **Act**: Call `validate(&self, token)` with invalid token.
- **Assert**: Returns `Err(AuthError)`.

## Code Review Checkpoints (R-02)

These are not automated tests but mandatory code review verification points:

- **CR-01**: Verify `subtle::ConstantTimeEq::ct_eq` is used for the token byte comparison, not `==` or `PartialEq`.
- **CR-02**: Verify no early-return exists between hex-decoding the presented token and the `ct_eq` call. Only permitted early-returns: missing Authorization header, non-"Bearer " prefix.
- **CR-03**: Verify that hex-decode failure (non-hex chars) either uses constant-time comparison against a dummy value or returns in a path that reveals no information about the stored token.
- **CR-04**: Grep for `ct_eq` in `src/http/auth.rs` — exactly one call site expected.

## Required Edge-Case Tests

### T-STA-12: test_empty_authorization_header_returns_401
- **Arrange**: Build request with `Authorization: ` (empty value).
- **Act**: Call the service.
- **Assert**: Response status 401. No panic.

### T-STA-13: test_bearer_with_extra_spaces_returns_401
- **Arrange**: Build request with `Authorization:  Bearer  <token>` (extra spaces).
- **Act**: Call the service.
- **Assert**: Response status 401 (strict parsing, no space normalization).

### T-STA-14: test_very_long_authorization_header_returns_401
- **Arrange**: Build request with `Authorization: Bearer ` + 1MB of hex chars.
- **Act**: Call the service.
- **Assert**: Response status 401. No OOM. Processing completes quickly.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-04 | T-STA-02 |
| AC-05 | T-STA-03, T-STA-05, CR-01, CR-02, CR-03 |
| AC-06 | T-STA-01, T-STA-08 (unit-level; full chain in lifecycle-integration) |
| AC-13 | Health bypass tests are in health-handler.md |
