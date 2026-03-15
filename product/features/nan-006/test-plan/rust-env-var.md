# Test Plan: C1 — Rust UNIMATRIX_TICK_INTERVAL_SECS env var

## Unit Tests (in background.rs #[cfg(test)])

### test_read_tick_interval_default
- Arrange: UNIMATRIX_TICK_INTERVAL_SECS not set (remove_var)
- Act: call read_tick_interval()
- Assert: returns 900

### test_read_tick_interval_custom
- Arrange: set UNIMATRIX_TICK_INTERVAL_SECS = "30"
- Act: call read_tick_interval()
- Assert: returns 30
- Cleanup: remove_var

### test_read_tick_interval_invalid
- Arrange: set UNIMATRIX_TICK_INTERVAL_SECS = "not-a-number"
- Act: call read_tick_interval()
- Assert: returns 900 (fallback)
- Cleanup: remove_var

## Compile-Time Validation
- `cargo build --workspace` must succeed — no use of removed `TICK_INTERVAL_SECS` constant
- Clippy must not warn on the new function

## Notes on Test Isolation
- Env var tests are inherently sensitive to parallel execution
- These tests are in `background.rs` test module — if cargo runs them in parallel with other
  env-mutating tests, flakiness could occur
- Acceptable risk: no other tests in this workspace mutate UNIMATRIX_TICK_INTERVAL_SECS
- If flaky, fix with: `#[serial_test::serial]` (check if serial_test is a dev-dep) or use
  `std::sync::Mutex` as a test guard
