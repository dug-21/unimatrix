## ADR-003: v20→v21 Migration Atomicity — Single Transaction for Both ADD COLUMN Statements

### Context

crt-043 adds two columns in a single schema migration step (v20 → v21):
- `cycle_events.goal_embedding BLOB NULL`
- `observations.phase TEXT NULL`

SR-04 identifies the risk that if one ALTER TABLE succeeds and the other fails, the schema
version counter may still be bumped, leaving the database in an inconsistent state.

The existing migration infrastructure (`migration.rs`) wraps all `run_main_migrations` work
in a single outer transaction opened in `migrate_if_needed()`:

```rust
let mut txn = conn.begin().await?;
let main_result = run_main_migrations(&mut txn, current_version, db_path).await;
match main_result {
    Ok(()) => { txn.commit().await?; }
    Err(e) => { let _ = txn.rollback().await; return Err(e); }
}
```

All `current_version < N` blocks in `run_main_migrations` share this outer transaction.
The schema_version counter bump at the end of the function (`INSERT OR REPLACE INTO counters`)
is inside the same transaction. If any statement fails, the entire transaction rolls back —
including the version bump.

The `pragma_table_info` pre-check pattern (established in vnc-010, entry #1264) guards each
ADD COLUMN statement against re-execution on an already-migrated database:

```rust
// Pre-check idempotency before ALTER TABLE (entry #1264 pattern)
let col_exists: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'"
)
.fetch_one(&mut **txn)
.await?;

if col_exists == 0 {
    sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal_embedding BLOB")
        .execute(&mut **txn)
        .await?;
}
```

Both checks must execute before either ALTER runs, so that a partially-applied previous attempt
(if it somehow occurred outside a transaction) is detected correctly.

### Decision

**The v20→v21 migration uses the existing outer transaction from `migrate_if_needed` as its
atomicity boundary. No additional `BEGIN`/`COMMIT` is needed.**

The `current_version < 21` block in `run_main_migrations` must:

1. Check `pragma_table_info('cycle_events')` for `goal_embedding` column existence
2. Check `pragma_table_info('observations')` for `phase` column existence
3. If either check shows the column absent, execute the corresponding ALTER TABLE
4. At the end of the v21 block, bump schema_version to 21 with
   `UPDATE counters SET value = 21 WHERE name = 'schema_version'`

Both ALTER TABLE statements execute on `&mut **txn` (the outer transaction). If either fails,
the outer transaction rolls back. The schema_version counter is not bumped. On the next
`Store::open()`, the migration re-runs from the beginning of the v21 block — both pre-checks
execute again, and only the absent column is added.

**Order:** `cycle_events.goal_embedding` first, then `observations.phase`. Order is arbitrary
but must be consistent to avoid confusion in code review.

**Idempotency guarantee:** A database already at v21 returns immediately at the
`current_version >= CURRENT_SCHEMA_VERSION` guard in `migrate_if_needed` — neither pre-check
nor ALTER TABLE runs. A database where the v21 block partially succeeded (one column added,
one absent) is corrected on the next open: the pre-check for the added column returns 1
(skip), the pre-check for the absent column returns 0 (add it).

**CURRENT_SCHEMA_VERSION** must be updated from 20 to 21 in `migration.rs`. The constant is
referenced in `db.rs` at the fresh-database initialization path and in the `context_status`
tool response — both will automatically reflect the new version.

**Note on entry #378 lesson (test against real v20 database):** The v21 migration must be
validated in an integration test that opens a real v20 database through `Store::open()`, not
just a fresh schema. The existing `migration_integration` test pattern must be extended to
cover v20→v21, verifying both columns appear and the schema_version counter reads 21.

### Consequences

Easier:
- Both column additions are atomic — no partial-migration inconsistency possible
- The `pragma_table_info` pre-checks make the v21 block safe to re-run after partial failure
- No additional transaction management code required; the outer transaction is already present

Harder:
- SQLite's `ALTER TABLE ADD COLUMN` does not support `IF NOT EXISTS`, so the pre-check pattern
  (two extra SELECT queries per column) is required for every new column
- Both pre-checks must execute before either ALTER; the implementation must not interleave
  check-then-alter for each column independently (this would be correct but less readable)
