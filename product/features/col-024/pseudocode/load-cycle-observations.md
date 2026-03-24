# Component: SqlObservationSource::load_cycle_observations + cycle_ts_to_obs_millis
# File: crates/unimatrix-server/src/services/observation.rs

## Purpose

Implement the `load_cycle_observations` method on `SqlObservationSource`. This is the
primary observation attribution path for col-024: it uses `cycle_events` timestamps to
derive time windows, then discovers sessions via `topic_signal` match, then loads
observation records filtered to those windows.

Also introduces the `cycle_ts_to_obs_millis` module-private helper that enforces the
unit conversion contract (ADR-002).

Both live in the same file as the existing `ObservationSource` impl, after the closing
brace of the existing `observation_stats` method and before `block_sync`.

## New/Modified Functions

### `cycle_ts_to_obs_millis` (new private helper)

```
// Module-private, placed above or below block_sync (not inside the impl block).
// Must appear before load_cycle_observations which calls it.

/// Convert a cycle_events.timestamp (Unix epoch seconds) to the millisecond
/// unit used by observations.ts_millis.
///
/// cycle_events.timestamp is written by unix_now_secs() (i64, seconds).
/// observations.ts_millis is (unix_now_secs() as i64).saturating_mul(1000) (i64, ms).
/// Both tables use the same epoch; this bridges the unit difference.
///
/// saturating_mul guards against i64 overflow on adversarially large timestamps
/// (E-05 edge case: i64::MAX as input saturates to i64::MAX, not panic).
#[inline]
fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64 {
    ts_secs.saturating_mul(1000)
}
```

### `SqlObservationSource::load_cycle_observations` (new impl method)

This is added inside the `impl ObservationSource for SqlObservationSource` block, after
the existing `observation_stats` method.

