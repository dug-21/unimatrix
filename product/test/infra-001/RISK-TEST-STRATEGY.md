# Risk-Based Test Strategy: infra-001

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | MCP client JSON-RPC framing errors cause false test failures | High | Medium | High |
| R-02 | Server subprocess orphaning on test failure leaves database locked | High | Medium | High |
| R-03 | Response format changes in server silently break assertion logic | Medium | High | High |
| R-04 | ONNX Runtime version mismatch between build and test-runtime stages | High | Low | Medium |
| R-05 | Embedding model unavailable at Docker build time | Medium | Low | Medium |
| R-06 | Test data generators produce inputs that don't exercise server behavior | Medium | Medium | Medium |
| R-07 | Timeout calibration: too short causes flaky tests, too long hides hangs | Medium | Medium | Medium |
| R-08 | Docker tmpfs unavailable in CI environment | Medium | Low | Low |
| R-09 | stderr buffer deadlock: server stderr fills pipe buffer, blocks stdout writes | High | Medium | High |
| R-10 | Deterministic seed doesn't reproduce failure due to timing-dependent behavior | Medium | Medium | Medium |
| R-11 | Volume tests exceed Docker tmpfs allocation (512MB) | Medium | Low | Low |
| R-12 | Module-scoped shared server accumulates corrupt state across tests | Medium | Medium | Medium |

## Risk-to-Scenario Mapping

### R-01: MCP Client JSON-RPC Framing Errors
**Severity**: High
**Likelihood**: Medium
**Impact**: Tests fail for harness reasons, not server bugs. Undermines trust in the test suite itself.

**Test Scenarios**:
1. Verify client correctly handles multi-line JSON responses (server may emit tracing before JSON)
2. Verify client handles responses with trailing whitespace or newlines
3. Verify client correctly matches response IDs to request IDs under rapid sequential calls
4. Verify client handles server responses that arrive in fragments (partial reads)

**Coverage Requirement**: Protocol suite must exercise rapid sequential requests (P-08) and verify every response matches its request ID. Client unit tests (implicit via suite usage) validate framing under stress.

### R-02: Server Subprocess Orphaning
**Severity**: High
**Likelihood**: Medium
**Impact**: Orphaned server holds database lock, causing all subsequent tests using that temp dir to fail with lock error. Cascade failure.

**Test Scenarios**:
1. Verify fixture teardown kills server even when test raises exception
2. Verify SIGTERM fallback to SIGKILL works when server doesn't respond to shutdown
3. Verify database is not locked after fixture teardown (attempt to open it)
4. Verify server process is not running after `client.shutdown()`

**Coverage Requirement**: Edge case suite must include test_server_handles_sigterm (E-24). Fixture teardown must be verified indirectly by running 100+ tests in sequence without lock errors.

### R-03: Response Format Changes Break Assertions
**Severity**: Medium
**Likelihood**: High
**Impact**: Server updates cause widespread test failures unrelated to actual regressions.

**Test Scenarios**:
1. Verify assertion helpers handle all three response formats (summary, markdown, json)
2. Verify parse_entry works on each tool that returns entries
3. Verify assertion helpers fail with clear messages when response structure is unexpected

**Coverage Requirement**: ADR-001 abstraction layer. Edge case test E-22 (all formats x all tools) validates format handling. JSON format tests in each tool section.

### R-04: ONNX Runtime Version Mismatch
**Severity**: High
**Likelihood**: Low
**Impact**: Server binary crashes or produces wrong embeddings. All semantic search tests fail.

**Test Scenarios**:
1. Verify server starts successfully and completes MCP handshake (P-01)
2. Verify semantic search returns results (first search test triggers embedding)
3. Verify embedding consistency check passes (contradiction suite)

**Coverage Requirement**: Protocol suite P-01 validates server startup. Search tests in tools suite validate embedding pipeline. If ONNX version is wrong, these fail immediately and obviously.

### R-05: Embedding Model Unavailable at Build Time
**Severity**: Medium
**Likelihood**: Low
**Impact**: Docker build fails or model downloads at test time (violating offline requirement).

