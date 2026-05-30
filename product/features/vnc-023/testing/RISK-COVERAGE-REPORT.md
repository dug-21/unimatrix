# Risk Coverage Report: vnc-023

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Extension propagation regression — ResolvedIdentity silently lost | security suite (20 tests: capability enforcement), tools suite (185 tests: all tool invocations), smoke tests | PASS | Full |
| R-02 | ServerHandler::initialize trait signature incompatibility | compile gate, `test_srv_u02_initialize_inserts_name_under_stdio_key`, `test_srv_u05_initialize_truncates_at_256_chars`, protocol `test_initialize_returns_capabilities`, `test_server_info` | PASS | Full |
| R-03 | #[non_exhaustive] struct literal migration logic error | `test_get_info_name`, `test_get_info_version_matches_cargo_pkg`, `test_get_info_has_tools_capability`, `test_get_info_instructions`, `test_get_info_custom_instructions`, protocol `test_server_info` | PASS | Full |
| R-04 | allowed_origins config wiring disconnected (4-hop chain) | `test_http_config_toml_with_allowed_origins`, `test_streamable_config_allowed_origins_field_assignment`, `test_setting_allowed_origins_preserves_allowed_hosts`, main.rs inspection (config.http.allowed_origins.clone() passed to ProjectRouter::new) | PASS | Full |
| R-05 | CVE-2026-42559 not fully resolved | Cargo.toml: `version = "=1.7.0"`, Cargo.lock: `rmcp 1.7.0`, `test_streamable_config_default_allowed_hosts_non_empty`, code review: no allowed_hosts override in McpAdapter | PASS | Full |
| R-06 | Behavioral default regression (keep_alive, init_timeout) | lifecycle suite (60 passed, no timeout failures), existing session tests pass | PASS | Full |
| R-07 | UDS IntoTransport blanket impl failure | compile gate (`cargo build --release` succeeds), no explicit `transport-async-rw` in Cargo.toml | PASS | Full |
| R-08 | serve_client test helper renamed or moved | compile gate (`cargo test -p unimatrix-server --lib` compiles), all 3470 unit tests compile | PASS | Full |
| R-09 | Backward-incompatible config deserialization | `test_http_config_default_has_empty_allowed_origins`, `test_http_config_toml_without_allowed_origins_succeeds`, `test_http_config_full_toml_without_allowed_origins`, `test_http_config_toml_with_empty_allowed_origins` | PASS | Full |
| R-10 | http crate version mismatch (TypeId footgun) | `cargo tree -i http` shows single version `http v1.4.0`, R-01 integration tests pass (extension extraction works) | PASS | Full |
| R-11 | ErrorData::invalid_params signature changed | compile gate, `git diff tools.rs` = 0 changes, 8 call sites unchanged | PASS | Full |
| R-12 | Description string not returned in initialize response | `test_get_info_returns_description` asserts `"Self-learning knowledge engine for agentic workflows"` | PASS | Full |
| R-13 | allowed_origins vs allowed_hosts interaction confusion | `test_setting_allowed_origins_preserves_allowed_hosts`, code review: McpAdapter sets only `config.allowed_origins`, does not touch `allowed_hosts` | PASS | Full |

## Test Results

### Unit Tests
- Total: 3471
- Passed: 3470
- Failed: 1 (pre-existing: `test_schema_integer_type_preserved_for_all_nine_fields` — unrelated to vnc-023, schemars nullable integer type mismatch)

### Integration Tests

| Suite | Total | Passed | Failed | XFail | XPass |
|-------|-------|--------|--------|-------|-------|
| smoke | 23 | 23 | 0 | 0 | 0 |
| protocol | 13 | 13 | 0 | 0 | 0 |
| tools | 188 | 185 | 0 | 3 | 0 |
| security | 20 | 20 | 0 | 0 | 0 |
| lifecycle | 67 | 60 | 0 | 5 | 2 |
| **Total** | **311** | **301** | **0** | **8** | **2** |

