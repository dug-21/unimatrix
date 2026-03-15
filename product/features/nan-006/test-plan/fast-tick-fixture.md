# Test Plan: C2 — fast_tick_server fixture + UnimatrixClient extra_env

## Functional Tests

### fast_tick_server yields working client
- Fixture successfully starts server with extra_env
- Can call context_store, context_search without error
- Verified by: test_tick_liveness uses fast_tick_server and calls search/status

### extra_env is actually passed to subprocess
- Verified indirectly: tick fires at ~30s (not ~900s) in test_tick_liveness
- Direct verification: check server stderr for log line containing "tick interval set to 30s"
  (from the info! log in read_tick_interval)

### Teardown behavior
- Server shuts down cleanly after yield
- Verified: no orphaned processes after test suite

## Integration Tests (via test_tick_liveness)
- After 45s, server still responds to search and status
- This proves the 30s tick fired AND the server recovered

## client.py extra_env
- Verify UnimatrixClient accepts extra_env parameter without error
- Verify extra_env dict is merged into env before Popen
- Edge case: extra_env=None (default) — no change to env dict
- Edge case: extra_env={} (empty dict) — no change to env dict
