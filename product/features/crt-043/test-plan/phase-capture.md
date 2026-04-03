# Test Plan: phase-capture

Component covers: `ObservationRow` struct extension and all four write sites in `dispatch_request` in `crates/unimatrix-server/src/uds/listener.rs`, plus `insert_observation` / `insert_observations_batch` SQL changes.

---

## Component Responsibilities Under Test

1. `ObservationRow` has a `phase: Option<String>` field.
2. At each of the four write sites, `phase` is captured from `session_registry.get_state(session_id).and_then(|s| s.current_phase.clone())` **before** the `spawn_blocking` closure, not inside it.
3. `insert_observation` binds `phase` in the SQL INSERT statement.
4. `insert_observations_batch` binds `phase` for each row in the batch INSERT.
5. When `get_state` returns `None` (session unknown), `phase` is `None` and the row is written with `phase IS NULL`.

---

## Four Write Sites

| Site | Dispatch path | Existing closure type |
|------|--------------|----------------------|
| 1. RecordEvent | `dispatch_request` → RecordEvent arm | `spawn_blocking_fire_and_forget` |
| 2. rework_candidate | `post_tool_use_rework_candidate` path | `tokio::task::spawn_blocking` with JoinHandle |
| 3. RecordEvents batch | RecordEvents arm → `insert_observations_batch` | `spawn_blocking_fire_and_forget` |
| 4. ContextSearch | ContextSearch arm → `insert_observation` | (verify path during implementation) |

Each site is an **independent integration point** — missing phase capture at any one of them is R-03. Tests must independently exercise each site.

---

## Unit Test Expectations

### PHASE-U-01: RecordEvent write site captures phase (R-03, AC-09, AC-10)

```rust
#[tokio::test]
async fn test_phase_captured_record_event_site() {
    // Arrange: fresh store + session registry with session "sess-001" having current_phase = "design".
    // Arrange: construct a RecordEvent ImplantEvent for "sess-001".
    // Act: invoke dispatch_request (or the RecordEvent arm directly).
    // Synchronize: await spawn_blocking completion.
    // Assert: SELECT phase FROM observations WHERE session_id = 'sess-001' ORDER BY id DESC LIMIT 1
    //         returns 'design'.
}
```

---

### PHASE-U-02: rework_candidate write site captures phase (R-03, AC-09)

```rust
#[tokio::test]
async fn test_phase_captured_rework_candidate_site() {
    // Arrange: session with current_phase = "implementation".
    // Arrange: construct event that triggers the rework_candidate observation path.
    // Act: invoke the rework_candidate path.
    // Synchronize: await the JoinHandle.
    // Assert: read back row; assert phase = 'implementation'.
}
```

---

### PHASE-U-03: RecordEvents batch write site captures phase (R-03, AC-09)

```rust
#[tokio::test]
async fn test_phase_captured_record_events_batch_site() {
    // Arrange: session with current_phase = "validation".
    // Arrange: construct a RecordEvents event with 3 sub-events.
    // Act: invoke dispatch_request (RecordEvents arm).
    // Synchronize: await batch spawn_blocking completion.
    // Assert: SELECT phase FROM observations WHERE session_id = ? ORDER BY id ASC
    //         returns 3 rows all with phase = 'validation'.
}
```

---

### PHASE-U-04: ContextSearch write site captures phase (R-03, AC-09)

```rust
#[tokio::test]
async fn test_phase_captured_context_search_site() {
    // Arrange: session with current_phase = "analysis".
    // Arrange: construct a ContextSearch ImplantEvent for that session.
    // Act: invoke the ContextSearch observation write path.
    // Synchronize: await spawn completion.
    // Assert: read back observation row; assert phase = 'analysis'.
}
```

Note: If the ContextSearch path does not always write an observation row (conditional path), the test must exercise the condition under which a row is written.

---

### PHASE-U-05: Observation with no active cycle produces NULL phase (R-03, AC-10b)

```rust
#[tokio::test]
async fn test_phase_null_when_no_active_cycle() {
    // Arrange: fresh session registry with no session registered for "sess-new".
    // Arrange: RecordEvent for "sess-new".
    // Act: invoke RecordEvent path.
    // Synchronize: await completion.
    // Assert: SELECT phase FROM observations WHERE session_id = 'sess-new' LIMIT 1
    //         returns NULL (not an error, not an empty string).
}
```

