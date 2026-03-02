# Test Plan: signal-dispatch

## Component Scope

`crates/unimatrix-server/src/uds_listener.rs`, `crates/unimatrix-server/src/server.rs`
(PendingEntriesAnalysis, process_session_close, run_confidence_consumer, run_retrospective_consumer)

## Unit Tests

### PendingEntriesAnalysis (R-07)

**`test_pending_entries_analysis_upsert_new`**
- Empty PendingEntriesAnalysis
- upsert(EntryAnalysis { entry_id: 1, rework_flag_count: 1, ... })
- Assert: entries.len() == 1, entries[1].rework_flag_count == 1

**`test_pending_entries_analysis_upsert_existing`**
- upsert entry_id=1 with rework_flag_count=1, rework_session_count=1
- upsert entry_id=1 again with rework_flag_count=1, rework_session_count=1
- Assert: entries[1].rework_flag_count == 2, rework_session_count == 2 (additive)

**`test_pending_entries_analysis_cap_at_1000`** (AC related to R-07)
- Insert 1001 entries with distinct entry_ids (rework_flag_count=1 for each)
- Assert: entries.len() == 1000

**`test_pending_entries_analysis_cap_drops_lowest_rework_count`** (R-07 scenario 2)
- Insert 999 entries with rework_flag_count=5
- Insert 1 entry with rework_flag_count=1 (entry_id=999)
- Insert 1 more entry (triggers cap) — entry_id=999 (rework_flag_count=1) should be dropped
- Assert: entry_id=999 absent

**`test_pending_entries_analysis_drain_all`**
- Insert 5 entries
- drain_all() returns 5 entries
- entries.len() == 0 after drain

**`test_pending_entries_analysis_drain_empty`**
- drain_all() on empty struct → empty Vec, no error

### run_confidence_consumer (AC-04, AC-12, FR-06.2b)

**`test_run_confidence_consumer_increments_helpful_count`** (AC-04)
- Setup: test db with 2 entries (entry_id=1, entry_id=2)
- Insert Helpful signal for entry_ids [1, 2]
- Run run_confidence_consumer
- Fetch entries 1 and 2, assert helpful_count incremented by 1 each

**`test_run_confidence_consumer_updates_success_session_count`** (FR-06.2b)
- Setup: test db with entry_id=1
- Insert Helpful signal for entry_ids [1]
- Run run_confidence_consumer with PendingEntriesAnalysis
- Assert: entries_analysis[1].success_session_count == 1

**`test_run_confidence_consumer_deduplicates_entry_ids`**
- Insert 2 Helpful signals both containing entry_id=1
- Run consumer
- Assert: helpful_count incremented only once for entry_id=1 (dedup across signals)

**`test_run_confidence_consumer_skips_missing_entry`** (AC-11 related, R-11)
- Insert Helpful signal for entry_id=999 (does not exist in db)
- Run run_confidence_consumer
- Assert: no panic, no crash, warning logged (check tracing or mock)

**`test_run_confidence_consumer_performance`** (AC-12, NFR-01.2)
- Insert Helpful signal for 50 entry_ids (all entries pre-created)
- Time run_confidence_consumer
- Assert: duration < 100ms

**`test_run_confidence_consumer_drain_failure_no_crash`** (FR-05.3)
- Simulate drain_signals returning an error (use mock store or closed db)
- run_confidence_consumer should return without panicking

### run_retrospective_consumer (R-07, FR-06)

**`test_run_retrospective_consumer_creates_entry_analysis`**
- Insert Flagged signal for entry_ids [1, 2] where entries exist in db
- Run run_retrospective_consumer
- Assert: PendingEntriesAnalysis has entries for 1 and 2 with rework_flag_count=1, rework_session_count=1

**`test_run_retrospective_consumer_increments_existing`**
- Pre-populate PendingEntriesAnalysis with entry_id=1, rework_flag_count=2
- Insert new Flagged signal for entry_id=1
- Run consumer
- Assert: rework_flag_count == 3

**`test_run_retrospective_consumer_missing_entry_uses_empty_strings`**
- Flagged signal for entry_id=9999 (not in db)
- Run consumer
- Assert: entry_id=9999 in PendingEntriesAnalysis with title="" (graceful)

**`test_run_retrospective_consumer_empty_signals_noop`**
- No Flagged signals in queue
- Run consumer
- Assert: PendingEntriesAnalysis unchanged

### process_session_close integration (AC-02, AC-06)

**`test_process_session_close_success_generates_helpful_signal`**
- register_session("s1")
- record_injection("s1", [(1, 0.9), (2, 0.8)])
- process_session_close("s1", "success")
- Assert: store has 0 remaining signals (consumer ran and drained)
- Assert: entries 1 and 2 have helpful_count incremented

**`test_process_session_close_rework_generates_flagged_signal`**
- Setup session crossing rework threshold
- process_session_close("s1", "success")  // server overrides to Rework
- Assert: helpful_count NOT incremented
- Assert: PendingEntriesAnalysis has entries with rework_flag_count > 0

**`test_process_session_close_no_signals_when_no_injections`**
- Session with no injections
- process_session_close("s1", "success")
- Assert: SIGNAL_QUEUE remains empty, no consumer errors

**`test_process_session_close_stale_sweep_runs_first`** (FR-09.1)
- Register session "s1" (stale) with injection, Register session "s2" (current)
- Set s1.last_activity_at to stale
- process_session_close("s2", "success")
- Assert: s1 was swept (signals generated for s1 too), s2 signals generated

**`test_rework_candidate_dispatch_to_session_registry`**
- Dispatch RecordEvent { event_type: "post_tool_use_rework_candidate", payload: {tool_name: "Edit", file_path: "foo.rs", had_failure: false} }
- Assert: session_registry.get_state(session_id).rework_events.len() == 1

## Integration Tests (MCP-visible behavior)

These require the full server binary and are validated by the Python test harness.

**`test_retrospective_entries_analysis_present`** (AC-07)
- Via MCP: store an entry, simulate a rework session via hook events, call context_retrospective
- Assert: response includes "entries_analysis" key with entry data

**`test_retrospective_entries_analysis_absent_when_no_flagged_signals`** (AC-13)
- Fresh server with no signals
- Call context_retrospective
- Assert: response JSON does NOT contain "entries_analysis" key

## Edge Cases

- Consumer runs when SIGNAL_QUEUE is empty → no error
- Both consumers run sequentially → no deadlock, no race
- PendingEntriesAnalysis is Arc<Mutex<...>> — concurrent access from UDS listener and MCP handler
