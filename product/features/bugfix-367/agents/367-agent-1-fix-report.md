# Agent Report: 367-agent-1-fix

**Feature**: bugfix-367 — dead_knowledge_deprecation_pass window too narrow
**Branch**: bugfix/367-dead-knowledge-threshold

## Changes Made

### `/crates/unimatrix-server/src/background.rs`

1. Line 881: `DEAD_KNOWLEDGE_SESSION_THRESHOLD` changed from `20` to `1000`
2. Line 1001: `detect_dead_knowledge_candidates(..., 5)` changed to `detect_dead_knowledge_candidates(..., DEAD_KNOWLEDGE_SESSION_THRESHOLD)`
3. Line 3480–3481 (test): Updated `insert_synthetic_sessions(&store, 6)` to `insert_synthetic_sessions(&store, DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1)` to satisfy the minimum-sessions guard under the new threshold

## Test Results

- **1908 passed; 0 failed** (full workspace)
- One test failed initially (`test_dead_knowledge_deprecation_pass_unit`) — it hardcoded 6 sessions which was above the old window of 5 but below the new 1000. Fixed by using `DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1`.
- One test appeared flaky during concurrent run (`col018_context_search_creates_observation`) — confirmed pre-existing by passing in isolation both before and after the changes. Not caused by our fix.
- Clippy errors are pre-existing (in `unimatrix-store`, unrelated to our file).

## New Tests

none — existing test updated, no new test functions added

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `unimatrix-server` — no results returned (non-blocking)
- Stored: nothing novel to store — the fix is a straightforward constant change. The one non-obvious finding (that `detect_dead_knowledge_candidates` uses its `window` argument as both a minimum-sessions guard AND a "recent" definition, meaning fetch window and detection window must match) is already captured in the in-code comment at the call site.
