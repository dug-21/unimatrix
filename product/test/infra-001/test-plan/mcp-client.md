# Test Plan: C2 — MCP Client Library

## Scope

The MCP client is tested indirectly through all 8 test suites. Every test that calls `server.context_*()` exercises the client. The protocol suite specifically validates client behavior edge cases.

## Direct Validation Points

| Test ID | Suite | What It Validates |
|---------|-------|------------------|
| P-01 | Protocol | initialize() completes MCP handshake |
| P-06 | Protocol | Client survives malformed input (server doesn't crash) |
| P-08 | Protocol | Rapid sequential requests get correct responses (ID matching) |
| P-10 | Protocol | shutdown() produces clean process exit |
| P-11 | Protocol | Client handles server receiving invalid UTF-8 |
| P-12 | Protocol | Client handles large request payload |
| E-21 | Edge Cases | 100 rapid sequential stores (stress test pipe I/O) |
| E-24 | Edge Cases | Server process cleaned up on teardown |
| V-01..V-15 | Volume | 1K+ operations through client (implicit stress test) |

## Indirect Validation

Every test in every suite exercises:
- Subprocess spawning (via fixture)
- MCP initialize handshake (via fixture)
- JSON-RPC request/response cycle (via tool calls)
- stderr drain thread (server logs during every test)
- Shutdown sequence (via fixture teardown)
- Timeout enforcement (all calls use 10s default)

## Risk Coverage

| Risk | Client Responsibility | Test |
|------|---------------------|------|
| R-01 | Line-based JSON parsing, ID matching | P-08, implicit in all tests |
| R-02 | Shutdown: SIGTERM -> SIGKILL fallback | P-10, E-24 |
| R-09 | Async stderr drain thread | Implicit (200+ tests with server logging) |
| R-07 | Timeout enforcement | TimeoutError raised on hang (tested if server hangs) |

## Error Scenarios

| Scenario | Expected Behavior | Test |
|----------|------------------|------|
| Server crashes mid-call | ServerDied exception with returncode + stderr | Implicit via P-11 |
| Server doesn't respond | TimeoutError after configured timeout | Implicit if server hangs |
| Pipe breaks | BrokenPipeError propagated | Implicit via shutdown edge cases |
| Initialize fails | ClientError with details | Fixture fails, test skipped |
