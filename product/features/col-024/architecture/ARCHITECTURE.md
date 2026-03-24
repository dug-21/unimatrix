# col-024: Architecture — Cycle-Events-First Observation Lookup and Topic Signal Enrichment

## System Overview

`context_cycle_review` (MCP tool, `tools.rs`) produces retrospective reports by
loading observations attributed to a named feature cycle. The current attribution
path relies on `sessions.feature_cycle`, which is written asynchronously and is
unreliable: it can be NULL, stale, or absent due to races and server restarts.

col-024 introduces a more reliable primary path using the `cycle_events` table,
which records `cycle_start` / `cycle_stop` timestamps synchronously. The new path
identifies relevant sessions by time window + `topic_signal` column rather than the
async-written `sessions.feature_cycle` column.

A complementary write-time enrichment ensures that observations recorded after
`context_cycle(start)` carry a `topic_signal` even when the hook event's input text
contains no recognizable feature ID pattern. The session registry already holds the
authoritative in-memory `feature` value (set synchronously by `set_feature_force`
on `cycle_start`), so it can serve as a reliable fallback.

These two changes together close the attribution gap: the enrichment fills
`topic_signal` on write so that the new cycle-events-first read path can find sessions.

## Component Breakdown

### 1. `ObservationSource` trait (`unimatrix-observe/src/source.rs`)

Adds one method:

```rust
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>;
```

The trait lives in `unimatrix-observe`, which has no dependency on
`unimatrix-store`. The new method follows the existing sync-trait contract
(returns `Result<_, ObserveError>`).

### 2. `SqlObservationSource` (`unimatrix-server/src/services/observation.rs`)

Implements `load_cycle_observations`. The entire three-step algorithm runs inside a
single `block_sync` call (ADR-001) to minimise async-runtime blocking.

Steps executed inside one `block_sync`:

0. Count-only pre-check: `SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?1`.
   If zero, return `Ok(vec![])` immediately — signals "no cycle_events rows" to the
   caller (distinguishable from "rows exist but no observations matched" at log time).
1. Query `cycle_events` for time windows (`cycle_start` / `cycle_stop` pairs).
2. For each window, query `observations` for distinct `session_id` where
   `topic_signal = cycle_id AND ts_millis BETWEEN start_ms AND stop_ms`.
3. Query `observations` for all records from those session IDs within the
   combined time window, then filter in Rust to retain only records that fall
   inside at least one `(start_ms, stop_ms)` interval.

A named conversion helper (`cycle_ts_to_obs_millis`) converts
`cycle_events.timestamp` (seconds) to the millisecond unit used by
`observations.ts_millis` (ADR-002).

### 3. `context_cycle_review` handler (`unimatrix-server/src/mcp/tools.rs`)

The observation-loading closure is restructured from two-path to three-path:

```
Primary:  load_cycle_observations(feature_cycle)
          → if non-empty: use result as attributed observations
Legacy-1: load_feature_observations(feature_cycle)   [sessions.feature_cycle]
          → if non-empty: use result as attributed observations
Legacy-2: load_unattributed_sessions()
          → attribute_sessions(...)
```

When the primary path returns empty, a structured log event fires to surface the
fallback activation (ADR-003).

### 4. Topic-signal enrichment helper (`unimatrix-server/src/uds/listener.rs`)

A private free function `enrich_topic_signal` centralises the fallback logic
(ADR-004) to avoid per-site drift (SR-05):

```rust
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String>
```

Returns `extracted` unchanged when it is `Some(_)`. When `extracted` is `None`,
reads `session_registry.get_state(session_id)` and returns the in-memory
`state.feature` if present. This is a synchronous Mutex lock (~microseconds);
no `await`, no `spawn_blocking`.

Returns `extracted` unchanged when it is `Some(_)`. When `extracted` differs from the
session registry feature, a `tracing::debug!` fires with both values for attribution
forensics (ADR-004). This is the only observation point for cross-signal mismatches
(e.g., input text mentioning `bugfix-342` while the session is attributed to `col-024`).

Applied at all four write sites:

