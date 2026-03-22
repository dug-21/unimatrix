## ADR-003: CYCLE_EVENTS Uses Direct Write Pool, Not Analytics Drain

### Context

`unimatrix-store` has two write mechanisms:

1. **Direct write pool** (`write_pool`): used for integrity tables where writes must be immediately visible to subsequent reads. Examples: `entries`, `entry_tags`, `feature_entries` (via `record_feature_entries`).
2. **Analytics drain** (`AnalyticsWrite` channel + background task): used for observational telemetry that can tolerate eventual consistency and queue shedding under backpressure. Examples: `query_log`, `injection_log`, `observations`, `co_access`.

The question is which path `CYCLE_EVENTS` should use.

`CYCLE_EVENTS` shares characteristics with both:
- Like telemetry: it records lifecycle events in an append-only log; individual rows are not read back immediately after write.
- Like structural tables: the data is read by `context_cycle_review` to construct the phase narrative, and correctness depends on the full event sequence being present.

Two risks with the analytics drain:
1. **Queue shedding**: under backpressure the analytics queue sheds events (`try_send` semantics). A shed `CYCLE_EVENTS` row means a phase transition is silently lost from the audit trail.
2. **`seq` interplay**: seq is computed via `SELECT MAX(seq)+1` in the DB write task. If the event is delayed in the analytics queue, the seq value computed at drain time may be out of order relative to other cycle events that were written via the direct pool.

The analytics drain is documented as "NEVER call this for integrity table writes" in `db.rs`. The hook latency budget (40ms transport timeout) applies to the hook response, not to the DB write — the DB write is already fire-and-forget in both paths.

### Decision

`CYCLE_EVENTS` writes use the **direct write pool** via a new `SqlxStore::insert_cycle_event(...)` async method. The call is made inside a `spawn_blocking_fire_and_forget` task in the UDS listener (consistent with the existing pattern for `update_session_keywords`, `update_session_feature_cycle`), not on the analytics drain.

The `seq` computation (`SELECT COALESCE(MAX(seq), -1) + 1`) happens inside the same spawned task, on the same write-pool connection, providing as much serialization as the write pool allows.

### Consequences

**Easier**:
- No CYCLE_EVENTS rows are silently dropped under backpressure.
- Phase narrative data in `context_cycle_review` is complete (no silent gaps from queue shedding).
- `seq` computation and INSERT are logically co-located.
- Consistent with the codebase's stated rule: direct write pool for structural data.

**Harder**:
- The write pool has a max of 2 connections and serializes writes. Under high-frequency cycle events (unusual in practice), this could add latency to other write-pool operations. In practice, lifecycle events (start/phase-end/stop) are low-frequency (3–10 per feature cycle).
- Adds a new method to `SqlxStore`; test coverage required.
