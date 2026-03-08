# vnc-010: Specification — Quarantine State Restoration

## Domain Model

### Modified Entities

**EntryRecord** — Add field:
- `pre_quarantine_status: Option<u8>` — The status the entry held before quarantine. NULL when not quarantined. Set on quarantine, cleared on restore.

**entries table** — Add column:
- `pre_quarantine_status INTEGER` — Nullable. Maps to Status enum via u8 representation (0=Active, 1=Deprecated, 2=Proposed).

### Status Transitions

```
Active(0)      --quarantine--> Quarantined(3) [pre_quarantine_status=0]
Deprecated(1)  --quarantine--> Quarantined(3) [pre_quarantine_status=1]
Proposed(2)    --quarantine--> Quarantined(3) [pre_quarantine_status=2]
Quarantined(3) --restore-----> {pre_quarantine_status} [pre_quarantine_status=NULL]
```

Invalid transitions (rejected):
- Quarantined -> Quarantine (idempotent, returns success with "already quarantined")
- Non-Quarantined -> Restore (rejected, "entry is not quarantined")

## Acceptance Criteria

### AC-1: Quarantine from Deprecated Status

**Given** an entry with status=Deprecated
**When** `context_quarantine` is called with the entry's ID and action="quarantine"
**Then** the entry's status becomes Quarantined AND pre_quarantine_status is set to 1 (Deprecated)

### AC-2: Quarantine from Proposed Status

**Given** an entry with status=Proposed
**When** `context_quarantine` is called with the entry's ID and action="quarantine"
**Then** the entry's status becomes Quarantined AND pre_quarantine_status is set to 2 (Proposed)

### AC-3: Quarantine from Active Status (Existing Behavior)

**Given** an entry with status=Active
**When** `context_quarantine` is called with the entry's ID and action="quarantine"
**Then** the entry's status becomes Quarantined AND pre_quarantine_status is set to 0 (Active)

### AC-4: Restore to Pre-Quarantine Status

**Given** an entry with status=Quarantined and pre_quarantine_status=1 (Deprecated)
**When** `context_quarantine` is called with action="restore"
**Then** the entry's status becomes Deprecated AND pre_quarantine_status is set to NULL

### AC-5: Restore with Missing Pre-Quarantine Status (Fallback)

**Given** an entry with status=Quarantined and pre_quarantine_status=NULL
**When** `context_quarantine` is called with action="restore"
**Then** the entry's status becomes Active (fallback) AND pre_quarantine_status is set to NULL

### AC-6: Idempotent Quarantine

**Given** an entry already in Quarantined status
**When** `context_quarantine` is called with action="quarantine"
**Then** the tool returns success with message "already quarantined" and no state changes

### AC-7: Schema Migration v7 to v8

**Given** a database at schema version 7
**When** the store is opened
**Then** the `pre_quarantine_status` column is added to the entries table AND existing quarantined entries have pre_quarantine_status backfilled to 0 (Active) AND schema_version is updated to 8

### AC-8: Status Counter Integrity

**Given** an entry with status=Deprecated
**When** quarantined and then restored
**Then** total_deprecated decrements by 1 on quarantine, total_quarantined increments by 1 on quarantine, total_quarantined decrements by 1 on restore, total_deprecated increments by 1 on restore — net zero change after round-trip

### AC-9: Audit Trail

**Given** a quarantine or restore operation
**When** the operation completes
**Then** an audit_log entry records the operation, agent_id, entry_id, and outcome (including the pre_quarantine_status value in the detail string)

### AC-10: Restore with Invalid Pre-Quarantine Status

**Given** an entry with status=Quarantined and pre_quarantine_status=99 (invalid)
**When** `context_quarantine` is called with action="restore"
**Then** the entry's status becomes Active (fallback) AND the audit detail notes the fallback

## Constraints

- C-1: The migration must be backward-compatible — new column is nullable, no NOT NULL constraint
- C-2: The migration must be idempotent — re-running on a v8 database is a no-op
- C-3: `pre_quarantine_status` must only contain values 0, 1, or 2 (Active, Deprecated, Proposed) — never 3 (Quarantined)
- C-4: Correction eligibility is unchanged — Deprecated and Quarantined entries remain ineligible for correction
- C-5: Search/lookup filtering is unchanged — quarantined entries remain excluded from results

## API Changes

### context_quarantine

No parameter changes. Behavioral change:
- **Before**: Rejects entries not in Active status with "only active entries can be quarantined"
- **After**: Accepts entries in Active, Deprecated, or Proposed status

Response includes pre_quarantine_status information in the detail text when quarantining or restoring.

## Test Strategy Summary

- Unit tests: Status::try_from round-trips, EntryRecord serialization with new field
- Integration tests: Quarantine from each status, restore to each status, round-trip counter integrity, migration test, fallback behavior
- Existing test preservation: All current quarantine/restore tests must pass (Active path unchanged)
