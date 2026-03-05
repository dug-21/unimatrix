## ADR-001: AUTOINCREMENT Primary Key for Observations Table

### Context

The ASS-015 research proposed a composite primary key `(session_hash, ts_millis)` for the observations table. `session_hash` would be an integer hash of the session_id string, and `ts_millis` the event timestamp in milliseconds.

This design has a collision risk: Claude Code fires PreToolUse immediately followed by the tool execution and PostToolUse. These events can arrive within the same millisecond for the same session. A composite key on `(session_hash, ts_millis)` would reject the second event as a duplicate, silently losing data.

Additionally, `session_hash` adds complexity (hash function choice, collision handling for different session IDs that hash to the same integer).

### Decision

Use `INTEGER PRIMARY KEY AUTOINCREMENT` as the sole primary key. Session lookup uses the existing `idx_observations_session` index on `session_id TEXT`.

```sql
CREATE TABLE IF NOT EXISTS observations (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT    NOT NULL,
    ts_millis    INTEGER NOT NULL,
    hook         TEXT    NOT NULL,
    tool         TEXT,
    input        TEXT,
    response_size INTEGER,
    response_snippet TEXT
);
```

### Consequences

- **No collision risk**: Multiple events in the same millisecond for the same session are stored correctly.
- **Simpler schema**: No hash function, no composite key, no collision handling.
- **Slightly larger rows**: AUTOINCREMENT adds a monotonic counter. Negligible for this use case.
- **Session queries use index**: `WHERE session_id = ?` uses `idx_observations_session`, not PK lookup. This is marginally slower than PK lookup but still fast (B-tree index on TEXT).
- **Retention cleanup**: `DELETE FROM observations WHERE ts_millis < ?` uses `idx_observations_ts`. No PK dependency.
