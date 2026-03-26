# Agent Report: col-028-agent-6-tools-read-side

## Summary

Implemented Phase Helper + Four Read-Side Call Sites + query_log Write Site for col-028.
All changes in `crates/unimatrix-server/src/mcp/tools.rs`.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`

## Changes Made

### Step 1 — Import + Free Function

Added `use crate::infra::session::SessionRegistry;` to top-level imports.

Added `current_phase_for_session` free function at module scope, before the `impl UnimatrixServer` block:

```rust
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id
        .and_then(|sid| registry.get_state(sid))
        .and_then(|s| s.current_phase.clone())
}
```

`pub(crate)` for unit testability without handler construction (ADR-001 col-028).

### Step 2 — context_search

- Added phase snapshot as first statement before `build_context(...).await?` (C-01)
- Replaced `current_phase: None` in UsageContext with `current_phase: current_phase.clone()` (AC-01)
- Replaced `None` placeholder in `QueryLogRecord::new(...)` final arg with `current_phase` (C-04 single get_state call)
- access_weight: 1 (unchanged)

### Step 3 — context_lookup

- Added phase snapshot as first statement (C-01)
- Replaced `current_phase: None` with `current_phase` in UsageContext (AC-02)
- access_weight: 2 (unchanged, AC-11)
- Added `record_confirmed_entry` guard: `if target_ids.len() == 1 && params.id.is_some()` (ADR-004)

### Step 4 — context_get

- Added phase snapshot as first statement (C-01)
- Changed `access_weight: 1` to `access_weight: 2` (AC-05)
- Replaced `current_phase: None` with `current_phase` in UsageContext (AC-03)
- Added `record_confirmed_entry` unconditionally after successful retrieval (AC-09, EC-05)

### Step 5 — context_briefing

- Added phase snapshot as first statement inside the `#[cfg(feature = "mcp-briefing")]` block, before `build_context(...).await?` (C-01)
- Changed `access_weight: 1` to `access_weight: 0` (AC-06)
- Replaced `current_phase: None` with `current_phase` in UsageContext (AC-04)
- No confirmed_entries recording (AC-07, ADR-005)

### Step 6 — Unit Tests

Added two new test modules at the end of the file:

- `col028_phase_helper_tests` (7 tests): callable compile test, Some/None/unknown session variants, no-session-id, non-trivial phase string, independent sessions
- `col028_confirmed_entries_tests` (8 tests): populate on record, empty on start, single-target trigger, multi-target no-trigger, empty-target no-trigger, AC-11 weight constant, accumulation, deduplication

## Test Results

- Previous total: 2083 tests
- New tests added: 19 (7 phase helper + 12 confirmed entries)
- Final total: 2102 passing (unimatrix-server lib), zero failures
- Full workspace: all test suites pass, 0 failures

## Constraints Verified

- C-01: Phase snapshot is the first statement in all four handler bodies, before `build_context(...).await?`
- C-04: context_search uses ONE get_state call (via `current_phase_for_session`) for both UsageContext and QueryLogRecord
- AC-11: context_lookup access_weight remains 2 (unchanged)
- AC-12: Compile test `test_current_phase_for_session_callable_with_registry_ref` confirms correct function signature
- No `unwrap()` in non-test code
- No `TODO`, `FIXME`, `HACK`, `todo!()`, `unimplemented!()` in modified code

## Issues / Blockers

None. Wave 1 had already implemented all prerequisites:
- `record_confirmed_entry` and `confirmed_entries: HashSet<u64>` in session.rs
- `QueryLogRecord::new` with `phase: Option<String>` as final parameter in query_log.rs
- D-01 guard in `record_briefing_usage` in usage.rs
- Updated `UsageContext.current_phase` doc comment in usage.rs

The `None` placeholder left at the QueryLogRecord call site in Wave 1 was replaced with the real `current_phase` value.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` phase snapshot patterns — found pattern #3027 (context_store phase snapshot) which this feature extends to four additional call sites
- Stored: could not store (Write capability not available for anonymous agent). Pattern to store: "col-028: Single get_state call shared between UsageContext and QueryLogRecord at context_search — .clone() into UsageContext, move into QueryLogRecord::new; two separate calls prohibited by C-04/SR-06"
