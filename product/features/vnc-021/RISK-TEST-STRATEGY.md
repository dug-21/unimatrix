# Risk-Based Test Strategy: vnc-021

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | rmcp StreamableHttpService drops http::Extensions internally, breaking ResolvedIdentity propagation from StaticTokenAuth to build_context_with_external_identity | High | Medium | Critical |
| R-02 | Timing side-channel in token validation — early-return on header parsing or hex decoding leaks information about token value before reaching ConstantTimeEq | High | Low | High |
| R-03 | build_context_with_external_identity seam activated for the first time — assumptions in identity resolution (trust level, capability set, agent_id format) may not hold for bearer-token callers | High | Medium | Critical |
| R-04 | HTTP connection flood saturates tokio runtime, starving UDS listeners, background ticks, NLI inference, and write queue | High | Medium | Critical |
| R-05 | Token file generated with wrong permissions (not 0600) or readable by other users, leaking the bearer token | High | Low | High |
| R-06 | TLS startup validation allows server to bind with invalid/expired/mismatched cert+key, serving broken TLS to clients | Medium | Low | Medium |
| R-07 | Health endpoint auth bypass path-match is too broad — prefix match on "/health" accidentally exempts "/healthz", "/health/debug", or other paths from auth | High | Medium | Critical |
| R-08 | Graceful shutdown leaves HTTP sessions orphaned — CancellationToken does not propagate to in-flight HTTP/MCP sessions, causing data loss or hung connections | Medium | Medium | High |
| R-09 | Semaphore permit leak — connection task panics or errors before releasing the semaphore permit, permanently reducing available connection slots | Medium | Medium | High |
| R-10 | credential_type not written as "static_token" in audit_log for HTTP requests — identity injection path bypasses or misses the audit field | Medium | Medium | High |
| R-11 | Request body size limit not enforced before rmcp processes the body, allowing memory exhaustion via large POST payloads | Medium | Medium | High |
| R-12 | /observe stub returns 501 without requiring auth, exposing endpoint existence to unauthenticated callers | Low | Low | Low |
| R-13 | ProjectRouter single-project default path diverges from future multi-project path — seam becomes dead code that breaks when W2-6 activates | Medium | Medium | Medium |
| R-14 | Config parsing defaults incorrect — tls.enabled defaults to true when only cert_path is present (key_path missing), causing startup crash instead of graceful fallback | Medium | Low | Medium |
| R-15 | Token file with trailing newline, BOM, or non-hex characters loaded without validation, causing all auth to fail silently | Medium | Low | Medium |
| R-16 | HTTP listener activated in stdio mode, mixing exit semantics (stdin EOF vs HTTP keepalive) | Low | Low | Low |
| R-17 | CallerId::HttpBearer accidentally exempt from rate limiting due to incomplete exhaustive match handling | Medium | Low | Medium |
| R-18 | Connection timeout (30s) not enforced on slow-read/slow-write attacks — client holds connection indefinitely, consuming semaphore permit | Medium | Medium | High |

## Risk-to-Scenario Mapping

### R-01: rmcp Extension Propagation Failure
**Severity**: High
**Likelihood**: Medium
**Impact**: All HTTP-authenticated MCP tool calls fail or execute with wrong/missing identity. Audit log records "none" instead of "static_token". The entire HTTP transport is unusable for authenticated operations.

**Test Scenarios**:
1. Send authenticated MCP tool call via HTTP; assert that `build_context_with_external_identity` receives the `ResolvedIdentity` inserted by `StaticTokenAuth` (not `None`)
2. Execute tool via HTTP; query audit_log and assert credential_type = "static_token" and agent_attribution matches clientInfo.name
3. If extensions are dropped: verify the adapter boundary (ADR-003) fallback (task-local or side-channel) injects identity correctly

**Coverage Requirement**: End-to-end integration test from HTTP request through auth middleware through rmcp through tool dispatch through audit log verification. This is the single most important test for vnc-021.

