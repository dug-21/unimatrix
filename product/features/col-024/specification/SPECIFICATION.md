# SPECIFICATION: col-024 — Cycle-Events-First Observation Lookup and Topic Signal Write-Time Enrichment

## Objective

`context_cycle_review` currently uses `sessions.feature_cycle` as its primary observation
discovery path. That column is populated asynchronously and can be NULL, stale, or absent,
causing sessions to be silently missed. This feature redesigns the lookup to use
`cycle_events` timestamps as the authoritative primary path, and enriches `topic_signal`
at write time in `listener.rs` so that observations written after `context_cycle(start)`
carry the cycle's attribution even when their input text contains no recognizable feature ID.

---

## Functional Requirements

### FR-01: cycle_events-First Lookup Method

`ObservationSource` must gain a new method `load_cycle_observations(cycle_id: &str)`
declared on the trait and implemented on `SqlObservationSource`. The method must be
callable via trait dispatch (`&dyn ObservationSource`).

### FR-02: Time-Window Extraction (Step 1)

`load_cycle_observations` must query `cycle_events WHERE cycle_id = ?1 ORDER BY timestamp
ASC, seq ASC` and pair each `cycle_start` row with the next `cycle_stop` row to form
`(start_ts, stop_ts)` windows. If a `cycle_start` has no following `cycle_stop`, the
implicit stop is `unix_now_secs()` (open-ended window for in-progress cycles). The result
is a list of one or more non-overlapping time windows.

### FR-03: Session Discovery via topic_signal (Step 2)

For each window produced by FR-02, `load_cycle_observations` must execute:

```sql
SELECT DISTINCT session_id
FROM observations
WHERE topic_signal = ?1
  AND ts_millis >= ?2 AND ts_millis <= ?3
```

where `?2` is `window.start_ts * 1000` and `?3` is `window.stop_ts * 1000`. Results
across all windows must be unioned into a single deduplicated set of session IDs.

### FR-04: Timestamp Unit Conversion

`cycle_events.timestamp` is stored as Unix epoch **seconds** (written by
`unix_now_secs()`). `observations.ts_millis` is stored in **milliseconds**. All window
boundary comparisons against `observations.ts_millis` must apply the conversion
`cycle_events.timestamp * 1000`. This conversion must be implemented via a named helper
or constant — no raw `* 1000` literals in SQL construction code.

### FR-05: Full Observation Load with Window Filtering (Step 3)

`load_cycle_observations` must load all observations for the discovered session IDs
bounded by `[min_window_start_ms, max_window_stop_ms]` in SQL, then filter in Rust to
retain only records whose `ts_millis` falls within at least one `(start_ms, stop_ms)`
window. The returned records must use the existing 7-column SELECT shape and be parsed
via `parse_observation_rows`.

### FR-06: Empty-on-No-Cycle-Events Semantics

When `cycle_events` contains no rows for the given `cycle_id`, `load_cycle_observations`
must return `Ok(vec![])`. This signals to the caller that the legacy fallback path should
activate; it must not return an error.

### FR-07: context_cycle_review Lookup Order

The observation loading block in `context_cycle_review` (tools.rs) must replace its
current fast path with the following ordered sequence:

1. `load_cycle_observations(feature_cycle)` — if result is non-empty, use as attributed
   and skip remaining steps.
2. `load_feature_observations(feature_cycle)` — legacy sessions-table path; if non-empty,
   use as attributed and skip remaining step.
3. `load_unattributed_sessions()` + `attribute_sessions(...)` — content-based attribution
   fallback.

### FR-08: Legacy Fallback Observability

When `load_cycle_observations` returns empty and the caller falls back to
`load_feature_observations`, a structured log event at `tracing::debug!` level must be
emitted, identifying the `feature_cycle` value and the fact that the primary path
returned empty.

### FR-09: RecordEvent topic_signal Enrichment

In the `HookRequest::RecordEvent` handler (listener.rs ~line 684), before
`extract_observation_fields(&event)` is called, if `event.topic_signal` is `None`,
the implementation must check `session_registry.get_state(&event.session_id)` and, if
the session has a non-None `feature`, set the observation's `topic_signal` to that
feature value. If `event.topic_signal` is already `Some`, it must be used unchanged.

### FR-10: Rework Candidate topic_signal Enrichment

In the rework candidate handler (listener.rs ~line 592), the same enrichment logic as
FR-09 must apply: if `event.topic_signal` is `None` and the session registry has a
non-None `feature` for that session, the observation row must carry that feature as
`topic_signal`.

### FR-11: RecordEvents Batch topic_signal Enrichment

