# Agent Report: 391-agent-1-fix

**Feature**: col-025 (bugfix — set_current_goal None guard)
**Bug**: fix(col-025): set_current_goal should not overwrite existing goal with None

## Changes Made

### File Modified
`crates/unimatrix-server/src/uds/listener.rs`

**Change 1 — Guard at call site (~L2461):**
Wrapped `session_registry.set_current_goal(...)` with `if goal.is_some() { ... }` so the write only fires when the incoming payload contains a goal. A `cycle_start` with no `goal` key now leaves the session's existing goal intact.

**Change 2 — Doc comment (~L6348):**
Updated the test doc comment from "set_current_goal is unconditional: a cycle_start without a goal key always yields None" to the correct "set_current_goal is guarded: a cycle_start without a goal key does not overwrite a previously set goal."

**Change 3 — Test assertion (~L6411):**
Flipped `assert_eq!(state_after_second.current_goal, None, ...)` to `assert_eq!(state_after_second.current_goal.as_deref(), Some("existing goal"), ...)` to verify the preserved goal, not the (now-wrong) cleared state. Inline comment at L6388 also updated.

## Tests

- `cargo test -p unimatrix-server` — all pass, no failures
- `test_cycle_start_missing_goal_does_not_overwrite_existing` now correctly asserts goal preservation
- Clippy errors present are all pre-existing in `unimatrix-engine` and `unimatrix-observe` — zero errors in `crates/unimatrix-server`

## Commit

`f290fb3` — `fix(col-025): guard set_current_goal so cycle_start without goal preserves existing goal`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — not invoked (no novel pattern search needed; the fix is a straightforward guard matching an existing pattern already present in `set_current_phase`)
- Stored: nothing novel to store — the `if Some` guard pattern for session state writes is already well-established in the codebase and visible in `set_current_phase`. The bug was a missing application of a known pattern, not a new discovery.
