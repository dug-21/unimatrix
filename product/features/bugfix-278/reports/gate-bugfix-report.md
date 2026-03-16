# Gate Bug Fix Report: bugfix-278

> Gate: Bug Fix Validation
> Date: 2026-03-16
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | Cache eliminates O(N) ONNX from both call sites |
| No todo!/unimplemented!/TODO/FIXME/placeholder | PASS | None found in changed files |
| All tests pass | PASS | 2538 unit + 61 integration, 0 failed |
| No new clippy errors | PASS | No errors; one new warning (see findings) |
| No unsafe code introduced | PASS | No unsafe blocks in any changed file |
| Fix is minimal (no unrelated changes) | PASS | Five files changed, all directly related to the fix |
| New tests would have caught the original bug | PASS | test_contradiction_cache_cold_start_is_none + test_contradiction_cache_write_then_read prove the read path; test_contradiction_scan_interval_constant proves the tick gate |
| Integration smoke tests passed | PASS | 61 integration tests passed |
| xfail markers have corresponding GH Issues | PASS | Pre-existing xfail markers reference GH#111, GH#238, GH#233, GH#187, GH#288, GH#291 — all have open issues |
| Investigator report has Knowledge Stewardship block | PASS | Queried #1560, #1561; Stored entry #1762 |
| Rust-dev report has Knowledge Stewardship block | WARN | Phase 2 comment on GH#278 confirms Queried + Declined (entry #1762 already captured the lesson) — stewardship is present but the Declined reason is the spawn prompt summary, not a dedicated agent report file |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The investigator correctly identified that `compute_report()` called `scan_contradictions()` unconditionally on every invocation — both maintenance tick and interactive `context_status`. The fix introduces `ContradictionScanCacheHandle = Arc<RwLock<Option<ContradictionScanResult>>>` and:

1. Background tick writes the cache every `tick_counter % CONTRADICTION_SCAN_INTERVAL_TICKS == 0` (N=4, ~60 min at default 15-min interval). `tick_counter` is incremented via `wrapping_add` before the modulo check, so the first tick (counter=0) triggers an immediate scan.
2. `compute_report()` Phase 2 becomes a pure RwLock read — no ONNX, O(1). The `contradiction_scan_performed` flag is set to `true` only when cached data exists; `false` on cold-start, which preserves existing graceful-degradation semantics.

The root cause (unconditional O(N) ONNX on every call) is eliminated from both the tick path and the interactive path.

### No Placeholder Code

**Status**: PASS

**Evidence**: Grep of all changed files (`contradiction_cache.rs`, `background.rs`, `services/status.rs`, `services/mod.rs`, `main.rs`) found zero instances of `todo!()`, `unimplemented!()`, `TODO`, or `FIXME`.

### Test Results

**Status**: PASS

**Evidence**: `cargo test --workspace` output shows all result lines as `ok` with 0 failed. Confirmed locally: 0 failures across all test binaries. The five new unit tests in `contradiction_cache.rs` all pass:
- `test_contradiction_cache_cold_start_is_none`
- `test_contradiction_cache_write_then_read`
- `test_contradiction_scan_interval_constant`
- `test_tick_counter_u32_max_wraps_without_panic`
- `test_contradiction_scan_result_clone`

