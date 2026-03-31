# Agent Report: 458-agent-1-fix

**Feature**: bugfix-458
**Task**: Fix orphaned-edge compaction to delete GRAPH_EDGES rows for quarantined entries (status=3)

## Changes Made

### Files Modified

- `crates/unimatrix-server/src/background.rs`

  **Change 1** — Production compaction SQL (run_single_tick, ~line 519):
  Replaced `NOT IN (SELECT id FROM entries)` with `NOT IN (SELECT id FROM entries WHERE status != ?1)` and added `.bind(Status::Quarantined as u8 as i64)`.

  **Change 2** — Test helper SQL (run_graph_edges_compaction, ~line 2934):
  Same SQL change as Change 1. Added mirror comment:
  `// IMPORTANT: this SQL must remain identical to the production DELETE in run_single_tick. If you change one, change both.`

  **Change 3** — Two new tests added after existing compaction tests:
  - `test_background_tick_compaction_removes_quarantined_source_edges`
  - `test_background_tick_compaction_removes_quarantined_target_edges`

## Branch

`bugfix/458-orphaned-edge-quarantine-compaction`

## New Tests

- `test_background_tick_compaction_removes_quarantined_source_edges` — inserts active entry A (id=20) and quarantined entry Q (id=21, status=3), inserts Q→A edges (CoAccess + Supports), runs compaction, asserts both edges deleted and A's own edges preserved
- `test_background_tick_compaction_removes_quarantined_target_edges` — inserts active entry A (id=30) and quarantined entry Q (id=31, status=3), inserts A→Q edges (CoAccess + Supports), runs compaction, asserts both edges deleted

## Test Results

- `cargo test -p unimatrix-server`: **2605 passed, 0 failed** (across all test binaries)
- All 6 compaction tests pass (including 2 new)

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3906 and #3907 (pre-existing knowledge for this exact bug, confirming approved approach). Also returned #3762 and #3883 (related compaction/tick patterns).
- Stored: entry #3908 "GRAPH_EDGES compaction DELETE must filter quarantined entries by status, not just existence" via `/uni-store-pattern` — captures the trap that `NOT IN (SELECT id FROM entries)` silently fails for quarantined entries since they remain in the table, plus the test-mirror comment convention.