```
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>> {
    // Obtain the write pool. All three steps share this connection (ADR-001).
    // write_pool_server() returns &SqlitePool; max_connections=1 (S-04 known limitation).
    let pool = self.store.write_pool_server();

    // cycle_id must be captured for use inside the async closure.
    // It is &str from the caller; clone to owned String for 'static move.
    let cycle_id = cycle_id.to_string();

    // Single block_sync entry (ADR-001). All three steps are awaited sequentially
    // inside this one async block. No nested block_sync. No per-step block_sync.
    block_sync(async move {

        // ---- Step 0: Count-only pre-check ----
        // Distinguishes "no cycle_events rows" (AC-15 case A) from
        // "rows exist but no match" (AC-15 case B). Both return Ok(vec![]).
        // This is diagnostic only — both cases produce Ok(vec![]) to the caller.
        let count_row = sqlx::query(
                "SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?1"
            )
            .bind(&cycle_id)
            .fetch_one(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let row_count: i64 = count_row.get::<i64, _>(0);

        if row_count == 0 {
            // No cycle_events rows — pre-col-024 feature or unknown cycle_id.
            // Return empty; caller will activate legacy fallback (FM-01).
            return Ok(vec![]);
        }

        // ---- Step 1: Fetch event rows and pair into time windows ----
        // Fetch (event_type, timestamp) for this cycle_id ordered chronologically.
        // seq is advisory tie-breaker within the same second.
        let event_rows = sqlx::query(
                "SELECT event_type, timestamp \
                 FROM cycle_events \
                 WHERE cycle_id = ?1 \
                 ORDER BY timestamp ASC, seq ASC"
            )
            .bind(&cycle_id)
            .fetch_all(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        // Pair rows into (start_ms, stop_ms) windows.
        // Window pairing algorithm:
        //   - Iterate rows in order.
        //   - A cycle_start row opens a pending window (start_ts = row.timestamp).
        //   - A cycle_stop row closes the most-recently opened pending window.
        //   - cycle_phase_end rows are ignored for windowing (E-02).
        //   - At end of rows: any unclosed pending start becomes an open-ended window
        //     with stop = unix_now_secs() (ADR-005).
        //   - Malformed sequence (two cycle_start with no intervening cycle_stop):
        //     the second cycle_start opens a new pending window, the first pending
        //     start is closed at the second start's timestamp (defensive: prevents
        //     infinitely-open windows from duplicate starts, E-03).

        let mut windows: Vec<(i64, i64)> = Vec::new();  // (start_ms, stop_ms)
        let mut pending_start: Option<i64> = None;       // seconds, not yet converted

        for row in &event_rows {
            let event_type: &str = row.get::<String, _>(0).as_str();
            // Note: get returns owned String; use a local binding.
            // Rewrite without as_str() borrow:
            let event_type: String = row.get::<String, _>(0);
            let ts_secs: i64 = row.get::<i64, _>(1);

            match event_type.as_str() {
                "cycle_start" => {
                    if let Some(prev_start) = pending_start {
                        // Malformed: second cycle_start without cycle_stop (E-03).
                        // Close the previous pending start at this start's timestamp.
                        windows.push((
                            cycle_ts_to_obs_millis(prev_start),
                            cycle_ts_to_obs_millis(ts_secs),
                        ));
                    }
                    pending_start = Some(ts_secs);
                }
                "cycle_stop" => {
                    if let Some(start) = pending_start.take() {
                        windows.push((
                            cycle_ts_to_obs_millis(start),
                            cycle_ts_to_obs_millis(ts_secs),
                        ));
                    }
                    // cycle_stop with no pending start: ignore (defensive).
                }
                _ => {
                    // cycle_phase_end or any unknown event_type: ignore for windowing (E-02).
                }
            }
        }

        // Close any open-ended window (ADR-005).
        // KNOWN LIMITATION (ADR-005): an abandoned cycle (cycle_start, no cycle_stop)
        // will include all observations with matching topic_signal up to the present.
        // This is accepted behavior; no max-age cap is applied.
        if let Some(start) = pending_start {
            let now_secs: i64 = unix_now_secs() as i64;
            windows.push((
                cycle_ts_to_obs_millis(start),
                cycle_ts_to_obs_millis(now_secs),
            ));
        }

        if windows.is_empty() {
            // cycle_events rows exist (row_count > 0) but no parseable windows.
            // E.g., only cycle_phase_end rows. Return empty; legacy fallback activates.
            return Ok(vec![]);
        }

        // ---- Step 2: Per-window session discovery ----
        // For each window, SELECT DISTINCT session_id WHERE topic_signal = cycle_id
        // AND ts_millis BETWEEN start_ms AND stop_ms.
        // Union all results; deduplicate via HashSet to prevent duplicate Step 3 queries (R-11).
        let mut session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (start_ms, stop_ms) in &windows {
            // NOTE: No raw * 1000 here — start_ms and stop_ms are already in millis
            // from cycle_ts_to_obs_millis. Binding them directly is correct.
            let sid_rows = sqlx::query(
                    "SELECT DISTINCT session_id \
                     FROM observations \
                     WHERE topic_signal = ?1 \
                       AND ts_millis >= ?2 \
                       AND ts_millis <= ?3"
                )
                .bind(&cycle_id)
                .bind(start_ms)
                .bind(stop_ms)
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            for row in sid_rows {
                session_ids.insert(row.get::<String, _>(0));
            }
        }

        if session_ids.is_empty() {
            // cycle_events rows exist and windows are valid, but no observations carry
            // topic_signal = cycle_id within any window.
            // Enrichment may not have been active when these observations were written.
            // Return empty; legacy fallback activates (AC-15 case B).
            return Ok(vec![]);
        }

        // ---- Step 3: Load observations for discovered sessions ----
        // SQL bounds: use [min_window_start_ms, max_window_stop_ms] to narrow the scan
        // to the combined window range. The timestamp index (idx_observations_ts) applies.
        // Rust post-filter retains only records inside at least one (start_ms, stop_ms) pair.

        let min_ms: i64 = windows.iter().map(|(s, _)| *s).min().expect("windows non-empty");
        let max_ms: i64 = windows.iter().map(|(_, e)| *e).max().expect("windows non-empty");

        // Build parameterized IN clause for session_ids.
        // Pattern follows existing load_feature_observations and load_unattributed_sessions.
        let sid_vec: Vec<String> = session_ids.into_iter().collect();
        let placeholders: String = sid_vec
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 3))   // ?1=min_ms, ?2=max_ms in this query variant
            .collect::<Vec<_>>()
            .join(",");

        // Note on parameter binding order:
        // The SQL uses positional params ?1 and ?2 for ts_millis bounds, then ?3..?N for
        // session IDs. The actual query construction must match the bind order exactly.
        // Prefer building the query so that ts_millis bounds come AFTER session IDs in the
        // WHERE clause but are bound first — OR use a consistent ordering. Implementation
        // agent should follow the existing pattern in load_feature_observations (bind IDs
        // first, then reuse the same approach). The specific binding index scheme is
        // implementation detail; what matters is correctness.
        //
        // Suggested SQL (7-column shape matching parse_observation_rows):
        let sql = format!(
            "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet \
             FROM observations \
             WHERE session_id IN ({placeholders}) \
               AND ts_millis >= ?1 \
               AND ts_millis <= ?2 \
             ORDER BY ts_millis ASC",
            placeholders = placeholders
        );

        // Bind min_ms as ?1, max_ms as ?2, then each session_id.
        // (Adjust placeholder indices in the format string to match — implementation
        //  agent should reconcile the ?1/?2 for bounds with ?3..?N for IDs.)
        // Simplified final form: bind session_ids as ?1..?N, then bounds as ?N+1 and ?N+2;
        // adjust SQL accordingly. Follow the load_feature_observations pattern exactly
        // (bind IDs via a loop, append bound params for timestamp range).
        //
        // RECOMMENDED: use two separate .bind() chains:
        //   let mut q = sqlx::query(&sql).bind(min_ms).bind(max_ms);
        //   for sid in &sid_vec { q = q.bind(sid); }
        // And adjust SQL so ?1=min_ms, ?2=max_ms, ?3..?N=session_ids.
        // The implementation agent must reconcile the exact parameter index scheme.

        let mut q = sqlx::query(&sql).bind(min_ms).bind(max_ms);
        for sid in &sid_vec {
            q = q.bind(sid);
        }

        let rows = q
            .fetch_all(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        // Parse via parse_observation_rows (NFR-05 — security bounds apply: 64 KB input
        // limit, JSON depth check). The 7-column SELECT shape is required.
        let all_records = parse_observation_rows(rows, &self.registry)?;

        // Rust post-filter: retain records inside at least one window (R-09).
        // The SQL [min_ms, max_ms] bound may include gap-period observations from
        // multi-window cycles; this filter removes them.
        let filtered: Vec<ObservationRecord> = all_records
            .into_iter()
            .filter(|rec| {
                let ts = rec.ts as i64;   // rec.ts is u64 (ms); cast to i64 for comparison
                windows.iter().any(|(start, stop)| ts >= *start && ts <= *stop)
            })
            .collect();

        Ok(filtered)
    })
}
```

