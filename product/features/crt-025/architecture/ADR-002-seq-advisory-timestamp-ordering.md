## ADR-002: seq Is Advisory; Timestamp Is the True Ordering for CYCLE_EVENTS

### Context

`CYCLE_EVENTS` requires a sequence number (`seq`) scoped per `cycle_id` to reconstruct the ordered phase lifecycle of a feature. The SCOPE specifies `SELECT COALESCE(MAX(seq), -1) + 1` as the seq generation strategy.

SR-02 flags a concurrency risk: if two sessions for the same `cycle_id` concurrently emit events, both `SELECT MAX(seq)` reads can return the same value before either INSERT completes. The result is duplicate seq values for the same `cycle_id`.

Two options were considered:

**Option A: Enforce strict monotonicity via serialization or locking.**
Require all CYCLE_EVENTS writes for a given `cycle_id` to go through a single serialized task (e.g., a per-feature-id tokio channel or a database-level exclusive lock per cycle_id). This prevents duplicate seq values but adds significant complexity: a per-feature channel map or advisory locking mechanism that must be managed in the UDS listener.

**Option B: Advisory seq; true ordering via `(timestamp, seq)` at query time.**
Compute seq via `SELECT COALESCE(MAX(seq), -1) + 1` inside the fire-and-forget spawn. Accept that under concurrent cross-session writes, two events may receive the same seq value. At query time, order by `(timestamp ASC, seq ASC)` — timestamp is the stable tiebreaker since it is captured synchronously before the spawn.

Analysis of the actual concurrency model:

The UDS listener already serializes all events **per-session** — each session connection is handled by a single spawned task, and events within a session arrive sequentially. The race SR-02 describes requires two *different* sessions to both emit `cycle_phase_end` events for the same `feature_cycle` simultaneously. In practice this is rare: feature cycles typically have one active session at a time. Even when multiple sessions share a feature, they seldom emit lifecycle events (phase-end, stop) at exactly the same millisecond.

More importantly, `seq` duplicate values do not break correctness in this schema. The phase narrative reconstruction algorithm depends on the *logical order* of lifecycle events, not on strict seq uniqueness. A `(timestamp, seq)` composite ordering is sufficient for correct phase sequence reconstruction.

### Decision

`seq` is **advisory**: it provides a best-effort monotonic label per `cycle_id` but is not guaranteed unique under concurrent cross-session writes.

**Generation**: `SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?` inside the fire-and-forget spawn task.

**Ordering at query time**: `ORDER BY timestamp ASC, seq ASC`. Timestamp is captured synchronously in the UDS listener before the spawn (via `unix_now_secs()`), ensuring it reflects the causal ordering of events within a session.

**No advisory-seq column**: seq remains `INTEGER NOT NULL`. In the rare duplicate case, the two rows are distinguishable by `id` (AUTOINCREMENT primary key) and `timestamp`.

This matches the SCOPE §Constraints §Sequence Numbering guidance and is consistent with the established pattern in this codebase: observation events use a similar best-effort ordering.

### Consequences

**Easier**:
- Implementation is simple: one `SELECT MAX` query inside the spawn, no per-feature-id channel or locking.
- Schema is minimal: no advisory flag columns.
- Phase narrative reconstruction is correct in the common single-session-per-feature case (which covers all current protocol usage).

**Harder**:
- In the rare concurrent cross-session case, seq values for the same `cycle_id` may not be unique. Any future code that assumes `(cycle_id, seq)` uniqueness must use `id` (AUTOINCREMENT) as the unique key instead.
- Query ordering must explicitly use `(timestamp ASC, seq ASC)`, not `seq ASC` alone.
