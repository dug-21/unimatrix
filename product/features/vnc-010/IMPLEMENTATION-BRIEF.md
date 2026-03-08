# vnc-010: Implementation Brief

## Summary

Allow quarantine from any non-quarantined status (Active, Deprecated, Proposed) and restore entries to their pre-quarantine status instead of always Active. Schema migration v7->v8 adds `pre_quarantine_status` column.

## Implementation Chunks

### Chunk 1: Schema & Store (unimatrix-store)

**Files**:
- `crates/unimatrix-store/src/schema.rs` — Add `pre_quarantine_status: Option<u8>` to EntryRecord
- `crates/unimatrix-store/src/migration.rs` — Add v7->v8 migration step, bump CURRENT_SCHEMA_VERSION to 8
- `crates/unimatrix-store/src/db.rs` — Add column to CREATE TABLE, update ENTRY_COLUMNS, update entry_from_row, update INSERT in store method, update schema_version init counter to 8

**Changes**:
1. Add `pre_quarantine_status: Option<u8>` field to `EntryRecord` (after `unhelpful_count`, with `#[serde(default)]`)
2. Add `pre_quarantine_status INTEGER` to entries CREATE TABLE in `create_tables()`
3. Update `ENTRY_COLUMNS` constant to include `pre_quarantine_status`
4. Update `entry_from_row()` to read the new column (nullable i64 -> Option<u8>)
5. Update entry INSERT statement to include `pre_quarantine_status` (NULL for new entries)
6. Add migration block for `current_version < 8`: ALTER TABLE + backfill UPDATE
7. Bump `CURRENT_SCHEMA_VERSION` from 7 to 8
8. Update fresh-db counter init from 7 to 8

**Tests**: EntryRecord serialization with new field, entry_from_row with NULL and valid values

### Chunk 2: Server Logic (unimatrix-server)

**Files**:
- `crates/unimatrix-server/src/mcp/tools.rs` — Remove Active-only guard in quarantine path
- `crates/unimatrix-server/src/server.rs` — Update change_status_with_audit to set/clear pre_quarantine_status, update restore_with_audit to use pre_quarantine_status

**Changes**:
1. In `context_quarantine` (tools.rs): Remove the `entry.status != Status::Active` check and its error response. The only remaining guard is the idempotent check for already-quarantined entries.
2. In `change_status_with_audit` (server.rs): When `new_status == Quarantined`, add `pre_quarantine_status = old_status as u8` to the UPDATE SQL. When restoring (new_status != Quarantined), set `pre_quarantine_status = NULL`.
3. In `restore_with_audit` (server.rs): Fetch entry first, read `pre_quarantine_status`, convert via `Status::try_from()`, fall back to Active. Pass the resolved status to `change_status_with_audit`.
4. Update audit detail strings to include pre_quarantine_status info.

**Tests**: Integration tests for all status transitions, counter integrity, fallback behavior

### Chunk 3: Migration Integration Test

**Files**:
- New test in `crates/unimatrix-store/` migration test module or integration tests

**Changes**:
1. Test: create v7 database with quarantined entry, open store (triggers migration), verify pre_quarantine_status=0
2. Test: re-open store, verify no re-migration (idempotent)

## Ordering

Chunk 1 -> Chunk 2 -> Chunk 3 (sequential dependency)

## ADR References

- Unimatrix #600: ADR-001 — Option<u8> storage for pre_quarantine_status
- Unimatrix #601: ADR-002 — Restore fallback to Active

## Risk Hotspots

1. **entry_from_row column index**: Adding a column shifts indices if positional. Verify ENTRY_COLUMNS ordering matches the SELECT.
2. **UPDATE SQL in change_status_with_audit**: Two SQL variants (with/without modified_by) both need the pre_quarantine_status column added.
3. **Migration guard**: Must handle the case where ALTER TABLE fails because column already exists (re-run after partial migration).

## Effort Estimate

Small. ~100-150 lines of production code changes, ~200 lines of test code. Single-session implementation.
