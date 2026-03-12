# Test Plan: C6 — Postinstall

## Unit Tests (packages/unimatrix/test/postinstall.test.js)

### R-08 Scenarios (all 5 from Risk Strategy)

- `test_postinstall_with_binary_calls_model_download`: Mock `execFileSync` to succeed. Assert it was called with `[resolveBinary(), 'model-download']` args. Assert exit code 0.
- `test_postinstall_network_failure_exits_0`: Mock `execFileSync` to throw (simulating download failure). Assert stderr contains warning. Assert `process.exitCode` is 0 or undefined (never 1).
- `test_postinstall_binary_missing_exits_0`: Mock `resolveBinary()` to throw. Assert stderr contains "binary not found" or similar warning. Assert exit code 0.
- `test_postinstall_disk_full_exits_0`: Mock `execFileSync` to throw with a disk-full-like error. Assert exit code 0.
- `test_postinstall_model_already_cached_succeeds`: Mock `execFileSync` to succeed quickly (model-download is a no-op when cached). Assert exit code 0.

### Error Wrapping

- `test_all_code_paths_wrapped_in_try_catch`: Inspect the postinstall source (or test by mocking every possible throw point) to ensure no unhandled exception can cause a non-zero exit.

## Integration Tests (Shell)

- `test_postinstall_runs_without_crashing`: With a built binary available, run `node postinstall.js`. Assert exit code 0.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-08 | Postinstall blocks/fails npm install | All 5 tests above ensure unconditional exit 0 |
