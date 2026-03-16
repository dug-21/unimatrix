# Security Review: 276-security-reviewer

## Risk Level: low

## Summary

The diff introduces a two-level supervisor pattern in `spawn_background_tick` to restart the background tick task after a panic, addressing GH#276. The change is a minimal, self-contained restructuring of one function in `background.rs` plus an integration test activation in `test_availability.py`. No new external inputs, no new deserialization paths, no new dependencies, and no secrets are introduced. The supervisor design is sound: the `is_cancelled()` guard on shutdown prevents a panic-misclassification that could cause a spurious restart during graceful shutdown.

## Findings

### Finding 1: Unbounded Restart Loop on Persistent Panic
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:246-253`
- **Description**: If `background_tick_loop` panics on every invocation (e.g., due to a corrupted store state that triggers a panic on every tick), the supervisor will restart indefinitely at 30-second intervals. There is no backoff escalation, no maximum restart count, and no circuit-breaker. In a persistent-panic scenario this produces a sustained log flood and continuous Arc clone/drop churn at 30-second intervals until the process is restarted by an operator. This is not exploitable from outside the process — the panic source would have to be internal — but represents a reliability concern under degenerate conditions.
- **Recommendation**: Consider a restart counter with exponential backoff (e.g., cap at 5 minutes) or a maximum restart threshold that switches to a degraded-mode log and stops retrying. This is an improvement, not a blocker; the 30-second cooldown already prevents tight infinite spin.
- **Blocking**: no

### Finding 2: Inner Task Orphan on Outer Abort
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:221-256`
- **Description**: When `graceful_shutdown` calls `tick_handle.abort()`, the outer supervisor task receives a cancellation. Tokio propagates this as a cancellation at the next await point in the outer task. If the outer task is currently blocked at `inner_handle.await` (the common steady-state case), the outer task cancellation propagates to abort the inner task as well — this is correct Tokio behavior. However, if cancellation arrives while the outer task is in the `tokio::time::sleep(Duration::from_secs(30))` cooldown between a panic and restart (rare timing window), the inner task for that restart iteration has not yet been spawned; the sleep is cancelled cleanly and the loop exits. The inner task from the previous (panicked) iteration is already resolved at that point. There is no orphan risk in practice, but the code has no explicit inner task abort on shutdown — it relies entirely on Tokio's cascade abort semantics. This is correct but worth noting as a documentation gap.
- **Recommendation**: The behavior is correct. A code comment at the `inner_handle.await` site clarifying that outer abort cascades to abort the inner task would reduce future maintainer confusion. Not blocking.
- **Blocking**: no