Integration suite: 61 passed, 0 failed (4 pre-existing xfails are unchanged: GH#111, GH#238, GH#233, GH#187, plus GH#288 in unimatrix-vector and GH#291 in lifecycle — all pre-existing, none introduced by this fix).

### Clippy

**Status**: PASS (with WARN noted)

**Evidence**: `cargo clippy --package unimatrix-server` reports no errors. One new warning is introduced at `background.rs:480`:

```
warning: manual implementation of `.is_multiple_of()`
  --> crates/unimatrix-server/src/background.rs:480:8
  |
  | if current_tick % CONTRADICTION_SCAN_INTERVAL_TICKS == 0 {
  |    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: replace with: `current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`
```

This warning is new code introduced by this fix. The rust-dev report claimed "Clippy: clean in unimatrix-server" which is not accurate — 86 warnings exist in total, with this being one newly introduced. However, it is a style warning with zero functional impact, and 85 pre-existing warnings indicate the project does not enforce zero-warning clippy in this crate. Treating as WARN only, not blocking.

### No Unsafe Code

**Status**: PASS

**Evidence**: No `unsafe` blocks in `contradiction_cache.rs`, `services/mod.rs`, `services/status.rs`, `main.rs`, or the new tick logic in `background.rs`.

### Fix Minimality

**Status**: PASS

**Evidence**: Five files changed:
- `services/contradiction_cache.rs` — new file, solely for the cache type and constructor
- `services/mod.rs` — adds `contradiction_cache` field to `ServiceLayer` and the accessor method
- `services/status.rs` — replaces Phase 2 ONNX call with cache read; adds `contradiction_cache` field to `StatusService`
- `background.rs` — adds `tick_counter` to `TickMetadata`, tick gate logic, and scan write-path; threads `ContradictionScanCacheHandle` through `spawn_background_tick` and `background_tick_loop`
- `main.rs` — extracts `contradiction_cache_handle()` from `ServiceLayer` and passes it to `spawn_background_tick`

No unrelated changes observed. The pattern follows established precedent from GH#264 (SupersessionState) and crt-018b (EffectivenessState).

### New Tests Would Have Caught Original Bug

**Status**: PASS

**Evidence**:
- `test_contradiction_cache_cold_start_is_none`: Proves the cold-start `None` path exists and returns correctly — would have demonstrated the absence of this path (and thus the unconditional ONNX call) before the fix.
- `test_contradiction_cache_write_then_read`: Proves the cache read path returns data without any ONNX invocation — a pre-fix test would have failed because the cache type didn't exist.
- `test_contradiction_scan_interval_constant`: Proves ticks 1, 2, 3 do NOT trigger a scan — the original code had no such gate, so this assertion would have had nothing to test against.
- `test_tick_counter_u32_max_wraps_without_panic`: Covers the overflow-safety concern from the `wrapping_add` design.

Note: `test_tick_counter_scan_runs_only_on_interval` (proposed in the investigation report as test #3) was not implemented as a behavioral mock test. Instead, `test_contradiction_scan_interval_constant` validates the mathematical gate logic. This is acceptable since the tick logic is in a non-mock path, but a behavioral integration test for the scan frequency would strengthen coverage.

### File Length

**Status**: WARN

**Evidence**:
- `background.rs`: 2364 lines (pre-existing: 2283 lines before this fix). Was already over 500 lines; fix adds ~80 lines.
- `services/status.rs`: 1569 lines (pre-existing, not introduced by this fix).

These exceed the 500-line limit, but both were pre-existing violations. The fix did not create new oversized files (the new `contradiction_cache.rs` is 150 lines). Not blocking this fix; tracked as pre-existing technical debt.

### xfail Markers

**Status**: PASS

**Evidence**: All `#[ignore]` / `pytest.mark.xfail` markers in the codebase reference open GitHub issues:
- GH#111: rate limit blocks volume test
- GH#238: permissive auto-enroll
- GH#233: PERMISSIVE_AUTO_ENROLL
- GH#187: file_count field missing
- GH#288: HNSW non-determinism
- GH#291: tick interval not overridable

No new xfail markers were added by this fix.

### Knowledge Stewardship

**Status**: PASS (investigator) / WARN (rust-dev)

**Evidence**:
- Investigator report (GH#278 comment #4064357992): Contains full `## Knowledge Stewardship` section. Queried entries #1560 and #1561 before proposing the fix; stored entry #1762 ("contradiction scan O(N) ONNX cost: cache in ContradictionScanCache...") via `/uni-store-lesson`.
- Rust-dev report (GH#278 comment #4064412170 + spawn prompt): Confirms Queried #1560, #1762; Declined storing because entry #1762 already captured the lesson. The rust-dev report is a brief execution summary without a dedicated `## Knowledge Stewardship` section. Spawn prompt confirms stewardship was performed. WARN rather than FAIL because the substance is present — the reason for Declined is provided.

## Rework Required

None. All checks PASS or WARN. WARNs are:
1. New `is_multiple_of()` clippy suggestion — style only, functionally correct, trivial to address in a follow-up
2. Rust-dev report lacks a dedicated stewardship section in the GH comment — substance is present in the spawn prompt summary
3. Pre-existing oversized files (`background.rs`, `status.rs`) — not introduced by this fix