In the `HookRequest::RecordEvents` handler (listener.rs ~line 784), enrichment must be
applied per-event in the batch before `extract_observation_fields` is called to build
`obs_batch`. For each event in the batch where `event.topic_signal` is `None`, the
session registry must be consulted and the fallback applied identically to FR-09.

### FR-12: ContextSearch topic_signal Enrichment

In the `HookRequest::ContextSearch` handler (listener.rs ~line 824), after
`extract_topic_signal(&query)` is called, if `topic_signal` is `None` and the session
has a non-None `feature` in the registry, the `ObservationRow.topic_signal` field must
be set to that feature value. If `extract_topic_signal` returns `Some`, its result must
be used unchanged.

### FR-13: Enrichment is Best-Effort

The enrichment in FR-09 through FR-12 is conditional on the session registry having a
registered feature at the time of the write. If `session_registry.get_state(sid)` returns
`None` (session not found) or `state.feature` is `None` (no cycle started), the
observation is written with `topic_signal: None`. No error is raised; enrichment silently
degrades to the pre-col-024 behavior for that observation.

### FR-14: Enrichment Does Not Override Explicit Signal

The session registry feature must only be used when `extract_topic_signal` returns `None`
(or `event.topic_signal` is `None`). An existing explicit signal must never be replaced
by the registry value, even if the registry feature differs.

### FR-15: New Unit Tests in services/observation.rs

Unit tests must be added to `services/observation.rs` covering:
- The primary path (FR-01 through FR-05): cycle with one `(start, stop)` window returns
  matching observations.
- Multi-window deduplication (FR-02, FR-05): resumed cycle with two windows includes
  observations from both windows and excludes observations between windows.
- No-rows empty return (FR-06): `load_cycle_observations` returns empty when no
  `cycle_events` rows exist.

Tests must use `setup_test_store()` + `SqlxStore::insert_cycle_event(...)` for
`cycle_events` rows and direct `sqlx::query` inserts for `observations` rows, following
the pattern of existing tests in that file.

---

## Non-Functional Requirements

### NFR-01: Sync Trait Contract

`load_cycle_observations` must be a synchronous method on `ObservationSource` returning
`Result<Vec<ObservationRecord>, ObserveError>`. It must not use `async fn`. The
implementation in `SqlObservationSource` must bridge to async sqlx via a single
`block_sync(async { ... })` call enclosing all three steps (FR-02, FR-03, FR-05), not
multiple separate `block_sync` calls.

### NFR-02: No New Migration

Schema version remains at 15. All required columns (`cycle_events.cycle_id`,
`cycle_events.timestamp`, `cycle_events.seq`, `observations.topic_signal`,
`observations.ts_millis`) already exist. No `ALTER TABLE` or `CREATE TABLE` statements
are permitted.

### NFR-03: No New Index

No new index on `observations.topic_signal` is added. The Step 2 query (FR-03) is
bounded by the `idx_observations_ts` timestamp index narrowing the scan to the cycle
window. This is accepted at current scale (~100 K observations, ~3 K rows per typical
6-hour window). The deferral decision must be documented with a volume threshold trigger:
revisit when a single cycle window exceeds 20 K rows.

### NFR-04: Enrichment Latency Budget

The enrichment reads (FR-09 through FR-12) are synchronous in-memory Mutex reads on
`session_registry`. Each read must complete within the existing handler latency budget;
no blocking I/O or spawn is introduced for the enrichment itself.

### NFR-05: parse_observation_rows Reuse

The new method must use the existing `parse_observation_rows` function with the same
7-column SELECT shape (`session_id, ts_millis, hook, tool, input, response_size,
response_snippet`). The 64 KB input size check and JSON depth check inside
`parse_observation_rows` apply unchanged.

### NFR-06: Backward Compatibility

The existing `load_feature_observations`, `discover_sessions_for_feature`,
`load_unattributed_sessions`, and `observation_stats` method signatures and behaviors
must not change. The `ObservationRecord` type in `unimatrix-core` must not change.

---

## Acceptance Criteria

