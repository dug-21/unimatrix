# Gate Bugfix Report: bugfix-468

> Gate: Bugfix Validation
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | SQL fix targets both the NULL-shadowing and DESC-ordering root causes atomically |
| No todo!/unimplemented!/TODO/FIXME/placeholders | PASS | None in changed files |
| Bug-specific tests pass | PASS | Both new tests pass independently confirmed |
| Full workspace test suite passes | PASS | 0 failed across all crates |
| Clippy clean on changed crate | PASS | `cargo clippy -p unimatrix-store` 0 warnings |
| Clippy workspace-wide | WARN | Pre-existing collapsible_if in unimatrix-engine/src/auth.rs (unrelated, pre-dates this fix) |
| No unsafe code introduced | PASS | No unsafe in changed files |
| Fix is minimal (no unrelated changes) | PASS | Exactly 2 files changed; diff confirms only targeted SQL + test changes |
| New tests catch the original bug | PASS | `test_multi_session_null_start_preserves_original_goal` is an exact reproduction |
| Integration smoke tests passed | PASS | 22/22 smoke tests pass |
| xfail markers have GH Issues | PASS | XPASS on `test_search_multihop_injects_terminal_active` references GH#406 (open) — follow-up needed |
| Knowledge Stewardship: fix agent | PASS | `## Knowledge Stewardship` block present with Queried/Stored entries |
| Knowledge Stewardship: verify agent | PASS | `## Knowledge Stewardship` block present with Queried/Stored entries |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The original bug was `ORDER BY timestamp DESC, seq DESC LIMIT 1` returning a NULL-goal row (written by a second swarm session at a later timestamp) before the original goal row. The fix in `crates/unimatrix-store/src/db.rs` applies two inseparable changes:

1. `AND goal IS NOT NULL` in the WHERE clause — filters NULL-goal rows at the DB level
2. `ORDER BY timestamp ASC, seq ASC` — first-written-goal-wins semantics

The agent report correctly explains why both changes are necessary together: applying only the NULL filter with DESC ordering would still give "latest non-null goal wins", which is semantically fragile under concurrent swarm sessions. The fix is correct and complete.

### No Stubs or Placeholders

**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or placeholder functions in either changed file.

The `.unwrap_or(0)` at `db.rs:396` and `.unwrap()` calls in test code are pre-existing patterns — not introduced by this fix (confirmed via `git show b715898`).

### Bug-Specific Tests

**Status**: PASS

Two tests independently verified by this validator:

- `test_goal_correction_first_written_goal_is_preserved` (T-V16-14 renamed): Two non-NULL cycle_start rows; first written is returned. PASS (1/1).
- `test_multi_session_null_start_preserves_original_goal` (T-V16-15 new): Exact reproduction of GH#468 — second session inserts NULL-goal cycle_start; original goal is preserved. PASS (1/1).

Both tests confirm they would have caught the original bug: with the pre-fix query (`ORDER BY timestamp DESC`), `test_multi_session_null_start_preserves_original_goal` would have returned `None` instead of `Some("original goal")`.

### Full Workspace Test Suite

**Status**: PASS

`cargo test --workspace` — 0 failures. Test result lines all show 0 failed.

### Clippy

**Status**: WARN (pre-existing, unrelated)

`cargo clippy -p unimatrix-store --features test-support -- -D warnings`: clean (0 warnings, 0 errors).

`cargo clippy --workspace -- -D warnings`: one pre-existing error in `crates/unimatrix-engine/src/auth.rs:113` — collapsible `if let` pattern. This error predates this fix (last touched by commit `f02a43b`, crt-014). Not introduced by this fix. Should be tracked in a separate cleanup issue.

### No Unsafe Code Introduced

**Status**: PASS

No `unsafe` blocks in `crates/unimatrix-store/src/db.rs` or `crates/unimatrix-store/tests/migration_v15_to_v16.rs`.

### Fix Is Minimal

**Status**: PASS

`git show b715898 --stat` shows exactly 2 files changed: `crates/unimatrix-store/src/db.rs` (+13 lines) and `crates/unimatrix-store/tests/migration_v15_to_v16.rs` (+84/-17 lines). All test changes are directly related to updating the semantics assertion and adding the new regression test. No unrelated changes.

### New Tests Catch Original Bug

**Status**: PASS

`test_multi_session_null_start_preserves_original_goal` is a direct reproduction of GH#468:
- Inserts `cycle_start` with `goal = Some("original goal")` at timestamp 1700000001
- Inserts second `cycle_start` with `goal = None` at timestamp 1700000002
- Asserts `get_cycle_start_goal` returns `Some("original goal")`

With the original DESC-ordered query, this test would have returned `None` (the NULL row at the higher timestamp would have sorted first, then been flattened to `None` by the `Option::flatten()` call). The test assertion would have failed, correctly catching the bug.

### Integration Smoke Tests

**Status**: PASS

Agent-2 report confirms 22/22 smoke tests passed. Lifecycle suite: 41 passed, 2 xfailed (pre-existing tick-interval tests), 1 xpassed.

### XPASS: test_search_multihop_injects_terminal_active

**Status**: WARN (action item, not a gate blocker)

This test is marked `xfail` with reason referencing GH#406. It is now unexpectedly passing. This is unrelated to this fix (the fix is scoped to a single SQL query in `get_cycle_start_goal`). GH#406 remains open.

**Action required for bugfix leader**: Remove the xfail marker from `test_search_multihop_injects_terminal_active` in a follow-up PR and close GH#406.

### Knowledge Stewardship

**Status**: PASS (both agents)

- Agent-1 (fix): `## Knowledge Stewardship` block present. Queried context_search (entries #3958, #3959). Stored: nothing novel — lessons already captured. Rationale provided.
- Agent-2 (verify): `## Knowledge Stewardship` block present. Queried context_briefing (entries #3958, #3959, #2380). Stored: nothing novel — patterns already captured. Rationale provided.

## Action Items for Bugfix Leader (Non-blocking)

| Item | Priority | Action |
|------|----------|--------|
| XPASS on `test_search_multihop_injects_terminal_active` | Low | Remove xfail marker, close GH#406, file follow-up PR |
| Pre-existing clippy error in `unimatrix-engine/src/auth.rs:113` | Low | File cleanup issue or handle in next pass |

## Knowledge Stewardship

- Stored: nothing novel to store — gate-level bugfix patterns for NULL-shadowing SQL fixes and first-written-wins semantics corrections are already captured in Unimatrix entries #3958 and #3959 by the fix agent.
