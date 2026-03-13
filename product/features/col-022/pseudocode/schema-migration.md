# col-022: schema-migration -- Pseudocode

## Purpose

Add `keywords TEXT` column to the `sessions` table (schema v11 -> v12) and update `SessionRecord` to include the new field. ADR-003, ADR-005.

## File 1: `crates/unimatrix-store/src/migration.rs`

### Modify: `CURRENT_SCHEMA_VERSION`

Change from `11` to `12`.

### Modify: `migrate_if_needed` -- add v11->v12 block

Insert new migration block before the "Update schema version" statement (line ~202), after the existing `if current_version < 11` block:

```
// v11 -> v12: keywords column on sessions (col-022)
if current_version < 12:
    // Guard: check if column already exists (handles partial migration re-run)
    has_keywords = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'keywords'",
        [], |row| row.get::<_, i64>(0) > 0
    ).unwrap_or(false)

    if !has_keywords:
        conn.execute_batch(
            "ALTER TABLE sessions ADD COLUMN keywords TEXT;"
        ).map_err(StoreError::Sqlite)?

    // No data backfill: existing sessions have keywords = NULL (correct default)
```

**Pattern**: follows the v8 (pre_quarantine_status) and v10 (topic_signal) column-addition pattern with the `pragma_table_info` idempotency guard.

## File 2: `crates/unimatrix-store/src/sessions.rs`

### Modify: `SessionRecord` struct

Add new field after `total_injections`:

```
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub status: SessionLifecycleStatus,
    pub compaction_count: u32,
    pub outcome: Option<String>,
    pub total_injections: u32,
    pub keywords: Option<String>,  // NEW: JSON array string, e.g. '["kw1","kw2"]'
}
```

### Modify: `SESSION_COLUMNS` constant

```
const SESSION_COLUMNS: &str =
    "session_id, feature_cycle, agent_role, started_at, ended_at, \
     status, compaction_count, outcome, total_injections, keywords";
```

Position 10 (0-indexed 9) is `keywords`.

### Modify: `session_from_row` function

Add keywords extraction. The existing function uses named column access (`row.get("column_name")`), so no index-based coupling:

```
fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord>:
    Ok(SessionRecord {
        session_id: row.get("session_id")?,
        feature_cycle: row.get("feature_cycle")?,
        agent_role: row.get("agent_role")?,
        started_at: row.get::<_, i64>("started_at")? as u64,
        ended_at: row.get::<_, Option<i64>>("ended_at")?.map(|v| v as u64),
        status: SessionLifecycleStatus::try_from(row.get::<_, i64>("status")? as u8)
            .unwrap_or(SessionLifecycleStatus::Active),
        compaction_count: row.get::<_, i64>("compaction_count")? as u32,
        outcome: row.get("outcome")?,
        total_injections: row.get::<_, i64>("total_injections")? as u32,
        keywords: row.get("keywords")?,  // NEW: Option<String>, NULL -> None
    })
```

### Modify: `insert_session` method

Add `keywords` to the INSERT statement:

```
conn.execute(
    "INSERT OR REPLACE INTO sessions (session_id, feature_cycle, agent_role,
        started_at, ended_at, status, compaction_count, outcome, total_injections, keywords)
     VALUES (:sid, :fc, :ar, :sa, :ea, :st, :cc, :oc, :ti, :kw)",
    named_params! {
        // ... existing 9 params ...
        ":kw": &record.keywords,
    },
)
```

### Modify: `update_session` method

Add `keywords` to the UPDATE statement:

```
conn.execute(
    "UPDATE sessions SET feature_cycle = :fc, agent_role = :ar,
        started_at = :sa, ended_at = :ea, status = :st,
        compaction_count = :cc, outcome = :oc, total_injections = :ti,
        keywords = :kw
     WHERE session_id = :sid",
    named_params! {
        // ... existing 9 params ...
        ":kw": &record.keywords,
    },
)
```

## Error Handling

- Migration uses the established pattern: `pragma_table_info` guard for idempotency, `StoreError::Sqlite` wrapping.
- If `ALTER TABLE` fails (column already exists without guard, disk full), the migration transaction rolls back. Server refuses to start with standard migration error.
- `session_from_row` for `keywords`: `row.get("keywords")?` returns `None` for NULL. If the column does not exist (impossible after migration, but defense-in-depth), rusqlite returns an error that propagates up.

## Key Test Scenarios

1. **Round-trip with keywords**: insert session with `keywords = Some("[\"kw1\",\"kw2\"]")`, read back, verify exact match on all fields including keywords
2. **Round-trip without keywords**: insert session with `keywords = None`, read back, verify `keywords` is `None` and all other fields correct
3. **Migration on fresh v11 database**: run `migrate_if_needed`, verify `keywords` column exists, schema version is 12
4. **Migration idempotency**: run migration twice, verify no error on second run (pragma guard)
5. **Existing sessions after migration**: verify existing rows have `keywords = NULL`
6. **SESSION_COLUMNS count matches session_from_row**: compile-time or test-time assertion that the comma-separated column count in `SESSION_COLUMNS` equals the number of fields in `SessionRecord`
7. **update_session with keywords**: update a session to set keywords, read back, verify persisted
8. **scan_sessions_by_feature with keywords**: verify sessions returned include keywords field