| Site | Where | How |
|------|-------|-----|
| `RecordEvent` | line ~684 | call before `extract_observation_fields`, override `event.topic_signal` in the `ObservationRow` |
| `RecordEvents` batch | line ~784-785 | per-event call inside the map that builds `obs_batch` |
| `rework candidate` | line ~592 | same pattern as `RecordEvent` |
| `ContextSearch` | line ~842 | replace inline `topic_signal.clone()` with `enrich_topic_signal(topic_signal, sid, session_registry)` |

For the `RecordEvent` and rework-candidate paths, `extract_observation_fields`
already reads `event.topic_signal`. The enrichment cannot mutate `event` (borrow
conflict with the registry read), so the `ObservationRow` is constructed with the
enriched value explicitly, or `extract_observation_fields` receives a modified copy
of the event. The implementation team should favour an explicit override on the
`ObservationRow` after the call to `extract_observation_fields` to avoid touching
the immutable `ImplantEvent`.

## Component Interactions

```
listener.rs
  RecordEvent / RecordEvents / ContextSearch
      │
      ▼
  enrich_topic_signal(extracted, sid, session_registry)
      │ reads session_registry.get_state(sid).feature
      ▼
  ObservationRow { topic_signal: enriched }
      │
      ▼ (fire-and-forget spawn_blocking)
  insert_observation / insert_observations_batch
      │ writes observations.topic_signal
      ▼
  observations table

tools.rs: context_cycle_review
      │
      ▼ (spawn_blocking_with_timeout)
  SqlObservationSource::load_cycle_observations(cycle_id)
      │  Step 1: cycle_events → [(start_ms, stop_ms), ...]
      │  Step 2: observations → DISTINCT session_ids WHERE topic_signal = cycle_id
      │  Step 3: observations → all records for those session_ids; Rust-filter to windows
      ▼
  Vec<ObservationRecord>  ← non-empty → use as attributed
      │ empty
      ▼
  load_feature_observations   ← non-empty → use as attributed; log fallback event
      │ empty
      ▼
  load_unattributed_sessions + attribute_sessions   ← legacy content-based scan
```

## Technology Decisions

See individual ADR files:

- ADR-001: Single `block_sync` entry per `load_cycle_observations` invocation
- ADR-002: Named unit-conversion helper (`cycle_ts_to_obs_millis`)
- ADR-003: Structured log event on primary-path fallback (SR-06)
- ADR-004: Shared `enrich_topic_signal` helper for all write sites (SR-05)
- ADR-005: Open-ended window capped at `unix_now_secs()`; no additional max-age cap (SR-03)

## Integration Points

### Existing components consumed

| Component | API | Notes |
|-----------|-----|-------|
| `SqlxStore` | `write_pool_server()` | Pool used for all sqlx queries in `SqlObservationSource` |
| `SqlxStore::insert_cycle_event` | `(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp: i64)` | Test fixture; not called by new production code |
| `SessionRegistry::get_state` | `fn get_state(&self, session_id: &str) -> Option<SessionState>` | Read `state.feature: Option<String>` |
| `unimatrix_observe::extract_topic_signal` | `fn extract_topic_signal(input: &str) -> Option<String>` | Unchanged; enrichment activates only on `None` return |
| `parse_observation_rows` | 7-column SELECT shape; internal to `observation.rs` | Reused unchanged by the new method |
| `block_sync` | internal `fn block_sync<F,T>(fut: F) -> T` | Used by all `ObservationSource` trait impls |

### New surface introduced