### R-02: Timing Side-Channel in Token Validation
**Severity**: High
**Likelihood**: Low
**Impact**: Attacker recovers bearer token byte-by-byte via statistical timing analysis. Full server compromise.

**Test Scenarios**:
1. Code review: verify no early-return between hex-decode of presented token and `subtle::ConstantTimeEq` comparison — the only permitted early-returns are on missing Authorization header or non-"Bearer " prefix (ADR-001)
2. Verify that malformed hex tokens (odd length, non-hex chars) still reach a constant-time comparison or return in fixed time without leaking valid token prefix length
3. Verify response body and status code are identical for missing-header, wrong-token, and malformed-token cases (FR-10, FR-11)

**Coverage Requirement**: Code review checkpoint verifying `subtle::ConstantTimeEq` usage. Integration tests verifying identical 401 responses for all rejection paths.

### R-03: build_context_with_external_identity First Activation
**Severity**: High
**Likelihood**: Medium
**Impact**: Tools execute with wrong capabilities, wrong trust level, or wrong agent_id. Capability checks may over-grant or deny legitimate operations. Audit trail is incorrect.

**Test Scenarios**:
1. Execute a tool that requires `Capability::Write` via HTTP bearer auth; verify it succeeds (identity has Write capability)
2. Execute a tool via HTTP; verify `ToolContext.audit_ctx.source` contains `agent_id = "http-bearer"` and credential_type = "static_token"
3. Execute same tool via UDS; verify identity resolution uses `resolve_agent` path (not `build_context_with_external_identity`) and credential_type = "none"
4. Verify agent_attribution is populated from MCP initialize clientInfo.name, not from the bearer token

**Coverage Requirement**: Integration tests exercising the full identity chain for both HTTP and UDS paths in the same test suite, proving they produce distinct audit records.

### R-04: HTTP Connection Flood / Runtime Starvation
**Severity**: High
**Likelihood**: Medium
**Impact**: Background ticks miss deadlines, NLI inference timeouts, write queue backs up, UDS clients experience latency or disconnects. Ref Unimatrix #735 (spawn_blocking pool saturation from unbatched writes).

**Test Scenarios**:
1. Open max_concurrent_sessions (32) connections; verify the 33rd connection is rejected (TCP RST)
2. With 32 HTTP connections active, verify UDS tool calls still complete within normal latency bounds
3. Verify semaphore is acquired pre-TLS (before TlsAcceptor.accept()), not post-TLS

**Coverage Requirement**: Integration test proving connection limit enforcement and UDS isolation under HTTP load.

### R-05: Token File Permission Vulnerability
**Severity**: High
**Likelihood**: Low
**Impact**: Any local user reads the token file and gains full MCP access over HTTP.

**Test Scenarios**:
1. Generate token on first run; verify file permissions are exactly 0600
2. Verify token file is written to data_volume, not to a path that could be baked into Docker image layers
3. Verify token is printed to stdout exactly once on generation, with `[UNIMATRIX TOKEN]` label
4. On subsequent start with existing token file, verify no stdout output for the token

**Coverage Requirement**: Unit test for `load_or_generate_token` verifying file permissions and stdout behavior.

### R-06: TLS Startup Validation Failure
**Severity**: Medium
**Likelihood**: Low
**Impact**: Server starts with broken TLS config, clients cannot connect, or worse — serves plaintext when operator expects TLS.

**Test Scenarios**:
1. Start with `tls.enabled = true` and missing cert_path — verify server refuses to start with descriptive error
2. Start with `tls.enabled = true` and invalid PEM data — verify server refuses to start
3. Start with `tls.enabled = true` and valid cert+key — verify TLS handshake succeeds with a client
4. Start with `tls.enabled = false` — verify plain HTTP works, no TLS handshake attempted

**Coverage Requirement**: Unit tests for `build_tls_acceptor` with valid/invalid inputs. Integration test for TLS vs plain HTTP paths.

