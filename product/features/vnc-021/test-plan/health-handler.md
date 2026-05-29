# Test Plan: Health Handler (`src/http/health.rs`)

Covers: C6 — Unauthenticated HTTP health endpoint
Risks: R-07 (health bypass too broad)

## Unit Tests

Tests target the health handler function directly.

### T-HH-01: test_health_returns_200_with_json_body
- **Arrange**: None (handler is stateless aside from version constants).
- **Act**: Call health handler with a GET request to `/health`.
- **Assert**: Response status 200. Content-Type is `application/json`. Body parses as JSON with keys `"version"` (string, matches crate version) and `"schema_version"` (integer, matches current DB migration version).

### T-HH-02: test_health_version_matches_crate_version
- **Arrange**: None.
- **Act**: Call health handler.
- **Assert**: `response.body["version"] == env!("CARGO_PKG_VERSION")`.

## Auth Bypass Tests (R-07 — Critical)

These tests exercise the StaticTokenAuth bypass logic for the health endpoint. They are the primary defense against R-07.

### T-HH-03: test_health_bypass_exact_path_get
- **Risk**: R-07
- **Arrange**: StaticTokenAuth wrapping PathRouter. No Authorization header.
- **Act**: Send `GET /health`.
- **Assert**: Response status 200 (auth bypassed, health handler reached).

### T-HH-04: test_health_bypass_rejects_trailing_slash
- **Risk**: R-07
- **Arrange**: StaticTokenAuth wrapping PathRouter. No Authorization header.
- **Act**: Send `GET /health/`.
- **Assert**: Response status 401 (NOT bypassed — trailing slash is a different path).

### T-HH-05: test_health_bypass_rejects_prefix_match
- **Risk**: R-07
- **Arrange**: No Authorization header.
- **Act**: Send `GET /healthz`.
- **Assert**: Response status 401 (NOT bypassed — prefix match must not work).

### T-HH-06: test_health_bypass_rejects_subpath
- **Risk**: R-07
- **Arrange**: No Authorization header.
- **Act**: Send `GET /health/debug`.
- **Assert**: Response status 401 (NOT bypassed).

### T-HH-07: test_health_bypass_rejects_post_method
- **Risk**: R-07
- **Arrange**: No Authorization header.
- **Act**: Send `POST /health`.
- **Assert**: Response status 401 or 405 (only GET is bypassed per ADR-002).

### T-HH-08: test_health_with_query_params
- **Risk**: R-07
- **Arrange**: No Authorization header.
- **Act**: Send `GET /health?param=value`.
- **Assert**: Behavior is defined — either 200 (query params ignored, path matches) or 401 (strict match including no query). Document the chosen behavior.

### T-HH-09: test_health_with_valid_auth_also_works
- **Arrange**: StaticTokenAuth with valid token. Request has valid Authorization header.
- **Act**: Send `GET /health` with valid auth.
- **Assert**: Response status 200. Health endpoint works regardless of auth presence.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-13 | T-HH-01, T-HH-02, T-HH-03 |
