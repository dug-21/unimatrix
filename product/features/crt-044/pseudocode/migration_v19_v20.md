# Component: migration_v19_v20
# File: crates/unimatrix-store/src/migration.rs

## Purpose

Add a `if current_version < 20` block to `run_main_migrations` that back-fills reverse edges for
all existing single-direction S1/S2 Informs and S8 CoAccess rows in GRAPH_EDGES. Bump
`CURRENT_SCHEMA_VERSION` from 19 to 20. This is a one-time data migration; subsequent runs are
idempotent via `INSERT OR IGNORE` and `NOT EXISTS` guards.

---

## Context: v18→v19 Template (reference for implementation)

The existing `if current_version < 19` block (lines 646-684 of migration.rs) is the direct
template. It uses:
- `INSERT OR IGNORE INTO graph_edges ... SELECT (swap) FROM graph_edges g WHERE ... NOT EXISTS(...)`
- `sqlx::query("UPDATE counters SET value = 19 WHERE name = 'schema_version'")`
- Both statements execute on `&mut **txn` (the outer transaction)
- Errors propagated via `.map_err(|e| StoreError::Migration { source: Box::new(e) })?`

The v19→v20 block follows exactly the same structure.

---

## Constant Change

```
// BEFORE (line 19):
pub const CURRENT_SCHEMA_VERSION: u64 = 19;

// AFTER:
pub const CURRENT_SCHEMA_VERSION: u64 = 20;
```

The doc comment on the same line updates accordingly:
```
/// Current schema version. Incremented from 19 to 20 by crt-044 (bidirectional S1/S2/S8 back-fill).
```

---

## New Function Block: `if current_version < 20` in `run_main_migrations`

### Placement

Insert immediately after the closing `}` of the `if current_version < 19` block (currently line 684)
and before the final `INSERT OR REPLACE INTO counters ... CURRENT_SCHEMA_VERSION` statement
(currently line 687).

### Block Header Comment

```
// v19 → v20: bidirectional S1/S2 Informs and S8 CoAccess edge back-fill (crt-044).
//
// Statement A (S1+S2 Informs): For every forward-only S1 or S2 Informs edge (a→b),
// inserts a reverse edge (b→a) with the same weight, source, and created_by, but
// created_at = now and bootstrap_only = 0.
//   - source IN ('S1', 'S2') is the discriminator (NOT created_by — see entry #3889, C-01).
//   - 'nli' and 'cosine_supports' Informs edges are excluded by this filter (C-04).
//
// Statement B (S8 CoAccess): Same swap pattern for S8 CoAccess edges.
//   - source = 'S8' excludes 'co_access' edges (already bidirectional from v18→v19).
//
// Both statements:
//   - INSERT OR IGNORE: UNIQUE(source_id, target_id, relation_type) prevents duplicates (C-02).
//   - NOT EXISTS sub-query: defence-in-depth to skip already-reverse rows on re-run (C-05).
//   - g.source is preserved in the inserted row (reverse S1 edge has source='S1', etc.).
//   - bootstrap_only = 0: reverse edges are always included in live graph traversal.
//   - created_at = strftime('%s','now'): records when the reverse was written, not observed.
if current_version < 20 {
    // ...statements below...
}
```

### Statement A — S1+S2 Informs Reverse Edge Back-fill

```
sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     SELECT
         g.target_id          AS source_id,
         g.source_id          AS target_id,
         g.relation_type      AS relation_type,
         g.weight             AS weight,
         strftime('%s','now') AS created_at,
         g.created_by         AS created_by,
         g.source             AS source,
         0                    AS bootstrap_only
     FROM graph_edges g
     WHERE g.relation_type = 'Informs'
       AND g.source IN ('S1', 'S2')
       AND NOT EXISTS (
         SELECT 1 FROM graph_edges rev
         WHERE rev.source_id = g.target_id
           AND rev.target_id = g.source_id
           AND rev.relation_type = 'Informs'
       )",
)
.execute(&mut **txn)
.await
.map_err(|e| StoreError::Migration { source: Box::new(e) })?;
```

### Statement B — S8 CoAccess Reverse Edge Back-fill

```
sqlx::query(
    "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at,
          created_by, source, bootstrap_only)
     SELECT
         g.target_id          AS source_id,
         g.source_id          AS target_id,
         g.relation_type      AS relation_type,
         g.weight             AS weight,
         strftime('%s','now') AS created_at,
         g.created_by         AS created_by,
         g.source             AS source,
         0                    AS bootstrap_only
     FROM graph_edges g
     WHERE g.relation_type = 'CoAccess'
       AND g.source = 'S8'
       AND NOT EXISTS (
         SELECT 1 FROM graph_edges rev
         WHERE rev.source_id = g.target_id
           AND rev.target_id = g.source_id
           AND rev.relation_type = 'CoAccess'
       )",
)
.execute(&mut **txn)
.await
.map_err(|e| StoreError::Migration { source: Box::new(e) })?;
```

### Schema Version Bump (inside the `if current_version < 20` block)

