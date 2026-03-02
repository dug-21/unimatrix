# Test Plan: session-signals

## Component Scope

`crates/unimatrix-server/src/session.rs` — new fields, new types, new methods

## Unit Tests

### SessionState Initialization (FR-03)

**`test_register_session_initializes_new_fields`**
- register_session("s1", None, None)
- get_state("s1")
- Assert: signaled_entries.is_empty(), rework_events.is_empty(), agent_actions.is_empty()
- Assert: last_activity_at > 0 (initialized to current time)

### record_rework_event (FR-03.2, R-08)

**`test_record_rework_event_appends`**
- register_session("s1")
- record_rework_event("s1", ReworkEvent { tool_name: "Edit", file_path: Some("foo.rs"), had_failure: false, timestamp: 1000 })
- get_state("s1") → assert rework_events.len() == 1

**`test_record_rework_event_updates_last_activity_at`** (R-08)
- register_session with current time
- record_rework_event with timestamp = current_time + 100
- Assert: last_activity_at == current_time + 100

**`test_record_rework_event_silent_noop_if_unregistered`**
- record_rework_event on unknown session_id
- No panic, no error

### record_agent_action (FR-03.3)

**`test_record_agent_action_appends`**
- register_session("s1")
- record_agent_action("s1", SessionAction { entry_id: 42, action: ExplicitUnhelpful, timestamp: 1000 })
- get_state("s1") → agent_actions.len() == 1

**`test_record_agent_action_silent_noop_if_unregistered`**
- record_agent_action on unknown session_id — no panic

### has_crossed_rework_threshold (R-03, ADR-002)

**`test_rework_threshold_zero_events`**
- Empty rework_events → false

**`test_rework_threshold_one_edit_no_failure`**
- Single Edit event, no Bash failure → false

**`test_rework_threshold_two_cycles`** (R-03 scenario 1)
- Edit(foo.rs) → Bash(fail) → Edit(foo.rs) → Bash(fail) → Edit(foo.rs)
- Count = 2 cycles (3 edits, 2 failure-separated pairs)
- Assert: false (threshold is 3)

**`test_rework_threshold_three_cycles`** (AC-08, R-03 scenario 2)
- Edit(foo.rs) → Bash(fail) → Edit(foo.rs) → Bash(fail) → Edit(foo.rs) → Bash(fail) → Edit(foo.rs)
- Count = 3 cycles (4 edits, 3 failure-separated pairs)
- Assert: true

**`test_rework_threshold_rapid_edits_no_failure`** (R-03 scenario 3)
- Edit(foo.rs) × 5, no Bash failures between them
- Assert: false (rapid multi-edit not rework per ADR-002)

**`test_rework_threshold_different_files`** (R-03 scenario 4)
- Edit(a.rs) → Bash(fail) → Edit(a.rs) → Bash(fail) → Edit(a.rs): 3 cycles on a.rs → true
- BUT: Edit(a.rs) → Bash(fail) → Edit(b.rs) → Bash(fail) → Edit(c.rs): 0 cycles per file → false

**`test_rework_threshold_edit_bash_fail_edit_pattern`** (R-03 scenario 5)
- Edit(foo.rs) → Bash(fail) → Edit(foo.rs): 1 cycle
- Assert: false (only 1 cycle, need 3)

**`test_rework_threshold_bash_fail_then_edit`**
- Bash(fail) → Edit(foo.rs) → Bash(fail) → Edit(foo.rs) → Bash(fail) → Edit(foo.rs)
- The Bash failure BEFORE the first edit doesn't count
- Assert: 2 failure-separated edit pairs = 2 cycles → false

**`test_rework_threshold_multiedit_per_path`**
- MultiEdit with paths [foo.rs, bar.rs] → Bash(fail) → MultiEdit [foo.rs, bar.rs] → Bash(fail) → MultiEdit [foo.rs, bar.rs]
- foo.rs: 3 cycles → true

### drain_and_signal_session (AC-02, AC-03, AC-05, AC-06, R-01, ADR-003)

