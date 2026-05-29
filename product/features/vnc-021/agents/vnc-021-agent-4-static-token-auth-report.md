# Agent Report: vnc-021-agent-4-static-token-auth

## Status: COMPLETE

## Files Modified
- `crates/unimatrix-server/src/http/auth.rs` -- full implementation (278 lines)
- `crates/unimatrix-server/src/http/auth/tests.rs` -- 19 unit tests (392 lines)

## Implementation Summary

Replaced placeholder stub with complete StaticTokenAuth tower middleware:

- `BearerValidator` trait with async `validate()` method (FR-14, W2-3 extensibility bridge)
- `StaticTokenValidator` -- concrete implementation using `subtle::ConstantTimeEq` (ADR-001)
- `StaticTokenAuthLayer` -- tower `Layer` factory
- `StaticTokenAuth<S>` -- tower `Service` wrapping inner service, generic over request body
- `unauthorized_response()` -- identical 401 JSON for all rejection paths (R-02 mitigation)
- Health bypass: exact `GET /health` match only, no prefix matching (ADR-002, R-07)
- Constants: `CREDENTIAL_TYPE_STATIC_TOKEN = "static_token"`, `HEALTH_PATH = "/health"`

### Design Deviation from Pseudocode
- Pseudocode specifies `TrustLevel::Standard` but codebase has no `Standard` variant. Used `TrustLevel::Restricted` which matches existing behavior for auto-enrolled agents with `[Read, Write, Search]` capabilities. This is correct -- HTTP bearer callers are non-privileged external callers.

## Tests: 19 passed, 0 failed

| Test ID | Test Name | Status |
|---------|-----------|--------|
| T-STA-01 | test_valid_bearer_token_passes_to_inner_service | PASS |
| T-STA-02 | test_missing_authorization_header_returns_401 | PASS |
| T-STA-03 | test_wrong_token_returns_401 | PASS |
| T-STA-04 | test_non_bearer_prefix_returns_401 | PASS |
| T-STA-05 | test_malformed_hex_token_returns_401 | PASS |
| T-STA-06 | test_short_hex_token_returns_401 | PASS |
| T-STA-07 | test_response_body_identical_for_all_rejection_paths | PASS |
| T-STA-08 | test_valid_token_inserts_resolved_identity_into_extensions | PASS |
| T-STA-09 | test_credential_type_constant_is_static_token | PASS |
| T-STA-10 | test_bearer_validator_trait_valid_token | PASS |
| T-STA-11 | test_bearer_validator_trait_invalid_token | PASS |
| T-STA-12 | test_empty_authorization_header_returns_401 | PASS |
| T-STA-13 | test_bearer_with_extra_spaces_returns_401 | PASS |
| T-STA-14 | test_very_long_authorization_header_returns_401 | PASS |
| -- | test_health_get_bypass_no_auth_needed | PASS |
| -- | test_health_post_no_bypass | PASS |
| -- | test_healthz_no_bypass | PASS |
| -- | test_health_trailing_slash_no_bypass | PASS |
| -- | test_health_path_constant | PASS |

## Code Review Checkpoints (R-02)
- CR-01: `subtle::ConstantTimeEq::ct_eq` used at line 112, not `==` or `PartialEq`
- CR-02: No early returns between hex-decode (line 101) and `ct_eq` call (line 112)
- CR-03: Hex-decode failure returns `AuthError::InvalidToken` without leaking stored token info
- CR-04: Exactly one `ct_eq` call site in auth.rs

## Issues
- 3 pre-existing test failures in `http::tls` module (CryptoProvider not installed) -- not caused by this component.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-001 (constant-time), ADR-002 (health bypass), ADR-006 (credential_type). All applied.
- Stored: nothing novel to store -- implementation followed established patterns from pseudocode and ADRs without discovering new gotchas.