**Test Scenarios**:
1. Verify Docker build succeeds with model pre-download step
2. Verify tests pass with network disconnected (AC-17)

**Coverage Requirement**: Build pipeline validation. Manual verification of offline behavior.

### R-06: Test Data Generators Produce Irrelevant Inputs
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Tests pass but don't actually exercise interesting server behavior. False confidence.

**Test Scenarios**:
1. Verify make_contradicting_pair produces entries the server's contradiction detection actually flags
2. Verify make_injection_payloads includes payloads the server's scanner actually detects
3. Verify make_entry produces entries with valid categories (in allowlist)

**Coverage Requirement**: Contradiction suite validates that generated contradicting pairs trigger detection. Security suite validates that generated injection payloads trigger scanning. Tools suite validates that generated entries store successfully.

### R-07: Timeout Calibration
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Too short: flaky tests in CI (slower than local). Too long: slow feedback when server hangs.

**Test Scenarios**:
1. Verify 10s default timeout is sufficient for all standard operations including first embedding load
2. Verify timeout exception includes diagnostic information (which call, how long)
3. Verify volume operations complete within extended timeout

**Coverage Requirement**: All suites run successfully with default 10s timeout. Volume suite may use extended timeouts documented per-test.

### R-08: Docker tmpfs Unavailable
**Severity**: Medium
**Likelihood**: Low
**Impact**: Tests run on regular disk I/O, slower but functional. Or docker-compose config rejected.

**Test Scenarios**:
1. Verify docker-compose.yml allows tmpfs override via environment variable
2. Verify tests pass without tmpfs (fallback to regular /tmp)

**Coverage Requirement**: Docker configuration validation. tmpfs is performance optimization, not correctness requirement.

### R-09: stderr Buffer Deadlock
**Severity**: High
**Likelihood**: Medium
**Impact**: Server blocks writing to stderr because Python isn't reading it. Server stops responding to stdin. Test hangs until timeout.

**Test Scenarios**:
1. Verify client drains stderr asynchronously (dedicated thread or non-blocking reads)
2. Verify verbose server logging (RUST_LOG=debug) doesn't cause hangs
3. Verify 100 rapid tool calls don't accumulate enough stderr to block

**Coverage Requirement**: Architecture C2 specifies async stderr drain. Volume suite (100 sequential stores) stress-tests the pipe. Edge case E-21 (rapid sequential stores) validates no hang.

### R-10: Deterministic Seeds Don't Reproduce Timing Failures
**Severity**: Medium
**Likelihood**: Medium
**Impact**: A test fails in CI, developer cannot reproduce locally because failure depends on scheduling, not data.

**Test Scenarios**:
1. Verify no test depends on timing for correctness (only timeouts for hang detection)
2. Verify concurrent tests use synchronization barriers, not sleep-based timing

**Coverage Requirement**: No `time.sleep` in test logic except for explicit timing tests (freshness decay). Concurrent tests use sequential operations with verification after each step.

### R-11: Volume Tests Exceed tmpfs
**Severity**: Medium
**Likelihood**: Low
**Impact**: Writes fail with ENOSPC. Volume tests error out.

**Test Scenarios**:
1. Verify 5K entries + vector index fits within 512MB tmpfs
2. Verify 1MB content test doesn't push total over limit

**Coverage Requirement**: Volume suite monitors /tmp usage. tmpfs size (512MB) is configurable via docker-compose.yml.

### R-12: Shared Server State Corruption
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Test B depends on state from test A. If test A changes (or fails), test B breaks — fragile ordering dependency.

**Test Scenarios**:
1. Verify module-scoped tests document their state dependencies
2. Verify module-scoped tests run in defined order (pytest-ordering or class-based)
3. Verify shared server tests are independent of execution order where possible

**Coverage Requirement**: Volume suite uses ordered test classes. Each test asserts on absolute state (count >= N), not relative state (count == previous + 1).

## Integration Risks

