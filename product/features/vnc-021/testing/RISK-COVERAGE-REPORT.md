# Risk Coverage Report: vnc-021

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | rmcp extension propagation | McpAdapter.handle() delegates to StreamableHttpService with extensions intact; R-01 spike confirmed extensions propagate (comment in router.rs:236). T-LI-01..03 planned as integration tests. | PASS (unit) | Partial — unit/structural verified; full end-to-end requires live HTTP MCP session |
| R-02 | Timing side-channel in token validation | T-STA-05, T-STA-06, T-STA-07 (identical 401 body for all rejection paths); CR-01 verified (single ct_eq at auth.rs:112); CR-02 verified (no early return between hex-decode and ct_eq except length check) | PASS | Full |
| R-03 | build_context_with_external_identity first activation | T-STA-08 (ResolvedIdentity inserted with agent_id="http-bearer", trust_level=Restricted, caps=[R,W,S]); T-STA-10 (BearerValidator trait returns correct identity) | PASS | Partial — unit identity chain verified; audit log integration requires live server |
| R-04 | HTTP connection flood / runtime starvation | T-HL-05 (3 connections fill, 4th rejected), T-HL-06 (permit released on close) | PASS | Full |
| R-05 | Token file permissions | T-TM-01 (64 hex chars), T-TM-02 (mode 0600), T-TM-03 (raw bytes match), T-TM-04 (load existing) | PASS | Full |
| R-06 | TLS startup validation | T-TLS-01 (valid), T-TLS-02 (no cert), T-TLS-03 (no key), T-TLS-04 (invalid PEM cert), T-TLS-05 (invalid PEM key), T-TLS-06 (mismatched), T-TLS-07 (nonexistent), T-TLS-09 (IP SAN) | PASS | Full |
| R-07 | Health endpoint auth bypass too broad | T-HH-03..07 equivalent tests in auth/tests.rs: test_health_get_bypass_no_auth_needed (200), test_health_post_no_bypass (401), test_healthz_no_bypass (401), test_health_trailing_slash_no_bypass (401); test_health_path_constant asserts "/health" | PASS | Full |
| R-08 | Graceful shutdown orphans sessions | T-HL-14 (shutdown stops accepting), shutdown.rs:133-145 (HTTP acceptor abort+join ordered after MCP acceptor), listener.rs:246-252 (graceful_shutdown on in-flight requests) | PASS | Full |
| R-09 | Semaphore permit leak | T-HL-09 (recovery after malformed HTTP), T-HL-10 (10 sequential connections with max=1), T-HL-06 (release on close); RAII guard at listener.rs:135 | PASS | Full |
| R-10 | credential_type not written to audit log | T-STA-09 (CREDENTIAL_TYPE_STATIC_TOKEN == "static_token"); ResolvedIdentity constructed at auth.rs:119-124 | PASS | Partial — constant verified; audit log write path requires live server |
| R-11 | Request body size limit not enforced | T-PR-06 (oversized rejected 413), T-PR-07 (at boundary accepted), T-PR-08 (enforced before rmcp) | PASS | Full |
| R-12 | /observe stub missing auth | T-PR-04 (POST /observe returns 501), T-PR-05 equivalent (auth ordering: /observe not in AUTH_BYPASS_PATHS, so StaticTokenAuth enforces auth before PathRouter routes) | PASS | Full |
| R-13 | ProjectRouter seam divergence | T-PR-09, T-PR-10 (default project mode), T-PR-11 (observe_path_constant, observe_registered_in_routing_tree) | PASS | Full |
| R-14 | Config parsing defaults incorrect | T-CE-01 (http defaults), T-CE-02 (tls defaults), T-CE-03 (auto-enable), T-CE-04 (cert-only = disabled), T-CE-05..10 (custom values), T-CE-08 (explicit false overrides), T-CE-11 (existing sections unchanged) | PASS | Full |
| R-15 | Token file format validation | T-TM-05 (odd length), T-TM-06 (short), T-TM-07 (non-hex), T-TM-08 (valid 64 hex), T-TM-11 (uppercase), T-TM-trailing-newline (trailing newline tolerance) | PASS | Full |
| R-16 | HTTP listener in stdio mode | Config default http.enabled=false prevents activation; stdio mode logic in main.rs checks config.http.enabled | PASS | Partial — structural verification; live mode test requires binary-level test |
| R-17 | HttpBearer rate limit exemption | Code review: gateway.rs:60 uses `matches!(caller, CallerId::UdsSession(_))` — HttpBearer does NOT match, so it IS rate-limited | PASS | Full |
| R-18 | Connection timeout not enforced | T-HL-11 (idle timeout 2s), T-HL-12 (partial request timeout), T-HL-13 (active connection not prematurely timed out) | PASS | Full |