| AC-ID | Statement | Verification Method |
|-------|-----------|---------------------|
| AC-01 | When `cycle_events` contains `(cycle_start, cycle_stop)` rows for a `cycle_id`, `load_cycle_observations` returns all observations from sessions that have `topic_signal = cycle_id` AND `ts_millis` within any `(start_ts * 1000, stop_ts * 1000)` window. | Unit test: insert cycle_events + observations, assert returned records match those within windows. |
| AC-02 | When a feature has multiple `(cycle_start, cycle_stop)` windows (e.g., resumed session), observations from all windows are included and observations between/outside windows are excluded. | Unit test: two disjoint windows, observations in window 1, between windows, and in window 2; assert only in-window observations returned. |
| AC-03 | When `cycle_events` has no rows for `cycle_id`, `load_cycle_observations` returns `Ok(vec![])`. | Unit test: no cycle_events rows inserted; assert empty vec returned, no error. |
| AC-04 | `context_cycle_review` invokes `load_cycle_observations` first; if non-empty it is used without calling `load_feature_observations`; if empty it falls back to `load_feature_observations`; if that is also empty it falls back to `load_unattributed_sessions` + `attribute_sessions`. | Integration test or mock: exercise each branch independently. |
| AC-05 | Observations written via the `RecordEvent` path receive `topic_signal` from the session registry's in-memory `feature` when `event.topic_signal` is `None` and the session has a registered feature. | Unit test: session with registered feature, RecordEvent with no topic_signal; assert stored observation has correct topic_signal. |
| AC-06 | Observations written via the `ContextSearch` path receive `topic_signal` from the session registry's in-memory `feature` when `extract_topic_signal(query)` returns `None` and the session has a registered feature. | Unit test: ContextSearch with non-feature-ID query, session with registered feature; assert stored observation topic_signal. |
| AC-07 | Observations written via the rework candidate path and the `RecordEvents` batch path receive `topic_signal` from the session registry's in-memory `feature` when `event.topic_signal` is `None` and the session has a registered feature. | Unit test per path: each with session having registered feature and no explicit topic_signal. |
| AC-08 | When `extract_topic_signal(input)` returns `Some(signal)`, the explicit signal is stored unchanged; the registry feature is not used. If the extracted signal differs from the session registry feature, a `tracing::debug!` is emitted with both values for attribution forensics. | Unit test: input containing a feature ID pattern with session attributed to a different feature; assert stored topic_signal equals extracted signal and debug log fires with both values. |
| AC-09 | Features predating `cycle_events` (no rows in the table for that cycle_id) continue to work via the existing `load_feature_observations` + `load_unattributed_sessions` path without behavioral change. | Existing `context_cycle_review` tests pass without modification. |
| AC-10 | `load_cycle_observations` is declared on the `ObservationSource` trait in `unimatrix-observe/src/source.rs` and implemented on `SqlObservationSource`; all existing trait tests remain green. | `cargo test` passes for `unimatrix-observe` and `unimatrix-server`. |
| AC-11 | Unit tests in `services/observation.rs` cover AC-01, AC-02, and AC-03 using `insert_cycle_event` for cycle_events rows. | `cargo test` in `unimatrix-server` runs and passes the new tests. |
| AC-12 | All existing `context_cycle_review` tests pass unmodified, confirming backward compatibility. | `cargo test` — no test deletions or modifications permitted for existing tests. |
| AC-13 | The timestamp unit conversion uses a named helper or constant — no raw `* 1000` literal appears in query-construction code for window boundary binding. | Code review: search for raw `* 1000` in the new `load_cycle_observations` implementation; must find zero occurrences. |
| AC-14 | When `load_cycle_observations` returns empty and the fallback to `load_feature_observations` activates, a `tracing::debug!` log line is emitted naming the `feature_cycle` value. | Test or log-capture: assert debug log is present on fallback activation. |
| AC-15 | `load_cycle_observations` distinguishes "no cycle_events rows for cycle_id" from "rows exist but no topic_signal match found" via a count-only pre-check query. The caller can log the distinction; the return value remains `Ok(vec![])` in both cases. | Unit test: insert cycle_events rows with no matching observations; verify internal count query fires and result is still `Ok(vec![])`. |

---

## Domain Models

### cycle_events

A table recording lifecycle transitions for a named feature cycle. Relevant columns:

| Column | Type | Semantics |
|--------|------|-----------|
| `cycle_id` | TEXT | Feature identifier (e.g., `col-024`). Foreign key to feature namespace. |
| `event_type` | TEXT | One of `cycle_start`, `cycle_phase_end`, `cycle_stop`. |
| `timestamp` | INTEGER | Unix epoch **seconds** (written by `unix_now_secs()`). |
| `seq` | INTEGER | Advisory monotonic counter. Tie-breaks on timestamp equality only. |

True ordering uses `ORDER BY timestamp ASC, seq ASC` (ADR-002 crt-025).

### observations

A table of hook telemetry records. Relevant columns:

