# Risk-Based Test Strategy: nan-006

## Risks

| ID | Risk | Priority | Mitigation |
|----|------|----------|------------|
| R-01 | Rust env var parsing fails silently (panic or wrong fallback) | High | Unit test: verify 900 fallback when unset, verify custom value when set |
| R-02 | fast_tick_server doesn't actually pass env var to subprocess | High | Integration test: verify tick fires at ~30s not ~900s |
| R-03 | xfail tests have strict=True (would fail the suite on expected failures) | High | Verify strict=False on all xfail markers |
| R-04 | test_tick_liveness wall-clock race (45s wait insufficient for 30s tick) | Medium | Use 45s wait (tick fires at ~30s, buffer of 15s) |
| R-05 | MCP client thread-safety violation (concurrent calls crash) | High | All calls sequential in tests |
| R-06 | pytest mark `availability` not registered → PytestUnknownMarkWarning | Medium | Verify registration in pytest.ini |
| R-07 | test_sustained_multi_tick exceeds 60s default timeout | High | @pytest.mark.timeout(150) applied |
| R-08 | USAGE-PROTOCOL.md update missed or incomplete | Low | Verify Pre-Release Gate section present |

## Test Priorities

1. R-01, R-02: Rust change and fixture correctness — foundational
2. R-03, R-05, R-07: Test correctness — prevents suite instability  
3. R-04: Timing calibration — reliability
4. R-06, R-08: Completeness checks
