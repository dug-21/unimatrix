# col-024 Test Plan: load_cycle_observations + cycle_ts_to_obs_millis
# File: `crates/unimatrix-server/src/services/observation.rs`

## Component Summary

Two new items in `services/observation.rs`:

1. `SqlObservationSource::load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>`
   — implements the `ObservationSource` trait method. Three steps inside one `block_sync`.
2. `cycle_ts_to_obs_millis(ts_secs: i64) -> i64` — module-private conversion helper using
   `saturating_mul(1000)`.

All unit tests live in the existing `#[cfg(test)]` block at the bottom of `observation.rs`.

---

## Test Helper Requirements

The existing `insert_observation` helper in the test module omits the `topic_signal` column.
A new version (or an extended version) is required:

```rust
async fn insert_observation_with_signal(
    store: &SqlxStore,
    session_id: &str,
    ts_millis: i64,
    hook: &str,
    topic_signal: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO observations \
         (session_id, ts_millis, hook, topic_signal) \
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(session_id).bind(ts_millis).bind(hook).bind(topic_signal)
    .execute(store.write_pool_server())
    .await.expect("insert observation with signal");
}
```

Constraint (SPEC 3): `cycle_events` rows must be inserted using `SqlxStore::insert_cycle_event`,
NOT raw SQL. Signature:

```rust
store.insert_cycle_event(
    cycle_id: &str,
    seq: i64,
    event_type: &str,        // "cycle_start" | "cycle_stop" | "cycle_phase_end"
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,          // Unix epoch SECONDS
).await.expect("insert cycle event");
```

All test timestamps: use fixed i64 constants (not `now()`) for determinism. Example:
- `T = 1_700_000_000_i64` (seconds for cycle_events)
- `T_MS = T * 1000` (milliseconds for observations ts_millis)

---

## Unit Test Expectations

### T-LCO-01: `load_cycle_observations_single_window`

**AC**: AC-01, AC-11, R-01 (positive inclusion test)
**Setup**:
- `insert_cycle_event("col-024", 0, "cycle_start", None, None, None, T)` where `T = 1_700_000_000`
- `insert_cycle_event("col-024", 1, "cycle_stop", None, None, None, T + 3600)`
- `insert_observation_with_signal("sess-1", T_MS + 60_000, "PostToolUse", Some("col-024"))`
- `insert_observation_with_signal("sess-1", T_MS - 1_000, "PostToolUse", Some("col-024"))` (before window — must be excluded)

**Assertions**:
- `let source = SqlObservationSource::new_default(Arc::clone(&store));`
- `let records = source.load_cycle_observations("col-024").unwrap();`
- `assert_eq!(records.len(), 1)` — only the in-window observation
- `assert_eq!(records[0].session_id, "sess-1")`
- `assert_eq!(records[0].ts_millis, T_MS + 60_000)` — verify timestamp field populated

**Why**: If `cycle_ts_to_obs_millis` is absent (raw `* 1000` missing), the window boundary is
`T` (not `T * 1000`), and the in-window observation at `T_MS + 60_000 = T*1000 + 60_000` is
far above the boundary — it would be included spuriously. The `before window` observation at
`T_MS - 1_000` tests the lower boundary.

---

### T-LCO-02: `load_cycle_observations_multiple_windows`

**AC**: AC-02, R-09 (Rust window-filter), R-11 (deduplication)
**Setup**:
- Window 1: `cycle_start` at `T`, `cycle_stop` at `T + 3600`
- Window 2: `cycle_start` at `T + 7200`, `cycle_stop` at `T + 10800`
- `insert_observation_with_signal("sess-1", T_MS + 1800_000, ...)` — window 1, in
- `insert_observation_with_signal("sess-1", T_MS + 5400_000, ...)` — gap (T+4500s), must be excluded
- `insert_observation_with_signal("sess-2", T_MS + 9000_000, ...)` — window 2, in
- All three have `topic_signal = "col-024"`