### R-07: Health Endpoint Auth Bypass Too Broad
**Severity**: High
**Likelihood**: Medium
**Impact**: Unauthenticated access to endpoints that should require auth. If bypass uses `starts_with("/health")`, paths like `/health/admin` or `/healthcheck` are unprotected.

**Test Scenarios**:
1. `GET /health` without auth — verify HTTP 200 (bypass works)
2. `GET /health/` without auth — verify HTTP 401 (no trailing-slash bypass)
3. `GET /healthz` without auth — verify HTTP 401 (no prefix-match bypass)
4. `GET /health?param=value` without auth — verify behavior is defined (either 200 or 401, but intentional)
5. `POST /health` without auth — verify HTTP 401 or 405 (only GET is bypassed per ADR-002)

**Coverage Requirement**: Integration tests verifying exact-match semantics for the `/health` bypass. The bypass list is a compile-time constant per ADR-002.

### R-08: Graceful Shutdown Orphans HTTP Sessions
**Severity**: Medium
**Likelihood**: Medium
**Impact**: In-flight tool calls lose results. Client sees connection drop without response. Data corruption if write is interrupted mid-transaction.

**Test Scenarios**:
1. Start HTTP session, begin tool call, trigger shutdown — verify in-flight request completes before listener closes
2. Verify CancellationToken propagation from LifecycleHandles to HTTP acceptor stops new connections
3. Verify shutdown sequence ordering: stop accepting new connections, drain in-flight, then close

**Coverage Requirement**: Integration test with concurrent shutdown and in-flight request.

### R-09: Semaphore Permit Leak
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Each leaked permit permanently reduces max connections by 1. After enough leaks, server accepts zero HTTP connections. Ref Unimatrix #1915 (UDS accept loop uses AtomicUsize with decrement-on-exit).

**Test Scenarios**:
1. Open connection, cause it to fail during TLS handshake — verify semaphore permit is released
2. Open connection, send malformed HTTP — verify permit is released after connection task exits
3. Open and close many connections sequentially — verify semaphore count returns to max after each close

**Coverage Requirement**: Integration test verifying permit recovery after connection failures. The permit must be held in a guard (RAII) that releases on drop, not manually released.

### R-10: credential_type Not Written to Audit Log
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Audit trail cannot distinguish HTTP-authenticated requests from UDS/stdio. Compliance and forensic analysis broken.

**Test Scenarios**:
1. Execute tool via HTTP; query audit_log where credential_type = "static_token" — verify row exists
2. Execute same tool via UDS; query audit_log where credential_type = "none" — verify row exists
3. Verify the constant `CREDENTIAL_TYPE_STATIC_TOKEN = "static_token"` is used (not a string literal in multiple places)

**Coverage Requirement**: Integration test querying audit_log after HTTP and UDS tool calls.

### R-11: Request Body Size Limit Not Enforced
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Attacker sends multi-GB POST body, exhausting server memory. Process killed by OOM.

**Test Scenarios**:
1. Send POST with body > 1 MB — verify HTTP 413 response before body is fully read
2. Send POST with body exactly 1 MB — verify request is accepted
3. Verify size limit is enforced before rmcp processes the body (in adapter boundary per ADR-003)

**Coverage Requirement**: Integration test with oversized body.

### R-12: /observe Stub Missing Auth
**Severity**: Low
**Likelihood**: Low
**Impact**: Unauthenticated callers can confirm the server exists and has an /observe endpoint. Minimal information leak.

**Test Scenarios**:
1. `POST /observe` without auth — verify HTTP 401 (not 501)
2. `POST /observe` with valid auth — verify HTTP 501 with exact JSON body

**Coverage Requirement**: Integration test verifying auth-before-501 ordering.

### R-13: ProjectRouter Seam Divergence
**Severity**: Medium
**Likelihood**: Medium
**Impact**: W2-6 activation requires restructuring instead of configuration change. Wasted structural investment.