All xfail markers are pre-existing and have corresponding GH Issues. No new xfail markers added.

### Integration Test Fix

**test_malformed_json_handled** (protocol suite): Updated to reflect rmcp 1.7 behavior change. rmcp 1.7 returns a JSON-RPC -32700 (Parse Error) response instead of closing the connection on malformed input (documented in Implementation Brief as "Stdio parse resilience" automatic improvement). Test now verifies the server stays alive and continues to accept valid requests after receiving malformed JSON. This is a test assertion correction (triage category 3), not a code fix.

## Verification Items

| Item | Method | Result |
|------|--------|--------|
| V-01: Cargo.toml rmcp version | grep | `version = "=1.7.0"` confirmed |
| V-02: All 6 feature flags | inspection | `server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session` present |
| V-03: Cargo.lock resolves 1.7.0 | grep | `name = "rmcp"` / `version = "1.7.0"` confirmed |
| V-04: Single http crate version | cargo tree | `http v1.4.0` (single version) |
| V-05: Workspace compiles | cargo build --release | exit 0 |
| V-06: No explicit transport-async-rw | grep | 0 matches in Cargo.toml |
| V-07: tools.rs unchanged | git diff | 0 lines changed, 8 ErrorData::invalid_params call sites intact |
| V-08: Description string present | grep | `"Self-learning knowledge engine for agentic workflows"` in server.rs |
| C-01: No struct literals | grep | 0 matches for `Implementation {`, `ServerInfo {`, `ClientInfo {` |

## Gaps

None. All 13 risks from RISK-TEST-STRATEGY.md have full test coverage.

## Edge Case: keep_alive During Long Tool Execution

Per Implementation Brief PR Note 1: rmcp 1.7 defaults session `keep_alive` to 5 minutes. If a tool call takes >5 minutes, the session may be cleaned up mid-execution. This edge case is documented but not directly testable without significant harness infrastructure (mock slow tool). Lifecycle suite tests pass without timeout failures, confirming normal operation is unaffected. The risk is limited to tool calls exceeding 5 minutes (uncommon in practice).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | Cargo.toml has `version = "=1.7.0"`, `cargo build --release` exits 0, all 6 feature flags present |
| AC-02 | PASS | 0 matches for `Implementation {` or `ServerInfo {` in production code; `test_get_info_*` tests (7) pass |
| AC-03 | PASS | 0 matches for `ClientInfo {` in test module; `cargo test -p unimatrix-server --lib` compiles; 3470 tests pass |
| AC-04 | PASS | `git diff tools.rs` = 0 lines; 8 `ErrorData::invalid_params` call sites unchanged |
| AC-05 | PASS | `cargo build --release` exits 0; lifecycle suite (60 passed) shows no session timeout failures |
| AC-06 | PASS | `cargo build --release` exits 0; no `transport-async-rw` in Cargo.toml |
| AC-07 | PASS | Security suite (20 passed): capability enforcement tests validate ResolvedIdentity propagation; tools suite (185 passed) exercises full HTTP->rmcp->tool chain; single `http v1.4.0` version |
| AC-08 | PASS | `test_get_info_returns_description` asserts `"Self-learning knowledge engine for agentic workflows"`; grep confirms string in server.rs |
| AC-09 | PASS | 8 allowed_origins tests pass: config deserialization (5 tests), router wiring (3 tests); main.rs passes `config.http.allowed_origins.clone()` |
| AC-10 | PARTIAL | `cargo clippy -p unimatrix-server --no-deps -- -D warnings` shows pre-existing warnings in other files (not in vnc-023 modified files); `cargo clippy --workspace` blocked by pre-existing unimatrix-observe/unimatrix-engine warnings |
| AC-11 | PASS | `cargo test --workspace --lib`: 3470 passed, 1 failed (pre-existing, unrelated) |
| AC-12 | PASS | `test_srv_u02_initialize_inserts_name_under_stdio_key` + 9 other server tests pass; initialize signature compiles; client_type_map population logic unchanged |