**Assertions**:
- `assert_eq!(records.len(), 2)` — exactly two observations, not three
- `records` contains observations at `T_MS + 1800_000` and `T_MS + 9000_000`
- `records` does NOT contain observation at `T_MS + 5400_000`
- `assert_eq!(records.len(), 2)` — not 4 (deduplication: sess-1 is not queried twice)

**Why**: Tests R-09 (Rust window-filter for gap exclusion) and R-11 (session-ID deduplication
preventing double-counting for sessions that span multiple windows).

---

### T-LCO-03: `load_cycle_observations_no_cycle_events`

**AC**: AC-03, R-07
**Setup**: Empty `cycle_events` table. No `insert_cycle_event` calls.

**Assertions**:
- `let result = source.load_cycle_observations("col-024");`
- `assert!(result.is_ok(), "must not return Err for missing cycle_events rows")`
- `assert_eq!(result.unwrap(), vec![], "must return Ok(vec![])")`

**Why**: If the implementation uses `?` on a "no rows" result (which is not an error in sqlx),
this test catches it. Validates FM-01 partial: missing cycle_events must not error.

---

### T-LCO-04: `load_cycle_observations_no_cycle_events_count_check`

**AC**: AC-15 (first case)
**Setup**: No `cycle_events` rows for `"col-024"`.

**Assertions**:
- `let result = source.load_cycle_observations("col-024");`
- `assert!(result.is_ok())`
- `assert!(result.unwrap().is_empty())`

**Notes**: This test is identical in outcome to T-LCO-03 but is a distinct test for the
AC-15 count pre-check distinction. The code review (Stage 3c) must verify the implementation
performs a `SELECT COUNT(*)` before Step 1 and returns early when count = 0. The unit test
cannot directly assert this internal behavior, but the Stage 3c tester must confirm it via
code inspection (search for `COUNT` in the `load_cycle_observations` body).

---

### T-LCO-05: `load_cycle_observations_rows_exist_no_signal_match`

**AC**: AC-15 (second case)
**Setup**:
- `insert_cycle_event("col-024", 0, "cycle_start", ..., T)` and `insert_cycle_event("col-024", 1, "cycle_stop", ..., T + 3600)`
- `insert_observation_with_signal("sess-1", T_MS + 1800_000, "PostToolUse", None)` — no `topic_signal`

**Assertions**:
- `let result = source.load_cycle_observations("col-024");`
- `assert!(result.is_ok())`
- `assert!(result.unwrap().is_empty(), "rows exist but no topic_signal match → Ok(vec![])")`

**Why**: Distinguishes "no cycle_events rows" (Step 0 short-circuit) from "rows exist but Step 2
finds no sessions" (falls through all steps to `Ok(vec![])`). Both return `Ok(vec![])` to the
caller; the fallback log (AC-14 / ADR-003) differentiates them at the `context_cycle_review` level.

---

### T-LCO-06: `load_cycle_observations_open_ended_window`

**AC**: R-06, ADR-005 documentation
**Setup**:
- `insert_cycle_event("col-024", 0, "cycle_start", ..., T)` — no stop event
- `insert_observation_with_signal("sess-1", T_MS + 60_000, "PostToolUse", Some("col-024"))`
  — in window (T to now)

**Assertions**:
- `let records = source.load_cycle_observations("col-024").unwrap();`
- `assert!(!records.is_empty(), "observation after cycle_start must be in open-ended window")`
- `assert_eq!(records[0].session_id, "sess-1")`

**Notes**: The stop boundary is `unix_now_secs()`. The test timestamp must be recent enough
(e.g., within the last hour) or use a dynamic `T` derived from `SystemTime::now() - 300s`.

---

### T-LCO-07: `load_cycle_observations_excludes_outside_window`

**AC**: R-01 (boundary exclusion), AC-01 implicit
**Setup**:
- Window: `T` to `T + 3600`
- `insert_observation_with_signal("sess-1", T_MS - 1, "PostToolUse", Some("col-024"))` — 1ms before start
- `insert_observation_with_signal("sess-1", T_MS + 3_600_001, "PostToolUse", Some("col-024"))` — 1ms after stop

**Assertions**:
- `assert_eq!(records.len(), 0)` — both observations excluded

