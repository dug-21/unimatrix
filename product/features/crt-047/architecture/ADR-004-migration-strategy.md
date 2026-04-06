## ADR-004: Migration Strategy — v23 to v24 Atomicity and Three-Path Update

### Context

SR-03 flags that three locations must all be updated when adding columns to an existing
table: `migration.rs` (incremental migration logic), `db.rs` (fresh-schema DDL), and the
legacy static DDL array (which is `migration_compat.rs`, only used for very old v5→v6
migrations — not relevant here). In practice, for the v24 migration, two active paths
matter: `migration.rs` and `db.rs`.

The v23→v24 change adds multiple columns to `cycle_review_index`. SQLite does not support
`ADD COLUMN IF NOT EXISTS`, so each column requires a `pragma_table_info` pre-check (the
established pattern, confirmed in crt-043 ADR-003 entry #4088).

The migration must be atomic: if any column addition fails, the schema version must NOT
advance to 24. The codebase uses an outer transaction atomicity boundary: all `ADD COLUMN`
statements and the `UPDATE counters SET value = N` are executed within the same
transaction. If any step fails, the entire transaction rolls back and the version stays at
23.

SR-02 flags that a parallel in-flight feature could claim v24 before crt-047 merges. This
is a pre-delivery check: `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs`
must confirm v23 immediately before the delivery pseudocode phase begins.

### Decision

**Migration block structure** (`migration.rs` v23→v24):

The seven new columns (five snapshot columns + `corrections_system` + `first_computed_at`
per ADR-001 and ADR-002) are all added in a single `if current_version < 24` block within
the existing outer transaction. Each column uses a `pragma_table_info` pre-check:

```rust
if current_version < 24 {
    // Column: corrections_total
    let has_col: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = 'corrections_total'"
    ).fetch_one(&mut **txn).await.map_err(...)? > 0;
    if !has_col {
        sqlx::query("ALTER TABLE cycle_review_index ADD COLUMN corrections_total INTEGER NOT NULL DEFAULT 0")
            .execute(&mut **txn).await.map_err(...)?;
    }
    // ... repeat for corrections_agent, corrections_human, corrections_system,
    //     deprecations_total, orphan_deprecations, first_computed_at
    
    sqlx::query("UPDATE counters SET value = 24 WHERE name = 'schema_version'")
        .execute(&mut **txn).await.map_err(...)?;
}
```

The `schema_version` counter is updated to 24 at the END of the block, after all columns
are verified or added. If any individual `ALTER TABLE` fails (e.g., a race condition or
partial schema state), the transaction rolls back and the counter stays at 23.

`first_computed_at` uses `DEFAULT 0` (not `DEFAULT NULL`) so that existing rows have a
stable, non-NULL value. The baseline ordering query treats `first_computed_at = 0` rows
as legacy rows with unknown temporal position. In `get_curation_baseline_window()`, rows
with `first_computed_at = 0` are excluded from the ordered window to prevent legacy
rows from dominating the top of the list:

```sql
SELECT ... FROM cycle_review_index
WHERE first_computed_at > 0
ORDER BY first_computed_at DESC
LIMIT ?1
```

**`db.rs` fresh-schema DDL update**:

The `CREATE TABLE IF NOT EXISTS cycle_review_index` statement in `db.rs` must include all
seven new columns with their `DEFAULT` clauses. The DDL must be byte-consistent with the
post-migration schema to avoid a schema drift between fresh databases (which get the DDL
directly) and migrated databases (which get the migration path). This is the existing
pattern documented in `db.rs` line 952-953: "DDL must be byte-identical to the vNN
migration block."

**`CURRENT_SCHEMA_VERSION`** in `migration.rs` is bumped from `23` to `24`. The constant
in `db.rs` is not a separate constant — it references `crate::migration::CURRENT_SCHEMA_VERSION`
via binding (confirmed at db.rs line 978). Only `migration.rs` needs the version bump.

**Required integration test** (AC-14):

The test must open a real database through `Store::open()` against a synthetic v23
database (one that has `cycle_review_index` without the new columns), not just call the
migration function in isolation. Evidence from SR-03 and GH #378: isolation testing of
the migration function does not catch DDL path bugs. The test must verify:

1. `Store::open()` succeeds on the v23 fixture database
2. All seven new columns appear in `pragma_table_info('cycle_review_index')`
3. Existing rows in `cycle_review_index` have `DEFAULT 0` for all new integer columns
4. `CURRENT_SCHEMA_VERSION` counter equals 24 after migration

The v23 fixture database should be a minimal SQLite file with the old `cycle_review_index`
DDL and at least one pre-existing row.

### Consequences

- **Easier**: Atomicity is guaranteed — a partial migration (some columns added, some not)
  cannot leave the database in a state where `schema_version = 24` but columns are
  missing. The `pragma_table_info` pre-check makes individual column additions idempotent,
  so a retry after partial failure (e.g., process crash mid-migration) works correctly.
- **Easier**: Fresh-schema databases and migrated databases converge to the same schema
  because `db.rs` DDL and `migration.rs` ADD COLUMN statements are maintained in sync.
- **Harder**: Seven columns is more boilerplate in the migration block than one. The
  `pragma_table_info` pre-check pattern must be repeated seven times. This is accepted
  cost — the alternative (`DROP TABLE / RECREATE`) would lose data.
- **Pre-delivery check required**: SM must verify `CURRENT_SCHEMA_VERSION = 23` in
  `migration.rs` before delivery pseudocode begins. If another feature has already bumped
  to 24, the version number in this ADR and all design artifacts must be renumbered.
- **Consequence**: The `first_computed_at = 0` exclusion in `get_curation_baseline_window()`
  means legacy cycle rows (migrated from v23 with no real `first_computed_at`) are
  invisible to the baseline window. This is correct behavior: they have no curation data
  and no temporal anchor. Operators who want historical cycles in the baseline must
  `force=true` each one to populate both the snapshot data and a real `first_computed_at`.
