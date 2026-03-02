# col-006 Test Plan Overview

## Test Strategy

Testing follows a risk-driven approach rooted in RISK-TEST-STRATEGY.md. 23 risks (R-01 through R-23) are mapped to ~70-95 new test scenarios across 7 components. The existing 1199-test suite serves as the primary regression gate for engine extraction (R-01).

### Test Levels

| Level | Scope | Framework | Location |
|-------|-------|-----------|----------|
| Unit | Individual functions, types, error paths | `#[cfg(test)]` modules | Within each source file |
| Component Integration | UDS round-trip, hook subprocess | `#[tokio::test]` | `crates/unimatrix-engine/tests/`, `crates/unimatrix-server/tests/` |
| Regression | All existing MCP tool behavior | Existing suite | `cargo test --workspace` |
| Benchmark | Latency (Ping/Pong < 50ms) | `#[test] #[ignore]` | `crates/unimatrix-server/tests/` |

### Risk-to-Test Mapping

| Priority | Risks | Test Focus |
|----------|-------|------------|
| Critical | R-01, R-02, R-19 | Engine extraction regression (existing 1199 tests), re-export verification, UDS crash isolation |
| High | R-03, R-04, R-07, R-08, R-10, R-14, R-18 | Socket lifecycle ordering, stale socket cleanup, wire framing errors, auth UID bypass, fd leak, no-tokio-in-hook |
| Medium | R-05, R-06, R-09, R-11, R-12, R-13, R-16, R-17, R-22 | SocketGuard RAII, latency budget, malformed JSON, lineage check, stdin parsing, concurrent connections, queue limits, pruning |
| Low | R-15, R-20, R-21, R-23 | Queue corruption recovery, bootstrap idempotency, ProjectPaths extension, shutdown drain |

### Cross-Component Test Dependencies

1. **engine-extraction** must pass before any other component tests run (it changes the crate graph)
2. **wire-protocol** unit tests are prerequisite for transport and uds-listener tests
3. **transport** tests need a real UDS listener (provided by test helper `TestUdsServer`)
4. **uds-listener** tests need the full server infrastructure (store, auth)
5. **hook-subcommand** integration tests need a running server with UDS listener

## Test Infrastructure Requirements

### New Test Helpers

| Helper | Location | Purpose |
|--------|----------|---------|
| `TestUdsServer` | `unimatrix-engine/tests/` or inline | Spawns a UDS listener in tempdir, accepts connections, echoes/asserts requests |
| `RawUdsClient` | `unimatrix-engine/tests/` or inline | Low-level UDS client sending raw bytes for malformed input tests |
| `EventQueueFixture` | `unimatrix-engine/tests/` or inline | Controlled event queue directory with preset files and timestamps |

### Existing Infrastructure Reused

| Infrastructure | From | Reused For |
|---------------|------|-----------|
| `TestDb` | `unimatrix-store/src/test_helpers.rs` | Engine extraction verification |
| `tempfile::TempDir` | External crate | Socket files, queue directories, PID files |
| `PidGuard` test pattern | `unimatrix-server/src/pidfile.rs` | SocketGuard tests (same RAII pattern) |

## Estimated Test Counts

| Component | Unit Tests | Integration Tests | Total |
|-----------|-----------|------------------|-------|
| engine-extraction | 2-3 (ProjectPaths) | 0 new (1199 existing) | 2-3 new |
| wire-protocol | 12-15 | 0 | 12-15 |
| transport | 4-6 | 4-6 | 8-12 |
| authentication | 6-8 | 2-3 | 8-11 |
| event-queue | 10-12 | 0 | 10-12 |
| uds-listener | 3-5 | 6-8 | 9-13 |
| hook-subcommand | 3-5 | 4-6 | 7-11 |
| **Total new** | **~40-54** | **~16-23** | **~56-77** |

Plus the existing 1199 tests as regression gate.

## Integration Harness Plan

### Existing Suite Coverage

col-006 modifies the server binary but does NOT change any MCP tool behavior. The engine extraction is purely structural (re-exports). Therefore:

| Suite | Relevance | Reason |
|-------|-----------|--------|
| `smoke` | MANDATORY | Verify MCP still works after binary changes |
| `tools` | RUN | Verify all 10 tools work after engine extraction |
| `lifecycle` | RUN | Verify multi-step flows still work |
| `confidence` | RUN | Confidence scoring moved to engine -- verify no regression |
| `protocol` | RUN | MCP handshake still works with modified binary |
| `volume` | SKIP | No storage changes |
| `security` | SKIP | No security boundary changes via MCP |
| `contradiction` | SKIP | No contradiction logic changes |
| `edge_cases` | SKIP | No edge case behavior changes |

### New Integration Tests Needed

col-006 introduces behavior that is NOT testable through MCP (the UDS transport is a separate channel). New Rust integration tests are needed:

1. **UDS round-trip test**: Start server, connect via UDS, send Ping, verify Pong
2. **Hook subprocess test**: Start server, spawn `unimatrix-server hook Ping`, verify stdout contains Pong
3. **SessionStart/Stop end-to-end**: Start server, pipe SessionStart JSON to hook subprocess, verify server log
4. **Graceful degradation test**: No server running, hook subprocess exits 0, queue file created
5. **Concurrent UDS connections**: 5 connections in parallel, all receive responses

These are Rust integration tests in `crates/unimatrix-server/tests/` or `crates/unimatrix-engine/tests/`, not Python integration tests. The infra-001 Python harness tests MCP behavior; col-006's new behavior is UDS-based.

### No New Python Integration Tests

col-006 does not add new MCP tools or change existing tool behavior. The existing Python suites verify MCP regression. New UDS-specific tests are Rust integration tests.

## Acceptance Criteria Coverage

| AC-ID | Verification | Component Test Plans |
|-------|-------------|---------------------|
| AC-01 | Integration test: socket exists, mode 0o600 | uds-listener |
| AC-02 | Integration test: concurrent connections + MCP | uds-listener |
| AC-03 | Integration test: hook subprocess + UDS | hook-subcommand |
| AC-04 | Benchmark test: Ping/Pong < 50ms | hook-subcommand |
| AC-05 | Unit test: Transport trait methods | transport |
| AC-06 | Regression: cargo test --workspace passes | engine-extraction |
| AC-07 | Unit test: UID verification | authentication |
| AC-08 | Unit + file test: graceful degradation | hook-subcommand, event-queue |
| AC-09 | Integration test: stale socket cleanup | uds-listener |
| AC-10 | Integration test: shutdown removes socket | uds-listener |
| AC-11 | Unit test: bootstrap_defaults idempotency | hook-subcommand |
| AC-12 | Unit test: queue rotation, limits, pruning | event-queue |
| AC-13 | End-to-end: SessionStart/Stop round-trip | hook-subcommand + uds-listener |
