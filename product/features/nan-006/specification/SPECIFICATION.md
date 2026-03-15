# Specification: nan-006 — Availability Test Suite

## Requirements

### R1: UNIMATRIX_TICK_INTERVAL_SECS env var
- At startup, read `UNIMATRIX_TICK_INTERVAL_SECS` from environment
- Parse as u64; if missing or unparseable, fall back to 900
- Use this value as the tick interval throughout the process lifetime
- Log the chosen interval at startup (info level)

### R2: fast_tick_server fixture
- Function-scoped (fresh server per test, like `server`)
- Passes `UNIMATRIX_TICK_INTERVAL_SECS=30` as environment variable to subprocess
- Otherwise identical to `server` fixture (same timeout, same teardown)
- Must be exported from suites/conftest.py

### R3: test_availability.py suite
All 6 tests must be present:
- `test_tick_liveness`: fast_tick_server + insert entries + wait 45s + verify search + status succeed
- `test_cold_start_request_race`: fresh server + immediate search+store before warmup + verify no crash
- `test_concurrent_ops_during_tick`: xfail(strict=False, reason="Pre-existing: GH#277 — no handler timeouts") — 8 sequential MCP calls within tick window, each < 15s
- `test_read_ops_not_blocked_by_tick`: xfail(strict=False, reason="Pre-existing: GH#277 — no handler timeouts") — 5 search + 5 get within tick window, each < 10s
- `test_sustained_multi_tick`: xfail(strict=False, reason="Pre-existing: GH#275 — unwrap kills tick permanently") + @pytest.mark.timeout(150) — 3 tick cycles ~100s
- `test_tick_panic_recovery`: @pytest.mark.skip(reason="Deferred: depends on GH#276")

Wall-clock deadlines use `time.time()` NOT pytest-timeout.
All MCP calls sequential (client not thread-safe).

### R4: USAGE-PROTOCOL.md Pre-Release Gate
Add section: "### Pre-Release Gate" under "When to Run"
Content: `pytest -m availability` must pass before tagging a release.
Update "When to Run" table to include `availability` tier row.

### R5: pytest.ini mark registration
Add to markers in pytest.ini:
`availability: Time-extended reliability tests (tick liveness, sustained operation, mutex pressure). Pre-release gate only (~15-20 min).`
