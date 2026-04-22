## ADR-004: Four-Column audit_log Migration Idempotency via Pre-Flight pragma_table_info Checks

### Context

VNC-014 delivers the full ASS-050 schema migration: four new columns on `audit_log` plus two
indexes and two append-only DDL triggers, all in a single schema version bump.

SQLite `ALTER TABLE ADD COLUMN` is not idempotent. If the process crashes between the first
ALTER and the schema_version counter commit, a re-run will fail on the already-added column.
This is a known pattern — pattern #4092 (confirmed in Unimatrix, first established vnc-010,
extended in crt-043) documents the guard.

The multi-column ordering rule from pattern #4092 is critical here: run ALL four
`pragma_table_info` existence checks before executing ANY ALTER TABLE. This prevents a
partial-state scenario where:
1. Check-A passes, ALTER-A runs
2. Process crashes before schema_version bump
3. Re-run: Check-A sees column exists (skip), Check-B runs against partially-applied state

Running all four checks first produces a consistent decision set before any mutations begin.

The migration also installs `CREATE TRIGGER` statements. `CREATE TRIGGER IF NOT EXISTS` is
idempotent by the `IF NOT EXISTS` clause — no guard needed. Similarly `CREATE INDEX IF NOT
EXISTS` is idempotent.

`credential_type` has default `'none'` (not `''`). This is intentional (OSS default transport
is stdio, not credentialed). The field is not nullable — empty string is not used.

All four columns in one version bump is mandated by SCOPE.md constraints — the ASS-050 spec
is complete and they must land together.

The schema version cascade pattern (#4125) applies: `CURRENT_SCHEMA_VERSION` in `migration.rs`
must be bumped from 24 to 25, and all cascade touchpoints updated (sqlite_parity.rs column count,
server.rs version assertions, previous migration test file renamed to `at_least_N`).

Additionally, the **append-only triggers fundamentally break the existing GC path** in
`retention.rs::gc_audit_log()` (DELETE by timestamp) and the import path in `import/mod.rs`
(DELETE FROM audit_log during full import). Both must be remediated (ADR-005).

### Decision

Migration block `if current_version < 25`:

1. Run four `pragma_table_info('audit_log')` checks:
   - `has_credential_type`
   - `has_capability_used`
   - `has_agent_attribution`
   - `has_metadata`

2. Execute only the ALTER TABLE statements for columns that do not yet exist:
   ```sql
   ALTER TABLE audit_log ADD COLUMN credential_type   TEXT NOT NULL DEFAULT 'none';
   ALTER TABLE audit_log ADD COLUMN capability_used   TEXT NOT NULL DEFAULT '';
   ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT '';
   ALTER TABLE audit_log ADD COLUMN metadata          TEXT NOT NULL DEFAULT '{}';
   ```

3. Create indexes (inherently idempotent):
   ```sql
   CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
   CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);
   ```

4. Create append-only triggers (inherently idempotent):
   ```sql
   CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
       BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;
   CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
       BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
   ```

5. Bump schema_version to 25.

Step 3 and 4 run unconditionally (idempotent DDL). The pre-flight checks in step 1 govern only
the ALTER TABLE statements in step 2.

### Consequences

Easier:
- Migration is safe to re-run on partial completion
- All four columns land atomically relative to the version bump
- Triggers and indexes are idempotent by DDL clause

Harder:
- The append-only triggers permanently change the semantics of `audit_log` — all existing
  DELETE paths must be removed or replaced BEFORE the migration runs (ADR-005)
- The schema version cascade checklist (#4125) applies: 7+ touchpoints across test files
- `db.rs` `create_tables_if_needed()` DDL must be updated to match the migration DDL exactly
  (byte-identical for the new columns and triggers)
