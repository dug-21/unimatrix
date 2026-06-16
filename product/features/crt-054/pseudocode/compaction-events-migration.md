# Component 7 — `compaction_events` table + migration

**Files**:
- `crates/unimatrix-store/src/migration.rs` (modify) — `CURRENT_SCHEMA_VERSION` at `:22`; `run_main_migrations` upgrade chain (`:126`+); the last-block intra-stamp note (`:1404`); the final `INSERT OR REPLACE ... schema_version` stamp (`:1409`).
- `crates/unimatrix-store/src/db.rs` (modify) — `create_tables_if_needed` (`:534`) fresh-create path.

**ADRs**: ADR-008 (crt-054 owns ONLY `compaction_events` + the NEXT `CURRENT_SCHEMA_VERSION` bump; NOT `SUMMARY_SCHEMA_VERSION`, NOT `cycle_review_index`).
**Patterns**: #4153 (three-path bump), #4092 (`CREATE TABLE/INDEX IF NOT EXISTS` idempotent), #4484 (cascade-file existence), #4095 (merge-order version reconciliation).

## Purpose

Create the durable, insert-only `compaction_events` table on all three paths (fresh-create in `db.rs`, the migration upgrade block, and the version stamp), guarded and idempotent, taking the NEXT `CURRENT_SCHEMA_VERSION` bump (28 → 29 or 30 by merge order). crt-054 touches NO other table and does NOT bump `SUMMARY_SCHEMA_VERSION`.

## The DDL (identical in both `db.rs` and `migration.rs`)

```sql
-- Surface A: durable, content-free, insert-only compaction-event ledger (crt-054).
-- compacted_at is Unix SECONDS (server wall clock). The PostToolUse ts/1000
-- normalization at the gate is crt-055's, not crt-054's.
CREATE TABLE IF NOT EXISTS compaction_events (
    id           INTEGER PRIMARY KEY,
    session_id   TEXT    NOT NULL,
    compacted_at INTEGER NOT NULL,   -- Unix SECONDS (NOT millis)
    high_water   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_compaction_events_session ON compaction_events(session_id);
```

The "Unix SECONDS" comment MUST appear in BOTH the fresh-create DDL and the migration upgrade block (AC-01a).

## Path 1 — fresh-create (`db.rs:534` `create_tables_if_needed`)

Append the table + index alongside the existing `CREATE TABLE IF NOT EXISTS` list (e.g. after the `cycle_events` / `observations` blocks). Same `sqlx::query(...).execute(pool).await?` style as the surrounding tables. `CREATE TABLE/INDEX IF NOT EXISTS` is idempotent on re-run.

## Path 2 — migration upgrade block (`migration.rs run_main_migrations`)

Add a new guarded block at the END of the upgrade chain, after the `if current_version < 28 { ... }` block (`:1384`):

```
if current_version < N {            // N = 29 or 30 (merge-order coordinated — see below)
    // CREATE TABLE IF NOT EXISTS is idempotent; no pragma pre-check strictly needed,
    // but follow the file convention. Single new table, no ALTER on an existing one,
    // so there is no column-existence guard to do.
    sqlx::query("CREATE TABLE IF NOT EXISTS compaction_events ( ... Unix SECONDS comment ... )")
        .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_compaction_events_session ON compaction_events(session_id)")
        .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // This becomes the new LAST block → the final INSERT OR REPLACE schema_version stamp
    // (below) stamps N. The PRIOR last block (v<28) previously relied on being last; since
    // a new block now follows it, ADD an intra-block stamp to the v28 block:
    //   UPDATE counters SET value = 28 WHERE name = 'schema_version'   (per the :1404 note)
    // so the v28 block's intermediate version is observable before this v29/30 block runs.
}
```

### Three things to update together (file convention, `:1404`-`:1410`):

1. **`CURRENT_SCHEMA_VERSION`** at `:22`: `28` → `N` (29 or 30).
2. **The v28 block intra-stamp**: the v28 block's comment (`:1399-1404`) explicitly says: *"If a v29 block lands after this one, add `UPDATE counters SET value = 28` here at that time (R-11)."* Do exactly that — add the intra-stamp to the v28 block so it is no longer the last block.
3. **The final stamp** (`:1409-1410`) `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)` binds `CURRENT_SCHEMA_VERSION` — automatically becomes `N`.

## Merge-order coordination (N = 29 vs 30) — SR-04 / R-04 / AC-01

crt-054 (`compaction_events`) and crt-055 (`cycle_review_index` columns) both take the next bump on DISJOINT tables. The number is set by merge order at the SM gate:
- First-merged feature → `N = 29`.
- Second-merged feature → retroactively `N = 30`; the second feature updates its `if current_version < N` guard, its `CURRENT_SCHEMA_VERSION`, the intra-stamp on the now-prior block, and the pinned-version assert (if any) in ONE change before delivery.
- **Mandatory pre-delivery check (#4095)**: `grep CURRENT_SCHEMA_VERSION migration.rs` immediately before finalizing; if crt-055 merged first and claimed 29, crt-054 moves to 30.

This is an SM coordination point, NOT a code decision crt-054 can make alone. The pseudocode is written with `N` as the placeholder; the implementer resolves it at the gate.

## Pinned-version assert

If `migration.rs` carries a pinned-version assert/test (the brief's "update the pinned-version assert"), bump it to `N` in the same change. (Confirm location at implementation — search for an assert tying a test to `CURRENT_SCHEMA_VERSION`.)

## Out of scope (ADR-008, AC-15)

- Do NOT bump `SUMMARY_SCHEMA_VERSION` (`cycle_review_index.rs` — crt-055 owns 4→5).
- Do NOT ALTER `cycle_review_index` or any existing table.
- No `feature_cycle` column, no content column on `compaction_events`.

## Error handling

- Migration failures map to `StoreError::Migration { source }` (file convention).
- `CREATE ... IF NOT EXISTS` makes re-run idempotent (a partially-applied upgrade re-runs cleanly).

## Key test scenarios (hints)

- Fresh DB: `compaction_events` present via `pragma_table_info` with the exact columns/types; index on `session_id` present; cascade-file existence asserted (#4484) (AC-01).
- DB upgraded from v28: the upgrade block adds the table, existence-guarded, re-run idempotent (AC-01).
- The `compacted_at` DDL carries an explicit "Unix SECONDS" comment in BOTH fresh-create and upgrade paths (AC-01a).
- Schema has no `feature_cycle`/content column (AC-03).
- Structural/grep: crt-054's diff touches neither `cycle_review_index` nor `SUMMARY_SCHEMA_VERSION` (AC-15).
- Merge-order reconciliation: SM-gate `grep` check (process, not unit test) — N reconciled to 29/30 before delivery (R-04).
