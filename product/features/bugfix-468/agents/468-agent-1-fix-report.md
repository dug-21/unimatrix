# Agent Report: 468-agent-1-fix

**Feature:** bugfix-468
**Issue:** GH #468 — `context_cycle_review` returns no goal when multiple `cycle_start` events exist for the same `feature_cycle`
**Branch:** `bugfix/468-cycle-start-null-shadows-goal`

## Changes Made

### Part 1 — SQL fix

File: `crates/unimatrix-store/src/db.rs`

`SqlxStore::get_cycle_start_goal` query updated:
- Added `AND goal IS NOT NULL` to the `WHERE` clause — excludes NULL-goal rows at the DB level
- Changed `ORDER BY timestamp DESC, seq DESC` to `ORDER BY timestamp ASC, seq ASC` — first-written-goal-wins semantics

The two changes are inseparable. Applying only the NULL filter while keeping DESC would still give "latest non-null goal wins", which is semantically fragile under concurrent swarm sessions.

Updated the doc comment to document the first-written-goal-wins semantic and the GH #468 motivation.

### Part 2 — Test changes

File: `crates/unimatrix-store/tests/migration_v15_to_v16.rs`

1. **RENAMED** `test_uds_truncate_then_overwrite_last_writer_wins` → `test_goal_correction_first_written_goal_is_preserved`
   - The old name encoded the buggy last-writer-wins semantic
   - Final assertion flipped: now expects `"first goal"` (not `"second goal"`)
   - `assert_ne` flipped to confirm the later goal is NOT returned

2. **ADDED** `test_multi_session_null_start_preserves_original_goal` (T-V16-15)
   - Exact reproduction of GH #468
   - Inserts `cycle_start` with `goal = Some("original goal")` at timestamp 1700000001
   - Inserts second `cycle_start` with `goal = None` at timestamp 1700000002
   - Asserts `get_cycle_start_goal` returns `Some("original goal")`

## Test Results

```
cargo test -p unimatrix-store --features test-support
```

- 377 tests across all integration test suites
- 0 failures
- Both new/renamed tests confirmed passing:
  - `test_goal_correction_first_written_goal_is_preserved ... ok`
  - `test_multi_session_null_start_preserves_original_goal ... ok`

`cargo clippy -p unimatrix-store --features test-support -- -D warnings`: zero warnings.

## Files Modified

- `crates/unimatrix-store/src/db.rs`
- `crates/unimatrix-store/tests/migration_v15_to_v16.rs`

## New Tests

- `test_goal_correction_first_written_goal_is_preserved` (renamed + assertion flipped from old T-V16-14)
- `test_multi_session_null_start_preserves_original_goal` (new T-V16-15, exact bug reproduction)

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — not called (bug fix with fully specified root cause and fix in spawn prompt; no ambiguity requiring briefing)
- Searched: `mcp__unimatrix__context_search` for existing lessons on this root cause — found entries #3958 and #3959 already covering this bug and test-splitting pattern
- Stored: nothing novel — lessons #3958 and #3959 already capture both the ordering-semantics bug and the test-rename pattern for this fix
