# Test Plan Overview: nan-006 — Availability Test Suite

## Test Strategy

This feature IS a test suite. The "meta-test" question is: does the test suite itself work correctly?

### Unit Tests (Rust)
- `background.rs` env var parsing: 3 unit tests for read_tick_interval()
- These go in background.rs `#[cfg(test)]` module

### Integration Tests
- `pytest -m smoke` — mandatory gate (ensures existing harness unbroken)
- `pytest -m availability` — the new suite itself IS the integration test

### Verification Tests (structural)
- pytest.ini has `availability` mark registered
- USAGE-PROTOCOL.md has Pre-Release Gate section
- test_availability.py has all 6 tests with correct markers

## Risk-to-Test Mapping

| Risk ID | Risk | Test(s) | Priority |
|---------|------|---------|----------|
| R-01 | Rust env var parsing fails silently | test_read_tick_interval_default, _custom, _invalid | High |
| R-02 | fast_tick_server doesn't pass env var | test_tick_liveness (tick fires at ~30s proves it) | High |
| R-03 | xfail tests have strict=True | Code review: verify strict=False | High |
| R-04 | 45s wait insufficient for 30s tick | test_tick_liveness design: 30+15=45s buffer | Medium |
| R-05 | MCP client thread-safety violation | Code review: all calls sequential | High |
| R-06 | availability mark not registered | pytest --markers verification | Medium |
| R-07 | test_sustained_multi_tick exceeds 60s | @pytest.mark.timeout(150) applied | High |
| R-08 | USAGE-PROTOCOL.md update missed | Read file and verify section present | Low |

## Integration Harness Plan

### Existing suites to run
- `smoke`: mandatory gate — this feature touches harness infrastructure (conftest.py, client.py)
  Run: `pytest -m smoke` from product/test/infra-001/
- No other existing suites need re-running (no server tool logic changed)

### New tests this feature adds
- `suites/test_availability.py` — entirely new suite, all marked `@pytest.mark.availability`
- 5 runnable + 1 skip stub
- Run: `pytest -m availability`

### Expected results
- Smoke: all pass (no existing test logic changed)
- Availability: test_tick_liveness PASS, test_cold_start_request_race PASS, 3x XFAIL, 1x SKIP

## Cross-Component Dependencies

| Component | Depends On | Test Dependency |
|-----------|-----------|-----------------|
| C2 (fast_tick_server) | C1 (Rust env var) | Can't test C2 without C1 compiled |
| C3 (test_availability.py) | C2 (fixture) | C3 imports fast_tick_server |
| C3 (test_tick_liveness) | C1 timing | tick must fire at ~30s not ~900s |
