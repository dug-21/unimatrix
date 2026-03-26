# Gate Bug Fix Validation Report: col-025 / GH #391

> Gate: Bug Fix Validation
> Date: 2026-03-26
> Issue: GH #391
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | `if goal.is_some()` guard prevents unconditional None overwrite |
| No todo!/unimplemented!/TODO/FIXME | PASS | Zero occurrences in changed file |
| All tests pass | PASS | 2075 unit + 164 integration, 0 failures |
| No new clippy warnings | PASS | One pre-existing warning in unimatrix-engine/src/auth.rs (not touched) |
| No unsafe code introduced | PASS | Diff contains no `unsafe` additions |
| Fix is minimal | PASS | Single file changed (listener.rs), 13 insertions / 9 deletions |
| New test would have caught original bug | PASS | Test assertion flipped from None to Some("existing goal") |
| Integration smoke tests passed | PASS | 20/20 smoke, 37+2xfail lifecycle, 94+1xfail tools, 13/13 protocol |
| xfail markers have GH Issues | PASS | All xfail tests are pre-existing and unrelated to this fix |
| Knowledge stewardship — investigator | PASS | 391-agent-1-fix-report.md contains ## Knowledge Stewardship with Queried/Stored entries |
| Knowledge stewardship — tester | PASS | 391-agent-2-verify-report.md contains ## Knowledge Stewardship with Queried/Stored entries |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosed root cause was that `set_current_goal` fired unconditionally even when `goal` was `None`, overwriting any previously set goal. The fix wraps the call in `if goal.is_some()`:

```rust
// Before
session_registry.set_current_goal(&event.session_id, goal.clone());

// After
if goal.is_some() {
    session_registry.set_current_goal(&event.session_id, goal.clone());
}
```

This matches the existing `set_current_phase` guard pattern already present in the codebase. The session-resume call site (~L588) remains unconditional, consistent with ADR-004 §Decision which explicitly mandates it ("The `set_current_goal` call always runs — even on DB error — to ensure `current_goal` is deterministically set").

**AC-02 preservation**: A fresh session (where `current_goal` is initialized to `None` by `register_session`) receiving a bare `cycle_start` with no goal key still correctly results in `current_goal = None` — the guard skips the write, the field remains at its initialized `None`. `test_uds_cycle_start_no_goal_sets_none` verifies this and passes.

### No Stubs or Placeholders

**Status**: PASS

**Evidence**: Zero matches for `todo!`, `unimplemented!`, `TODO`, `FIXME` in the changed file (grep confirmed 0 occurrences).

### All Tests Pass

**Status**: PASS

**Evidence** (from 391-agent-2-verify-report.md, independently verified):
- `test_cycle_start_missing_goal_does_not_overwrite_existing`: PASS (independently re-run, confirmed `ok`)
- `test_uds_cycle_start_no_goal_sets_none`: PASS (independently re-run, confirmed `ok`)
- unimatrix-server unit suite: 2075 passed, 0 failed
- Full workspace build: `Finished dev profile`, 0 errors
- Smoke: 20/20 PASS
- Lifecycle: 37 passed, 2 pre-existing xfail
- Tools: 94 passed, 1 pre-existing xfail
- Protocol: 13/13 PASS

### No New Clippy Warnings

**Status**: PASS

**Evidence**: The one clippy error (`collapsible_if` in `unimatrix-engine/src/auth.rs:113`) is pre-existing. `git diff HEAD~1..HEAD -- crates/unimatrix-engine/src/auth.rs` returns no diff — the file was not touched by this fix.

### No Unsafe Code

**Status**: PASS

**Evidence**: `git show f290fb3` diff contains zero `+` lines with `unsafe`. The single existing `unsafe` mention in listener.rs is in a doc comment (L2736) describing an existing concern, not a code block, and was not introduced by this fix.

### Fix Minimality

**Status**: PASS

**Evidence**: Commit `f290fb3` touches exactly one file (`crates/unimatrix-server/src/uds/listener.rs`), 13 insertions, 9 deletions. Changes are:
1. The guard (`if goal.is_some()` wrapping the call site at ~L2461) — the actual fix
2. Updated doc comment on the call site to reflect new semantics
3. Updated test doc comment + inline comment
4. Flipped test assertion from `None` to `Some("existing goal")`

No unrelated changes are present.

### New Test Would Have Caught Original Bug

**Status**: PASS

**Evidence**: `test_cycle_start_missing_goal_does_not_overwrite_existing` (T-389-03) previously asserted `current_goal == None` on the second bare `cycle_start`, pinning the buggy behavior. Had the fixed assertion (`Some("existing goal")`) been present before the fix, the test would have failed against the pre-fix code, detecting the bug.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: 20/20 smoke passed. Lifecycle suite (most directly relevant — tests `test_cycle_start_with_goal_persists_across_restart`, `test_cycle_goal_drives_briefing_query`, `test_cycle_review_knowledge_reuse_cross_feature_split`) all passed.

### xfail Markers

**Status**: PASS

**Evidence**: No new xfail markers were added by this fix. The 3 pre-existing xfail tests are:
- `test_auto_quarantine_after_consecutive_bad_ticks` — requires tick interval env var
- `test_dead_knowledge_entries_deprecated_by_tick` — background tick, not testable at integration boundary
- One in tools suite (pre-existing, unrelated)

All were marked xfail prior to this fix.

### Knowledge Stewardship — Investigator (391-agent-1-fix)

**Status**: PASS

**Evidence**: `391-agent-1-fix-report.md` contains `## Knowledge Stewardship` with:
- `Queried:` entry (notes `/uni-query-patterns` was considered and rationale given)
- `Stored:` entry with reason ("the `if Some` guard pattern... is already well-established")

### Knowledge Stewardship — Tester (391-agent-2-verify)

**Status**: PASS

**Evidence**: `391-agent-2-verify-report.md` contains `## Knowledge Stewardship` with:
- `Queried:` entry (`/uni-knowledge-search` for "gate verification steps testing", entries #553, #2326, #3257, #2957 found)
- `Stored:` entry with reason ("standard Rust Option handling; no new testing technique emerged")

## Pseudocode Note (Informational — Not a Gate Block)

The original pseudocode in `pseudocode/cycle-event-handler.md` (line 68) shows `session_registry.set_current_goal` called unconditionally. The fix departs from this pseudocode intentionally — the pseudocode was the source of the bug. The fix is backed by the architecture (ADR-004 documents the session-resume unconditional requirement, which implies by contrast that the start-event path should be guarded), specification (NFR-02: backward compatibility, AC-02: absent goal yields None on fresh sessions), and the existing `set_current_phase` guard precedent. No pseudocode update is required for a bugfix; the correct behavior is now captured in the test.

## Rework Required

None.