This covers the session registry miss path (R-03 edge case: `get_state` returns `None`).

---

### PHASE-U-06: Phase captured at call time, not at write time (R-04, AC-10a)

**This is the mandatory timing test for R-04.**

```rust
#[tokio::test]
async fn test_phase_capture_timing_pre_spawn() {
    // Arrange: session "sess-timing" with current_phase = "design".
    // Act part 1: call the RecordEvent observation path (which captures phase before spawn_blocking).
    // Immediately after the call (before await): change current_phase to "delivery" on the session.
    // Synchronize: await spawn_blocking completion.
    // Assert: observation row has phase = 'design' (the value at capture time, not 'delivery').
}
```

This test validates the pre-capture contract: the closure captures `phase` by move from the outer scope, not `session_registry`. If phase were captured inside spawn_blocking, it would be `None` or `delivery` after the concurrent update. The expected result is `design`.

---

### PHASE-U-07: insert_observation binds phase correctly (AC-09)

```rust
#[tokio::test]
async fn test_insert_observation_binds_phase() {
    // Arrange: build ObservationRow directly with phase = Some("delivery".to_string()).
    // Act: call insert_observation(&store, &obs) directly (synchronous call in spawn_blocking context).
    // Assert: SELECT phase FROM observations LIMIT 1 returns 'delivery'.
}
```

Tests the SQL binding layer independently of the dispatch path. Validates that `phase` is in the INSERT column list and bound at the correct position.

---

### PHASE-U-08: insert_observations_batch binds phase per row (AC-09)

```rust
#[tokio::test]
async fn test_insert_observations_batch_binds_phase() {
    // Arrange: 3 ObservationRows with phases: Some("design"), None, Some("validation").
    // Act: call insert_observations_batch(&store, &obs_batch) directly.
    // Assert: 3 rows written; phases are 'design', NULL, 'validation' in order.
}
```

---

### PHASE-U-09: Empty string phase vs NULL (edge case)

```rust
#[tokio::test]
async fn test_phase_empty_string_stored_as_empty_not_null() {
    // Arrange: session with current_phase = Some("") (if set_current_phase allows empty strings).
    // Assert: row written with phase = '' (empty string), not NULL.
    // Note: This tests the as-is storage behavior. If set_current_phase rejects empty strings,
    //       this test verifies that path returns an error and no observation is written with phase = ''.
    // The delivery agent must clarify and document this edge case.
}
```

---

## Integration Test Expectation

No new infra-001 integration tests are planned for phase-capture beyond the lifecycle regression suite and smoke gate. The `phase` column is an internal write-path column with no MCP-visible retrieval in crt-043.

The existing `test_lifecycle.py::test_cycle_lifecycle_full_flow` exercises the full cycle start/phase_end/stop sequence through MCP, which incidentally exercises the `handle_cycle_event` paths. That test does not assert the `phase` column but it does validate the overall server health.

---

## Acceptance Criteria Traceability

| AC-ID | Covered By | Test |
|-------|-----------|------|
| AC-07 | schema-migration.md MIG-V21-U-03 | `test_v20_to_v21_both_columns_present` (phase column on observations) |
| AC-08 | grep / compile | `grep 'phase: Option<String>' listener.rs` |
| AC-09 | PHASE-U-01..U-04, U-07, U-08 | Per-site DB read-back tests + direct insert_observation/batch tests |
| AC-10a | PHASE-U-06 | Timing test: captured before spawn_blocking |
| AC-10b | PHASE-U-05 | NULL phase when no active session |

---

## Code Review Assertions (Static)

1. `ObservationRow` struct has `phase: Option<String>` field — no other fields renamed or removed.
2. At **each of the four write sites** in `dispatch_request`, the phase capture line appears before the `spawn_blocking` / `spawn_blocking_fire_and_forget` call:
   ```rust
   let phase = session_registry.get_state(&event.session_id)
       .and_then(|s| s.current_phase.clone());
   ```
   The closure captures `phase` by move, not `session_registry`.
3. `insert_observation` SQL includes `phase` in the column list with a bound parameter at the correct position (after `topic_signal`).
4. `insert_observations_batch` binds `phase` for each row in the iteration.
5. Neither `insert_observation` nor `insert_observations_batch` has a default fallback that silently coerces `None` to an empty string — `None` must produce SQL `NULL`.