**Test Scenarios**:
1. Verify all HTTP MCP requests flow through ProjectRouter (not directly to StreamableHttpService)
2. Verify ProjectRouter.default_project is used when no slug prefix in URL
3. Verify /observe is registered in ProjectRouter routing tree (FR-24)

**Coverage Requirement**: Integration test proving request flow through ProjectRouter. Code review verifying W2-6 activation path.

### R-14: Config Parsing Defaults Incorrect
**Severity**: Medium
**Likelihood**: Low
**Impact**: Server crashes on startup or binds with unexpected TLS state.

**Test Scenarios**:
1. Config with no `[tls]` section — verify tls.enabled = false
2. Config with cert_path only (no key_path) — verify tls.enabled = false
3. Config with both cert_path and key_path — verify tls.enabled = true
4. Config with `[http]` section empty — verify defaults: enabled=false, port=8443, bind="0.0.0.0"

**Coverage Requirement**: Unit tests for config parsing with all permutations.

### R-15: Token File Format Validation
**Severity**: Medium
**Likelihood**: Low
**Impact**: Server loads corrupted token file, all auth comparisons fail, no client can authenticate.

**Test Scenarios**:
1. Token file with trailing newline — verify either stripped or rejected with clear error
2. Token file with 63 hex chars (odd length) — verify rejected
3. Token file with non-hex characters — verify rejected
4. Token file with exactly 64 hex chars — verify accepted

**Coverage Requirement**: Unit tests for `load_or_generate_token` format validation.

### R-16: HTTP Listener in Stdio Mode
**Severity**: Low
**Likelihood**: Low
**Impact**: Conflicting exit semantics: stdio exits on stdin EOF, but HTTP keepalive prevents exit.

**Test Scenarios**:
1. Start in stdio mode with `[http] enabled = true` — verify HTTP listener is NOT started
2. Start in daemon mode with `[http] enabled = true` — verify HTTP listener IS started

**Coverage Requirement**: Integration test verifying mode-specific listener activation.

### R-17: HttpBearer Rate Limit Exemption
**Severity**: Medium
**Likelihood**: Low
**Impact**: HTTP callers bypass rate limiting, enabling abuse. Contradicts product vision non-negotiable #6.

**Test Scenarios**:
1. Code review: verify `CallerId::HttpBearer` match arm in rate limiter does NOT return the UdsSession exemption
2. Send rapid HTTP tool calls exceeding rate limit — verify rate limiting is applied

**Coverage Requirement**: Code review of exhaustive match. Integration test if rate limiting is testable.

### R-18: Connection Timeout Not Enforced
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Slow-read attacker holds semaphore permit for hours, gradually exhausting connection slots.

**Test Scenarios**:
1. Open connection, complete TLS, send no data — verify connection dropped after 30s
2. Open connection, send partial HTTP request, stall — verify timeout
3. Verify timeout wraps the entire connection task (TLS + HTTP + MCP session) per ADR-004

**Coverage Requirement**: Integration test with idle connection verifying timeout enforcement.

## Integration Risks

1. **rmcp <-> tower middleware boundary** (R-01): The critical integration is whether `http::Extensions` survive rmcp's internal request processing. The adapter boundary (ADR-003) provides a fallback, but the primary path must be tested end-to-end. Ref Unimatrix #4367 — rmcp 0.16 has known traps with Peer constructors, http dep linkage, and initialize return types.

2. **StaticTokenAuth <-> PathRouter ordering** (R-07): Auth bypass for `/health` happens in StaticTokenAuth before PathRouter. If the path-matching logic is inconsistent between the two layers (e.g., StaticTokenAuth bypasses `/health` but PathRouter normalizes the path differently), requests could either bypass auth on non-health paths or require auth on health.

