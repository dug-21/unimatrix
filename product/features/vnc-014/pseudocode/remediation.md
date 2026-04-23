# Component: Append-Only Remediation (retention.rs + import/mod.rs)

## Purpose

Remove the two DELETE paths on `audit_log` that will be rejected by the
`audit_log_no_delete` trigger installed in the v24→v25 migration.

1. `gc_audit_log()` in `retention.rs` — replace with a no-op that preserves
   the function signature (callers may still reference it).
2. `drop_all_data()` in `import/mod.rs` — remove `DELETE FROM audit_log`
   from the bulk delete statement.

**Files modified:**
- `crates/unimatrix-store/src/retention.rs`
- `crates/unimatrix-server/src/import/mod.rs`

---

## Modified Functions

### `SqlxStore::gc_audit_log` (retention.rs)

**Current behavior**: Issues `DELETE FROM audit_log WHERE timestamp < (strftime('%s', 'now') - ?1 * 86400)`.

**New behavior**: No-op returning `Ok(0)`. A WARN is logged explaining why GC
is deferred.

```
/// Time-based audit log GC — deferred (append-only model).
///
/// The audit_log table is protected by BEFORE DELETE triggers installed in
/// schema v25 (vnc-014 / ASS-050). Any DELETE statement will be rejected by
/// the trigger with ABORT. Time-based GC is therefore not possible without
/// a DROP+recreate strategy.
///
/// Retention policy for audit_log is deferred to a future feature.
/// This method is retained as a no-op to preserve the call signature used
/// by background.rs.
///
/// Returns Ok(0) — no rows deleted.
pub async fn gc_audit_log(&self, retention_days: u32) -> Result<u64>:
    tracing::warn!(
        retention_days,
        "gc_audit_log is a no-op: audit_log is append-only (vnc-014). \
         Time-based GC deferred to future retention policy feature."
    )
    return Ok(0)
```

The function body loses its connection acquisition and DELETE statement.
The `retention_days` parameter is retained in the signature (used only in
the log message) to avoid changing callers in `background.rs`.

**Background tick caller**: The call site in `background.rs` that calls
`gc_audit_log` continues to work — it receives `Ok(0)` and logs 0 deleted
rows. No change needed at that call site.

**Tests in retention.rs**: The existing `test_gc_audit_log_retention_boundary`
and `test_gc_audit_log_epoch_row_deleted` tests insert audit rows and assert
on deletion counts. These tests will break when `gc_audit_log` becomes a no-op.

The tests must be updated:
- Remove assertions on rows deleted by `gc_audit_log` (will always be 0)
- If the test's purpose was to verify `gc_audit_log` behavior, replace it
  with a test that verifies the no-op behavior: call `gc_audit_log(180)` and
  assert the return value is `Ok(0)` and rows are unchanged.
- Tests that insert audit rows directly (using raw INSERT) will trigger the
  append-only INSERT path but not the DELETE trigger — inserts remain valid.
  Only DELETE and UPDATE are blocked.

Note: The raw `INSERT INTO audit_log ...` statements in the test helper closures
within retention.rs tests (which insert with explicit event_id and timestamp)
are INSERT operations, not DELETEs — they remain valid under the append-only
model.

---

### `drop_all_data` (import/mod.rs)

**Current behavior**: Executes a multi-table DELETE including `DELETE FROM audit_log`.

**New behavior**: Remove `DELETE FROM audit_log` from the statement. Audit
history is preserved across import resets (ADR-005).

```
// BEFORE:
async fn drop_all_data(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>>:
    sqlx::query(
        "DELETE FROM entry_tags;
         DELETE FROM co_access;
         DELETE FROM feature_entries;
         DELETE FROM outcome_index;
         DELETE FROM audit_log;
         DELETE FROM agent_registry;
         DELETE FROM vector_map;
         DELETE FROM entries;
         DELETE FROM counters;"
    ).execute(pool).await?
    Ok(())

// AFTER:
async fn drop_all_data(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>>:
    // audit_log is excluded: append-only triggers (vnc-014 / ASS-050 schema v25)
    // reject DELETE statements. Audit history is preserved across import resets
    // per ADR-005. See retention.rs gc_audit_log for the GC deferral note.
    sqlx::query(
        "DELETE FROM entry_tags;
         DELETE FROM co_access;
         DELETE FROM feature_entries;
         DELETE FROM outcome_index;
         DELETE FROM agent_registry;
         DELETE FROM vector_map;
         DELETE FROM entries;
         DELETE FROM counters;"
    ).execute(pool).await?
    Ok(())
```

The comment must be present — delivery agent decision, documented in the PR
per SPECIFICATION.md FR-11 requirement.

---

## Semantic Change: Import Reset Behavior

After this change, `unimatrix import --force` no longer clears `audit_log`.
This is an intentional behavioral change:

- Audit log accumulates indefinitely (append-only by design)
- An import reset preserves all prior audit history
- The row count in `audit_log` is unaffected by `--force`

Integration tests for the import path that previously relied on `audit_log`
being empty after `drop_all_data` must be updated to not assert on audit log
row counts post-import-reset (IR-04).

---

## Error Handling

- `gc_audit_log`: the new implementation returns `Ok(0)` unconditionally.
  No database connection is acquired. No new error paths.
- `drop_all_data`: removing one DELETE line cannot introduce new errors.
  If the previous multi-statement execution succeeded, the new version
  (with fewer statements) also succeeds.

---

## Key Test Scenarios

1. **gc_audit_log no-op (R-01, AC-05b)**: Call `gc_audit_log(180)` on a
   database with audit rows. Assert return value is `Ok(0)`. Assert row
   count in `audit_log` is unchanged. Assert WARN was logged.

2. **drop_all_data does not clear audit_log (R-01, AC-05b, IR-04)**:
   Insert an audit row. Call `drop_all_data`. Assert the audit row is still
   present. Assert all other tables (entries, co_access, etc.) are empty.

3. **Trigger fires on manual DELETE (AC-05b, R-01)**: After migration to v25,
   attempt `sqlx::query("DELETE FROM audit_log WHERE event_id = 1").execute(...)`.
   Assert error message contains `"audit_log is append-only: DELETE not permitted"`.

4. **Trigger fires on UPDATE (AC-05b, R-01)**: After migration,
   attempt `sqlx::query("UPDATE audit_log SET detail = 'x' WHERE event_id = 1").execute(...)`.
   Assert error message contains `"audit_log is append-only: UPDATE not permitted"`.

5. **INSERT still works (regression)**: After trigger installation, call
   `log_audit_event` and confirm the row is inserted successfully. The BEFORE
   DELETE and BEFORE UPDATE triggers do not affect INSERT.

6. **gc_audit_log old tests updated**: Confirm that `test_gc_audit_log_retention_boundary`
   and `test_gc_audit_log_epoch_row_deleted` are either removed or replaced with
   no-op behavior tests. They must not assert positive row deletion counts.

7. **Background tick survives (R-12 adjacent)**: If the background tick calls
   `gc_audit_log`, it receives `Ok(0)` and continues normally. No panic, no error
   logged at ERROR level.