| Risk | Components | Mitigation |
|------|-----------|------------|
| Python subprocess.Popen + Rust binary I/O incompatibility | C2 (client) + server binary | Test on same Docker base OS. Pin Python and glibc versions. |
| pytest fixture teardown order with module-scoped fixtures | C5 (fixtures) + C6 (suites) | Use pytest's built-in fixture finalization. Avoid fixture cross-dependencies. |
| docker-compose build context too large (copies entire workspace) | C1 (Docker) + workspace | Use `.dockerignore` to exclude target/, .git/, product/ (except test/infra-001/). |
| Embedding model path differs between Docker and local | C1 (Docker) + server binary | Server discovers model from standard paths. Verify model cache dir in Docker matches server expectation. |

## Edge Cases

- Empty database: all read tools should return empty/zero, not error
- Maximum-length fields: topic at 100 chars, content at ~1MB, 10 tags
- Minimum-length fields: 1-char content, 1-char topic
- Unicode normalization: does the server normalize NFC/NFD? Tests should discover and document behavior.
- Rapid operations: 100 stores in tight loop, interleaved store+search
- Server restart: data persists across shutdown/restart cycle
- PID file cleanup: fixture teardown removes PID file so next test doesn't see stale PID
- SIGTERM during write: server should persist or rollback, not corrupt database

## Security Risks

| Risk | Untrusted Input | Damage Potential | Blast Radius |
|------|----------------|------------------|--------------|
| Test harness itself is not a security boundary | N/A | N/A | N/A |
| Tests validate server's content scanning | Injection payloads in fixtures | Verifies server rejects them | Server-side validation |
| Tests validate capability enforcement | Agent IDs with different trust levels | Verifies server enforces access control | Server-side authorization |
| Docker image contains compiled binary | N/A | Binary is built from source in same pipeline | Container-scoped |

The harness itself doesn't accept untrusted input. It generates controlled test data and sends it to the server. Security testing validates the SERVER's defenses, not the harness's.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|------------------|----------|
| Docker build fails (Rust compilation) | Non-zero exit from docker compose | Fix Rust code, rebuild |
| Server crashes during test | Client timeout, test fails with diagnostic log | Fixture teardown cleans up. Investigate server stderr log. |
| Server hangs (no response) | Client timeout (10s), test fails | Fixture teardown sends SIGKILL. Investigate server stderr log. |
| Database lock contention | Server startup fails, test fails immediately | Fixture teardown ensures cleanup. No retry needed (unique temp dirs). |
| Embedding model not found | First search call fails or returns empty | Verify model pre-download in Docker image. |
| tmpfs full | Write operations fail with I/O error | Increase tmpfs size in docker-compose.yml. |
| pytest crash | Non-zero exit, partial results | Investigate pytest error output. Results may be incomplete. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (ONNX version coupling) | R-04 | ONNX Runtime version derived from Cargo.lock. Documented in Dockerfile comments. |
| SR-02 (Rust toolchain drift) | — | Addressed by pinning exact Rust version in Dockerfile. Not an architecture risk — build config. |
| SR-03 (JSON-RPC fragility) | R-01, R-09 | Client architecture (C2) specifies line-based reading, async stderr drain, timeout with diagnostics. |
| SR-04 (Embedding model download) | R-05 | Pre-download in Docker image layer. Offline test execution guaranteed. |
| SR-05 (225 test ambition) | — | Accepted. Suites are prioritized by risk. Minimum viable count per suite defined in specification. |
| SR-06 (Black-box observability) | R-03, R-06 | JSON format responses provide structured data. Assertion abstraction layer (ADR-001). Generator validation via suite usage. |
| SR-07 (Test maintenance burden) | R-03 | ADR-001 response abstraction pattern. Centralized parsing in assertions.py. |
| SR-08 (MCP protocol assumptions) | R-01 | Protocol suite validates assumptions. Server capabilities checked during initialize. |
| SR-09 (Server startup timing) | R-07 | Client uses readiness polling (initialize handshake), not fixed sleep. Configurable timeout. |
| SR-10 (redb exclusive lock) | R-02 | Unique temp dir per test. Aggressive fixture teardown with SIGKILL fallback. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-02, R-09, R-04) | 13 scenarios |
| Medium | 6 (R-03, R-06, R-07, R-10, R-11, R-12) | 14 scenarios |
| Low | 2 (R-05, R-08) | 3 scenarios |
| **Total** | **12** | **30 scenarios** |
