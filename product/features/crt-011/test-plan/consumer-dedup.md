# crt-011: Test Plan — consumer-dedup

## Unit Tests for Consumer Dedup Fixes

### T-CON-01: test_confidence_consumer_dedup_same_session

**Target:** `run_confidence_consumer`
**Scenario:** 2 Helpful signals with same session_id, both containing the same entry_id
**Setup:**
1. Create store, pending, entry_store via existing helpers
2. Insert a test entry to get a valid entry_id
3. Insert 2 SignalRecords: both session_id="sess-A", entry_ids=[entry_id], type=Helpful
4. Call run_confidence_consumer

**Assertions:**
- `pending.entries[entry_id].success_session_count == 1` (not 2)
- Entry exists in pending (was fetched)

**Risk covered:** R-01 (three-pass race)

### T-CON-02: test_confidence_consumer_different_sessions_count_separately

**Target:** `run_confidence_consumer`
**Scenario:** 2 Helpful signals with different session_ids, both containing the same entry_id
**Setup:**
1. Same as T-CON-01 but signal_1 has session_id="sess-A", signal_2 has session_id="sess-B"

**Assertions:**
- `pending.entries[entry_id].success_session_count == 2` (each session counts once)

**Risk covered:** R-01, R-04 (multi-session correctness)

### T-CON-03: test_retrospective_consumer_rework_session_dedup

**Target:** `run_retrospective_consumer`
**Scenario:** 2 Flagged signals with same session_id, both containing the same entry_id
**Setup:**
1. Create store, pending, entry_store
2. Insert test entry
3. Insert 2 SignalRecords: both session_id="sess-A", entry_ids=[entry_id], type=Flagged
4. Call run_retrospective_consumer

**Assertions:**
- `pending.entries[entry_id].rework_session_count == 1` (deduped)
- `pending.entries[entry_id].rework_flag_count == 2` (NOT deduped, per ADR-002)

**Risk covered:** R-03 (semantic distinction)

### T-CON-04: test_retrospective_consumer_flag_count_not_deduped

**Target:** `run_retrospective_consumer`
**Scenario:** 3 Flagged signals with same session_id, all containing the same entry_id
**Setup:**
1. Same as T-CON-03 but with 3 signals

**Assertions:**
- `pending.entries[entry_id].rework_flag_count == 3` (every event counted)
- `pending.entries[entry_id].rework_session_count == 1` (only one unique session)

**Risk covered:** R-03 (explicitly tests the NOT-deduped counter)