### Finding 3: Log Verbosity on Expected Shutdown Panic
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:247-251`
- **Description**: The error log at the panic branch (`tracing::error!`) correctly fires on an unexpected panic. The log message "background tick panicked; restarting in 30s" will not fire during normal shutdown because `is_cancelled()` correctly catches the abort signal before reaching this branch. No false-positive log noise is introduced. This is a non-finding included for completeness.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4: Integration Test Uses Hardcoded Content Strings as MCP Input
- **Severity**: low
- **Location**: `product/test/infra-001/suites/test_availability.py:301-307`
- **Description**: The new `test_tick_panic_recovery` stores entries with content strings like `"tick supervisor liveness entry {i}: availability testing data"`. These are test-controlled, constant strings with no external sourcing. No injection risk exists. This is a non-finding included for completeness.
- **Recommendation**: No action required.
- **Blocking**: no

## OWASP Assessment

| Category | Assessment |
|---|---|
| Injection | Not applicable. No new external inputs, command execution, SQL, or format strings. The error message log uses `%join_err` Display format, not a user-controlled string. |
| Broken Access Control | Not applicable. The change is internal to the background tick subsystem. No trust boundaries crossed, no privilege levels involved. |
| Security Misconfiguration | Not applicable. No new configuration parameters, environment variables, or defaults introduced. |
| Vulnerable Components | Not applicable. No new dependencies introduced. Existing `tokio`, `tracing`, and `std::sync::Arc` are used. |
| Data Integrity Failures | The supervisor restarts `background_tick_loop` after a panic. The inner loop correctly holds `Arc<Store>` and `Arc<VectorIndex>` — these are shared-reference counted handles, not cloned copies of data. A restart does not introduce a stale or split-brain data view. No integrity risk. |
| Deserialization Risks | Not applicable. No new deserialization of external data. |
| Input Validation | Not applicable. No new external inputs. The `auto_quarantine_cycles: u32` parameter is a `Copy` scalar passed by value — already validated upstream at server startup (Constraint 14, `AUTO_QUARANTINE_CYCLES_MAX`). |

## Blast Radius Assessment

Worst case if the supervisor loop has a subtle bug:

1. **Subtle bug: `is_cancelled()` guard removed or inverted** — graceful shutdown would misclassify the abort as a panic, enter the 30-second sleep before the loop can exit, delaying shutdown by up to 30 seconds. This would manifest as a hang in `graceful_shutdown` at `timeout(1s, handle).await` — the 1-second timeout would fire, the handle would be dropped, and shutdown would proceed. The inner task would be left orphaned briefly until the runtime shuts down. Blast radius: up to 1 second extra shutdown delay, no data loss.

2. **Subtle bug: `Arc::clone` omitted for one parameter per iteration** — the compiler would reject this; `Arc<T>` does not implement `Copy`, so a missing `Arc::clone` at one of the 10 `Arc::clone` call sites would be a compile error. This class of bug cannot be introduced silently.

3. **Subtle bug: supervisor `break` on `Ok(())` removed** — the loop would re-spawn `background_tick_loop` when it exits normally (which should not happen in practice). If the tick loop exits cleanly due to an internal gate, the supervisor would restart it immediately in an infinite tight loop. This would be a CPU-intensive regression detectable by monitoring but not a security issue.

The overall blast radius is bounded and safe. All failure modes produce either observable errors, compilation failures, or recoverable supervisor misbehavior — none produce silent data corruption or privilege escalation.

## Regression Risk

**Low.** The change is a strict behavioral superset of the original code: the original code spawned one fire-and-forget task; this change wraps it in an outer supervisor loop. The tick loop body (`background_tick_loop`) is entirely unchanged. The `JoinHandle` return type of `spawn_background_tick` is unchanged (`tokio::task::JoinHandle<()>`). The shutdown path in `graceful_shutdown` calls `handle.abort()` then `timeout(1s, handle).await` — this pattern works correctly against the new outer supervisor handle (the abort cancels the outer task, which propagates to abort the inner task at the next await point, and the timeout gives 1 second for resolution).

Regression scenarios checked:
- Multiple calls to `spawn_background_tick` (prevented by caller convention, not the diff — same as before)
- Signal-driven shutdown (SIGTERM → `graceful_shutdown` → `tick_handle.abort()`) — correct, covered by `test_supervisor_abort_exits_cleanly_without_restart`
- Background tick panic recovery — correct, covered by `test_supervisor_panic_causes_30s_delay_then_restart`
- Normal tick operation (no panic) — `Ok(())` breaks the loop, identical semantics to the original single-spawn

## Dependency Safety

No new dependencies introduced. The diff uses only existing APIs: `tokio::spawn`, `tokio::time::sleep`, `Duration`, `Arc::clone`, `tracing::error!`, and `JoinError::is_cancelled()` — all from crates already in the workspace.

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff. The `SYSTEM_AGENT_ID` constant (`"system"`) pre-exists and is not a credential.

## PR Comments

- Posted 1 comment on PR #285 (general findings summary, non-blocking)
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store -- the unbounded-restart-loop observation is a known trade-off in simple supervisor designs; no new generalizable anti-pattern specific to this codebase emerged from this review.