3. **LifecycleHandles <-> HTTP acceptor** (R-08): The HTTP acceptor must be inserted into the existing shutdown sequence (between UDS MCP acceptor and hook IPC per ARCHITECTURE.md). If the shutdown ordering is wrong, HTTP sessions may be killed before UDS sessions, or vice versa.

4. **HttpConfig <-> UnimatrixConfig merge** (R-14): The existing two-level config hierarchy (global + per-project, replace semantics) must work with the new `[http]` and `[tls]` sections. If the merge logic is not extended to cover these sections, per-project overrides silently fail.

5. **CallerId::HttpBearer <-> rate limiter exhaustive match** (R-17): Adding a new enum variant requires updating every match arm. The compiler enforces this, but the semantic behavior of each arm (exempt vs. rate-limited) must be correct, not just present.

## Edge Cases

1. **Port 0 (OS-assigned)**: Config allows port 0 for testing. Verify the actual bound address is returned and usable.
2. **Concurrent token file generation**: Two server instances starting simultaneously in the same data volume — one should win, the other should load. Or both should fail-safe.
3. **Empty Authorization header**: `Authorization: ` (no "Bearer" prefix) — must return 401, not panic.
4. **Authorization header with extra spaces**: `Authorization:  Bearer  <token>` — behavior must be defined.
5. **Very long Authorization header**: Header with megabytes of data — must not cause OOM before auth check.
6. **Self-signed cert with IP SAN**: Personal cloud users use IP addresses, not domains. Cert validation must work with IP SANs.
7. **HTTP/2 vs HTTP/1.1**: rmcp's StreamableHttpService may require HTTP/1.1 for SSE. If a client sends HTTP/2, behavior must be defined.
8. **Multiple concurrent MCP sessions over HTTP**: Each HTTP session gets its own StreamableHttpService clone. State isolation must be verified (no cross-session leakage).
9. **Token file on read-only filesystem**: Container with read-only root FS but writable data volume — token generation must use data_volume path, not CWD.
10. **Max request body exactly at limit**: 1,048,576 bytes (1 MB) — boundary condition for size check (off-by-one).

## Security Risks

