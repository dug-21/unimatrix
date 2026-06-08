# Test Plan — C7 `migration.rs` v27→v28 topic_source

Source: ADR-005. AC: AC-05. Risks: R-11, R-12, R-20. File: extend the migration idempotence test family in `crates/unimatrix-store/src/migration.rs` (mirror the v9→v10 `topic_signal` precedent at migration.rs:219-237). `cargo test -p unimatrix-store`.

Additive `observations.topic_source TEXT NULL`; pragma-guarded idempotent ALTER (all pragma checks before any ALTER, #4092/C-07); `CURRENT_SCHEMA_VERSION = 28`; version stamp at the end of `run_main_migrations` in one transaction. No backfill.

## Idempotence ×3 (R-11, FR-20)

### migration_fresh_db_adds_topic_source_column
- Fresh DB → migrate → `pragma_table_info('observations')` shows `topic_source`; column type TEXT, nullable.

### migration_already_migrated_db_is_noop
- Run the migration twice → second run is a no-op via the `pragma_table_info` guard (no error, no duplicate column, no double-apply).

### migration_pre_migration_v27_to_v28
- A DB stamped at v27 → migrate → version becomes 28; `topic_source` added; existing rows preserved.

## No-backfill / existing rows NULL (R-20, FR-20)

### migration_leaves_existing_rows_null
- Pre-existing observation rows → after migration, `topic_source IS NULL` (no backfill by design, ADR-005 §3).

### distribution_window_post_migration_only (R-20, methodology)
- Documentation/assertion: any before/after `topic_source` distribution comparison windows on **post-migration** rows only — pre-migration NULLs are excluded so SR-06's F6 conclusion is not drawn over the wrong rows.

## Pragma discipline (C-07, #4092)

### all_pragma_checks_precede_any_alter
- Assert (test or code-review item) the `pragma_table_info` check precedes the `ALTER` (SQLite has no `IF NOT EXISTS` for `ADD COLUMN`); the version stamp lands once at the end of `run_main_migrations`.

## Version uniqueness (R-11 — delivery-time check)

### current_schema_version_is_28_unique
- `CURRENT_SCHEMA_VERSION == 28`. **At delivery (post-rebase)**: confirm no other landed migration (crt-052/vnc-027 follow-ups) claims v28 — the version number must be unique against the rebased main, and the v28 block must be in-order. (This is a delivery-time check, not only a unit assertion.)

## Coverage requirement
`pragma_table_info` precedes the ALTER; idempotent re-run is a no-op; existing rows remain NULL; the version number is unique against the rebased main at delivery; distribution measurement is documented to window post-migration.
