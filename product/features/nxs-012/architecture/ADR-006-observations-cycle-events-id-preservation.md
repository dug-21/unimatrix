## ADR-006: observations.id and cycle_events.id Preserved Through Export/Import

### Context

Both `observations.id` and `cycle_events.id` are INTEGER PRIMARY KEY AUTOINCREMENT columns. Unlike `graph_edges.id` (ADR-005), these ids have downstream significance:

- `observations.id` serves as the extraction tick watermark -- the analytics pipeline tracks "last processed observation id" to avoid reprocessing. Fresh AUTOINCREMENT ids after import would reset this watermark, causing all imported observations to be reprocessed.
- `cycle_events.id` is used for sequencing within a cycle (alongside `seq`) and as a stable reference in cycle review output.

The `audit_log.event_id` export/import already follows this pattern: export the id, import with explicit id via parameterized INSERT.

### Decision

Export `observations.id` and `cycle_events.id` as regular integer fields. Import with explicit id binding in the INSERT statement (same pattern as `insert_audit_log`).

The inserter functions include the `id` column in the INSERT:

```sql
-- insert_observation
INSERT INTO observations (id, session_id, ts_millis, hook, tool, input,
    response_size, response_snippet, topic_signal, phase)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)

-- insert_cycle_event
INSERT INTO cycle_events (id, cycle_id, seq, event_type, phase, outcome,
    next_phase, timestamp, goal, goal_embedding)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)
```

Plain INSERT (not INSERT OR IGNORE) is used for both -- duplicate ids indicate data corruption and should surface as errors (consistent with AC-18 for graph_edges).

### Consequences

- Extraction tick watermarks are preserved across export/import -- no reprocessing of already-processed observations
- Cycle event sequencing is stable across export/import
- Duplicate id conflicts are surfaced as errors, not silently ignored
- Risk SR-05 is addressed: `--force` is the only import path, and plain INSERT surfaces conflicts rather than masking them