| Column | Type | Semantics |
|--------|------|-----------|
| `session_id` | TEXT | Identifies the agent session that produced the record. |
| `ts_millis` | INTEGER | Unix epoch **milliseconds**. |
| `topic_signal` | TEXT (nullable) | Feature cycle ID extracted from input text or enriched from session registry. |
| `hook` | TEXT | Hook type (e.g., `UserPromptSubmit`, `PostToolUse`). |
| `tool` | TEXT (nullable) | Tool name for tool-use hooks. |
| `input` | TEXT (nullable) | Truncated input text (max 4096 chars). |
| `response_size` | INTEGER (nullable) | Byte count of the tool response. |
| `response_snippet` | TEXT (nullable) | Truncated response text. |

### Time Window

A `(start_ts, stop_ts)` pair derived from `cycle_events`. `start_ts` comes from a
`cycle_start` row. `stop_ts` comes from the next `cycle_stop` row, or `unix_now_secs()`
if no stop row exists (open-ended). Window boundaries are in **seconds**; all comparisons
against `observations.ts_millis` require conversion via the unit helper.

### ObservationSource (trait)

Defined in `unimatrix-observe/src/source.rs`. Abstraction over observation storage.
Implemented by `SqlObservationSource` in `unimatrix-server`. All methods are synchronous,
bridged to async sqlx via `block_sync`. After col-024, the trait has five methods:

| Method | Description |
|--------|-------------|
| `load_cycle_observations(cycle_id)` | **New.** Primary path: cycle_events-first lookup. |
| `load_feature_observations(feature_cycle)` | Legacy: sessions-table lookup. |
| `discover_sessions_for_feature(feature_cycle)` | Legacy: session ID discovery. |
| `load_unattributed_sessions()` | Fallback: NULL feature_cycle sessions. |
| `observation_stats()` | Aggregate counts. |

### topic_signal

A nullable `TEXT` column on `observations` that carries the feature cycle ID attributed
to an observation. Written at observation ingestion time. Two sources:

1. **Explicit**: `extract_topic_signal(input_text)` — pattern-matches a feature ID from
   the event's text content.
2. **Registry enrichment** (new in col-024): `session_registry.get_state(sid)?.feature`
   — the in-memory feature registered synchronously when `CYCLE_START_EVENT` fires.

Enrichment is best-effort: if the registry has no feature for the session at write time,
`topic_signal` remains `None`.

### session_registry

An in-memory structure keyed by `session_id`. Holds `SessionState`, which includes a
`feature: Option<String>` field set synchronously by `set_feature_force` when a
`CYCLE_START_EVENT` is handled. Reads are a Mutex lock lasting microseconds. The registry
is the authoritative in-memory source of the active cycle for a session.

---

## User Workflows

### Workflow 1: Retrospective on a cycle_events-attributed feature

1. Agent calls `context_cycle(start, feature_cycle="col-024")` — `cycle_events` row
   written, session registry `feature` set to `col-024`.
2. Subsequent hook events (RecordEvent, ContextSearch, rework) arrive. Because the
   session registry has `feature = "col-024"`, observations with no explicit
   `extract_topic_signal` result are enriched with `topic_signal = "col-024"`.
3. Agent calls `context_cycle(stop)` — second `cycle_events` row written.
4. Agent calls `context_cycle_review(feature_cycle="col-024")`.
5. Server calls `load_cycle_observations("col-024")`: reads the `(start_ts, stop_ts)`
   window, finds session IDs with `topic_signal = "col-024"` in that window, loads
   their observations.
6. Non-empty result returned; retrospective report generated. No fallback activated.

### Workflow 2: Legacy feature with no cycle_events rows

1. Feature `bugfix-277` predates schema v15; no `cycle_events` rows exist.
2. Agent calls `context_cycle_review(feature_cycle="bugfix-277")`.
3. Server calls `load_cycle_observations("bugfix-277")` — returns `Ok(vec![])`.
4. Debug log emitted: primary path returned empty for `bugfix-277`.
5. Server falls back to `load_feature_observations("bugfix-277")` — sessions-table lookup.
6. If non-empty, report generated. Otherwise, `load_unattributed_sessions` +
   `attribute_sessions` runs.

### Workflow 3: In-progress cycle (open-ended window)

1. Agent calls `context_cycle(start, feature_cycle="col-024")`.
2. No `cycle_stop` event yet.
3. Agent calls `context_cycle_review(feature_cycle="col-024")` mid-session.
4. `load_cycle_observations`: `(start_ts, unix_now_secs())` window constructed.
5. Observations written since cycle start with `topic_signal = "col-024"` are returned.
6. Report covers current-session observations; no error for missing stop event.

---

## Constraints