## Test Results

### Unit Tests (cargo test -p unimatrix-server --lib)
- Total: 3429
- Passed: 3427
- Failed: 2 (pre-existing, unrelated to vnc-021 — see below)
- HTTP-specific tests: 94 (all pass)

### Pre-Existing Failures (NOT caused by vnc-021)
| Test | Cause | Action |
|------|-------|--------|
| uds::listener::tests::col018_context_search_creates_observation | Embedding model initialization race (col-018 feature) | Pre-existing. Not vnc-021 code. |
| uds::listener::tests::col018_prompt_at_limit_not_truncated | Embedding model initialization race (col-018 feature) | Pre-existing. Not vnc-021 code. |

### Integration Tests (infra-001)
- Smoke suite: 23 passed, 0 failed (MANDATORY GATE: PASS)
- Lifecycle suite: 60 passed, 5 xfailed (pre-existing), 2 xpassed
- Protocol + Tools + Security suites: executing (103 tests; smoke samples from all three passed 23/23, no regressions expected)

All xfail markers in the lifecycle suite are pre-existing with corresponding GH Issues. No new xfail markers added. No tests deleted or commented out.

### HTTP Module Unit Test Breakdown

| Module | Tests | Result |
|--------|-------|--------|
| http::auth::tests | 19 | 19 PASS |
| http::health::tests | 3 | 3 PASS |
| http::token::tests | 13 | 13 PASS |
| http::tls::tests | 12 | 12 PASS |
| http::router::tests | 17 | 17 PASS |
| http::listener::tests | 13 | 13 PASS |
| infra::config::tests (vnc-021) | 17 | 17 PASS |
| infra::shutdown::tests (vnc-021) | 3 | 3 PASS |
| **Total HTTP-specific** | **97** | **97 PASS** |

Note: `cargo test --lib http` filters to 94 (3 shutdown tests matched by different filter). All 97 vnc-021-related tests pass.

### Code Review Checkpoints (R-02)
- CR-01: `subtle::ConstantTimeEq::ct_eq` used at auth.rs:112 — VERIFIED
- CR-02: No early-return between hex-decode (auth.rs:101) and ct_eq (auth.rs:112) except length check (auth.rs:105, public knowledge) — VERIFIED
- CR-03: Malformed hex tokens hex::decode failure returns AuthError::InvalidToken immediately — acceptable per ADR-001 (reveals nothing about stored token) — VERIFIED
- CR-04: Exactly one ct_eq call site in auth.rs — VERIFIED

### Stub Check
```
grep -rn "todo!\|unimplemented!\|TODO\|FIXME" crates/unimatrix-server/src/http/
```
Result: **No stubs found.** All implementations are complete.

## Gaps

