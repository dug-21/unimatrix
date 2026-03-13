# Security Review: bugfix-236-security-reviewer

## Risk Level: low

## Summary
The bugfix addresses three server reliability issues (ghost process, tick contention, handler timeouts) with well-scoped changes. No new attack surface, no input validation regressions, no new dependencies. One non-blocking observation about unrelated commits bundled into the PR, and one informational note about remaining bare spawn_blocking calls outside the retrospective handler.

## Findings

### Finding 1: Unrelated commits bundled in bugfix PR
- **Severity**: low
- **Location**: commits eaaad10 and f8007ff (skill rename, #232)
- **Description**: The PR branch contains two commits for a skill rename refactor (#232) that are unrelated to the bugfix (#236). These commits touch only documentation and skill definition files (.claude/ directory), not Rust code. They carry no security risk but have not been through this bugfix's design review pipeline.
- **Recommendation**: Consider rebasing so that the bugfix PR only contains #236 changes, or acknowledge in the PR description that the skill rename is an intentional inclusion.
- **Blocking**: no

### Finding 2: Timeout does not cancel the in-flight spawn_blocking task
- **Severity**: low
- **Location**: crates/unimatrix-server/src/infra/timeout.rs:33
- **Description**: When `spawn_blocking_with_timeout` times out, the tokio timeout fires but the underlying `spawn_blocking` task continues running on the blocking thread pool. This is by design (tokio does not support cancelling blocking tasks), and the code correctly returns an error to the client. However, the abandoned task still holds the Store mutex until completion, meaning subsequent requests may also time out until the long-running operation finishes. This is an inherent limitation of the approach, not a bug.
- **Recommendation**: Document this behavior (the task continues after timeout) in the function's doc comment. The current doc says "Returns ServerError if the task panics or the timeout expires" but does not mention the task continues. This helps future maintainers understand the failure cascade.
- **Blocking**: no

### Finding 3: SIGKILL escalation is properly guarded
- **Severity**: informational
- **Location**: crates/unimatrix-server/src/infra/pidfile.rs:214-229
- **Description**: Verified that the SIGKILL escalation is safe. The PID originates from a PID file (read_pid_file parses u32), is validated as alive (is_process_alive), and is confirmed to be a unimatrix process (is_unimatrix_process checks /proc/{pid}/cmdline on Linux) before terminate_and_wait is called. There is no PID injection vector. The SIGKILL is a last resort after the SIGTERM timeout expires.
- **Recommendation**: None needed. The guard chain is sound.
- **Blocking**: no

### Finding 4: Remaining bare spawn_blocking calls in tools.rs
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs lines 1346, 1405, 1441, 1659, 1686, 1764, 1787, 1827, 1841, 1855
- **Description**: The fix applies spawn_blocking_with_timeout to 5 calls in the context_retrospective handler (the ones that previously used .unwrap()). However, approximately 10 other spawn_blocking calls remain in tools.rs without timeout wrappers. Some are fire-and-forget (correctly excluded per the doc comment). Others (lines 1346, 1405, 1441, 1659, 1686, 1827, 1841, 1855) could theoretically block indefinitely. The fix report acknowledges this scope limitation ("Applied to 5 direct spawn_blocking calls in context_retrospective handler"). This is not a regression (these calls existed before the fix) but is worth noting for a follow-up pass.
- **Recommendation**: Consider filing a follow-up issue to audit all spawn_blocking calls in tools.rs for timeout coverage.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | No new external inputs added. PID comes from internal PID file, not user input. Timeout durations are compile-time constants. |
| Path traversal | No file path operations added or modified. |
| Injection | PID is passed as a string argument to the `kill` command via std::process::Command (no shell injection). The pid.to_string() conversion from u32 is safe. |
| Deserialization | No new deserialization of untrusted data. |
| Error handling | Errors return ServerError to the MCP client. No internal state leakage. No panics in production paths (the .unwrap() calls on spawn_blocking results have been replaced with proper error handling). |
| Access control | No changes to trust boundaries or capability checks. |
| Dependencies | No new dependencies (Cargo.toml unchanged). |
| Secrets | No hardcoded secrets, API keys, or credentials. |

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

1. **Cancellation token race**: If the cancellation token is cancelled before the rmcp service loop fully starts, the server could exit immediately on startup. This is very unlikely since the signal handler task only fires on actual SIGTERM/SIGINT. The failure mode is safe (clean exit, not corruption).

2. **Tick timeout too aggressive**: If the 120-second tick timeout is too short for legitimate large-database operations, ticks would be repeatedly aborted. This degrades background maintenance quality but does not affect data integrity (work is idempotent). The failure mode is degraded performance, not corruption.

3. **Handler timeout cascading**: If the 30-second MCP handler timeout fires while the Store mutex is contended, the client gets an error but the blocking task continues. Multiple concurrent requests could all time out while the background tick holds the mutex. This is a denial-of-service scenario but is strictly better than the pre-fix behavior (indefinite hang).

4. **SIGKILL data corruption**: If SIGKILL hits a process while it is writing to SQLite, the database could be left in an inconsistent state. However, SQLite uses WAL journaling and is designed to survive process crashes. The SIGKILL only fires after SIGTERM + timeout, and only against verified unimatrix processes. This is an acceptable last resort.

## Regression Risk

1. **graceful_shutdown signature change**: The function no longer accepts a server future parameter. Any caller that passes the old signature would fail at compile time. Since this is internal API (not exposed via MCP), the blast radius is contained to this crate.

2. **shutdown_signal made pub**: Previously private, now public. This is additive and cannot break existing callers.

3. **spawn_blocking .unwrap() removal**: The 5 calls that previously used .unwrap() now return errors via ? operator. This changes behavior from "panic on JoinError" to "return error to client." This is strictly better but could theoretically change observable behavior if any downstream code relied on the panic (extremely unlikely in an MCP handler context).

4. **xfail marker on test_multi_agent_interaction**: This test was already failing due to bugfix-228's permissive auto-enroll. Adding the xfail marker with a GH issue reference (#238) is correct practice.

## PR Comments
- Posted 1 comment on PR #239
- Blocking findings: no

## Knowledge Stewardship
- nothing novel to store -- all findings are specific to this PR's code changes. The timeout-not-cancelling-task pattern is a well-known tokio limitation, not a project-specific anti-pattern worth storing.
