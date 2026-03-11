# Agent Report: crt-018-agent-4-effectiveness-store

## Files Modified

- `crates/unimatrix-store/src/read.rs`

## Changes

### Structs Added
- `EffectivenessAggregates` -- raw effectiveness data returned by `compute_effectiveness_aggregates()`, with raw scalar fields (session_count, earliest_session_at, latest_session_at) instead of DataWindow to avoid store->engine dependency.
- `EntryInjectionStats` -- per-entry injection and outcome counts.
- `EntryClassificationMeta` -- entry metadata for classification (title, topic, trust_source, helpful/unhelpful counts).

### Methods Added to `Store` impl
1. `compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates>` -- 4 SQL queries under one `lock_conn()`:
   - Query 1: Entry injection stats with deduplication via `SELECT DISTINCT entry_id, session_id FROM injection_log` subquery
   - Query 2: Active topics (distinct non-NULL, non-empty feature_cycle from sessions)
   - Query 3: Calibration rows (per-injection confidence + outcome boolean)
   - Query 4: Data window (session count, min/max started_at)

2. `load_entry_classification_meta(&self) -> Result<Vec<EntryClassificationMeta>>` -- loads active entries (status=0) with NULL/empty topic mapped to "(unattributed)" in SQL.

### Deviation from Pseudocode SQL
The pseudocode Query 1 uses `COUNT(DISTINCT il.session_id)` for injection_count but counts success/rework/abandoned per-row with SUM CASE. This causes duplicate inflation when multiple injection_log records exist for the same (entry_id, session_id) pair -- outcome counts would be inflated by the number of injection records per session. Fixed by using a `SELECT DISTINCT entry_id, session_id FROM injection_log` subquery so all counts operate on deduplicated (entry, session) pairs. This aligns with the test plan's S-01 expectation.

## Test Results

20 tests pass, 0 fail (17 new effectiveness tests + 3 existing tests):
- S-01: COUNT DISTINCT session deduplication
- S-02: Multiple distinct sessions counted correctly
- S-03: Sessions with NULL outcome excluded
- S-04: Multiple entries with mixed outcomes
- S-05: NULL feature_cycle excluded from active_topics
- S-06: Empty string feature_cycle excluded
- S-07: Multiple distinct feature_cycles
- S-08: NULL feature_cycle session still contributes to injection stats
- S-09: Calibration rows include all injection records
- S-10: Data window from sessions with outcomes
- S-12: Active entries only
- S-13: NULL/empty topic mapped to "(unattributed)"
- S-14: Fields correctly populated
- S-15: Entry with no helpful/unhelpful counts
- S-16: Empty database returns empty aggregates
- S-17: Empty entry_classification_meta on empty DB
- S-18: Performance at scale (500 entries, 10K injections, <500ms)

S-11 is a code review check (single lock_conn scope) -- verified by inspection.

Full crate: 111 tests pass (103 lib + 8 integration), 0 failures.

## Issues

1. **Workspace build failure**: `unimatrix-engine` has `pub mod effectiveness;` in lib.rs but the file doesn't exist yet (engine agent's responsibility). The store crate builds and tests independently.

2. **File length**: `read.rs` is 1653 lines (1069 non-test). It was already well over 500 lines before this work. The architecture spec explicitly directs adding methods here. Splitting is deferred.
