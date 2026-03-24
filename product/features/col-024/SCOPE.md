# col-024: Cycle-Events-First Observation Lookup and Topic Signal Enrichment

## Problem Statement

`context_cycle_review` uses `sessions.feature_cycle` as its primary source of truth for
discovering which observations belong to a feature cycle. This path is unreliable: the
`sessions.feature_cycle` column is populated asynchronously (fire-and-forget) and can be
NULL if the cycle_start event races with session writes, if the server restarts mid-session,
or if a session was never explicitly attributed. When the fast path returns empty, the
existing fallback scans all NULL-feature_cycle sessions for content-based attribution —
but `sessions.feature_cycle` may also be stale (set to a prior feature) or entirely absent
from the table, causing observations to be silently missed.

A parallel problem exists at write time: `extract_topic_signal(input)` only fires when the
hook event's input text contains a recognizable feature ID pattern. Post-cycle observations
(e.g., tool calls issued after `context_cycle(start)` but before text mentioning the feature)
receive no `topic_signal`, making them invisible to the new cycle-events-first lookup path.

Both problems share a root cause: attribution relies on asynchronous or content-derived
side-effects rather than the authoritative cycle_events table, which records the definitive
start/stop timestamps for each cycle_id written synchronously.

## Goals

1. Redesign `context_cycle_review` to use `cycle_events` timestamps as the primary
   observation lookup path, replacing the `sessions.feature_cycle` fast path.
2. Enrich `topic_signal` at write time in `listener.rs` by falling back to the
   session registry's in-memory `feature` when `extract_topic_signal(input)` returns
   `None`, so observations written after `context_cycle(start)` carry attribution.
3. Preserve full backward compatibility: features predating cycle_events (bugfix-277,
   bugfix-323, etc.) must fall back to the existing sessions-table + content-attribution
   path.
4. Add a new `load_cycle_observations(cycle_id)` method to `ObservationSource` that
   implements the cycle-events-first algorithm.
5. Extend tests in `services/observation.rs` to cover the new primary path and
   time-window filtering logic.

## Non-Goals

- No changes to the `cycle_events` schema — it already has `cycle_id`, `event_type`, and
  `timestamp`; no `session_id` column will be added.
- No changes to `ObservationRecord` in `unimatrix-core` — `topic_signal` is a storage-level
  detail; the downstream pipeline receives the same type.
- No changes to the detection rules, metrics pipeline, or report format.
- No backfill of `topic_signal` for historical observations.
- No changes to the `sessions.feature_cycle` column or its write path.
- No index additions for `observations.topic_signal` beyond verifying that the two-step
  query (via time windows, then session IDs) uses existing indexes efficiently.
- The `topic_signal` enrichment fallback is only for the UDS listener paths, not for tests
  or batch-import paths.

## Background Research

### cycle_events schema (confirmed)

`cycle_events` columns: `id` (AUTOINCREMENT PK), `cycle_id` TEXT, `seq` INTEGER,
`event_type` TEXT (`cycle_start` / `cycle_phase_end` / `cycle_stop`), `phase` TEXT,
`outcome` TEXT, `next_phase` TEXT, `timestamp` INTEGER NOT NULL.

**No `session_id` column.** Session identification must go through
`observations.topic_signal = cycle_id` within the `(start, stop)` time windows.

Index: `idx_cycle_events_cycle_id ON cycle_events(cycle_id)`.

`seq` is advisory (ADR-002 crt-025); true ordering uses `ORDER BY timestamp ASC, seq ASC`.

### observations table (confirmed)

Columns: `id`, `session_id`, `ts_millis`, `hook`, `tool`, `input`, `response_size`,
`response_snippet`, `topic_signal`.

Indexes: `idx_observations_session ON observations(session_id)`,
`idx_observations_ts ON observations(ts_millis)`.

No index on `topic_signal`. The new query plan (step 2: session discovery via
`topic_signal = cycle_id AND ts_millis BETWEEN start AND stop`) will do a scan of
time-window rows, bounded by the timestamp index and further filtered on topic_signal.
This is acceptable given typical cycle durations.

`topic_signal` is NOT currently selected in `load_feature_observations` SQL — the SELECT
lists 7 columns (`session_id, ts_millis, hook, tool, input, response_size, response_snippet`)
and passes them to `parse_observation_rows`. The new method will not need to return
`topic_signal` to callers since `ObservationRecord` does not carry it; the column is only
used for session identification at query time.

### Current load path in `context_cycle_review` (tools.rs line 1217)

```
Fast path:   load_feature_observations(feature_cycle)
             -> sessions WHERE feature_cycle = X  (unreliable)
             -> observations WHERE session_id IN (...)
Fallback:    load_unattributed_sessions()
             -> sessions WHERE feature_cycle IS NULL
             -> attribute_sessions(...) content scan
```

The fast path returns empty when:
- `sessions.feature_cycle` was never written (async persist raced or server restarted)
- `sessions.feature_cycle` is set to a different prior feature (session reuse)

