# store-migration — Pseudocode

**File**: `crates/unimatrix-store/src/migration.rs`
**Changes**:
- Bump `CURRENT_SCHEMA_VERSION` from `12` to `13`
- Add constant `CO_ACCESS_BOOTSTRAP_MIN_COUNT`
- Add `v12 → v13` block in `run_main_migrations`

---

## Purpose

Bootstrap the `graph_edges` table from existing data sources on existing databases
upgrading from schema v12. The migration runs inside the existing `run_main_migrations`
transaction, following the `if current_version < N` guard pattern established by all
prior migration blocks.

---

## Constant Additions

At the top of migration.rs, alongside `CURRENT_SCHEMA_VERSION`:

```
/// Current schema version. Incremented from 12 to 13 by crt-021 (W1-1).
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 13;

/// Minimum co-access count to bootstrap a CoAccess edge into graph_edges.
/// Pairs below this threshold are too infrequent to represent meaningful relationships.
const CO_ACCESS_BOOTSTRAP_MIN_COUNT: i64 = 3;
```

`CO_ACCESS_BOOTSTRAP_MIN_COUNT` is `i64` to match sqlx binding conventions for SQLite
integer parameters. The value `3` matches FR-09 and the architecture §2b specification.

---

## v12→v13 Migration Block

Insert this block in `run_main_migrations` after the `v11 → v12` block
(or after the last existing `if current_version < 12` block):

```
-- v12 → v13: GRAPH_EDGES table + bootstrap inserts (crt-021)
IF current_version < 13:

    -- Step 1: Create graph_edges table (idempotent — CREATE TABLE IF NOT EXISTS)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS graph_edges (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id      INTEGER NOT NULL,
            target_id      INTEGER NOT NULL,
            relation_type  TEXT    NOT NULL,
            weight         REAL    NOT NULL DEFAULT 1.0,
            created_at     INTEGER NOT NULL,
            created_by     TEXT    NOT NULL DEFAULT '',
            source         TEXT    NOT NULL DEFAULT '',
            bootstrap_only INTEGER NOT NULL DEFAULT 0,
            metadata       TEXT    DEFAULT NULL,
            UNIQUE(source_id, target_id, relation_type)
        )"
    )
    .execute(&mut **txn).await?

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id ON graph_edges(source_id)"
    )
    .execute(&mut **txn).await?

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id ON graph_edges(target_id)"
    )
    .execute(&mut **txn).await?

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type)"
    )
    .execute(&mut **txn).await?

    -- Step 2: Bootstrap Supersedes edges from entries.supersedes
    --
    -- Edge direction: source_id = entry.supersedes (old/replaced),
    --                 target_id = entry.id (new/correcting).
    -- This matches graph.rs edge direction: pred_id → entry.id when entry.supersedes = pred_id.
    -- Outgoing edges point toward newer knowledge (ARCHITECTURE §1, ALIGNMENT-REPORT VARIANCE 1).
    --
    -- bootstrap_only = 0 because entries.supersedes is authoritative (not heuristic).
    -- INSERT OR IGNORE: idempotent on re-run via UNIQUE(source_id, target_id, relation_type).
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
         SELECT
             supersedes        AS source_id,
             id                AS target_id,
             'Supersedes'      AS relation_type,
             1.0               AS weight,
             strftime('%s','now') AS created_at,
             'bootstrap'       AS created_by,
             'entries.supersedes' AS source,
             0                 AS bootstrap_only
         FROM entries
         WHERE supersedes IS NOT NULL"
    )
    .execute(&mut **txn).await?

    -- Step 3: Bootstrap CoAccess edges from co_access (count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT)
    --
    -- Weight formula: COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)
    --   - MAX(count) OVER () is a window function computing max over the filtered rows.
    --   - NULLIF(..., 0) guards against a theoretical all-zero count table (division by zero → NULL).
    --   - COALESCE(..., 1.0) handles the case where the subquery selects zero rows
    --     (empty co_access table or all counts below threshold) — R-06 mitigation.
    --   - On a clean install with zero rows matching count >= 3, the INSERT selects zero rows
    --     and executes successfully with no data written.
    --   - On a populated table, this produces normalized weights in (0.0, 1.0].
    --
    -- bootstrap_only = 0 because co_access counts at threshold >= 3 are authoritative signals.
    -- INSERT OR IGNORE: idempotent on re-run.
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
         SELECT
             entry_id_a        AS source_id,
             entry_id_b        AS target_id,
             'CoAccess'        AS relation_type,
             COALESCE(
                 CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0),
                 1.0
             )                 AS weight,
             strftime('%s','now') AS created_at,
             'bootstrap'       AS created_by,
             'co_access'       AS source,
             0                 AS bootstrap_only
         FROM co_access
         WHERE count >= ?1"
    )
    .bind(CO_ACCESS_BOOTSTRAP_MIN_COUNT)
    .execute(&mut **txn).await?

    -- Step 4: No Contradicts bootstrap. shadow_evaluations has no entry ID pairs.
    --         All Contradicts edges are created at runtime by W1-2 NLI. (FR-10, AC-08)
    --         This comment block documents the decision; no SQL is emitted.

    -- Step 5: Update schema version to 13
    sqlx::query("UPDATE counters SET value = 13 WHERE name = 'schema_version'")
        .execute(&mut **txn).await?
```

