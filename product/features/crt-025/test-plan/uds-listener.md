# Test Plan: UDS Listener (Component 5)

File: `crates/unimatrix-server/src/uds/listener.rs`
Risks: R-01 (Critical — primary coverage), R-07, AC-05, AC-07, AC-08

---

## Unit Test Expectations

The UDS listener dispatch function can be tested with a real `SessionRegistry` and a mock
or in-memory `SqlxStore`. Focus: that `set_current_phase` is called before the spawn, and
that the phase transition table is correctly implemented.

### Phase Transition Logic per Event Type (FR-05, Architecture Component 5)

**`test_listener_cycle_start_with_next_phase_sets_session_phase`** (FR-05.2, R-01)
- Arrange: `SessionRegistry` with a registered session, `current_phase = None`
- Act: dispatch a `HookRequest::RecordEvent { event_type: "cycle_start", payload: {next_phase: "scope"} }`
  through the listener handler's synchronous section
- Assert: BEFORE the spawned DB task runs, `session.current_phase == Some("scope")`
- Assert: `insert_cycle_event` is called (fire-and-forget)

**`test_listener_cycle_start_without_next_phase_no_phase_change`** (FR-05.2 edge)
- Act: dispatch `cycle_start` with no `next_phase` in payload
- Assert: `session.current_phase` remains `None`

**`test_listener_cycle_phase_end_with_next_phase_updates_phase`** (FR-05.3, R-01)
- Arrange: `current_phase = Some("scope")`
- Act: dispatch `cycle_phase_end` with `next_phase = "design"`
- Assert: `current_phase == Some("design")` immediately after the synchronous section
- Assert: DB write is fire-and-forget (spawned task, not blocking the handler)

**`test_listener_cycle_phase_end_without_next_phase_no_change`** (FR-05.3 edge)
- Arrange: `current_phase = Some("scope")`
- Act: dispatch `cycle_phase_end` with no `next_phase`
- Assert: `current_phase` still `Some("scope")`

**`test_listener_cycle_stop_clears_phase`** (FR-05.4)
- Arrange: `current_phase = Some("testing")`
- Act: dispatch `cycle_stop`
- Assert: `current_phase == None` synchronously

### R-01: Synchronous-Before-Spawn Ordering

**`test_listener_phase_mutation_before_db_spawn`** (R-01 Critical)

This is the most important test in Component 5. The design guarantee is that `set_current_phase`
executes in the handler's synchronous task code, before any `spawn_blocking` call for the DB write.

- Arrange: use a `BlockingStore` mock that records when `insert_cycle_event` is first called
- Act: dispatch `cycle_phase_end` with `next_phase = "implementation"`
- Assert: at the moment `insert_cycle_event` is invoked by the spawned task, the in-memory
  `SessionState.current_phase` is already `Some("implementation")` — not None or "scope"

If a direct mock is not feasible, the causal test is:
- Dispatch `cycle_phase_end` to set `current_phase = "design"`
- Immediately (in the same tokio task, no yield) check `session.current_phase`
- Assert it is already updated — the `spawn_blocking` for the DB has not yet run but the
  in-memory state is already updated

### `seq` Computation (R-07, AC-08)

**`test_listener_seq_monotonic_three_events`** (AC-08)
- Arrange: dispatch three `cycle_start`/`cycle_phase_end`/`cycle_stop` events for the same
  `cycle_id` using the full dispatch path (not direct DB insert)
- After all three events: query `cycle_events WHERE cycle_id = ?`
- Assert: three rows exist with `seq` values `{0, 1, 2}` in `ORDER BY timestamp ASC, seq ASC`

**`test_listener_seq_advisory_does_not_crash_on_concurrent`** (R-07)
- Arrange: two sessions for the same `cycle_id`, each dispatching a `cycle_phase_end` event
  concurrently (using `tokio::join!`)
- Assert: both inserts succeed (no error); rows have distinct `id` values (AUTOINCREMENT)
- Assert: no panic; `(timestamp, seq)` ordering is valid even with seq collision

---

## Integration Test Expectations

The UDS listener is exercised end-to-end through:

1. **Server-level integration tests** (R-01 causal chain) — these call the full server including
   the UDS listener's dispatch logic
2. **infra-001 lifecycle suite** — `test_phase_tag_store_cycle_review_flow` exercises the full
   UDS path via MCP protocol

### Critical R-01 Integration Test (server-level)

**`test_phase_end_then_store_sees_new_phase`** (R-01 Critical, AC-05)
- Arrange: fresh server with a registered session
- Act:
  1. Send `context_cycle(type="start", topic="t", next_phase="scope")` via full MCP path
  2. Immediately send `context_store(content="...", topic="t", category="decision", agent_id="test")`
- Assert: query `feature_entries WHERE entry_id = <stored_id>`
  → `phase = "scope"` (not NULL)
- This verifies that the synchronous mutation completed before `context_store` executed

This test CANNOT pass if `set_current_phase` is queued behind any async dispatch. If it were
queued, the store would see `phase = None`.

**`test_stop_then_store_sees_null_phase`** (R-01, AC-07)
- Act:
  1. `context_cycle(type="start", next_phase="scope")`
  2. `context_cycle(type="stop")`
  3. `context_store(...)`
- Assert: `feature_entries.phase IS NULL`

---

## Assertions Summary

- `set_current_phase` call is in the synchronous section of the dispatch handler (before any `spawn`)
- All three event types (`cycle_start`, `cycle_phase_end`, `cycle_stop`) are handled in the dispatch
- `cycle_phase_end` without `next_phase` does NOT clear `current_phase` (keeps previous value)
- `cycle_stop` always clears to `None`
- Fire-and-forget DB write does not block the handler's return
