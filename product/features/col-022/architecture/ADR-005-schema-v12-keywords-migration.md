## ADR-005: Schema v12 Migration for Keywords Column

### Context

The `sessions` table needs a new `keywords TEXT` column (ADR-003). The current schema version is 11 (set by nxs-010). The migration infrastructure supports sequential version bumps with ALTER TABLE statements.

SR-09 notes the sessions table is shared across multiple subsystems. The migration must be non-destructive and backward compatible.

### Decision

Increment `CURRENT_SCHEMA_VERSION` from 11 to 12. The migration adds a single nullable column:

```sql
ALTER TABLE sessions ADD COLUMN keywords TEXT;
```

Migration steps:
1. Check `schema_version` counter -- if < 12, run migration
2. Execute `ALTER TABLE sessions ADD COLUMN keywords TEXT`
3. Update `schema_version` counter to 12

No data backfill: existing sessions have `keywords = NULL`, which is the correct default (they were created before explicit cycle lifecycle existed).

`SessionRecord` field uses `#[serde(default)]` so that rows without the column (impossible after migration, but defensive) deserialize correctly.

`SESSION_COLUMNS` constant updated to include `keywords` at the end (position 10, 0-indexed 9). `session_from_row` updated to read column index 9.

### Consequences

**Easier:**
- ALTER TABLE ADD COLUMN is the simplest possible migration -- no data copying, no table recreation, no index changes.
- NULL default means no backfill logic needed.
- Existing queries that do not reference `keywords` are unaffected.

**Harder:**
- Schema version bump means old binaries cannot connect to a v12 database (standard constraint, accepted by all prior migrations).
- `SESSION_COLUMNS` must be updated atomically with `session_from_row` to avoid column index mismatch. This is a compile-time coupling (both in the same file), so risk is low.
