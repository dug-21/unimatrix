## ADR-001: Goal Stored on cycle_events Start Row, Not sessions Table

### Context

col-025 needs a durable, queryable store for the feature goal text. Two candidate
tables exist:

- `sessions`: per-session record, written at `SessionRegister` time. Subject to
  retention cleanup. Already has a dead `keywords TEXT` column (crt-025 WA-1) that
  was never populated. Session rows are transient; they do not survive across
  multiple sessions working the same feature cycle.

- `cycle_events`: structural audit trail written synchronously at `cycle_start` time
  using the direct write pool (ADR-003 crt-025, Store::insert_cycle_event). Rows
  persist for the full lifecycle of a feature cycle. The `idx_cycle_events_cycle_id`
  index makes point lookups cheap. Already serves as the authoritative, durable
  record of when a feature started, what phases it went through, and when it ended.

Goal is a property of a *feature cycle*, not a *session*. A single feature cycle
may span multiple sessions (e.g., after a server restart). If goal were stored on
the session row, it would be lost on the next session registration for the same
feature — precisely the resume path that makes goal retrieval necessary.

The data modeler recommendation (SCOPE.md §Background) and the resolved design
decision 1 confirm this choice.

### Decision

`goal TEXT` is added as a nullable column on the `cycle_events` table (schema
v15 → v16). It is written only on `cycle_start` event rows. `cycle_phase_end`
and `cycle_stop` rows always have `goal = NULL`.

The resume-path query is:

```sql
SELECT goal FROM cycle_events
WHERE cycle_id = ?1 AND event_type = 'cycle_start'
LIMIT 1
```

This is served by the existing `idx_cycle_events_cycle_id` index. The `LIMIT 1`
guard ensures correct behavior even if multiple `cycle_start` rows exist for the
same cycle_id (defensive; the normal lifecycle has exactly one).

The `sessions.keywords TEXT` column is not touched by this feature. It remains
dead and is tracked for cleanup separately (SCOPE.md §Non-Goals, SR-04).

### Consequences

- Goal survives server restarts; session boundaries do not matter.
- Goal is available to retrospective review (it lives in the same event log used
  by `context_cycle_review`).
- Callers that do not provide `goal` produce `NULL` in `cycle_events`; existing
  behavior is unchanged (graceful degradation to topic-ID fallback).
- Schema migration is additive (ALTER TABLE ADD COLUMN) with an idempotency guard.
  Old binaries cannot connect to a v16 database; this is the standard constraint.
- `insert_cycle_event` gains one parameter (`goal: Option<&str>`). All call sites
  must be updated. Currently one call site exists in `handle_cycle_event` (listener.rs).
