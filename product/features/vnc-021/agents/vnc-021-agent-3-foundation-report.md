# Agent Report: vnc-021-agent-3-foundation

## Task
Implement foundation layer for vnc-021 HTTP transport: scaffolding, config extensions (C7), token manager (C4), and CallerId variant.

## Files Modified
- `Cargo.lock` -- updated with new dependencies
- `crates/unimatrix-server/Cargo.toml` -- added tokio-rustls, rustls-pemfile, subtle, tower, hyper, hyper-util, hex; enabled rmcp HTTP features
- `crates/unimatrix-server/src/infra/config.rs` -- added HttpConfig, TlsConfig structs with serde defaults, is_enabled() auto-detect, validate_http_config(), validate_tls_config(), merge support, ConfigError variants, 25 unit tests
- `crates/unimatrix-server/src/lib.rs` -- added `pub mod http;`
- `crates/unimatrix-server/src/services/mod.rs` -- added `CallerId::HttpBearer(String)` variant

## Files Created
- `crates/unimatrix-server/src/http/mod.rs` -- module declarations for all 6 submodules
- `crates/unimatrix-server/src/http/token.rs` -- load_or_generate_token with 0600 perms, hex validation, 13 unit tests
- `crates/unimatrix-server/src/http/auth.rs` -- placeholder (C2, later wave)
- `crates/unimatrix-server/src/http/tls.rs` -- placeholder (C5, later wave)
- `crates/unimatrix-server/src/http/health.rs` -- placeholder (C6, later wave)
- `crates/unimatrix-server/src/http/router.rs` -- placeholder (C3, later wave)
- `crates/unimatrix-server/src/http/listener.rs` -- placeholder (C1, later wave)

## Tests
- **Token manager**: 13 passed, 0 failed (T-TM-01 through T-TM-11 + trailing newline + idempotent)
- **Config extensions**: 25 passed, 0 failed (T-CE-01 through T-CE-12 + validation + merge + is_enabled)
- **Workspace**: 5156+ passed, 0 failed (zero regressions)

## Design Decisions
- `ServerError::ProjectInit` reused for token errors (consistent with existing file I/O error patterns)
- `ConfigError::HttpFieldInvalid` and `ConfigError::TlsFieldInvalid` added for validation errors (consistent with existing pattern)
- HTTP/TLS merge uses section-level replace semantics (not field-by-field within HttpConfig)
- Token file trailing newline tolerated via `trim_end()` per R-15 mitigation
- Uppercase hex accepted in token files (hex::decode handles case-insensitive)
- rand 0.9 API: `rand::fill()` instead of `OsRng.fill_bytes()` (0.9 breaking change)

## Issues
None. All components implemented per pseudocode. No blockers.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found ADR entries #4665-4670, used config testing pattern #3928 (parse via UnimatrixConfig with [section] header), dual-site default pattern #3817
- Stored: nothing novel to store -- implementation followed established patterns; rand 0.9 API change is well-documented upstream
