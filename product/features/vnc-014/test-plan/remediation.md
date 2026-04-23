# Test Plan: Append-Only Remediation (retention.rs + import/mod.rs)

## Component Summary

Two DELETE paths on `audit_log` must be removed before the append-only triggers land:

1. **`retention.rs` `gc_audit_log()`**: Issues `DELETE FROM audit_log WHERE timestamp < ...`.
   Per ADR-005, this function is removed or replaced with a no-op returning `Ok(0)`.
2. **`import/mod.rs` `drop_all_data()`**: Issues `DELETE FROM audit_log;` as part of a bulk
   data reset. The `DELETE` line must be removed; audit history is preserved across imports.

These changes must be tested to ensure:
- No SQLite ABORT errors are raised when production code runs post-trigger-installation
- Existing `gc_audit_log` test infrastructure (currently using raw INSERTs and DELETEs)
  does not conflict with the append-only trigger

**Critical ordering**: The DELETE paths must be removed from production code BEFORE the
trigger DDL is applied in the migration. Test setup must reflect this ordering.

---

## Unit Tests

### REM-U-01: `gc_audit_log` removed or returns `Ok(0)` without issuing any DELETE

**Risk**: R-01, AC-05b
**Arrange**: Open a fresh v25 store (triggers installed). Insert one `audit_log` row with a
timestamp older than the retention threshold.
**Act**: Call `store.gc_audit_log(1)` (1-day retention — row would be GC'd by old logic).
**Assert**:
- Returns `Ok(0)` — no rows "deleted"
- `SELECT COUNT(*) FROM audit_log` still returns 1 — row was NOT deleted
- No `SqliteError` with "append-only" message raised

If `gc_audit_log` is fully removed, this test becomes: verify the function does not exist
in the compiled binary (confirmed by compile error at any call site).

---

### REM-U-02: Existing `gc_audit_log` tests updated for no-op or removal

**Risk**: R-01
**Context**: `retention.rs` contains tests that insert rows into `audit_log` and verify
they are deleted by `gc_audit_log`. These tests (`test_gc_audit_log_retention_boundary`,
`test_gc_protected_tables_row_level`, `test_gc_audit_log_epoch_row_deleted`) will FAIL once
triggers are installed if they still use `DELETE` for teardown.

**Required update**: Each of these tests must either:
- Be removed (if `gc_audit_log` is fully removed), OR
- Be rewritten to use in-memory databases (`:memory:`) so teardown is by DB destruction,
  not row deletion

**Assert**: `cargo test --workspace` passes with zero failures in `retention.rs` tests.

---

### REM-U-03: `drop_all_data` completes without error — audit rows preserved

**Risk**: R-01, AC-05b, IR-04
**Arrange**: Open a v25 store (triggers installed). Insert 3 `audit_log` rows and 5 `entries`
rows.
**Act**: Call `import::drop_all_data(pool)` (or the updated function that removes only
non-audit data).
**Assert**:
- Returns `Ok(_)` — no "append-only" ABORT error
- `SELECT COUNT(*) FROM audit_log` returns 3 (unchanged — audit history preserved)
- `SELECT COUNT(*) FROM entries` reflects the reset (0 or baseline count)

---

### REM-U-04: Raw DELETE via sqlx raises trigger ABORT — trigger is enforced

**Risk**: R-01, AC-05b (verification that trigger fires)
**Arrange**: Open a v25 store. Insert one `audit_log` row.
**Act**: Execute `sqlx::query("DELETE FROM audit_log WHERE event_id = ?1").bind(event_id)`.
**Assert**:
- Returns `Err(_)`
- Error string contains `"audit_log is append-only: DELETE not permitted"`

---

### REM-U-05: Raw UPDATE via sqlx raises trigger ABORT

**Risk**: R-01, AC-05b
**Arrange**: Open a v25 store. Insert one `audit_log` row.
**Act**: Execute `sqlx::query("UPDATE audit_log SET detail = 'x' WHERE event_id = ?1").bind(id)`.
**Assert**:
- Returns `Err(_)`
- Error string contains `"audit_log is append-only: UPDATE not permitted"`

---

### REM-U-06: `gc_audit_log` call site removed from background tick

**Risk**: R-01
**Assert**: Code inspection — no call to `gc_audit_log` in `background.rs` background tick
scheduling. `cargo grep "gc_audit_log" crates/unimatrix-server/src/background.rs` returns 0.

---

### REM-U-07: `drop_all_data` in import/mod.rs does not contain `DELETE FROM audit_log`

**Risk**: R-01
**Assert**: Code inspection — the `drop_all_data` function body in
`crates/unimatrix-server/src/import/mod.rs` does not contain the string
`"DELETE FROM audit_log"`.
`cargo grep "DELETE FROM audit_log" crates/unimatrix-server/src/import/mod.rs` returns 0.

---

## Interaction with Existing Retention Test Suite

The existing `retention.rs` test suite has approximately 6 tests that use `audit_log` rows
and `gc_audit_log`. These must be audited before Stage 3b implementation:

| Existing Test | Required Change |
|---------------|----------------|
| `test_gc_audit_log_retention_boundary` | Remove or rewrite (uses DELETE-based teardown) |
| `test_gc_protected_tables_row_level` | Rewrite to use `:memory:` db |
| `test_gc_audit_log_epoch_row_deleted` | Remove (test validates deleted behavior) |
| `test_gc_protected_tables_regression` | Verify: does it touch audit_log via DELETE? |
| `test_gc_query_log_pruned_with_cycle` | Likely unaffected (tests query_log, not audit_log) |
| `test_gc_cascade_delete_order` | Verify: does it include audit_log? |

**Stage 3c reviewer action**: Before running `cargo test`, verify none of the surviving
retention tests issue raw DELETEs against `audit_log` on a triggered database.

---

## Import Path Variant Decision

The specification (FR-11, OQ-C) presents two options for `drop_all_data`:

- **Option A (preferred)**: Replace `DELETE FROM audit_log` with a DROP + re-CREATE or
  call a store method.
- **Option B**: Use a `cfg(test)` flag or explicit import-mode flag to disable the trigger.

The delivery agent must choose one and document the decision. REM-U-03 must pass regardless
of which option is chosen. The test plan is written to be option-agnostic: it tests the
observable behavior (no error, audit rows preserved), not the implementation mechanism.