### Token as Bearer Credential
- **Untrusted input**: The `Authorization` header value from any network client. Attacker controls the full header content.
- **Damage from malformed input**: Panic in hex decoding could crash the server. Buffer overflow in header parsing (mitigated by hyper's limits). Timing leak from non-constant-time comparison.
- **Blast radius**: Token compromise gives full MCP access — read, write, search across all stored knowledge. No per-tool authorization granularity in vnc-021.
- **Mitigations**: `subtle::ConstantTimeEq` (ADR-001), identical 401 response for all rejection paths (FR-10/FR-11), token file 0600 permissions.

### TLS Configuration
- **Untrusted input**: PEM cert/key files on disk. If an attacker can write to cert_path/key_path, they can MITM all connections.
- **Damage**: Server serves attacker's certificate; clients connect to a MITM'd server. All bearer tokens and MCP data exposed.
- **Blast radius**: Complete confidentiality compromise of all HTTP traffic.
- **Mitigations**: File permission checks at startup. No hot-reload (restart required). Operator responsibility for file integrity.

### Health Endpoint Information Disclosure
- **Untrusted input**: Any network client can reach `/health` without auth.
- **Damage**: Reveals server version and schema version. Enables targeted attacks against known vulnerabilities in specific versions.
- **Blast radius**: Low — version information is generally considered acceptable for health endpoints. No sensitive data exposed.
- **Mitigations**: Response contains only version and schema_version — no internal state, no connection counts, no error details.

### Request Body as Attack Vector
- **Untrusted input**: POST body from authenticated callers (or unauthenticated on non-health paths before auth check).
- **Damage**: Memory exhaustion via oversized body. Malformed JSON-RPC causing rmcp panics.
- **Blast radius**: Server process crash (DoS). No data corruption — SQLite transactions are atomic.
- **Mitigations**: 1 MB body size limit (NFR-01). rmcp handles JSON-RPC parsing (delegated trust).

### Path Traversal in Router
- **Untrusted input**: Request URI path from any client.
- **Damage**: If path matching uses prefix rather than exact match, auth bypass (R-07). No filesystem path traversal risk — router dispatches to handlers, not to files.
- **Blast radius**: Auth bypass on non-health endpoints.
- **Mitigations**: Exact path match for `/health` (ADR-002, compile-time constant bypass list).

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| TLS cert/key invalid at startup | Server refuses to start with descriptive error message (AC-11) | Operator fixes cert/key files, restarts |
| Token file missing on startup | Generate new token, write file, print to stdout (FR-08) | Automatic |
| Token file corrupted | Server refuses to start with descriptive error | Operator deletes token file, restarts (new token generated) |
| rmcp extension propagation fails | Adapter boundary (ADR-003) falls back to task-local identity injection | Automatic via adapter workaround |
| HTTP connection limit reached | New connections receive TCP RST | Automatic — permits released as connections close |
| Request body exceeds 1 MB | HTTP 413 returned, body not fully read | Automatic |
| Connection idle > 30s | Connection dropped by timeout | Automatic |
| Shutdown signal during HTTP session | In-flight requests complete, new connections rejected, then listener closes | Automatic via CancellationToken |
| Bearer token incorrect | HTTP 401 with JSON error body, identical timing to missing-header case | Client retries with correct token |
| Port already in use | Server refuses to start with "address already in use" error | Operator changes port or stops conflicting process |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (rmcp StreamableHttpService lightly-adopted API) | R-01 (extension propagation), R-11 (body size limit) | Architecture isolates rmcp behind thin adapter boundary (ADR-003). Adapter copies extensions if rmcp drops them. Body size enforced pre-rmcp. |
| SR-02 (rmcp extension propagation unproven) | R-01 (extension propagation), R-03 (identity injection first activation) | ADR-003 adapter provides fallback. R-01 is the top-priority integration test. Spike test recommended before full build-out (ARCHITECTURE.md open question #1). |
| SR-03 (Claude Code bug #28293) | -- | Addressed in client documentation (AC-25, FR-30). No architecture-level risk — `-H` workaround is the primary documented path. |
| SR-04 (dependency version conflicts) | -- | Architecture pins versions matching lockfile transitives (NFR-09). Verified in Unimatrix #4661. Low residual risk. |
| SR-05 (ProjectRouter dead code) | R-13 (seam divergence) | Architecture routes all HTTP requests through ProjectRouter even in single-project mode. Integration test verifies the path is exercised. |
| SR-06 (/observe scope blur) | R-12 (/observe auth requirement) | Specification constrains /observe to static 501 response (FR-20). Test verifies zero handler logic. |
| SR-07 (POSIX-only curl hooks) | -- | Accepted scope limitation. Specification states POSIX-only (Constraint 11). No architecture-level risk. |
| SR-08 (runtime resource contention) | R-04 (connection flood), R-18 (connection timeout) | Pre-TLS semaphore (ADR-004) with configurable limit. Connection timeout prevents permit hoarding. |
| SR-09 (build_context_with_external_identity first activation) | R-03 (identity injection), R-10 (audit credential_type) | Integration tests exercise full chain: HTTP -> auth -> identity injection -> tool dispatch -> audit. Both HTTP and UDS paths tested side-by-side. |
| SR-10 (dual health surface confusion) | -- | Architecture clarifies: CLI `health` = UDS probe, HTTP `/health` = version endpoint. Distinct purposes, no semantic overlap. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-03, R-04, R-07) | 14 scenarios |
| High | 6 (R-02, R-05, R-08, R-09, R-10, R-11, R-18) | 20 scenarios |
| Medium | 5 (R-06, R-13, R-14, R-15, R-17) | 14 scenarios |
| Low | 2 (R-12, R-16) | 3 scenarios |
| **Total** | **17** | **51 scenarios** |