---

### T-LCO-08: `load_cycle_observations_phase_end_events_ignored`

**AC**: E-02 (edge case)
**Setup**:
- `insert_cycle_event("col-024", 0, "cycle_start", ..., T)`
- `insert_cycle_event("col-024", 1, "cycle_phase_end", ..., T + 1800)` — between start and stop
- `insert_cycle_event("col-024", 2, "cycle_stop", ..., T + 3600)`
- `insert_observation_with_signal("sess-1", T_MS + 900_000, "PostToolUse", Some("col-024"))`

**Assertions**:
- `assert_eq!(records.len(), 1)` — one window `(T, T+3600)`, observation is in it
- `assert_eq!(records[0].session_id, "sess-1")`

**Why**: Confirms `cycle_phase_end` does not split the window or open an extra window.

---

### T-LCO-09: `load_cycle_observations_saturating_mul_overflow_guard`

**AC**: E-05
**Setup**:
- `insert_cycle_event("col-024", 0, "cycle_start", ..., i64::MAX)` — adversarial timestamp
- `insert_cycle_event("col-024", 1, "cycle_stop", ..., i64::MAX)` — same for stop
- No observations

**Assertions**:
- `let result = source.load_cycle_observations("col-024");`
- `assert!(result.is_ok(), "must not panic on i64::MAX timestamp")`
- `assert!(result.unwrap().is_empty())`

**Why**: Without `saturating_mul`, `i64::MAX * 1000` overflows. If the implementation uses
`ts_secs * 1000` directly, this test panics or produces an error. `saturating_mul` clamps to
`i64::MAX`, which is a valid (if extreme) boundary.

---

### T-LCO-10: `load_cycle_observations_empty_cycle_id`

**AC**: E-06
**Setup**: No rows in any table.

**Assertions**:
- `let result = source.load_cycle_observations("");`
- `assert!(result.is_ok())`
- `assert!(result.unwrap().is_empty())`

---

### T-LCO-11: `cycle_ts_to_obs_millis_unit_test` (pure function)

**AC**: R-01 (helper correctness), AC-13 indirect
**Location**: In the `#[cfg(test)]` block, testing the private helper directly via `super::cycle_ts_to_obs_millis`

**Assertions**:
- `assert_eq!(cycle_ts_to_obs_millis(1_000), 1_000_000)`
- `assert_eq!(cycle_ts_to_obs_millis(0), 0)`
- `assert_eq!(cycle_ts_to_obs_millis(-1), -1000)`
- `assert_eq!(cycle_ts_to_obs_millis(i64::MAX), i64::MAX)` — saturating_mul clamps

---

## Integration Test Expectations

All tests use `#[tokio::test(flavor = "multi_thread")]` (matching existing tests in the file).
This satisfies I-02: the `block_in_place` branch of `block_sync` is exercised, not the
transient-runtime branch.

The multi-window test (T-LCO-02) specifically validates R-05 (no double `block_sync`) by
having the test succeed inside a `#[tokio::test]` context. If the implementation accidentally
calls `block_sync` twice, the test panics with "Cannot start a runtime from within a runtime."

---

## Code Review Gates (Stage 3c)

These assertions are verified by the Stage 3c tester via grep and code inspection, not by
running tests:

**AC-13**: No raw `* 1000` in the `load_cycle_observations` implementation block.
```bash
grep -n '\* 1000' crates/unimatrix-server/src/services/observation.rs
```
Must return zero matches within the `load_cycle_observations` function body.

**R-10**: `parse_observation_rows` is called on Step 3 results.
Verify via code inspection: the Step 3 `fetch_all` result is passed to `parse_observation_rows`.

**NFR-01**: Exactly one `block_sync(async { ... })` call exists in `load_cycle_observations`.
Verify via code inspection: no nested `block_sync` inside the closure.

**S-01**: `cycle_id` is bound as a parameter (`?1`), never interpolated via `format!`.
```bash
grep -n 'format!' crates/unimatrix-server/src/services/observation.rs
```
Must return zero matches inside `load_cycle_observations`.
