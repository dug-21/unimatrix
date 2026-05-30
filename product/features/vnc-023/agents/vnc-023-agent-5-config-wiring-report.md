# Agent Report: vnc-023-agent-5-config-wiring

## Status: COMPLETE

## Files Modified
- `crates/unimatrix-server/src/infra/config.rs` -- added `allowed_origins: Vec<String>` to HttpConfig + Default impl + 5 tests
- `crates/unimatrix-server/src/http/router.rs` -- wired `allowed_origins` through `ProjectRouter::new()` and `McpAdapter::new()` to `StreamableHttpServerConfig`
- `crates/unimatrix-server/src/http/router/tests.rs` -- added 4 wiring verification tests
- `crates/unimatrix-server/src/main.rs` -- passed `config.http.allowed_origins.clone()` to `ProjectRouter::new()`
- `crates/unimatrix-server/src/http/listener/tests.rs` -- fixed 2 HttpConfig struct literals to include `allowed_origins` field

## Tests: 10 pass / 0 fail

Config tests (5):
- T-AO-01: Default HttpConfig has empty allowed_origins
- T-AO-02: TOML without allowed_origins deserializes successfully
- T-AO-03: TOML with allowed_origins deserializes correctly
- T-AO-04: TOML with empty allowed_origins array
- T-AO-05: Full config without allowed_origins (regression guard)

Router wiring tests (4):
- T-RO-04: StreamableHttpServerConfig field assignment compiles and works
- T-RO-05: Default allowed_hosts is non-empty (CVE fix preserved)
- T-RO-06: Setting allowed_origins does not modify allowed_hosts
- T-RO-07: Default allowed_origins is empty (backward compat)

Existing test updated (1):
- T-CE-01: test_empty_config_http_defaults now asserts allowed_origins.is_empty()

## Build Status
- `cargo build --workspace` -- PASS (zero errors; warnings are pre-existing)
- `cargo test -p unimatrix-server -- <vnc-023 tests>` -- 10/10 PASS
- `cargo test -p unimatrix-server -- http::router::tests:: http::listener::tests::` -- 58/58 PASS (no regressions)
- `cargo test -p unimatrix-server -- infra::config::tests::` -- 411/411 PASS (no regressions)
- `cargo fmt` -- my files clean; server.rs has pre-existing formatting issues (other agent)

## Issues
None. All three components implemented per pseudocode without deviation.

## Implementation Notes
- `StreamableHttpServerConfig` is `#[non_exhaustive]` in rmcp 1.7.0 but has pub fields and `Default` impl. Used `let mut config = ::default(); config.allowed_origins = origins;` pattern (Strategy A from pseudocode).
- `allowed_hosts` defaults to `["localhost", "127.0.0.1", "::1"]` in rmcp 1.7 -- this IS the CVE-2026-42559 fix. Code explicitly does NOT touch this field.
- listener/tests.rs had 2 struct literal constructions of HttpConfig that needed the new field added.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 (#4701), rmcp migration scope pattern (#4699), InferenceConfig arc pattern (#4131). ADR-002 confirmed independent checks design.
- Stored: nothing novel to store -- the implementation was straightforward field addition and pass-through wiring with no gotchas beyond the expected `#[non_exhaustive]` handling documented in pseudocode.