When fast path returns empty AND the session has `feature_cycle` set to something other
than NULL, the fallback also misses it — sessions with any non-NULL feature_cycle are
excluded from `load_unattributed_sessions`.

### topic_signal write sites in listener.rs

Three observation write paths exist:

1. `RecordEvent` path (line 684): `extract_observation_fields(&event)` — uses
   `event.topic_signal` which comes from the hook wire protocol. No in-memory fallback.
2. Rework candidate path (line 592): same `extract_observation_fields(&event)` — same
   gap.
3. ContextSearch path (line 833): manually constructs `ObservationRow` with
   `topic_signal: topic_signal.clone()` where `topic_signal =
   extract_topic_signal(&query)`. No in-memory fallback.

The `session_registry.get_state(sid)?.feature` field is set synchronously by
`set_feature_force` (called from `handle_cycle_event` on `CYCLE_START_EVENT`) before
any downstream observation is written. This means the session registry always has the
correct `feature` value available at observation write time, even when
`extract_topic_signal(input)` returns None.

### ObservationSource trait (source.rs)

Current methods:
- `load_feature_observations(feature_cycle)` — sessions-table fast path
- `discover_sessions_for_feature(feature_cycle)` — sessions-table helper
- `load_unattributed_sessions()` — NULL-feature_cycle fallback
- `observation_stats()` — aggregate counts

The trait is implemented by `SqlObservationSource` in `services/observation.rs`.
The new method `load_cycle_observations(cycle_id)` belongs here, as it is the
new primary path that replaces the sessions fast path inside `context_cycle_review`.

### Legacy fallback requirement

Features with no `cycle_events` rows (all work predating crt-025 / schema v15):
`bugfix-277`, `bugfix-323`, `col-022`, and all earlier features. The legacy fallback
must remain intact: if `load_cycle_observations` returns empty due to no cycle_events
rows existing, `context_cycle_review` falls through to the existing
`load_feature_observations` + `load_unattributed_sessions` + `attribute_sessions` path.

### Test patterns (observation.rs)

Tests use `setup_test_store()` (tempfile + `open_test_store`) with direct `sqlx::query`
inserts via `write_pool_server()`. The same pattern applies for inserting
`cycle_events` rows — `store.insert_cycle_event(...)` is the available API on `SqlxStore`.
Tests call `source.load_feature_observations(...)` on `&dyn ObservationSource` to
validate via trait dispatch. New tests should follow the same pattern.

## Proposed Approach

### Change 1: topic_signal enrichment at write time

In `listener.rs`, at all three observation write sites, after `extract_topic_signal`
returns `None`, check `session_registry.get_state(&event.session_id)` and use
`state.feature` as the fallback `topic_signal`. This is a synchronous in-memory read
(Mutex lock for microseconds) already on the handler path.

The enrichment is conditional: only fill if the in-memory feature is non-None and the
extracted signal is None. Do not override an explicit extract_topic_signal result.

For the RecordEvent and RecordEvents paths, the enrichment happens before
`extract_observation_fields` builds the `ObservationRow`, or by passing the fallback
through the event's topic_signal. For the ContextSearch path, the enrichment replaces
the current `topic_signal: topic_signal.clone()` with the fallback when `topic_signal`
is None.

### Change 2: context_cycle_review redesign

Add `load_cycle_observations(cycle_id: &str)` to the `ObservationSource` trait and
implement it in `SqlObservationSource`:

**Step 1** — Query `cycle_events` for `(start, stop)` time windows:
```sql
SELECT event_type, timestamp
FROM cycle_events
WHERE cycle_id = ?1
ORDER BY timestamp ASC, seq ASC
```
Pair `cycle_start` events with the next `cycle_stop` event (or open-ended to now if no
stop exists) to produce a list of `(start_ts, stop_ts)` windows.

**Step 2** — Find confirmed session_ids via topic_signal:
```sql
SELECT DISTINCT session_id
FROM observations
WHERE topic_signal = ?1
  AND ts_millis >= ?2 AND ts_millis <= ?3
```
Run once per window; union results.

**Step 3** — Load all observations from those sessions within any window:
```sql
SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet
FROM observations
WHERE session_id IN (...)
  AND ts_millis >= [min_window_start] AND ts_millis <= [max_window_stop]
ORDER BY ts_millis ASC
```
Then filter in Rust to retain only records whose `ts_millis` falls within at least one
`(start, stop)` window (handles multiple disjoint windows for the same feature).

If `cycle_events` has no rows for `cycle_id`, return `Ok(vec![])` to signal the caller
to use the legacy fallback path.

**In `context_cycle_review` (tools.rs)**:
Replace the current fast path with:
```
Primary:  load_cycle_observations(feature_cycle)
          -> if non-empty: use as attributed
Legacy:   load_feature_observations(feature_cycle)  [sessions-table]
          -> if non-empty: use as attributed
Fallback: load_unattributed_sessions()
          -> attribute_sessions(...)
```

## Acceptance Criteria

- AC-01: When `cycle_events` contains `(cycle_start, cycle_stop)` rows for a
  `cycle_id`, `load_cycle_observations` returns all observations from sessions that
  have `topic_signal = cycle_id` AND `ts_millis` within any `(start, stop)` window.
