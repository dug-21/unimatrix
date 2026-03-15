# Agent Report: 276-agent-1-fix

**Bug:** GH #276 — background tick has no panic supervisor; one panic silently stops all maintenance forever
**Branch:** bugfix/276-background-tick-supervisor

## Files Modified

- `crates/unimatrix-server/src/background.rs`

## Changes

`spawn_background_tick` rewritten with two-level supervisor:
- Outer `tokio::spawn` contains a `loop` that owns the supervisor logic
- Each iteration clones all 14 `Arc<T>`/`Copy` parameters and spawns `background_tick_loop` as an inner task
- `inner_handle.await` matched on:
  - `Ok(())` → break (clean return)
  - `Err(e) if e.is_cancelled()` → break (aborted by shutdown)
  - `Err(e)` → log panic, `tokio::time::sleep(30s)`, continue loop
- Outer `JoinHandle` replaces original as `LifecycleHandles.tick_handle`

## New Tests

- `background::tests::test_supervisor_panic_causes_30s_delay_then_restart` — uses `#[tokio::test(start_paused = true)]` and `tokio::time::advance` to verify panic triggers 30s delay then restart without real wall-clock time
- `background::tests::test_supervisor_abort_exits_cleanly_without_restart` — verifies abort signal produces clean exit with no restart attempt

## Test Results

```
test background::tests::test_supervisor_panic_causes_30s_delay_then_restart ... ok
test background::tests::test_supervisor_abort_exits_cleanly_without_restart ... ok
test result: ok. ~2240 passed; 0 failed
```

Clippy: zero errors in touched files. Pre-existing 52 errors in `unimatrix-observe` (unrelated).

## Issues / Blockers

None. Fix is minimal — `spawn_background_tick` body only, no other files changed.

## Knowledge Stewardship

- Queried: Unimatrix entries #1366 (Tick Loop Error Recovery), #1367 (spawn_blocking_with_timeout), #733 (Panic Hook) via `/uni-query-patterns`
- Stored: entry #1684 "Background Task Panic Supervisor: Two-Level tokio::spawn with is_cancelled() Guard" — covers outer/inner spawn structure, `is_cancelled()` guard, Arc param cloning per iteration, and `start_paused = true` test pattern for time-based supervisor testing