```
// Bump the in-transaction schema_version to 20 so that if a subsequent
// migration block is added later, it observes the correct version baseline.
sqlx::query("UPDATE counters SET value = 20 WHERE name = 'schema_version'")
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
```

### Final Version Statement (existing line, updated value)

The final `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)` statement
at the end of `run_main_migrations` already uses `.bind(CURRENT_SCHEMA_VERSION as i64)`. Because
`CURRENT_SCHEMA_VERSION` is bumped to 20, this statement requires no text change — it picks up the
new constant value automatically.

---

## Error Handling

All three `sqlx::query(...).execute(...).await` calls in the `if current_version < 20` block use
the same `.map_err(|e| StoreError::Migration { source: Box::new(e) })?` propagation as the v18→v19
block. If any statement fails, the `?` propagates the error to `migrate_if_needed`, which rolls
back the outer transaction. The schema_version is not bumped to 20 and the next startup re-attempts
the entire block (idempotent via `INSERT OR IGNORE` + `NOT EXISTS`).

---

## Transaction Scope (FR-M-07)

The `if current_version < 20` block executes inside the outer transaction `txn` managed by
`migrate_if_needed`. No additional `BEGIN`/`COMMIT` is required. Both SQL statements and the
`UPDATE counters` version bump are part of the same atomic transaction.

---

## Idempotency (NFR-01, C-02, C-05)

Two independent layers:
1. `INSERT OR IGNORE` — UNIQUE(source_id, target_id, relation_type) prevents duplicate rows.
2. `NOT EXISTS` sub-query — defence-in-depth: rows that already have a reverse edge are excluded
   from the SELECT before the insert, avoiding unnecessary IGNORE-discards.

Running the block twice produces the same row count. No error is raised on the second run.

---

## Key Test Scenarios (for tester agent)

All scenarios use a SQLite fixture that pre-seeds GRAPH_EDGES at schema_version = 19, then calls
the v19→v20 migration block directly (or via `migrate_if_needed` starting from v19).

1. **S1 back-fill** — Insert forward-only `(source_id=1, target_id=2, relation_type='Informs',
   source='S1')`. Run migration. Assert `(source_id=2, target_id=1, relation_type='Informs',
   source='S1')` exists with `bootstrap_only=0`. (R-01, AC-09)

2. **S2 back-fill** — Insert forward-only `(source_id=3, target_id=4, relation_type='Informs',
   source='S2')`. Run migration. Assert `(source_id=4, target_id=3, relation_type='Informs',
   source='S2')` exists. (R-01, AC-09)

3. **S8 back-fill** — Insert forward-only `(source_id=5, target_id=6, relation_type='CoAccess',
   source='S8')`. Run migration. Assert `(source_id=6, target_id=5, relation_type='CoAccess',
   source='S8')` exists with `bootstrap_only=0`. (AC-09)

4. **Idempotency (double-run)** — Insert S1 forward edge only. Run migration twice. Assert
   GRAPH_EDGES row count is identical after second run. No error. (AC-07, R-09)

5. **Idempotency with pre-existing reverse** — Insert S1 forward edge `(1→2)` AND its reverse
   `(2→1)`. Run migration. Assert row count unchanged — no duplicate inserted. (AC-14, R-05)

6. **Exclusion: nli** — Insert `(source='nli', relation_type='Informs', source_id=7, target_id=8)`.
   Run migration. Assert no row with `source='nli'` and swapped IDs exists. (R-07, C-04)

7. **Exclusion: cosine_supports** — Insert `(source='cosine_supports', relation_type='Informs',
   source_id=9, target_id=10)`. Run migration. Assert no reverse row with `source='cosine_supports'`
   exists. (R-07)

8. **Exclusion: co_access** — Insert `(source='co_access', relation_type='CoAccess', source_id=11,
   target_id=12)`. Run migration. Assert no new row with `source='co_access'` from Statement B.
   (R-06, AC-09 test case 5)

9. **schema_version check** — After migration on a v19 fixture, query
   `SELECT value FROM counters WHERE name = 'schema_version'`. Assert result = 20. (R-10, AC-06)

10. **Empty table** — Run migration on a DB with zero rows in GRAPH_EDGES. Assert no error and
    row count remains zero. (edge case)

---

## Constraints Traced

| Constraint | How Satisfied |
|-----------|--------------|
| C-01 | WHERE clause uses `source` field, not `created_by` |
| C-02 | INSERT OR IGNORE — UNIQUE constraint provides idempotency |
| C-03 | Two separate statements — S1/S2 use `relation_type='Informs'`, S8 uses `'CoAccess'` |
| C-04 | `source IN ('S1','S2')` implicitly excludes nli/cosine_supports |
| C-05 | NOT EXISTS guard present in both statements |
| C-08 | `CURRENT_SCHEMA_VERSION` bumped to 20; block checks `current_version < 20` |
| FR-M-07 | Both statements execute inside outer transaction (no separate BEGIN/COMMIT) |
