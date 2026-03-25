# Gate Bugfix Report: #389

> Gate: Bugfix Validation
> Date: 2026-03-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | `goal` extraction added at Step 4b in `build_cycle_event_or_fallthrough`; payload insertion follows — matches diagnosed omission exactly |
| No todo!/unimplemented!/TODO/FIXME/placeholder | PASS | None in production code; "placeholder" in listener.rs test fixture is a string value, not a stub |
| All tests pass | PASS | 2075 passed, 0 failed (unimatrix-server) |
| No new clippy warnings | PASS | 13 pre-existing warnings in server crate; zero warnings in modified files |
| No unsafe code introduced | PASS | No `unsafe` blocks in hook.rs or listener.rs changes |
| Fix is minimal | PASS | hook.rs: 22 lines production code + 75 lines tests; listener.rs: import reorder (non-functional) + 246 lines tests only |
| New tests would have caught the original bug | PASS | `build_cycle_event_or_fallthrough_cycle_start_with_goal_in_payload` directly asserts `payload["goal"]` via the function under test |
| Integration smoke tests passed | PASS | 20/20 smoke; 37 passed + 2 xfailed (pre-existing GH#291) |
| xfail markers have corresponding GH Issues | PASS | Both xfail entries reference GH#291 (pre-existing, not introduced by this fix) |
| Knowledge stewardship: Queried/Stored entries present | PASS | Both agent reports have ## Knowledge Stewardship with Queried: and Stored:/declined entries |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosed root cause was that `build_cycle_event_or_fallthrough` extracted `phase`, `outcome`, `next_phase` from `tool_input` but omitted `goal`. The fix adds Step 4b (lines 632–651 in hook.rs): extracts `goal_opt` from `tool_input` when `validated.cycle_type == CycleType::Start`, then inserts it into `payload["goal"]` at lines 670–673. This precisely closes the gap the listener was already reading for (`payload.get("goal")`). The `MAX_GOAL_BYTES` constant that was defined-but-unreferenced in the function is now referenced in the truncation logic.

### No Stubs or Placeholders

**Status**: PASS

**Evidence**: `grep` for `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in hook.rs returns no matches. The single "placeholder" string in listener.rs (line 2872) is `fs::write(&sock_path, "placeholder")` — a test fixture creating a dummy file, not a stub function.

### All Tests Pass

**Status**: PASS

**Evidence**: Independent verification with `cargo test --package unimatrix-server` yielded 2075 passed, 0 failed — matches the agent-2 report exactly. All 7 new tests pass including both hook unit tests and listener integration tests.

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy --package unimatrix-server` produces 13 warnings. Grep for warnings targeting `uds/hook.rs` or `uds/listener.rs` returns no matches, confirming all warnings are pre-existing in other files. This aligns with agent-2's confirmation that `unimatrix-server` has no new clippy errors from the fix.

### No Unsafe Code

**Status**: PASS

**Evidence**: `grep "unsafe "` in both changed files returns no `unsafe` blocks. The single result in listener.rs is a doc comment ("Raw BEGIN/COMMIT executed against the pool directly is unsafe because..."), not a code construct.

### Fix is Minimal

**Status**: PASS

**Evidence**: The git diff against main shows:
- `hook.rs`: exactly 22 production lines added (Step 4b extraction + payload insertion), 75 test lines added
- `listener.rs`: import reorder only in production code (no semantic change), 246 lines of new tests

No unrelated changes included.

### New Tests Would Have Caught Original Bug

**Status**: PASS

**Evidence**: `build_cycle_event_or_fallthrough_cycle_start_with_goal_in_payload` calls `build_request("PreToolUse", &input)` with `"goal": "some goal text"` in `tool_input`, then asserts `event.payload["goal"].as_str() == Some("some goal text")`. Before the fix, this assertion would have failed because `payload["goal"]` would be absent. This is a direct regression test for the exact bug.

The integration test `test_subagent_start_fires_goal_branch_when_goal_set_via_hook_payload` provides end-to-end coverage: cycle_start with goal in payload → session registry goal populated → SubagentStart hits the goal-present branch. This would have caught the full failure chain before the fix.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: Agent-2 reports 20/20 smoke, 37 lifecycle passed + 2 xfailed. The 2 xfail tests reference GH#291 (pre-existing tick-interval limitation) and were already marked before this bugfix. No new xfail markers were added.

### xfail Markers Have GH Issues

**Status**: PASS

**Evidence**: Both xfail markers in `test_lifecycle.py` explicitly state `"Pre-existing: GH#291 — tick interval not overridable at integration level"`. No new xfail markers were added by this fix.

### Knowledge Stewardship

**Status**: PASS

**Evidence**:
- `389-agent-1-fix-report.md` contains `## Knowledge Stewardship` with `Queried:` (hook payload construction patterns search) and `Stored:` (entry #3484 via `/uni-store-pattern`).
- `389-agent-2-verify-report.md` contains `## Knowledge Stewardship` with `Queried:` (procedure/pattern queries) and `Stored: nothing novel to store -- agent-1 already captured the key pattern (entry #3484)`.

Both reports are compliant.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store -- validation confirmed a clean, minimal fix; all patterns captured by agent-1 (entry #3484). No systemic failure pattern to store from this gate.