1. **Timestamp units**: `cycle_events.timestamp` is seconds; `observations.ts_millis` is
   milliseconds. All window boundary comparisons must apply `cycle_ts_to_obs_millis(ts)`
   or equivalent named conversion. Verify: `handle_cycle_event` writes
   `unix_now_secs() as i64`; observations write `(unix_now_secs() as i64) * 1000`.
   The conversion is `* 1000`.

2. **Sync trait**: `ObservationSource` methods are synchronous. The new method must not
   use `async fn`. All three steps (FR-02, FR-03, FR-05) must execute inside a single
   `block_sync(async { ... })` closure to avoid nested runtime panics and to keep the
   blocking window contiguous.

3. **insert_cycle_event API**: Tests must use `SqlxStore::insert_cycle_event(...)` to
   write `cycle_events` rows. Raw SQL inserts for that table are not permitted in tests.

4. **No index on topic_signal**: The Step 2 query is bounded by the timestamp index. No
   new migration or index is added. The scale assumption is ~100 K observations / ~3 K
   rows per 6-hour window; revisit at 20 K rows per window.

5. **parse_observation_rows**: The 7-column SELECT shape, 64 KB input limit, and JSON
   depth check are non-negotiable. The new method must not bypass them.

6. **Enrichment scope**: FR-09 through FR-12 apply only to the four UDS listener write
   paths named in scope. Tests, batch-import paths, and any other write paths are
   explicitly excluded from enrichment.

7. **Abandoned cycles**: An open-ended window (`cycle_start` with no `cycle_stop`) uses
   `unix_now_secs()` as the implicit stop. This may over-include observations from a
   subsequent reuse of the same session if no stop event was ever written. This is
   accepted behavior; no maximum window cap is applied. Spec defines this as
   "best-effort" attribution for abandoned or in-progress cycles.

8. **Signal mismatch**: When `extract_topic_signal` returns a signal from a different
   feature than the registry feature, the explicit extracted signal is authoritative and
   the registry is not consulted (FR-14). The mismatch is not treated as an anomaly or
   logged; it is accepted behavior.

---

## Dependencies

| Dependency | Version / Location | Role |
|------------|--------------------|------|
| `unimatrix-observe` | workspace | `ObservationSource` trait definition |
| `unimatrix-store` | workspace | `SqlxStore`, `insert_cycle_event` API |
| `sqlx` | workspace | Async query execution |
| `tokio` | workspace | `block_in_place` for sync bridge |
| `tracing` | workspace | Debug log on fallback activation (AC-14) |
| `cycle_events` table | schema v15 | Time windows; already exists |
| `observations` table | schema v15 | Records with `topic_signal` + `ts_millis`; already exists |
| `session_registry` | in-memory (listener.rs) | `get_state(sid)?.feature` for enrichment |
| `extract_topic_signal` | `unimatrix-observe` | Existing text-pattern extractor |
| `parse_observation_rows` | `services/observation.rs` | Existing 7-column row parser |
| `block_sync` | `services/observation.rs` | Existing sync/async bridge |

---

## NOT in Scope

- Changes to `cycle_events` schema — no `session_id` column will be added.
- Changes to `ObservationRecord` in `unimatrix-core` — `topic_signal` is not surfaced to
  callers.
- Changes to detection rules, metrics pipeline, or report format.
- Backfill of `topic_signal` for historical observations predating col-024.
- Changes to `sessions.feature_cycle` column or its async write path.
- Adding an index on `observations.topic_signal`.
- Enrichment for test paths, batch-import paths, or any write path outside the four UDS
  listener handlers.
- A composite index on `(topic_signal, ts_millis)` — deferred; revisit at 20 K rows/window.
- A maximum window cap for abandoned cycles.
- Runtime validation that `cycle_events.timestamp` is in seconds (no unit assertion).

---

## Open Questions

**OQ-01** (for architect): SR-05 recommends a shared enrichment helper to avoid per-site
drift across the four write paths. Should a single `resolve_topic_signal(event_signal,
session_id, registry)` function be extracted, or is inline enrichment at each site
acceptable given the four locations are co-located in the same handler block?

**OQ-02** (for architect): SR-02 notes that a multi-step query loop inside one
`block_sync` call blocks the async runtime thread for the duration of all three steps.
For typical cycle sizes (~3 K observations), this is acceptable. Confirm the single
`block_sync` envelope (NFR-01) is the right boundary, or whether Step 2's per-window loop
warrants evaluation.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for observation lookup, topic_signal enrichment, cycle_events schema -- found #3366 (cycle_events-first lookup pattern), #3367 (topic_signal write-time enrichment pattern), #2999 (ADR-002 crt-025 seq advisory), #383 (ADR-002 col-012 ObservationSource independence), confirming established conventions match this spec.
