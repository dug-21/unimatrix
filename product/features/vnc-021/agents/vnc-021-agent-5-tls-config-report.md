# Agent Report: vnc-021-agent-5-tls-config

## Task
Implement TLS configuration (C5) -- `crates/unimatrix-server/src/http/tls.rs`

## Files Modified
- `crates/unimatrix-server/src/http/tls.rs` -- full implementation replacing placeholder
- `crates/unimatrix-server/src/error.rs` -- added `ServerError::Config(String)` variant
- `crates/unimatrix-server/Cargo.toml` -- added `rcgen = "0.13"` dev-dependency
- `Cargo.lock` -- updated for rcgen

## Implementation Summary

`build_tls_acceptor(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>`:
- Returns `Ok(None)` when `config.is_enabled()` is false (proxy-terminated mode)
- Returns `Ok(Some(TlsAcceptor))` when enabled with valid cert+key PEM files
- Returns `Err(ServerError::Config(...))` with descriptive messages for all failure modes
- Uses `rustls::crypto::ring::default_provider()` via `builder_with_provider()` to avoid process-level CryptoProvider installation requirement
- Two helper functions: `load_certs()` and `load_private_key()` wrapping `rustls_pemfile`

Added `ServerError::Config(String)` variant with Display, ErrorData mapping (ERROR_INTERNAL).

## Tests: 12 passed, 0 failed

| Test | Status |
|------|--------|
| T-TLS-01: test_valid_cert_and_key_returns_tls_acceptor | PASS |
| T-TLS-02: test_missing_cert_path_returns_error | PASS |
| T-TLS-03: test_missing_key_path_returns_error | PASS |
| T-TLS-04: test_invalid_pem_cert_returns_error | PASS |
| T-TLS-05: test_invalid_pem_key_returns_error | PASS |
| T-TLS-06: test_mismatched_cert_key_returns_error | PASS |
| T-TLS-07: test_nonexistent_cert_file_returns_error | PASS |
| T-TLS-08: test_tls_disabled_returns_none | PASS |
| T-TLS-08v: test_default_config_returns_none | PASS |
| T-TLS-09: test_cert_with_ip_san | PASS |
| Extra: test_nonexistent_key_file_returns_error | PASS |
| Extra: test_empty_cert_file_returns_error | PASS |

## Issues
- None. No blockers.

## Deviations from Pseudocode
- Pseudocode used `ServerConfig::builder()` which requires a process-level CryptoProvider. Changed to `ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))` to explicitly select the ring crypto provider without global state. This is functionally equivalent but avoids panics when no default provider is installed.
- `TlsAcceptor` does not implement `Debug`, so test assertions use a custom `expect_err()` helper instead of `Result::unwrap_err()`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-005 (#4669) confirming rustls 0.23 + tokio-rustls 0.26, configurable bypass, no hot-reload. No TLS-specific implementation patterns found.
- Stored: nothing novel to store -- the ring provider requirement is documented in rustls 0.23 release notes and will be obvious to any developer reading the rustls error message. The TlsAcceptor non-Debug issue is a minor test ergonomic that doesn't warrant a stored pattern.
