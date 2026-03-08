# vnc-010: Architecture — Quarantine State Restoration

## Overview

Extend the quarantine subsystem to accept entries in any non-quarantined status (Active, Deprecated, Proposed) and track the pre-quarantine status so restore returns entries to their original state.

## Changes by Crate

### unimatrix-store

#### Schema Change (v7 -> v8)

Add nullable column to `entries` table:

```sql
ALTER TABLE entries ADD COLUMN pre_quarantine_status INTEGER;
```

Migration step in `migrate_if_needed()`:

```sql
-- Add column (idempotent via IF NOT EXISTS not available for ALTER TABLE,
-- so guard with pragma_table_info check or use IF NOT EXISTS pattern)
ALTER TABLE entries ADD COLUMN pre_quarantine_status INTEGER;

-- Backfill: existing quarantined entries were quarantined from Active (old logic)
UPDATE entries SET pre_quarantine_status = 0 WHERE status = 3 AND pre_quarantine_status IS NULL;
```

Schema version bumps from 7 to 8. The `CURRENT_SCHEMA_VERSION` constant updates. The `db.rs` fresh-database schema adds the column.

#### EntryRecord

Add field:

```rust
#[serde(default)]
pub pre_quarantine_status: Option<u8>,
```

Using `Option<u8>` rather than `Option<Status>` for storage simplicity — the column is nullable INTEGER. Conversion to/from `Status` happens at the service layer.

**ADR-001**: `Option<u8>` over `Option<Status>` for the stored field. Status enum conversion at service layer avoids serde complexity and aligns with how `status` itself is stored as INTEGER.

#### entry_from_row / ENTRY_COLUMNS

Update the SQL column list and row-parsing function to include `pre_quarantine_status`. The column maps to `Option<u8>` via `row.get::<_, Option<i64>>()` converted to `Option<u8>`.

### unimatrix-server

#### context_quarantine tool handler (tools.rs)

**Quarantine path**: Remove the `entry.status != Status::Active` guard. Replace with:

```rust
if entry.status == Status::Quarantined {
    // idempotent — already quarantined
    return Ok(format_quarantine_success(...));
}
// All non-quarantined statuses are valid quarantine sources
```

**Restore path**: Instead of hardcoding `Status::Active`, read `pre_quarantine_status` from the entry and restore to that status. Validate via `Status::try_from()`. Fall back to Active if missing or invalid (SR-04).

#### change_status_with_audit (server.rs)

When `new_status == Quarantined`:
- Record `pre_quarantine_status = old_status as u8` in the UPDATE SQL
- Include the column in the UPDATE statement

When `new_status != Quarantined` (restore case):
- Clear `pre_quarantine_status` to NULL in the UPDATE SQL

#### quarantine_with_audit / restore_with_audit

`quarantine_with_audit` — no signature change. The pre_quarantine_status is derived from the entry's current status inside `change_status_with_audit`.

`restore_with_audit` — change to use pre_quarantine_status instead of hardcoded Active:

```rust
pub(crate) async fn restore_with_audit(
    &self,
    entry_id: u64,
    reason: Option<String>,
    audit_event: AuditEvent,
) -> Result<EntryRecord, ServerError> {
    // Fetch entry to read pre_quarantine_status
    let entry = self.entry_store.get(entry_id).await
        .map_err(|e| ServerError::Core(e))?;
    let restore_to = entry.pre_quarantine_status
        .and_then(|v| Status::try_from(v).ok())
        .unwrap_or(Status::Active);
    self.change_status_with_audit(
        entry_id, restore_to, reason, audit_event, true,
    ).await
}
```

**ADR-002**: Restore fallback to Active when `pre_quarantine_status` is NULL or invalid. This handles entries quarantined before the migration that were not caught by the backfill, and protects against data corruption. The fallback is logged in the audit trail.

## Data Flow

```
Quarantine:
  Entry(Active/Deprecated/Proposed)
    -> change_status_with_audit(Quarantined)
    -> SQL: SET status=3, pre_quarantine_status={old_status}
    -> Counter: decrement old, increment quarantined

Restore:
  Entry(Quarantined, pre_quarantine_status=X)
    -> read pre_quarantine_status -> Status::try_from(X)
    -> change_status_with_audit(restored_status)
    -> SQL: SET status=X, pre_quarantine_status=NULL
    -> Counter: decrement quarantined, increment X
```

## Integration Surfaces

- **SearchService**: No change. Already filters by status; quarantined entries excluded.
- **BriefingService**: No change. Delegates to SearchService.
- **ConfidenceService**: No change. Confidence recomputation after quarantine/restore is fire-and-forget and status-aware.
- **StatusService**: No change. Counter bookkeeping is generic.
- **SecurityGateway**: No change. Capability check (Admin) unchanged.

## Decisions

| ID | Decision | Rationale |
|----|----------|-----------|
| ADR-001 | Store pre_quarantine_status as Option<u8>, not Option<Status> | Aligns with status column (INTEGER), avoids serde enum complexity in schema |
| ADR-002 | Restore falls back to Active when pre_quarantine_status is NULL/invalid | Safety net for pre-migration entries and data corruption |
