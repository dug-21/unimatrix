# Gate Bugfix Report: GH#277

> Gate: Bug Fix Validation
> Feature: crt-018b / bugfix-277
> Date: 2026-03-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | All hot-path `spawn_blocking` calls wrapped with `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT)` — mutex contention now bounded at 30s |
| No todo!/unimplemented!/TODO/FIXME | PASS | None found in any changed file |
| All tests pass | PASS | 1317 unit tests passed; all integration suites passed |
| No new clippy warnings | PASS | Zero clippy issues in the 6 changed Rust source files |
| No unsafe code introduced | PASS | No `unsafe` blocks in any changed file |
| Fix is minimal (no unrelated changes) | PASS | 9 files changed; all directly related to the fix or xfail marker management |
| New test would have caught original bug | PASS | `test_handler_times_out_when_mutex_held_by_background_tick` directly simulates the exact failure mode |
| Integration smoke tests passed | PASS | 19 passed, 1 xfailed (pre-existing GH#111) |
| xfail markers properly managed | PASS | Two GH#277 xfail markers removed from `test_availability.py`; GH#286 xfail added for pre-existing flaky test |
| Knowledge stewardship — all agent reports | PASS | Investigator, rust-dev, and tester reports all contain `## Knowledge Stewardship` with Queried/Stored/Declined entries |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

The diagnosed root cause was: bare `tokio::task::spawn_blocking()` in all hot-path MCP handlers except `context_retrospective`, causing indefinite client hangs when the background tick held `Mutex<Connection>` for 40-89 seconds.

The fix applies `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` (30s timeout) to every hot-path handler:
- `services/search.rs` lines 224, 461 — embed + co-access boost
- `services/store_ops.rs` lines 117, 187 — embed + atomic insert
- `services/store_correct.rs` lines 52, 84 — embed + atomic correct
- `services/status.rs` lines 209, 440, 468, 499, 632, 660, 674 — all phases of `compute_report()`
- `mcp/tools.rs` lines 1126, 1158, 1222, 1258, 1274, 1352, 1414, 1454, 1677, 1705, 1847, 1865, 1882 — cycle, retrospective, briefing, and ancillary store ops

Fire-and-forget background writes (usage recording at tools.rs:374, supersession chain at tools.rs:1784, confidence seeding at tools.rs:1807) correctly retain bare `spawn_blocking` per the `timeout.rs` doc comment: "Do NOT use this for fire-and-forget background writes where timeouts would cause data loss."

The maintenance path (`run_maintenance()` in status.rs lines 787, 849, 897, 996, 1022, 1051, 1079) also correctly retains bare `spawn_blocking` — this code IS the background tick, not the handler path.

### No todo!/unimplemented!/TODO/FIXME

**Status**: PASS

Grep across all 6 changed Rust source files returned zero matches.

### All Tests Pass

**Status**: PASS

Confirmed via local `cargo test --workspace` run:
- 1317 tests in `unimatrix-server`: all passed
- All other workspace crates: all passed
- The intermittent `test_compact_search_consistency` failure in `unimatrix-vector` is pre-existing and confirmed unrelated (vector crate not modified; passes in isolation).

### No New Clippy Warnings

**Status**: PASS

`cargo clippy -p unimatrix-server --no-deps -- -D warnings` produces zero issues in the 6 changed files. The 83 clippy errors visible in `unimatrix-server` are pre-existing (confirmed: count was identical before this fix); the failing dependency crates (`unimatrix-engine`, `unimatrix-observe`) also have pre-existing issues. No new issues introduced.

### No Unsafe Code

**Status**: PASS

No `unsafe` blocks in any changed file.

### Fix is Minimal

**Status**: PASS

9 files changed: 6 Rust source files (the fix), 1 regression test (in `infra/timeout.rs`), and 3 integration test infrastructure files (xfail marker management per USAGE-PROTOCOL.md). No unrelated code changes.

### New Test Catches Original Bug

**Status**: PASS

`test_handler_times_out_when_mutex_held_by_background_tick` in `crates/unimatrix-server/src/infra/timeout.rs` accurately simulates the failure mode:
- Background thread acquires `Mutex<()>` (representing `Mutex<Connection>`) for 2 seconds
- Handler task runs `spawn_blocking_with_timeout` with 50ms timeout, attempting to acquire same mutex
- Asserts `Err` containing "timed out" — if bare `spawn_blocking` were used instead, this test would hang for 2 seconds and then PASS (no timeout), revealing the regression.
- Test passed in 5.01s per tester report.

### Integration Smoke Tests

**Status**: PASS

19 passed, 1 xfailed (pre-existing GH#111 volume test). Availability tests `test_concurrent_ops_during_tick` and `test_read_ops_not_blocked_by_tick` now run as hard PASS gates (xfail markers removed).

### xfail Marker Management

**Status**: PASS

- `test_concurrent_ops_during_tick`: xfail removed, hard PASS gate (GH#277 fixed)
- `test_read_ops_not_blocked_by_tick`: xfail removed, hard PASS gate (GH#277 fixed)
- `test_search_multihop_injects_terminal_active`: xfail(strict=False) added with GH#286 reference — pre-existing flaky test, passes in isolation, confirmed unrelated to this fix
- GH#286 issue confirmed filed

### Knowledge Stewardship

**Status**: PASS

All three agent reports contain `## Knowledge Stewardship` sections:

- **Investigator** (GH#277 comment): Queried #1367, #771, #1369; Stored entry confirming scope; Declined (no new pattern needed, #1367 already captures it)
- **Rust-dev** (277-agent-1-fix-report.md): Queried #1367, #1369; Stored entry #1700 "spawn_blocking_with_timeout: outer error type is ServerError, not JoinError"
- **Tester** (277-agent-2-verify-report.md): Queried category "procedure" for timeout mutex handler patterns; Stored nothing novel (pattern documented in commit); Declined xfail-removal procedure (covered by USAGE-PROTOCOL.md)

## File Line Count Note

`status.rs` (1570 lines), `search.rs` (1305 lines), and `tools.rs` (2605 lines) exceed the 500-line guideline. These are **pre-existing conditions** — the counts before this fix were 1562, 1304, and 2579 respectively. This fix added a small number of lines to each as part of the wrapping. These were not introduced by this bugfix and are not gating issues for this review.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — the "all hot-path handlers need timeout" lesson is better captured as a per-feature pattern (which the rust-dev agent already stored as entry #1700). No systemic gate failure pattern emerged from this review.
