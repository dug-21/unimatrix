# vnc-010: Quarantine State Restoration

## Problem Statement

Quarantine currently only works from Active status (crt-003 implementation). Two deficiencies:

1. **Cannot quarantine deprecated entries** (#43). Deprecated entries discovered to contain bad information (stale patterns, incorrect conventions) cannot be quarantined. They continue to leak through semantic search since deprecation only applies a confidence penalty — it does not fully hide entries like quarantine does.

2. **Restore always returns to Active**. The `restore_with_audit` method hardcodes `Status::Active` as the target. An entry that was Deprecated before quarantine should return to Deprecated, not Active — otherwise quarantine-then-restore promotes entries incorrectly.

## Root Cause

The original crt-003 quarantine design assumed quarantine would only apply to Active entries. No mechanism exists to track an entry's pre-quarantine status.

- `tools.rs:877`: `if entry.status != Status::Active` guard rejects Deprecated/Proposed
- `server.rs:833`: `restore_with_audit` hardcodes `Status::Active`
- `EntryRecord` has no `pre_quarantine_status` field
- `entries` table has no `pre_quarantine_status` column

## Scope

### In Scope

- Allow quarantine from Active, Deprecated, and Proposed statuses
- Add `pre_quarantine_status` field to EntryRecord (Option<u8>)
- Add `pre_quarantine_status` column to entries table (INTEGER, nullable)
- Schema migration v7 -> v8 (add column, backfill existing quarantined entries as Active)
- Restore returns entry to its pre-quarantine status instead of always Active
- Update context_quarantine tool validation
- Update change_status_with_audit to record/use pre_quarantine_status
- Unit and integration tests for all new paths

### Out of Scope

- Quarantine from Quarantined status (already idempotent — no change needed)
- Bulk quarantine operations
- Quarantine cascading (e.g., quarantining an entry and its supersession chain)
- Changes to confidence computation for quarantined entries
- UI/dashboard changes

## Key Stakeholders

- **unimatrix-store**: Schema change, EntryRecord field addition, migration
- **unimatrix-server**: Tool handler logic, change_status_with_audit updates
- **All agents**: Can now quarantine entries from any non-quarantined status

## Success Criteria

1. `context_quarantine` accepts entries in Active, Deprecated, or Proposed status
2. Pre-quarantine status is stored in the database when quarantine occurs
3. `context_quarantine action=restore` returns entries to their pre-quarantine status
4. Schema migration v7->v8 is backward-compatible (new column is nullable)
5. Existing quarantined entries (from v7) get backfilled with pre_quarantine_status=Active
6. All existing quarantine/restore tests continue to pass
7. New tests cover Deprecated->Quarantined->Deprecated round-trip

## Risks

- SR-01: Migration must handle existing quarantined entries that have no pre_quarantine_status (backfill as Active — the only valid source status under old logic)
- SR-02: Status counter bookkeeping must correctly decrement/increment for non-Active source statuses
- SR-03: The `correct` operation rejects quarantined entries — no change needed, but must verify Deprecated entries can still be corrected (they are rejected too, so no interaction)

## Dependencies

- None. This is Wave 1 (parallel, no prerequisites).

## Effort Estimate

Small. 1 schema column + 1 struct field + ~20 lines of logic changes + migration step + tests.
