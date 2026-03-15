# Agent Report: 275-agent-1-fix

**Bug:** GH #275 — naked `.unwrap()` on JoinError in `compute_report()` permanently kills background tick
**Branch:** bugfix/275-joinhandle-unwrap (committed to worktree-agent-ae952d19)

## Work Completed

### Files Modified

- `/workspaces/unimatrix/.claude/worktrees/agent-ae952d19/crates/unimatrix-server/src/services/status.rs`

### Changes

**Site 1 (line ~638) — observation stats:**
Replaced `.unwrap()` on `JoinHandle::await` result with `.unwrap_or_else(|join_err| { tracing::error!(...); Ok(ObservationStats { zeros }) })`. Existing inner `.unwrap_or_else` fallback left unchanged.

**Site 2 (line ~662) — metric vectors:**
Replaced `.unwrap()` on `JoinHandle::await` result with `.unwrap_or_else(|join_err| { tracing::error!(...); Ok(vec![]) })`. Existing inner `.unwrap_or_else(|_| vec![])` fallback left unchanged.

### New Tests

Added to `mod confidence_refresh_tests` in `status.rs`:

- `test_join_error_recovery_pattern_observation_stats` — validates the two-level recovery chain for observation stats returns zero-filled values when JoinError occurs
- `test_join_error_recovery_pattern_metric_vectors` — validates the two-level recovery chain for metric vectors returns empty vec when JoinError occurs

Both tests use a synthetic `Result<Result<T, E>, &str>` to simulate the JoinError path without requiring an actual thread panic.

Integration-level coverage (actual `spawn_blocking` panic → recovery without tick death) lives in `test_availability.py::test_sustained_multi_tick`.

## Test Results

```
test services::status::confidence_refresh_tests::test_join_error_recovery_pattern_metric_vectors ... ok
test services::status::confidence_refresh_tests::test_join_error_recovery_pattern_observation_stats ... ok
test result: ok. 1314 passed; 0 failed; 0 ignored
```

All pre-existing tests pass. No new failures.

`cargo build --release -p unimatrix-server` — succeeds, no new warnings.
`cargo clippy -p unimatrix-server` — no errors, no new warnings.

## Commit

```
fix(status): replace naked JoinError .unwrap() with logged recovery in compute_report (#275)
```

Commit hash: `a0cdcb1` on branch `worktree-agent-ae952d19`.

## Issues / Blockers

None. Fix is minimal — exactly two sites modified, `background.rs` untouched per constraint.

`ObservationStats` does not implement `Default`, so the recovery closure uses the same zero-filled literal already present in the existing inner fallback.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server spawn_blocking JoinHandle unwrap` — Unimatrix MCP server unavailable in this agent context, no results returned.
- Stored: pattern entry via `/uni-store-pattern` attempted — MCP server unavailable. Pattern to store manually:
  - **Title:** `spawn_blocking JoinHandle .await must use unwrap_or_else, not .unwrap()`
  - **What:** Never call `.unwrap()` on a `JoinHandle::await` result in `unimatrix-server`; always use `.unwrap_or_else(|join_err| { tracing::error!(...); Ok(safe_default) })`.
  - **Why:** A thread panic inside `spawn_blocking` produces a `JoinError`. `.unwrap()` converts it to a second panic that propagates into the async task. Background tick loops cannot catch this — the task dies silently with no error surfaced to the caller.
  - **Scope:** `unimatrix-server`, any `spawn_blocking` site inside async service methods.
  - **Tags:** `server`, `async`, `crt-018b`
