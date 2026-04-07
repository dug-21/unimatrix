## ADR-007: observations.hook is the Correct SQL Column Name for PreToolUse Filter

### Context

The SCOPE.md proposed approach (Step 1) and the SCOPE-RISK-ASSESSMENT both
reference filtering by `hook_event = 'PreToolUse'`. This is incorrect. The
actual SQL column name in the `observations` table is `hook`, not `hook_event`.

Verified from `unimatrix-store/src/db.rs` (line 818):

```sql
CREATE TABLE IF NOT EXISTS observations (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id       TEXT    NOT NULL,
    ts_millis        INTEGER NOT NULL,
    hook             TEXT    NOT NULL,   -- <-- column name is "hook"
    tool             TEXT,
    input            TEXT,
    ...
    phase            TEXT
)
```

The naming discrepancy arises because `ObservationRecord` (in-memory struct in
`unimatrix-core`) uses `event_type: String` as its Rust field name. The mapping
from the `hook` DB column to the `event_type` Rust field is performed at read
time:
- In `listener.rs` line 1893: `event_type: hook_str`
- In `observation.rs` (services layer): same mapping from row index 2 (hook)

The SCOPE.md reference to `hook_event` appears to be a transcription error from
an earlier schema design where the column may have been named `hook_event`.

There is no `hook_event` column anywhere in the schema. Using `hook_event` in
the SQL would cause a runtime error: `no such column: hook_event`.

### Decision

All SQL queries in this feature that filter or read the event type from the
`observations` table MUST use `o.hook`, not `o.hook_event` or `o.event_type`.

The canonical filter expression is: `AND o.hook = 'PreToolUse'`

This is captured in the Query A canonical form in ADR-002 and must be carried
forward into all SQL strings in the implementation.

Doc comment on `query_phase_freq_observations` MUST include:

```
/// # Column Name Note
///
/// The event-type filter uses `o.hook = 'PreToolUse'` (not `o.hook_event`).
/// The `hook` column stores the raw event type string written by the hook
/// listener (e.g., "PreToolUse", "PostToolUse"). The in-memory ObservationRecord
/// maps this to the `event_type` field at read time, but the DB column is `hook`.
```

### Consequences

- Prevents a runtime SQL error from using `hook_event` (which does not exist).
- Removes ambiguity for future implementers who might follow SCOPE.md's draft
  SQL without verifying the schema.
- No performance impact: the existing `idx_observations_ts` and
  `idx_observations_session` indexes do not index `hook`. The `hook =
  'PreToolUse'` predicate is a full-scan filter within the time-windowed result
  set. This is acceptable given the indexed `ts_millis > ?` predicate narrows
  the scan to the lookback window first.
