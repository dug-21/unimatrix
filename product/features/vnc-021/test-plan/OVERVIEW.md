# vnc-021 Test Strategy Overview

## Test Layers

| Layer | Scope | Tool |
|-------|-------|------|
| Unit | Per-function logic: config parsing, token I/O, TLS construction, auth bypass matching, health response | `cargo test` in `unimatrix-server` |
| Integration (Rust) | Cross-component chains: auth -> router -> identity injection -> audit, lifecycle shutdown, semaphore recovery | `cargo test` integration tests in `tests/` |
| Integration (infra-001) | Binary-level MCP protocol over stdio — regression gate | `pytest` suites via infra-001 harness |

## Risk-to-Test Mapping

| Risk | Priority | Component Plan | Test IDs |
|------|----------|---------------|----------|
| R-01 (rmcp extension propagation) | Critical | lifecycle-integration | T-LI-01, T-LI-02, T-LI-03 |
| R-02 (timing side-channel) | High | static-token-auth | T-STA-05, T-STA-06, T-STA-07 |
| R-03 (build_context first activation) | Critical | lifecycle-integration | T-LI-04, T-LI-05, T-LI-06, T-LI-07 |
| R-04 (connection flood / starvation) | Critical | http-listener | T-HL-05, T-HL-06, T-HL-07 |
| R-05 (token file permissions) | High | token-manager | T-TM-01, T-TM-02, T-TM-03, T-TM-04 |
| R-06 (TLS startup validation) | Medium | tls-config | T-TLS-01, T-TLS-02, T-TLS-03, T-TLS-04 |
| R-07 (health bypass too broad) | Critical | health-handler, path-router | T-HH-03, T-HH-04, T-HH-05, T-HH-06, T-HH-07 |
| R-08 (shutdown orphans sessions) | High | lifecycle-integration | T-LI-08, T-LI-09, T-LI-10 |
| R-09 (semaphore permit leak) | High | http-listener | T-HL-08, T-HL-09, T-HL-10 |
| R-10 (credential_type audit) | High | lifecycle-integration | T-LI-04, T-LI-05 |
| R-11 (body size limit) | High | path-router | T-PR-06, T-PR-07, T-PR-08 |
| R-12 (/observe auth ordering) | Low | path-router | T-PR-04, T-PR-05 |
| R-13 (ProjectRouter seam divergence) | Medium | path-router | T-PR-09, T-PR-10, T-PR-11 |
| R-14 (config defaults) | Medium | config-extensions | T-CE-01, T-CE-02, T-CE-03, T-CE-04 |
| R-15 (token file format) | Medium | token-manager | T-TM-05, T-TM-06, T-TM-07, T-TM-08 |
| R-16 (HTTP in stdio mode) | Low | lifecycle-integration | T-LI-11 |
| R-17 (HttpBearer rate limit) | Medium | lifecycle-integration | T-LI-12 |
| R-18 (connection timeout) | High | http-listener | T-HL-11, T-HL-12, T-HL-13 |

## Cross-Component Test Dependencies

1. **Auth -> Router -> Identity chain** (R-01, R-03, R-10): StaticTokenAuth inserts ResolvedIdentity into extensions; PathRouter forwards to rmcp; rmcp calls build_context_with_external_identity; audit records credential_type. This full chain must be tested as an integration test, not per-component.

2. **Auth bypass -> Router path matching** (R-07): StaticTokenAuth exempts `/health` by exact path + GET method; PathRouter independently routes `/health` to health_handler. Both must agree on exact-match semantics.

3. **Listener -> Semaphore -> TLS -> Auth** (R-04, R-09, R-18): Connection limiting happens pre-TLS. Semaphore permits must be RAII-guarded across TLS handshake, auth, and session lifetime. Permit recovery requires testing failure at each stage.

4. **Config -> TLS -> Listener** (R-06, R-14): TlsConfig defaults feed into build_tls_acceptor which feeds into start_http_listener. Invalid config must block startup.

## Existing Test Infrastructure

Extend, do not replace:
- `test_support.rs` / `TestHarness` — wraps ServiceLayer for Rust unit/integration tests
- `make_server()` — constructs a test UnimatrixServer (for MCP-level tests)
- infra-001 harness — binary-level MCP protocol tests over stdio

New HTTP-specific test helpers needed:
- `make_http_test_config()` — returns HttpConfig + TlsConfig with port 0 and TLS disabled
- `spawn_http_listener()` — starts listener on random port, returns (addr, shutdown_token)
- Self-signed cert/key fixture generation for TLS tests (via `rcgen` in dev-dependencies or pre-generated PEM files in test fixtures)

## Integration Harness Plan (infra-001)

### Existing Suites Applicable

| Suite | Relevance to vnc-021 | Action |
|-------|----------------------|--------|
| `protocol` | MCP handshake compliance — HTTP transport must produce identical protocol behavior | Run as regression gate |
| `tools` | All 12 tools — HTTP transport must not alter tool behavior | Run as regression gate |
| `lifecycle` | Store/search flows, restart persistence — HTTP sessions must not break these | Run as regression gate |
| `security` | Capability enforcement, input validation — HTTP callers must face same enforcement | Run as regression gate |
| `smoke` | Mandatory minimum gate per protocol | Run first |

### Suite Selection for vnc-021

Per the suite selection table: vnc-021 touches server tool logic, store/retrieval (via HTTP transport), security (auth middleware), and schema (config changes). Required suites:
- `smoke` (mandatory gate)
- `tools` (tool dispatch via HTTP must match stdio)
- `protocol` (MCP handshake compliance)
- `lifecycle` (multi-step flows unaffected)
- `security` (capability enforcement for HTTP callers)

### Gaps in Existing Suites

The infra-001 harness tests over **stdio**, not HTTP. It validates that the MCP protocol and tool dispatch work correctly through the stdio transport. vnc-021 adds HTTP transport, which:
1. Uses a different auth path (bearer token vs. none)
2. Introduces a new identity injection path (build_context_with_external_identity)
3. Adds new HTTP-only endpoints (/health, /observe)

These behaviors are **not testable** through the stdio harness. They require:
- Rust integration tests that start an HTTP listener and send HTTP requests
- Or a new infra-001 suite that connects over HTTP instead of stdio

### New Integration Tests Needed

**No new infra-001 suite tests needed for vnc-021.** Rationale:
- The infra-001 harness connects over stdio. HTTP transport is a parallel entry point to the same UnimatrixServer. Running existing suites over stdio validates that the shared server logic is unbroken.
- HTTP-specific behavior (auth, routing, TLS, health endpoint) is best tested as Rust integration tests because they need to construct HTTP requests, inspect HTTP responses, and manage TLS contexts — capabilities the Python harness does not have for HTTP.
- Adding HTTP transport support to infra-001 (new conftest fixtures, HTTP client, TLS handling) would be significant harness infrastructure work better suited to a dedicated infra issue.

**Rust integration tests cover the HTTP-specific gap:**
- `tests/http_integration.rs` — full HTTP request/response tests covering auth, routing, health, body limits, connection limits, shutdown
- Unit tests in `src/http/*.rs` — per-function correctness

### Regression Gate

Run all existing infra-001 suites to prove zero regression on stdio/UDS paths. Any failure triaged per USAGE-PROTOCOL.md: if caused by vnc-021 changes, fix; if pre-existing, xfail with GH Issue.
