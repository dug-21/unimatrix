# Test Plan: C3 — test_availability.py

## Expected Test Results

| Test | Mark | Expected Result | Rationale |
|------|------|----------------|-----------|
| test_tick_liveness | availability | PASS | Server survives tick; GH#275/#276 fixes landed per spawn prompt |
| test_cold_start_request_race | availability | PASS | No crash on cold start — graceful errors acceptable |
| test_concurrent_ops_during_tick | availability, xfail(strict=False) | XFAIL | GH#277 not fixed |
| test_read_ops_not_blocked_by_tick | availability, xfail(strict=False) | XFAIL | GH#277 not fixed |
| test_sustained_multi_tick | availability, xfail(strict=False), timeout(150) | XFAIL | GH#275 not fixed |
| test_tick_panic_recovery | availability, skip | SKIP | GH#276 not resolved |

## Marker Validation

### @pytest.mark.availability
- All 6 tests must have this marker
- `pytest -m availability` should collect all 6 tests
- `pytest -m "not availability"` should collect 0 tests from this file

### @pytest.mark.xfail(strict=False, ...)
- 3 tests: test_concurrent_ops_during_tick, test_read_ops_not_blocked_by_tick, test_sustained_multi_tick
- strict=False means: passing unexpectedly → XPASS (not a failure)
- reason must reference the GH issue number

### @pytest.mark.timeout(150)
- test_sustained_multi_tick only
- Default timeout is 60s; this test runs ~100-110s legitimately

### @pytest.mark.skip(...)
- test_tick_panic_recovery only
- reason must reference GH#276

## Assertion Logic

### test_tick_liveness
- After 45s sleep: search and status both return non-None responses with no error field

### test_cold_start_request_race
- After immediate requests: `client._process.poll() is None` (process still alive)
- Any graceful error or success from MCP calls is acceptable

### test_concurrent_ops_during_tick (xfail)
- Each of 8 ops: `time.time() <= deadline` where deadline = op_start + 15.0

### test_read_ops_not_blocked_by_tick (xfail)
- Each of 10 ops: `time.time() <= deadline` where deadline = op_start + 10.0

### test_sustained_multi_tick (xfail)
- After each of 3 cycles: search returns non-None, no error; status returns non-None, no error

### test_tick_panic_recovery (skip)
- Body is `pass` — no assertions needed for a stub

## `pytest -m availability` expected output

```
collected 6 items

suites/test_availability.py::test_tick_liveness PASSED
suites/test_availability.py::test_cold_start_request_race PASSED
suites/test_availability.py::test_concurrent_ops_during_tick XFAIL
suites/test_availability.py::test_read_ops_not_blocked_by_tick XFAIL
suites/test_availability.py::test_sustained_multi_tick XFAIL
suites/test_availability.py::test_tick_panic_recovery SKIPPED

====== 2 passed, 1 skipped, 3 xfailed in ~NNNs ======
```
