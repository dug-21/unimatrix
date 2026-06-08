# migration.rs — v27→v28 topic_source Column

**Source**: `crates/unimatrix-store/src/migration.rs` (extend). **ADR**: ADR-005.
**Constraints**: C-07 migration discipline (pragma-guarded, all checks before any
ALTER, version stamp at end of `run_main_migrations` in one transaction — #4092),
NFR-06 idempotent re-run.

## Purpose

Additive `observations.topic_source TEXT NULL` column, the F6 (#682)
retirement-gate evidence base. Lowest-risk change in the feature — twice-proven
pattern (v9→v10 topic_signal precedent at migration.rs:219-237; pattern
#4092/#1264).

## Version constant (migration.rs:22)

```rust
pub const CURRENT_SCHEMA_VERSION: u64 = 28;   // was 27
```

## New migration block (after the v27 block, ~migration.rs:1376, before the final stamp)

Insert immediately AFTER the `if current_version < 27 { ... }` block (:1333-1376)
and BEFORE the final `INSERT OR REPLACE ... schema_version` (:1379). Mirrors the
v9→v10 topic_signal precedent exactly:
```rust
    // v27 → v28: topic_source column on observations (vnc-030, ADR-005).
    // F6 (#682) retirement-gate evidence base. No backfill: pre-vnc-030 rows stay
    // NULL-source by design (inventing historical sources = the SR-04 "best guess").
    if current_version < 28 {
        let has_topic_source: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_source'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_topic_source {
            sqlx::query("ALTER TABLE observations ADD COLUMN topic_source TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
        }
        // Single-column block: the one pragma check IS "all checks before any ALTER".
        // No intra-block `UPDATE counters SET value = 28` is required — the final
        // INSERT OR REPLACE at :1379 stamps CURRENT_SCHEMA_VERSION (=28) for the
        // last block. (Earlier blocks intra-stamp only because LATER blocks must
        // observe the intermediate version; v28 is the last block, so the final
        // stamp suffices. If a v29 lands later, add `UPDATE counters SET value = 28`
        // here at that time — flagged for R-11 reviewer.)
    }
```

The final stamp (:1379-1385) already binds `CURRENT_SCHEMA_VERSION as i64` = 28 —
no change needed there beyond the constant bump.

## Discipline Notes

- **Idempotent**: the `pragma_table_info` pre-check makes a re-run a no-op on an
  already-migrated DB. A fresh DB (created at v28 schema) and a pre-migration DB
  (v27) both land the column exactly once.
- **Single transaction**: the block runs inside `run_main_migrations`'s `txn`; if
  the ALTER fails the transaction rolls back and `schema_version` stays at 27.
- **No backfill**: existing rows stay NULL `topic_source`. The F6 before/after
  distribution check windows on POST-migration rows only (R-20).
- **No index**: F6 reads are offline aggregate scans (ADR-005 §3).

## Data Flow

Schema-only; `topic_source` is NEVER on the wire. Written at insert time by the
listener record path (`?10` bind, listener-stamp-read.md). Read by F6 offline
aggregation.

## Error Handling

`.map_err(|e| StoreError::Migration { source: Box::new(e) })?` propagates ALTER
failure into transaction rollback — the existing crate error pattern. The pragma
check uses `.unwrap_or(false)` (a failed check conservatively assumes the column
is absent and attempts the idempotent ALTER, which is itself guarded by SQLite —
matches the v9→v10 precedent's `.unwrap_or(false)`).

## Key Test Scenarios (FR-20, R-11)

- Fresh DB → column present, version 28.
- Already-migrated DB (re-run) → no-op via pragma guard, no error, version 28.
- Pre-migration DB at v27 → column added, version stamped 28; existing rows remain
  NULL `topic_source`.
- **Version-collision check (delivery, R-11)**: confirm no other landed migration
  (crt-052/vnc-027 follow-ups) claims v28 against the rebased main; the version
  number must be unique. The block must be the last `if current_version < N` before
  the final stamp.

## Open Questions / Gaps

- **R-11 collision** is a delivery-time verification (confirm 28 is unique on
  rebased main), not a design gap.
- If a v29 migration lands after this in the same PR train, add the intra-block
  `UPDATE counters SET value = 28` so v29 observes the intermediate version
  (flagged inline above).
