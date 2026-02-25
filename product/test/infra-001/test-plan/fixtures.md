# Test Plan: C5 — pytest Fixtures

## Scope

Fixtures manage server lifecycle. Validated by running 200+ tests without resource leaks.

## Fixture Validation

| Fixture | Validation | Test |
|---------|-----------|------|
| server | Fresh server per test, no state leakage | L-06 (isolation test) |
| server | Teardown cleans up process | E-24 (server process not running after shutdown) |
| server | Binary path resolved correctly | Every test (fixture fails if binary not found) |
| shared_server | State accumulates across module | Volume suite (entries persist across tests) |
| shared_server | Single server per module | Volume tests share one server |
| populated_server | 50 entries loaded with controlled distribution | Tests using populated_server find expected data |

## Isolation Verification (AC-05)

The primary concern: does each function-scoped fixture produce a truly isolated server?

Test L-06 validates this explicitly:
1. Test A stores entry "only-in-A" in topic "test-a"
2. Test B (same module, runs after A) searches for "only-in-A"
3. Test B must NOT find the entry (different server instance, different temp dir)

Additionally, running 200+ tests sequentially without database lock errors proves:
- Each test gets a unique temp directory
- Teardown properly kills the server
- No file descriptor leaks

## Risk Coverage

| Risk | Fixture Responsibility | Validation |
|------|----------------------|------------|
| R-02 | Shutdown sequence (shutdown -> SIGTERM -> SIGKILL) | E-24, implicit in all tests |
| R-12 | Module-scoped fixtures only for accumulation suites | Volume uses shared_server, others use function-scoped |
| NFR-01 | Isolation per test | L-06 + 200+ tests without lock errors |
