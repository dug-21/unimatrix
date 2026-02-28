# Scope Risk Assessment: vnc-004

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `flock(2)` requires either `fs2` crate or raw `libc` calls; crate is `#![forbid(unsafe_code)]` — raw libc needs unsafe | Med | High | Evaluate `fs2` crate (safe wrapper). If unacceptable, use `std::process::Command` wrapper for flock like current kill-0 pattern. |
| SR-02 | `/proc/{pid}/cmdline` is Linux-only; macOS and other Unix lack `/proc` — identity verification has platform-dependent coverage | Med | Med | Design fallback chain: /proc/cmdline -> kill-0. Accept reduced identity confidence on non-Linux. Document coverage matrix. |
| SR-03 | Session timeout (Fix 5) could kill active long-running sessions if threshold is too aggressive | High | Med | Make timeout configurable with conservative default (30+ min). Ensure timeout only triggers when no tool calls are in flight. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Fix 4 (flock) and Fix 2 (PidGuard) overlap — both manage the PID file lifecycle. Design must clarify which owns the file and when. | Med | High | Architect should define a single PidGuard struct that owns both the flock and the file, not two separate mechanisms. |
| SR-05 | Fix 5 (watchdog) interacts with rmcp session internals — detecting "broken transport" may require rmcp API surface that doesn't exist | Med | Med | Investigate rmcp 0.16 API for session health signals before committing to approach. Scope the simpler timeout-only variant as fallback. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | graceful_shutdown currently removes PID file explicitly (step 4); PidGuard drop will also attempt removal — double-remove must be idempotent | Low | High | Existing `remove_pid_file` already handles NotFound silently. Verify PidGuard drop does the same. |
| SR-07 | Replacing `process::exit(1)` changes the exit code path — callers of `open_store_with_retry` must handle the new error variant without regressing | Med | Low | Add integration test that verifies locked-database produces proper error return, not exit. |

## Assumptions

- SCOPE.md assumes `fs2` or `libc` is acceptable as a new dependency. If the project has a strict dependency policy, this needs review (references "Dependencies" section).
- SCOPE.md assumes rmcp stdio transport will remain responsive to detect broken pipes. If rmcp buffers indefinitely on broken stdout, Fix 5 may need a different approach (references "Fix 5" section).
- SCOPE.md assumes PID recycling is the primary SIGTERM-wrong-process scenario. Container environments with PID namespace isolation may reduce this risk significantly (references "Fix 3" section).

## Design Recommendations

- **SR-01/SR-04**: Architect should design a unified `PidGuard` that encapsulates flock + PID write + drop cleanup as a single RAII type, avoiding split ownership.
- **SR-03/SR-05**: The watchdog/timeout should be the simplest viable approach — prefer `tokio::time::timeout` wrapping the session future over a separate monitoring task.
- **SR-02**: Document the platform coverage matrix explicitly in the architecture so testers know what to mock on non-Linux CI.
