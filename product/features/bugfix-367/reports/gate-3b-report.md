# Gate 3b Report: bugfix-367

> Gate: 3b (Code Review)
> Date: 2026-03-24
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause fidelity | PASS | Fix addresses the two-constant mismatch exactly as diagnosed |
| Minimal scope | PASS | Only `background.rs` changed; only two constant changes + test update |
| No placeholders/stubs | PASS | "placeholder" hits are SQL parameter placeholders, not stub code |
| No `.unwrap()` introduced | PASS | Diff adds no new `.unwrap()` calls |
| No unsafe code | PASS | Diff adds no `unsafe` blocks |
| Build passes | PASS | `cargo build --workspace` — 0 errors, 10 pre-existing warnings |
| All tests pass | PASS | 1908 passed, 0 failed (unimatrix-server); all suites clean |
| No new clippy warnings | PASS | Background.rs warnings at lines 573/1126 are pre-existing (same warnings present in main at 573/1122, line delta from 4 added lines) |
| No new bug-specific test | WARN | Approved scope: no new test written; existing test updated to use constant |
| Knowledge stewardship | PASS | `367-agent-1-fix-report.md` contains `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |

## Detailed Findings

### Root Cause Fidelity
**Status**: PASS
**Evidence**: The diagnosed root cause was a two-constant mismatch: `DEAD_KNOWLEDGE_SESSION_THRESHOLD = 20` (fetch window) vs hardcoded `window = 5` passed to `detect_dead_knowledge_candidates()`. The detection function uses `window` as both the minimum-sessions guard and the definition of "recently used" sessions, so entries accessed in sessions 6–20 were incorrectly treated as absent from recent sessions.

The fix addresses both sides of the mismatch:
1. `DEAD_KNOWLEDGE_SESSION_THRESHOLD: usize = 1000` (was 20) — widens the fetch window to a practical maximum
2. `detect_dead_knowledge_candidates(..., DEAD_KNOWLEDGE_SESSION_THRESHOLD)` (was hardcoded `5`) — makes the detection window match the fetch window

This eliminates the window-mismatch condition entirely.

### Minimal Scope
**Status**: PASS
**Evidence**: `git diff main..HEAD --name-only` shows exactly two files:
- `crates/unimatrix-server/src/background.rs` (the fix)
- `product/features/bugfix-367/agents/367-agent-1-fix-report.md` (the agent report)

The diff in `background.rs` shows exactly three changed lines:
- `-const DEAD_KNOWLEDGE_SESSION_THRESHOLD: usize = 20;` → `+const DEAD_KNOWLEDGE_SESSION_THRESHOLD: usize = 1000;`
- `-detect_dead_knowledge_candidates(&observations, &store_for_detection, 5)` → calls with `DEAD_KNOWLEDGE_SESSION_THRESHOLD`
- Test: `-insert_synthetic_sessions(&store, 6)` → `+insert_synthetic_sessions(&store, DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1)`

### No Placeholders or Stubs
**Status**: PASS
**Evidence**: The grep hit on "placeholder" at lines 895 and 930–941 refers to SQL `?N` parameter placeholders in a dynamic query builder — legitimate production code, not stub functions.

### Build
**Status**: PASS
**Evidence**: `cargo build --workspace` completes with 0 errors. 10 warnings in `unimatrix-server` are pre-existing.

### Tests
**Status**: PASS
**Evidence**: Full workspace test run: 1908 passed, 0 failed in the unimatrix-server suite; all other suites also 0 failed. The previously-failing test `test_dead_knowledge_deprecation_pass_unit` (which hardcoded 6 sessions — above the old window of 5 but below the new threshold of 1000) was corrected to use `DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1`, which is the correct approach.

### Clippy
**Status**: PASS
**Evidence**: Warnings at `background.rs:573` ("if statement can be collapsed") and `background.rs:1126` ("too many arguments") are present in both the fix branch and main (main has them at lines 573 and 1122 — the 4-line offset is from the comment and test changes). No new warnings introduced.

### No New Bug-Specific Test
**Status**: WARN (non-blocking, approved)
**Evidence**: The agent report states "none — existing test updated, no new test functions added." The spawn prompt explicitly pre-approved this: "No new bug-specific test was written. The fix is a constant change with no new logic — assess whether a missing-test finding is blocking or non-blocking given the nature of the change."

Assessment: non-blocking. The fix is a pure constant change; the existing `test_dead_knowledge_deprecation_pass_unit` test already exercises the deprecation pass, and the GH #351 regression test `test_dead_knowledge_false_positive_regression` uses `DEAD_KNOWLEDGE_SESSION_THRESHOLD + 5` sessions to verify the boundary behavior under the new constant. The behavioral correctness is covered.

### Knowledge Stewardship
**Status**: PASS
**Evidence**: `367-agent-1-fix-report.md` contains:
```
## Knowledge Stewardship
- Queried: /uni-query-patterns for `unimatrix-server` — no results returned (non-blocking)
- Stored: nothing novel to store — the fix is a straightforward constant change. The one non-obvious finding (that `detect_dead_knowledge_candidates` uses its `window` argument as both a minimum-sessions guard AND a "recent" definition, meaning fetch window and detection window must match) is already captured in the in-code comment at the call site.
```
Both `Queried:` and `Stored:` entries are present with reasons. Note: The spawn prompt indicated investigator report stewardship was pre-verified (marked with ✓) before this gate was spawned; only one agent report file is present in the worktree (`agents/367-agent-1-fix-report.md`), which belongs to the rust-dev agent and passes stewardship requirements.

## Rework Required

None.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for validation patterns before writing report
- Stored: nothing novel to store -- constant-only bug fixes are handled correctly by the existing test update pattern; no new gate failure pattern to record