---

## Error Handling

All `sqlx::query(...).execute(&mut **txn).await?` calls propagate `sqlx::Error` to the
caller via:
```
.map_err(|e| StoreError::Migration { source: Box::new(e) })?
```
This is the existing pattern in `run_main_migrations` for all prior migration blocks.
On error, the caller in `migrate_if_needed` calls `txn.rollback().await` and returns `Err`.
The schema_version counter is NOT updated in the event of a partial failure, so the next
startup will retry the entire v12→v13 block. All SQL uses `INSERT OR IGNORE` + `CREATE TABLE
IF NOT EXISTS`, so retry is safe (idempotent).

---

## Failure Modes

| Scenario | Behavior |
|----------|----------|
| Empty entries table | Step 2 INSERT selects zero rows; succeeds with no data written |
| Empty co_access table (R-06) | Step 3 INSERT selects zero rows; COALESCE never evaluated; succeeds |
| All co_access counts below 3 | Step 3 INSERT selects zero rows; succeeds with no data written |
| graph_edges already exists (re-run) | `CREATE TABLE IF NOT EXISTS` is a no-op; `INSERT OR IGNORE` skips duplicates |
| Partial migration failure | Transaction rolls back; schema_version stays at 12; retried on next startup |

---

## Key Test Scenarios

1. **v12→v13 on empty database** (AC-07b, R-06):
   - Create synthetic v12 database with zero entries and zero co_access rows.
   - Run `migrate_if_needed`.
   - Assert: no error, `schema_version = 13`, `graph_edges` table exists, zero rows in graph_edges.

2. **Supersedes bootstrap — basic case** (AC-05, AC-06):
   - Create synthetic v12 database with two entries: entry id=2 with `supersedes=1`, entry id=1 with `supersedes=NULL`.
   - Run `migrate_if_needed`.
   - Assert: one row in graph_edges with `source_id=1, target_id=2, relation_type='Supersedes', bootstrap_only=0, source='entries.supersedes'`.

3. **CoAccess bootstrap threshold and weight formula** (AC-07a, R-15):
   - Populate co_access with: `(1,2,count=2)`, `(1,3,count=3)`, `(2,4,count=5)`.
   - MAX(count) OVER () = 5.
   - Expected weights: count=3 → 3/5 = 0.6, count=5 → 5/5 = 1.0.
   - Run migration.
   - Assert: two rows in graph_edges (count=2 pair not present), `bootstrap_only=0`.
   - Assert weight for (1,3) pair ≈ 0.6 (within float tolerance).
   - Assert weight for (2,4) pair ≈ 1.0.
   - Assert weight for count=5 pair > weight for count=3 pair.

4. **Migration idempotency** (AC-18, R-08):
   - Run the migration twice on the same v12 database.
   - Assert: no unique constraint error, row counts identical after both runs.
   - Assert: `schema_version = 13` after both runs.

5. **No Contradicts rows** (AC-08):
   - After migration on any synthetic v12 database, assert zero rows in graph_edges
     with `relation_type='Contradicts'`.

6. **Supersedes edges are bootstrap_only=0** (AC-06):
   - After migration, assert all `relation_type='Supersedes'` rows have `bootstrap_only=0`.

7. **Supersedes edge direction matches graph.rs** (VARIANCE 1 confirmation):
   - After migration, for entry id=2 with `supersedes=1`:
     assert `source_id=1, target_id=2` in graph_edges (old → new direction).
