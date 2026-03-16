# Security Review: bugfix-277-security-reviewer

## Risk Level: low

## Summary

PR #287 (bugfix/277-hot-path-spawn-blocking-timeout) replaces bare `tokio::task::spawn_blocking`
calls in MCP hot-path handlers with `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)`,
converting indefinite client hangs into bounded 30-second timeout errors. The change is
mechanical and narrowly scoped. No new inputs, no new deserialization, no new dependencies,
and no secrets are introduced. One low-severity design note is flagged but is not blocking.

---

## Findings

### Finding 1: Thread pool leak on timeout (accepted design trade-off, not blocking)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/timeout.rs:33`
- **Description**: `spawn_blocking_with_timeout` uses `tokio::time::timeout` wrapped around
  `tokio::task::spawn_blocking`. When the outer timeout fires, the future returned by
  `spawn_blocking` is dropped and the caller receives `Err`, but the underlying OS thread that
  was spawned continues to run inside the blocking thread pool until it naturally completes
  (i.e., until it acquires the mutex and returns, which could be the full 40-89 seconds that
  triggered the original bug). During that window, the Tokio blocking thread pool has one
  additional occupied slot per timed-out handler. Under high request concurrency, repeated
  timeouts could gradually exhaust the blocking thread pool (default 512 threads), turning a
  latency regression into a denial-of-service.
- **Recommendation**: This is the standard Tokio trade-off — there is no way to forcibly cancel
  a thread mid-execution in safe Rust. The risk is bounded by the tick duration (40-89 seconds)
  and the realistic call rate for this MCP server. For the current deployment context, the
  practical blast radius is low. The fix is correct and preferable to the pre-fix state
  (indefinite hang). Consider documenting this thread-leak property in `timeout.rs` for future
  maintainers. No code change required.
- **Blocking**: no

### Finding 2: Double `map_err` in store_ops.rs insert path (logic review, not blocking)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/store_ops.rs:324-335`
- **Description**: After `spawn_blocking_with_timeout`, two sequential `map_err(...)?` calls
  are applied. The outer closure returns `Result<(u64, EntryRecord), ServerError>`, so
  `spawn_blocking_with_timeout` returns `Result<Result<(u64, EntryRecord), ServerError>, ServerError>`.
  The first `map_err` converts the outer `ServerError` (timeout or panic) to `ServiceError`.
  The second `map_err` converts the inner `ServerError` (business logic error). Both arms cover
  `ServerError::Core` and a fallback `other` branch, so error information is preserved for all
  paths. This is correct. The same pattern appears in `store_correct.rs`. No defect found; noted
  for clarity.
- **Recommendation**: No change required. The pattern is correct and matches the pre-existing
  structure from before the fix (the inner error mapping was already present; only the outer
  error mapping was updated from `JoinError` string formatting to structured `ServerError` matching).
- **Blocking**: no

### Finding 3: `let _ = spawn_blocking_with_timeout(...)` at tools.rs:1274

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:1274`
- **Description**: One new `spawn_blocking_with_timeout` call discards its return value with
  `let _`. This is the observation cleanup path (60-day DELETE). Unlike the fire-and-forget
  paths at lines 374, 1784, and 1807 (which intentionally retain bare `spawn_blocking` for
  data-integrity reasons as documented in `timeout.rs`), this path wraps with timeout but
  still discards the result. The practical effect is that if this cleanup times out, it silently
  fails without any log warning. This is not a security issue (old observation records are not
  security-sensitive), but it is a gap relative to the logging pattern used elsewhere in the
  diff (`tracing::warn!("... timed out or panicked: {e}")`).
- **Recommendation**: Consider adding a `tracing::warn` on the error path for observability,
  consistent with the pattern used in status.rs and other handlers. Not blocking.
- **Blocking**: no

---

## OWASP Assessment

| Category | Assessment |
|----------|------------|
| Injection (A03) | No new user inputs introduced. All inputs flow through existing validation paths unchanged. No SQL string interpolation added. |
| Broken Access Control (A01) | No changes to capability checks, trust boundaries, or privilege levels. |
| Security Misconfiguration (A05) | No new configuration. `MCP_HANDLER_TIMEOUT = 30s` is a hardcoded constant — intentional per the architecture, not a misconfiguration. |
| Vulnerable Components (A06) | No new dependencies introduced. All crate versions unchanged. |
| Data Integrity (A08) | Fire-and-forget paths (usage recording, supersession, confidence seeding) correctly retain bare `spawn_blocking` per the documented rationale, avoiding timeout-induced data loss. |
| Deserialization (A08) | No new deserialization code. |
| Input Validation (A03) | No new external inputs. No validation logic removed. |
| Secrets | No hardcoded secrets, tokens, API keys, or credentials in the diff. |
| Error information disclosure | Timeout errors map to `CoreError::JoinError("operation timed out")`, then to `ServerError::Core`, then to `ERROR_INTERNAL (-32603)` via the existing `From<ServerError> for ErrorData` impl. Clients receive a generic "Internal storage error" message with no internal state exposed. |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: A subtly wrong error-mapping conversion (e.g., a
missed `?` or incorrect variant match in the double `map_err` chains) could cause a handler to
return a misleading error code instead of the correct one, or silently swallow an error that
should propagate. The blast radius is: incorrect error responses on store/search/correct/status
MCP calls. This would not cause data corruption, privilege escalation, or information disclosure.
The failure mode is safe — the client receives an error response rather than a hang.

The thread pool exhaustion scenario (Finding 1) remains the worst-case availability concern,
but it requires sustained concurrent requests during a prolonged tick, which is low probability
in the current deployment context.

---

## Regression Risk

**Low.** The mechanical substitution of `tokio::task::spawn_blocking(f).await` with
`spawn_blocking_with_timeout(timeout, f).await` changes only the timeout behavior, not the
data path. Existing happy-path behavior is unchanged. The error type returned on failure changes
from `JoinError` (panic only) to `ServerError::Core(CoreError::JoinError(...))` (panic or
timeout), which is handled correctly by all callsites verified in the diff.

The integration test changes are minimal: two GH#277 xfail markers removed (now hard PASS),
one unrelated flaky test (GH#286) marked xfail(strict=False). This is correct maintenance.

---

## PR Comments

- Posted 1 comment on PR #287 via `gh pr review 287 --comment`
- Blocking findings: **no**

---

## Knowledge Stewardship

Nothing novel to store -- the thread-leak-on-timeout trade-off is a well-known Tokio pattern
applicable to any blocking-thread timeout; it does not rise to a project-specific lesson.
The double `map_err` pattern for unwrapping `Result<Result<T, E>, E>` is standard Rust
error handling and already present in the codebase prior to this fix.
