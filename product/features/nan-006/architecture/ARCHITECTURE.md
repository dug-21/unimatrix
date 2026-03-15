# Architecture: nan-006 — Availability Test Suite

## Components

### C1: Rust env var — background.rs
File: `crates/unimatrix-server/src/background.rs`
Change: Replace `const TICK_INTERVAL_SECS: u64 = 900` with a startup read of `UNIMATRIX_TICK_INTERVAL_SECS` env var, falling back to 900.
Scope: One function addition `read_tick_interval() -> u64` + one call site at `background_tick_loop` startup.

### C2: fast_tick_server fixture — harness/conftest.py
File: `product/test/infra-001/harness/conftest.py`
Change: Add `fast_tick_server` fixture identical to `server` but passing `UNIMATRIX_TICK_INTERVAL_SECS=30` as env var to subprocess.
Integration surface: `UnimatrixClient(binary, project_dir=str(tmp_path), extra_env={"UNIMATRIX_TICK_INTERVAL_SECS": "30"})`
Check UnimatrixClient constructor for how to pass env vars to subprocess.

### C3: test_availability.py
File: `product/test/infra-001/suites/test_availability.py`
New suite with `@pytest.mark.availability` on all tests.
Tests: test_tick_liveness, test_cold_start_request_race, test_concurrent_ops_during_tick (xfail #277), test_read_ops_not_blocked_by_tick (xfail #277), test_sustained_multi_tick (xfail #275, timeout 150), test_tick_panic_recovery (skip #276).

### C4: USAGE-PROTOCOL.md update
File: `product/test/infra-001/USAGE-PROTOCOL.md`
Add Pre-Release Gate section + add `availability` row to When to Run table.

### C5: pytest mark registration
File: `product/test/infra-001/pytest.ini`
Add `availability: Time-extended reliability tests (tick liveness, sustained operation, mutex pressure). Pre-release gate only.` to markers section.

## Integration Surface

UnimatrixClient must support passing extra environment variables to the subprocess. Check `harness/client.py` for subprocess spawn code.

## Wave Ordering

Wave 1: C1 (Rust env var) — must compile and be committed before Python tests can rely on it
Wave 2: C2 (fixture) + C5 (mark registration) — independent of each other, depend on C1 being deployed
Wave 3: C3 (test suite) + C4 (docs) — C3 depends on C2; C4 is independent