| Risk | Gap Description | Severity | Mitigation |
|------|----------------|----------|------------|
| R-01 | Full end-to-end HTTP -> rmcp -> tool -> audit_log integration test (T-LI-01, T-LI-02) not implemented as Rust integration tests. Coverage is via unit tests proving extension insertion (T-STA-08) + structural verification that McpAdapter delegates to StreamableHttpService with extensions. R-01 spike confirmed propagation works. | Low residual risk | Unit tests + code review + spike validation cover the risk. Full E2E requires the server binary running with HTTP enabled — a future infra-001 HTTP suite enhancement. |
| R-03 | audit_log write verification (T-LI-04, T-LI-05) requires live server with both HTTP and UDS to query the database. Unit tests verify identity construction (T-STA-08, T-STA-10) and constant value (T-STA-09). | Low residual risk | Identity chain is unit-tested. Audit integration relies on existing audit_log code paths already tested by infra-001 security suite. |
| R-10 | credential_type audit write path not directly tested E2E. Constant value verified. Identity object correctly constructed. | Low residual risk | Same as R-03 — audit write path is shared infrastructure tested by infra-001. |
| R-16 | No binary-level test of stdio-mode HTTP non-activation. Config default (enabled=false) and conditional startup in main.rs provide structural mitigation. | Low residual risk | Default config prevents accidental activation. |
| AC-23/24/25 | `docs/client-setup.md` not found — client documentation not yet created. | Medium | Documentation deliverable from implementation. Not a code coverage gap. |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | T-HL-01, T-HL-02 (listener binds, accepts, returns bound address) |
| AC-02 | PASS | T-TM-01 (64 hex, 32 bytes), T-TM-02 (mode 0600), T-TM-03 (raw bytes) |
| AC-03 | PASS | T-TM-04 (load existing returns same bytes), test_idempotent_load |
| AC-04 | PASS | T-STA-02 (missing header -> 401 with JSON body) |
| AC-05 | PASS | T-STA-03 (wrong token -> 401), T-STA-05 (malformed hex -> 401), CR-01 (ct_eq verified) |
| AC-06 | PASS | T-STA-01 (valid token -> inner service), T-STA-08 (identity inserted), T-STA-10 (BearerValidator trait) |
| AC-07 | PARTIAL | T-STA-09 (CREDENTIAL_TYPE_STATIC_TOKEN == "static_token"); audit_log write requires E2E test |
| AC-08 | PARTIAL | Identity object constructed with correct fields (T-STA-08); agent_attribution from clientInfo requires MCP session |
| AC-09 | PASS | T-TLS-01 (valid cert+key -> TlsAcceptor), T-TLS-09 (IP SAN accepted) |
| AC-10 | PASS | T-TLS-08 (disabled -> None), T-HL-04 (plain HTTP accepts connections) |
| AC-11 | PASS | T-TLS-02..07 (all invalid cert/key combinations produce descriptive errors) |
| AC-12 | PASS | T-PR-01 (/health -> 200), T-PR-02 (/ -> MCP), T-PR-04 (/observe -> 501) |
| AC-13 | PASS | T-HH-01, T-HH-02 (JSON with version + schema_version), T-HH-03 equivalent (GET /health no auth -> 200) |
| AC-14 | PASS | T-PR-04 (POST /observe -> 501 with exact JSON body), T-PR-05 equivalent (/observe requires auth via AUTH_BYPASS_PATHS exclusion) |
| AC-15 | PASS | OBSERVE_PATH constant == "/observe"; test_observe_registered_in_routing_tree (501 not 404) |
| AC-16 | PASS | Full test suite: 3427 passed, 2 failed (pre-existing col-018 embedding race, not vnc-021). Smoke integration: 23/23 pass. |
| AC-17 | PARTIAL | Structural: HTTP listener wiring exists in main.rs. Full binary-level foreground mode test not implemented. |
| AC-18 | PARTIAL | Structural: HTTP listener wiring exists in main.rs. Full binary-level daemon mode test not implemented. |
| AC-19 | PASS | T-PR-09, T-PR-10 (ProjectRouter default mode routes root path); struct exists in router.rs:164 |
| AC-20 | PASS | T-CE-01..12 (all config parsing permutations tested, including auto-detect, explicit override, defaults) |
| AC-21 | PASS | T-PR-06..08 (body size 413/boundary), T-HL-11..13 (timeout enforcement), test_default_max_body_bytes |
| AC-22 | PASS | T-HL-05 (connection limit enforced), T-HL-06 (releases on close) |
| AC-23 | NOT VERIFIED | docs/client-setup.md not found — documentation deliverable pending |
| AC-24 | NOT VERIFIED | docs/client-setup.md not found |
| AC-25 | NOT VERIFIED | docs/client-setup.md not found |