| Interface | Type / Signature | Defined in |
|-----------|-----------------|------------|
| `ObservationSource::load_cycle_observations` | `fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>` | `unimatrix-observe/src/source.rs` |
| `cycle_ts_to_obs_millis` | `fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64` — returns `ts_secs.saturating_mul(1000)` | `unimatrix-server/src/services/observation.rs` (module-private) |
| `enrich_topic_signal` | `fn enrich_topic_signal(extracted: Option<String>, session_id: &str, registry: &SessionRegistry) -> Option<String>` | `unimatrix-server/src/uds/listener.rs` (module-private) |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ObservationSource::load_cycle_observations` | `fn(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>` | `unimatrix-observe/src/source.rs` |
| `cycle_ts_to_obs_millis` | `fn(i64) -> i64` | `services/observation.rs` (private) |
| `enrich_topic_signal` | `fn(Option<String>, &str, &SessionRegistry) -> Option<String>` | `uds/listener.rs` (private) |
| `cycle_events` SQL | `SELECT event_type, timestamp FROM cycle_events WHERE cycle_id = ?1 ORDER BY timestamp ASC, seq ASC` | `SqlObservationSource` |
| session discovery SQL | `SELECT DISTINCT session_id FROM observations WHERE topic_signal = ?1 AND ts_millis >= ?2 AND ts_millis <= ?3` | `SqlObservationSource` (per window) |
| observation load SQL | 7-column SELECT with `session_id IN (…) AND ts_millis >= min AND ts_millis <= max ORDER BY ts_millis ASC` | `SqlObservationSource` |
| `context_cycle_review` fallback log | `tracing::debug!(cycle_id, path, "CycleReview: primary path empty, falling back")` | `tools.rs` |

## Data Flow: Timestamp Units

```
cycle_events.timestamp  (i64, Unix seconds)
        │
        │  cycle_ts_to_obs_millis(ts)  →  ts.saturating_mul(1000)
        ▼
window boundary (i64, Unix milliseconds)
        │
        │  compared against
        ▼
observations.ts_millis  (i64, Unix milliseconds)
```

All raw `* 1000` literals in window-boundary computation are forbidden; only
`cycle_ts_to_obs_millis` may perform this conversion.

## Lookup Order in `context_cycle_review`

```
1. load_cycle_observations(cycle_id)
   → returns Vec<ObservationRecord>
   → non-empty: proceed to detection pipeline
   → empty: log "primary path empty", continue to step 2

2. load_feature_observations(cycle_id)          [legacy: sessions.feature_cycle]
   → non-empty: proceed to detection pipeline
   → empty: continue to step 3

3. load_unattributed_sessions() + attribute_sessions(...)  [legacy content-scan]
   → result (possibly empty): check cached MetricVector if empty
```

## Schema Dependencies (No Changes Required)

All required columns exist in schema v15:

- `cycle_events`: `cycle_id`, `event_type`, `timestamp`, `seq` — no additions
- `observations`: `session_id`, `ts_millis`, `topic_signal` — no additions
- Index `idx_cycle_events_cycle_id` on `cycle_events(cycle_id)` — already present
- Index `idx_observations_ts` on `observations(ts_millis)` — bounds Step 2 scan

No schema migration is needed.

## Test Strategy

New tests in `unimatrix-server/src/services/observation.rs`:

| Test | Covers |
|------|--------|
| `load_cycle_observations_single_window` | AC-01: start+stop window returns matching observations |
| `load_cycle_observations_multiple_windows` | AC-02: multi-window disjoint filtering |
| `load_cycle_observations_no_cycle_events` | AC-03: empty return when no cycle_events rows exist |
| `load_cycle_observations_open_ended_window` | open-ended window uses `unix_now_secs()` stop |
| `load_cycle_observations_excludes_outside_window` | observations outside all windows excluded |

Enrichment tests in `unimatrix-server/src/uds/listener.rs` (unit tests for
`enrich_topic_signal`):

| Test | Covers |
|------|--------|
| `enrich_explicit_signal_unchanged` | AC-08: `Some(x)` is returned unchanged; debug log fires when x ≠ registry feature |
| `enrich_fallback_from_registry` | AC-05/06/07: `None` + registered feature → `Some(feature)` |
| `enrich_no_registry_entry` | `None` + unregistered session → `None` |
| `load_cycle_observations_no_cycle_events_count_check` | AC-15: count pre-check returns `Ok(vec![])` when zero cycle_events rows |
| `load_cycle_observations_rows_exist_no_signal_match` | AC-15: count pre-check distinguishes "rows exist but no match" case |

## Resolved Design Questions

1. **AC-08 / SR-04 — signal mismatch**: When `extract_topic_signal` returns `Some(x)` but
   the session registry has a different feature, `enrich_topic_signal` returns `x` unchanged
   AND emits `tracing::debug!` with both values. Explicit signal wins; log fires for forensics.

2. **Empty-result disambiguation**: A count-only pre-check (Step 0) distinguishes "no
   cycle_events rows" from "rows exist but no observations matched." Both return `Ok(vec[])`
   to the caller; the caller logs which case occurred via the structured fallback log (ADR-003).

3. **Abandoned cycles (SR-03)**: The open-ended window cap at `unix_now_secs()` (ADR-005)
   is accepted behavior. No max-age cap is applied. Document in function doc comment.