## State Machines

None. This is a stateless pure function on each invocation. All state is in the database
and the session registry (the latter is not read here).

## Initialization Sequence

`SqlObservationSource` is already initialized by its `new()` constructor. No new
initialization needed for this method. The `self.store.write_pool_server()` call obtains
a reference to the already-opened SQLite pool.

## Data Flow

```
Input:
  &self            -- SqlObservationSource (has store: Arc<SqlxStore>, registry: Arc<DomainPackRegistry>)
  cycle_id: &str   -- e.g. "col-024"

Internal:
  pool             -- &SqlitePool from write_pool_server()
  row_count        -- i64 from COUNT(*) pre-check
  event_rows       -- Vec of (event_type: String, timestamp: i64) from cycle_events
  windows          -- Vec<(i64, i64)> in milliseconds, derived via cycle_ts_to_obs_millis
  session_ids      -- HashSet<String> from per-window DISTINCT query
  all_records      -- Vec<ObservationRecord> from parse_observation_rows
  filtered         -- Vec<ObservationRecord> after Rust window-filter

Output:
  Ok(vec![])                -- no rows, no match, or no parseable windows
  Ok(Vec<ObservationRecord>) -- records inside at least one window, sorted by ts_millis ASC
  Err(ObserveError::Database(msg)) -- SQL failure
```

## Error Handling

