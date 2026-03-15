# Agent Report: 276-investigator

**Bug:** GH #276 — background tick has no panic supervisor; one panic silently stops all maintenance forever
**Role:** uni-bug-investigator

## Root Cause

`spawn_background_tick` in `background.rs:216–234` wraps `background_tick_loop` in a single fire-and-forget `tokio::spawn`. The returned `JoinHandle<()>` is stored as `LifecycleHandles.tick_handle` and is used only for `abort()` during shutdown — never for panic detection or restart.

`background_tick_loop` has an internal `loop` that catches `run_single_tick()` `Result` errors correctly. However, a panic anywhere in the async call chain outside `spawn_blocking` boundaries propagates out of the task, killing it permanently with no observable signal. A comment at lines 278–281 describes a nested-spawn intent that was never implemented.

## Affected Files

- `crates/unimatrix-server/src/background.rs:216–234` (single change site: `spawn_background_tick` body)

## Proposed Fix

Two-level `tokio::spawn` supervisor in `spawn_background_tick`:
- Outer loop spawns inner `background_tick_loop` task each iteration
- `Err(join_err) if join_err.is_cancelled() => break` — clean exit on shutdown abort
- `Err(join_err)` fallthrough — log panic, sleep 30s, restart
- All 14 parameters are `Arc<T>` or `Copy` scalars, cheap to re-clone per iteration
- Outer `JoinHandle` becomes the new `tick_handle` in `LifecycleHandles`

**Critical refinement over GH Issue proposal:** the `is_cancelled()` guard is essential — without it, `graceful_shutdown`'s `tick_handle.abort()` is misread as a panic, causing a spurious 30s restart during clean shutdown.

## Risk Assessment

Low. Single-function change, no public API surface changes, shutdown path protected.

## Missing Test

Supervisor behavior test using `#[tokio::test(start_paused = true)]` and `tokio::time::advance` to exercise:
1. Panic → 30s delay → restart (without real wall-clock time)
2. Abort → clean exit (no restart)

## Knowledge Stewardship

- Queried: Unimatrix entries #1366 (Tick Loop Error Recovery: Extract-and-Catch Pattern), #733 (Panic Hook for Long-Running Server Processes), #735 (spawn_blocking Pool Saturation from Unbatched Fire-and-Forget DB Writes) — provided in spawn context
- Stored: entry #1673 "Supervisor Pattern for fire-and-forget tokio::spawn: is_cancelled() guards abort on shutdown" — the `is_cancelled()` guard is a non-obvious necessity omitted from the ASS-020 recommendation; stored as generalizable pattern for future investigators
