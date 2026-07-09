# C1 — `cycle_tags` table + migration (schema v31)

**Files:** `crates/unimatrix-store/src/migration.rs`, `crates/unimatrix-store/src/db.rs`
**ADR:** ADR-001. **Risks:** R-01, R-13, R-10. **AC:** AC-03a–d.

## Purpose

Create the `cycle_tags(feature_cycle, tag)` junction — the durable source of truth for cycle
tags — on ALL THREE DB-init paths, and advance `CURRENT_SCHEMA_VERSION` 30→31 (version cascade #1).
This is `entry_tags` (create_tables_if_needed db.rs:573; migration.rs:1689) re-keyed
`entry_id → feature_cycle`, with **NO FK** (`feature_cycle` is free-text, no parent table — parity
with `cycle_events.cycle_id`).

## DDL (identical on every path — emitted via IF NOT EXISTS)

```sql
CREATE TABLE IF NOT EXISTS cycle_tags (
    feature_cycle TEXT NOT NULL,
    tag           TEXT NOT NULL,
    PRIMARY KEY (feature_cycle, tag)
);
CREATE INDEX IF NOT EXISTS idx_cycle_tags_tag ON cycle_tags(tag);
```

The `(tag)` index is the substrate for the deferred cross-cycle query direction (no re-migration
later, SR-04/NFR-7). PK `(feature_cycle, tag)` gives row integrity; it is NOT the freeze mechanism
(the freeze is C2's EXISTS guard + BEGIN IMMEDIATE).

## Path 0 — re-verify version is free (implementation start, SR-02/R-10)

```
PRECHECK (record in coverage report, not a runtime test):
    assert CURRENT_SCHEMA_VERSION currently reads 30 at HEAD  (migration.rs:26)
    if it reads != 30  →  a parallel feature claimed 31; STOP and flag for renumber
```

## Path 1 — constant bump (`migration.rs:26`)

```
CHANGE  const CURRENT_SCHEMA_VERSION: u64 = 30   →   31
```

## Path 2 — fresh-create (`db.rs::create_tables_if_needed`, ~:534, beside entry_tags ~:573)

```
FUNCTION create_tables_if_needed(pool):
    … existing entries / entry_tags DDL …
    execute("CREATE TABLE IF NOT EXISTS cycle_tags ( … PRIMARY KEY(feature_cycle, tag) )")
    execute("CREATE INDEX IF NOT EXISTS idx_cycle_tags_tag ON cycle_tags(tag)")
    … existing remaining DDL …
```

Place the two statements next to the `entry_tags` block so the fresh-create and migration DDL stay
lexically parallel (guards DDL drift between the two routes, R-01 scenario 4 / #376).

## Path 3 — migration step (`migration.rs`, a NEW block AFTER `if current_version < 30` ~:1474)

```
if current_version < 31 {
    execute("CREATE TABLE IF NOT EXISTS cycle_tags ( … PRIMARY KEY(feature_cycle, tag) )")
    execute("CREATE INDEX IF NOT EXISTS idx_cycle_tags_tag ON cycle_tags(tag)")
    // DO NOT stamp schema_version inside this block.
}
```

- Use `<` (not `==`) so the block runs for any DB older than 31 (pattern #836, #4153).
- **Idempotency guard = `CREATE TABLE/INDEX IF NOT EXISTS`.** A brand-new table needs NO
  `pragma_table_info`/`sqlite_master` COUNT pre-check — that pre-check is only for `ALTER TABLE ADD
  COLUMN` (this is not one). Re-running the block is a no-op (ADR-001 §3, R-13).
- **Schema-version stamp happens ONCE at the end of the main migration txn** (existing
  `INSERT OR REPLACE INTO counters … VALUES ('schema_version', CURRENT_SCHEMA_VERSION)` ~:1585-1587).
  Do NOT add a per-block stamp (pattern #836).
- This is additive DDL → runs inside the existing main `migrate_if_needed` transaction (ADR #820).

## Path 4 — pinned schema-version / migration-hygiene test (AC-03d)

```
UPDATE any test asserting schema_version == 30 (exact equality) to >= 31,
       OR capture-after-first-open + assert-equality-on-second-open for idempotency
       (pattern #4153: hardcoded == breaks the moment a later migration bumps past N).
```

## Error handling

- All DDL is infallible-by-construction under `IF NOT EXISTS`; a genuine sqlx failure propagates as
  `StoreError::Database` up the existing migration error path (unchanged).
- No new error variants.

## Key test scenarios (hints — full plan in test-plan/)

1. **Fresh-create (AC-03b):** init a v31 DB via `create_tables_if_needed`; assert `cycle_tags` exists
   with PK `(feature_cycle, tag)` and `idx_cycle_tags_tag` present.
2. **Migration (AC-03a/c):** init a **populated** v30 DB, migrate to v31; assert `cycle_tags` + index
   created, existing data intact, constant reads 31 (R-13 #378).
3. **Idempotent re-run:** run migration again on an already-v31 DB → no error (R-13).
4. **DDL parity (R-01 #376):** fresh-create schema and post-migration schema are structurally
   identical for `cycle_tags` (same DDL both routes).
5. **Pinned test updated and green (AC-03d).**
6. **Cascade separation:** this cascade is proven WITHOUT touching SUMMARY v6 assertions (SR-01).