- AC-02: When a feature has multiple `(start, stop)` windows (e.g., resumed session),
  observations from all windows are included and observations outside all windows are
  excluded.
- AC-03: When `cycle_events` has no rows for `cycle_id`, `load_cycle_observations`
  returns an empty vec (triggering legacy fallback in the caller).
- AC-04: `context_cycle_review` uses `load_cycle_observations` as the primary path;
  if it returns empty, it falls back to `load_feature_observations`; if that also
  returns empty, falls back to `load_unattributed_sessions` + `attribute_sessions`.
- AC-05: Observations written via the `RecordEvent` path receive `topic_signal` from
  the session registry's in-memory `feature` when `extract_topic_signal(input)` returns
  `None` and the session has a registered feature.
- AC-06: Observations written via the `ContextSearch` path receive `topic_signal` from
  the session registry's in-memory `feature` when `extract_topic_signal(query)` returns
  `None` and the session has a registered feature.
- AC-07: Observations written via the rework candidate path and the `RecordEvents` batch
  path receive `topic_signal` from the session registry's in-memory `feature` when
  `event.topic_signal` is `None` and the session has a registered feature.
- AC-08: When `extract_topic_signal(input)` returns `Some(signal)`, the explicit signal
  is used unchanged (the registry feature is NOT used as a fallback override).
- AC-09: Features predating `cycle_events` (no rows in the table) continue to work via
  the existing `load_feature_observations` + `load_unattributed_sessions` path without
  behavioral change.
- AC-10: `load_cycle_observations` is declared on the `ObservationSource` trait and
  implemented on `SqlObservationSource`; the trait's existing tests remain green.
- AC-11: Unit tests in `services/observation.rs` cover AC-01, AC-02, AC-03 using
  direct store inserts for `cycle_events` and `observations`.
- AC-12: The existing `context_cycle_review` tests (integration or unit) pass without
  modification, confirming backward compatibility.

## Constraints

- `cycle_events.timestamp` is `INTEGER` (Unix epoch seconds, same scale as `seq`
  advisory ordering). `observations.ts_millis` is INTEGER in milliseconds. The window
  comparison must convert: `cycle_events.timestamp * 1000` as the boundary against
  `observations.ts_millis`. Verify this unit alignment in the implementation.
- `ObservationSource` is a sync trait (methods return `Result<_, ObserveError>`), bridged
  from async sqlx via `block_sync` / `block_in_place`. The new method must follow the
  same sync-bridge pattern — no `async fn` on the trait.
- `SqlxStore::insert_cycle_event` is the only existing API for writing cycle_events rows;
  tests must use it rather than raw SQL to avoid positional bind bugs.
- The `observations` table has no index on `topic_signal`. The Step 2 query is bounded
  by the timestamp index + a narrow time window; this is acceptable for the expected
  volume. No new migration is required.
- `parse_observation_rows` reuses the existing 7-column SELECT shape and security bounds
  (64 KB input size check, JSON depth check). The new method must use the same parser.
- Schema version is currently 15 (v14→v15 added cycle_events). No schema migration is
  needed for this feature; all required columns already exist.
- The `block_sync` helper in `services/observation.rs` already handles "inside runtime"
  vs "outside runtime" contexts. New method implementations must use it.

## Resolved Design Decisions

1. **cycle_events.timestamp units**: Confirmed seconds — `handle_cycle_event` writes
   `unix_now_secs() as i64`. Window comparison must use `cycle_events.timestamp * 1000`
   against `observations.ts_millis`.

2. **Open-ended windows**: When a `cycle_start` exists but no `cycle_stop` follows,
   use `unix_now_secs()` as the implicit stop boundary. This allows `context_cycle_review`
   to work on in-progress features.

3. **RecordEvents batch path**: Must be fixed. The batch handler (`RecordEvents`, line 784)
   has the same topic_signal miss case. Enrichment applies per-event in the batch loop
   before `extract_observation_fields`. Covered by AC-07.

5. **Signal mismatch diagnostic**: When `extract_topic_signal` returns an explicit signal
   that differs from the session registry's current feature, emit `tracing::debug!` with
   both values. Aids attribution forensics post-deploy; low overhead.

6. **Empty-result disambiguation in load_cycle_observations**: Before returning `Ok(vec![])`
   when no observations are found, run a count-only query to distinguish "no cycle_events
   rows for this cycle_id" from "cycle_events rows exist but no topic_signal match was
   found." The distinction improves diagnostic clarity. An extra DB query on
   `context_cycle_review` (a one-time-per-feature call) is acceptable.

4. **topic_signal composite index**: Deferred. `context_cycle_review` is a one-time-per-
   feature call; a slight delay there is acceptable. Adding a write-path index on
   `topic_signal` would slow every observation insert (the hot path). Not needed at
   current scale (~100K observations, ~3K rows per typical 6-hour window).

## Tracking

https://github.com/dug-21/unimatrix/issues/372