**`test_drain_and_signal_success_session`** (AC-02)
- register_session("s1")
- record_injection("s1", [(1, 0.9), (2, 0.8), (3, 0.7)])
- drain_and_signal_session("s1", "success")
- Assert: Some(output) where helpful_entry_ids == [1,2,3], final_outcome == Success

**`test_drain_and_signal_removes_session`**
- After drain_and_signal_session, get_state("s1") returns None

**`test_drain_and_signal_idempotent`** (AC-03, R-01)
- drain_and_signal_session("s1", "success") → Some(...)
- drain_and_signal_session("s1", "success") → None (session already removed)

**`test_drain_and_signal_abandoned_empty_outcome`** (AC-05)
- Session with 3 injections, outcome=""
- drain_and_signal_session("s1", "")
- Assert: Some(output) with helpful_entry_ids=[], flagged_entry_ids=[], final_outcome == Abandoned

**`test_drain_and_signal_rework_override`** (AC-06)
- populate rework_events crossing threshold (3+ edit-fail-edit cycles on same file)
- drain_and_signal_session("s1", "success")
- Assert: final_outcome == Rework, flagged_entry_ids == entry_ids, helpful_entry_ids.is_empty()

**`test_drain_and_signal_explicit_unhelpful_excluded`** (AC-06, R-04)
- 3 injected entries (1, 2, 3)
- record_agent_action(entry_id=2, ExplicitUnhelpful)
- drain_and_signal_session("s1", "success")
- Assert: helpful_entry_ids == [1, 3] (2 excluded)

**`test_drain_and_signal_all_explicit_unhelpful`** (R-04 scenario 2)
- All 3 entries have ExplicitUnhelpful
- drain_and_signal_session → helpful_entry_ids.is_empty()

**`test_drain_and_signal_explicit_helpful_not_excluded`** (R-04 scenario 3)
- Entry_id=1 has ExplicitHelpful action
- drain_and_signal_session → entry_id=1 still in helpful_entry_ids (not excluded)

**`test_drain_and_signal_no_injections`** (R-13 scenario 1)
- Session with empty injection_history
- drain_and_signal_session("s1", "success")
- Assert: Some(output) with helpful_entry_ids.is_empty()

### sweep_stale_sessions (AC-09, R-08)

**`test_stale_session_sweep_evicts_old`** (AC-09)
- register_session("s1")
- Manually set last_activity_at = now - (4*3600 + 1)
- sweep_stale_sessions()
- Assert: session removed from registry; get_state("s1") == None
- Assert: sweep result contains ("s1", output) with non-empty helpful_entry_ids (if injections present)

**`test_stale_session_keeps_recent`** (R-08 scenario 1)
- register_session("s1")
- Set last_activity_at = now - (3*3600)
- sweep_stale_sessions()
- Assert: session still present (3h < 4h threshold)

**`test_stale_session_exact_boundary`** (R-08 scenario 2)
- Set last_activity_at = now - 4*3600 (exactly at boundary)
- Assert: swept (>= threshold means stale)

**`test_stale_session_no_injections_silent_eviction`** (FR-09.4, R-13)
- Stale session with empty injection_history
- sweep_stale_sessions()
- Assert: session removed, but NOT in output Vec (silent eviction)

**`test_stale_session_last_activity_updated_by_rework`** (R-08 scenario 5)
- Session registered 5h ago; rework_event timestamp = 1h ago
- last_activity_at should be 1h ago (not 5h ago)
- sweep_stale_sessions() → NOT swept (1h < 4h threshold)

### last_activity_at tracking (R-08)

**`test_last_activity_at_initialized_at_registration`**
- register_session → last_activity_at > 0

**`test_record_injection_updates_last_activity_at`** (FR-03.4)
- register_session at time T
- Advance time, record_injection
- Assert: last_activity_at >= T (updated)

## Edge Cases

- Empty session (no injections, no rework): drain returns Some(output) with empty lists, not None
- Session_id not registered: record_rework_event, record_agent_action, drain_and_signal_session all handle gracefully