| Failure Point | Behavior |
|---------------|----------|
| COUNT(*) query fails | `?` propagates `ObserveError::Database` to `block_sync` return; caller receives `Err` |
| cycle_events fetch fails | Same — `?` propagates `Err` |
| per-window session discovery query fails | Same — `?` propagates inside the loop |
| Step 3 observation load query fails | Same — `?` propagates |
| `parse_observation_rows` rejects a record | Record is skipped with `tracing::warn!` inside parser; not an error |
| Empty result at any valid point | `Ok(vec![])` returned; NOT an error |

The caller (`context_cycle_review`) must check: if `Err`, propagate to MCP error.
If `Ok(vec![])`, activate legacy fallback (not an error path).

## cycle_ts_to_obs_millis Edge Cases

- `ts_secs = 0` returns `0` (UNIX epoch). No harm.
- `ts_secs = i64::MAX` returns `i64::MAX` via saturating_mul (E-05). Window boundary
  clamped; no panic.
- `ts_secs < 0` returns a negative value. This should not happen since `unix_now_secs()`
  never produces negative values and cycle_events.timestamp is always written by it.

## Window Pairing Edge Cases

| Scenario | Behavior |
|----------|----------|
| Single cycle_start, no cycle_stop (E-01, open-ended) | One window: (start_ms, unix_now_secs_ms) |
| cycle_start + cycle_phase_end + cycle_stop (E-02) | One window: (start_ms, stop_ms); phase_end ignored |
| Two cycle_start with no cycle_stop between (E-03) | First start closed at second start's ts; second start open-ended or closed at eventual cycle_stop |
| cycle_stop with no pending start | Ignored defensively |
| Only cycle_phase_end rows | windows remains empty; Ok(vec![]) returned |

## Key Test Scenarios

Reference: RISK-TEST-STRATEGY.md and ARCHITECTURE.md test table.

| Test Name | Covers | Setup |
|-----------|--------|-------|
| `load_cycle_observations_single_window` | AC-01, R-01 | insert_cycle_event(start at T, stop at T+3600); insert observation at T+60s with topic_signal="col-024"; assert returned |
| `load_cycle_observations_outside_window_excluded` | AC-01, R-01 | Same setup; insert observation at T-1s; assert NOT returned |
| `load_cycle_observations_multiple_windows` | AC-02, R-09, R-11 | Two windows (T, T+1h) and (T+3h, T+4h); observations at T+30m (in), T+2h (gap, excluded), T+3h30m (in); assert exactly 2 records |
| `load_cycle_observations_no_cycle_events` | AC-03, R-07 | No insert_cycle_event; assert Ok(vec![]) not Err |
| `load_cycle_observations_open_ended_window` | ADR-005, R-06 | cycle_start at T, no stop; observation at T+60s; assert returned |
| `load_cycle_observations_count_precheck_distinguishes_empty_cases` | AC-15 | cycle_events rows inserted but no matching topic_signal in observations; assert Ok(vec![]) (rows exist, no match) |
| `load_cycle_observations_saturating_mul_no_panic` | E-05 | insert_cycle_event with timestamp=i64::MAX/1000; assert no panic |
| `load_cycle_observations_phase_end_ignored` | E-02 | insert cycle_start, cycle_phase_end, cycle_stop; assert one window, not split |
| `load_cycle_observations_parse_observation_rows_security_bounds` | R-10, NFR-05 | insert observation with input > 64 KB; assert record excluded (not panicked) |
| `load_cycle_observations_multi_window_no_duplication` | R-11 | single session with observations in both windows; assert each observation appears once |
| `load_cycle_observations_inside_tokio_test` | R-05, NFR-01 | multi-window test run inside #[tokio::test]; assert no double-block_in_place panic |

## Constraints

- No raw `* 1000` literal in this file's window-boundary code. Only `cycle_ts_to_obs_millis` (AC-13).
- Single `block_sync(async { ... })` containing all four steps (Step 0 through Step 3) (NFR-01, ADR-001).
- `parse_observation_rows` called with the 7-column SELECT shape exactly (NFR-05).
- `insert_cycle_event` test API used for all test fixture writes to cycle_events (Constraint 3).
- No schema change (NFR-02). No new index (NFR-03).
- Known limitation: write pool held for the duration of all three steps (S-04).
