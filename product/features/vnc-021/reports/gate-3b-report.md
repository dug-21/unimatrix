# Gate 3b Report: vnc-021

> Gate: 3b (Code Review)
> Date: 2026-05-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | WARN | TrustLevel::Restricted used instead of TrustLevel::Standard per pseudocode/architecture |
| Architecture compliance | PASS | All 6 ADRs reflected; component boundaries match |
| Interface implementation | PASS | Function signatures, types, and error handling match |
| Test case alignment | PASS | 76 HTTP tests covering all test plan scenarios |
| Code quality | PASS | Compiles clean, no stubs, no .unwrap() in non-test code, all files under 500 lines |
| Security | PASS | ConstantTimeEq used, exact path match for /health, input validation present |
| Knowledge stewardship | PASS | All 7 implementation agent reports have compliant blocks |

## Detailed Findings

### 1. Pseudocode Fidelity
**Status**: WARN
**Evidence**:
- `auth.rs` line 121: `trust_level: TrustLevel::Restricted`
- `pseudocode/static-token-auth.md` line 71: `trust_level: TrustLevel::Standard`
- `ARCHITECTURE.md` line 172: `trust_level: TrustLevel::Standard`

The implementation sets `TrustLevel::Restricted` for HTTP bearer callers, while both the pseudocode and architecture specify `TrustLevel::Standard`. The tests (auth/tests.rs lines 248, 269) assert `Restricted`, indicating this was an intentional implementation decision.

This is a WARN rather than FAIL because `Restricted` is more conservative (more secure by default). However, it departs from the approved design without documented rationale. If `Restricted` was chosen deliberately (e.g., because HTTP callers should have fewer privileges than local UDS callers), this should be documented. If unintentional, it should be corrected to `Standard`.

**Impact**: HTTP bearer callers may be denied operations that `Standard` trust level would permit, depending on how TrustLevel gates capabilities in the service layer.

### 2. Architecture Compliance
**Status**: PASS
**Evidence**:

**ADR-001 (Constant-time token validation)**: `auth.rs` line 112 uses `self.token_bytes.ct_eq(&presented_bytes).into()`. No early returns between hex decode (line 101) and the comparison. `subtle` is a direct dependency in Cargo.toml line 53.

**ADR-002 (Health endpoint auth bypass)**: `auth.rs` line 31 defines `AUTH_BYPASS_PATHS` as `[("/health", &Method::GET)]`. Line 191 uses exact path + method match via `.any(|(p, m)| path == *p && method == *m)`. No prefix matching. Tests verify `/healthz`, `/health/`, and `POST /health` are all rejected (auth/tests.rs lines 364-387).

**ADR-003 (rmcp adapter boundary)**: `router.rs` lines 238-326 define `McpAdapter` wrapping `StreamableHttpService`. Body size enforced pre-rmcp (line 292-300). R-01 spike result documented in comment (line 234): extensions DO propagate.

**ADR-004 (Pre-TLS connection limiting)**: `listener.rs` lines 113-125 acquire semaphore via `try_acquire_owned()` immediately after TCP accept, before TLS handshake. RAII guard ensures release (line 135). Connection timeout wraps entire task (line 138).

**ADR-005 (rustls with configurable bypass)**: `tls.rs` uses `tokio_rustls::TlsAcceptor` (line 72), returns `None` when TLS disabled (line 37). Config auto-detection in `config.rs` line 1729-1733.

**ADR-006 (credential_type = "static_token")**: `auth.rs` line 24 defines `CREDENTIAL_TYPE_STATIC_TOKEN = "static_token"`. Test at auth/tests.rs line 258 asserts the value.

**Component boundaries**: All new code lives under `src/http/` as specified. `lib.rs` declares `pub mod http` (line 29). `services/mod.rs` has `CallerId::HttpBearer` (line 79). `infra/shutdown.rs` has HTTP fields (lines 79-84). `main.rs` has HTTP startup wiring (lines 809-849).

### 3. Interface Implementation
**Status**: PASS
**Evidence**:

- `load_or_generate_token(data_dir: &Path) -> Result<Vec<u8>, ServerError>`: Matches pseudocode signature. Returns raw bytes.
- `build_tls_acceptor(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>`: Matches pseudocode. Returns `None` when disabled.
- `start_http_listener<S>(config, tls_acceptor, service, shutdown) -> Result<(JoinHandle, SocketAddr), ServerError>`: Matches pseudocode signature.
- `StaticTokenAuth<S>` implements `tower::Service` with correct type parameters.
- `PathRouter` implements `tower::Service<Request<ReqBody>>`.
- `ProjectRouter::new(server, max_body_bytes)` matches pseudocode (takes server + config parameter).
- `BearerValidator` trait uses `fn validate(&self, token: &str) -> Pin<Box<dyn Future<...>>>` -- functionally equivalent to the pseudocode's `async fn validate(...)` since Rust lacks native async trait methods.
- `HttpConfig` and `TlsConfig` structs in `config.rs` match pseudocode with correct defaults and `#[serde(default)]`.
- `LifecycleHandles` gains `http_acceptor_handle: Option<JoinHandle<()>>` and `http_listener_addr: Option<SocketAddr>`.

### 4. Test Case Alignment
**Status**: PASS
**Evidence**: 76 HTTP tests pass across 6 test modules:

| Component | Test File | Count | Coverage |
|-----------|-----------|-------|----------|
| token.rs | inline tests | 13 | T-TM-01 through T-TM-11 + trailing newline + idempotent |
| auth.rs | auth/tests.rs | 18 | T-STA-01 through T-STA-14 + health bypass (4 tests) + constant |
| tls.rs | inline tests | 11 | T-TLS-01 through T-TLS-09 + default + IP SAN + empty cert + nonexistent key |
| health.rs | inline tests | 3 | T-HH-01, T-HH-02 + schema version |
| router.rs | router/tests.rs | 16 | T-PR-01 through T-PR-14 + response format + constants |
| listener.rs | listener/tests.rs | 12 | T-HL-01, T-HL-02, T-HL-04, T-HL-05, T-HL-06, T-HL-09-T-HL-15 |
| config.rs | inline tests | ~20 | HTTP defaults, TLS auto-detect, validation (all permutations) |
| shutdown.rs | inline tests | 5 | HTTP fields, JoinHandle, abort+join, drop ordering |

Risk coverage verified:
- R-01: Extension propagation validated by McpAdapter comment + architecture flow
- R-02: Constant-time via T-STA-07 (identical 401 for all paths) + code review
- R-04: T-HL-05 (connection limit), T-HL-06 (release on close)
- R-05: T-TM-02 (permissions 0600)
- R-07: 4 bypass tests (GET /health ok, POST /health 401, /healthz 401, /health/ 401)
- R-09: T-HL-06, T-HL-09, T-HL-10 (semaphore recovery)
- R-11: T-PR-06, T-PR-07, T-PR-08 (body size limit)
- R-14: Config tests for all TLS permutations
- R-15: T-TM-05 through T-TM-08 + trailing newline
- R-17: Rate limiter uses `matches!(caller, CallerId::UdsSession(_))` at gateway.rs:60 -- HttpBearer is NOT exempt by exhaustive match fallthrough
- R-18: T-HL-11, T-HL-12 (idle + partial request timeout)

### 5. Code Quality
**Status**: PASS
**Evidence**:

- **Compiles without errors**: `cargo build --workspace` succeeds with 0 errors (26 warnings, none from http/ module).
- **No stubs or placeholders**: `grep -rn 'todo!\|unimplemented!\|TODO\|FIXME'` in http/ returns 0 results (excluding `TODO(W2-4)` comment in services/mod.rs which is not in scope).
- **No .unwrap() in non-test code**: All `.unwrap()` instances in http/ are in test code or `#[cfg(test)]` modules. Production code uses `.map_err()`, `.expect()` (only on static builders that cannot fail), or `?`.
- **File sizes**: All under 500 lines:
  - mod.rs: 19, token.rs: 288, auth.rs: 278, tls.rs: 397, health.rs: 73, router.rs: 353, listener.rs: 304
  - Test files: auth/tests.rs: 392, router/tests.rs: 372, listener/tests.rs: 478
- **`#![forbid(unsafe_code)]`**: Enforced in lib.rs line 1.

### 6. Security
**Status**: PASS
**Evidence**:

- **No hardcoded secrets**: Token generated at runtime via `rand::fill()` (token.rs:39), stored in data volume with 0600 permissions.
- **Input validation at boundaries**: Bearer token validated (hex decode + length check + constant-time compare). Body size checked before rmcp. Config validated at startup.
- **No path traversal**: Router uses exact path match (`path == *p`), not filesystem path operations. No file serving.
- **No command injection**: No shell/process invocations in HTTP code.
- **Serialization safety**: JSON responses use static string formatting. rmcp handles JSON-RPC deserialization (delegated trust).
- **Constant-time comparison**: `subtle::ConstantTimeEq` at auth.rs:112. Identical 401 response for all rejection paths (FR-10/FR-11 verified by T-STA-07).

### 7. Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**: All 7 implementation agent reports contain `## Knowledge Stewardship` blocks with `Queried:` and `Stored:` entries:

- vnc-021-agent-3-foundation-report.md: Queried briefing, stored "nothing novel -- established patterns"
- vnc-021-agent-4-static-token-auth-report.md: Queried briefing (ADR-001/002/006), stored "nothing novel"
- vnc-021-agent-5-tls-config-report.md: Queried briefing (ADR-005), stored "nothing novel"
- vnc-021-agent-6-health-handler-report.md: Queried briefing, stored "nothing novel"
- vnc-021-agent-7-path-router-report.md: Queried briefing + search (ADR-003), stored "nothing novel"
- vnc-021-agent-8-http-listener-report.md: Queried briefing (ADR-004), stored "nothing novel"
- vnc-021-agent-9-lifecycle-integration-report.md: Queried briefing (ADR-004/005/006 + patterns), stored "nothing novel"

All "nothing novel" entries include reasons.

## WARN Items Requiring Human Review

| Issue | What to Decide |
|-------|----------------|
| TrustLevel::Restricted vs TrustLevel::Standard | `auth.rs` line 121 uses `TrustLevel::Restricted` while architecture (line 172) and pseudocode (line 71) specify `TrustLevel::Standard`. Restricted is more conservative (secure-by-default). Human should confirm which trust level is correct for HTTP bearer callers. If `Restricted` is intended, update the architecture document. If `Standard` was intended, update auth.rs and tests. |
