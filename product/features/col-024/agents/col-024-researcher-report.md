# col-024-researcher Agent Report

## Summary

SCOPE.md written to `product/features/col-024/SCOPE.md`.

12 acceptance criteria defined (AC-01 through AC-12).
4 open questions surfaced.

## Key Findings

### cycle_events schema (confirmed from db.rs)

- Columns: `id`, `cycle_id`, `seq`, `event_type`, `phase`, `outcome`, `next_phase`,
  `timestamp` (INTEGER, epoch seconds).
- No `session_id` column — confirmed. Session identification requires observations.topic_signal.
- One index: `idx_cycle_events_cycle_id ON cycle_events(cycle_id)`.
- `seq` is advisory; ordering is `timestamp ASC, seq ASC` (ADR-002 crt-025).

### observations table (confirmed)

- `topic_signal TEXT` column already exists in schema.
- SELECT in `load_feature_observations` and `load_unattributed_sessions` does NOT
  include `topic_signal` — the column is used only at write time and in the proposed
  new lookup query. No SELECT-list change needed for existing paths.
- Indexes: `idx_observations_session ON observations(session_id)`,
  `idx_observations_ts ON observations(ts_millis)`.
- No index on `topic_signal` — noted as open question for performance.

### Current fast path failure modes

`load_feature_observations` queries `sessions WHERE feature_cycle = X`. This fails when:
1. `sessions.feature_cycle` async persist raced (fire-and-forget, can lose on server restart).
2. Session was attributed to a prior feature (set_feature_if_absent is immutable once set).
3. Session not in sessions table at all.

When fast path returns empty AND session has non-NULL `feature_cycle` (different value),
the fallback `load_unattributed_sessions` also misses it — only NULL-feature_cycle sessions
are returned by that path.

### topic_signal write gaps

Three write sites in listener.rs. All use `event.topic_signal` (from wire protocol) or
`extract_topic_signal(&text)`. None fall back to session registry. The session registry's
`state.feature` is set synchronously by `set_feature_force` inside `handle_cycle_event`
on CYCLE_START_EVENT — this runs BEFORE the observation is written (line 618 vs 684),
so the fallback is always safe to read.

### ObservationSource trait

Sync trait. `block_sync` helper in `services/observation.rs` bridges async sqlx.
New method `load_cycle_observations` must follow the same pattern. No async on the trait.

### Schema version

Currently v15. `cycle_events` and `observations.topic_signal` both exist in current schema.
No migration required.

### Legacy compatibility

All features predating crt-025 (col-022, col-020, bugfix-*, etc.) have no cycle_events
rows. `load_cycle_observations` returning empty on "no rows found" cleanly signals the
caller to use the legacy path.

### Test infrastructure

`services/observation.rs` uses `open_test_store` + `sqlx::query` direct inserts.
`SqlxStore::insert_cycle_event` is the correct API for test cycle_events writes.
Existing test fixtures (`insert_session`, `insert_observation`) are the model for new tests.

## Open Questions (for human)

1. **cycle_events.timestamp units** — confirmed as seconds from the `unix_now_secs()`
   call site? Need to verify at `handle_cycle_event` to confirm the multiply-by-1000
   comparison against `observations.ts_millis` is correct.

2. **Open-ended window (cycle_start, no cycle_stop)** — use `now()` as implicit stop,
   or exclude the open window? Affects in-progress feature reviews.

3. **RecordEvents batch path enrichment** — does the batch path also need the topic_signal
   fallback, or is it only the single-event path?

4. **topic_signal composite index** — add `(topic_signal, ts_millis)` now or defer?
   Not required for correctness; only affects performance at scale.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "observation attribution session feature cycle" — found entries #1067, #981, #756 (existing lessons on NULL feature_cycle silent failures and eager attribution immutability). No prior pattern for cycle-events-first lookup existed.
- Queried: `/uni-query-patterns` for "cycle_events schema design" — found entries #3040 (infra-001 seeding pattern) and #2999 (ADR-002 crt-025 seq ordering).
- Stored: entry #3366 "cycle_events-first observation lookup: session identification via topic_signal + time windows" via `/uni-store-pattern`
- Stored: entry #3367 "topic_signal write-time enrichment: session registry in-memory feature as fallback" via `/uni-store-pattern`
