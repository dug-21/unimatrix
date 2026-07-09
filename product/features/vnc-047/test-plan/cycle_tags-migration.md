# C1 — `cycle_tags` table + migration (schema v31)

> Files: `crates/unimatrix-store/src/migration.rs` (const :26 + `if current_version < 31` block),
> `crates/unimatrix-store/src/db.rs::create_tables_if_needed` (fresh-create).
> Risks: **R-01 (Critical)**, R-13 (Med), R-10 (Med). ACs: AC-03, AC-03a-d.
> Cascade #1 of two (real DB migration). Do NOT lump with the SUMMARY v6 cascade (report-field.md).

## Reuse
Copy `crates/unimatrix-store/tests/migration_v29_to_v30.rs` → `migration_v30_to_v31.rs`
(the per-version-step integration test pattern). Fresh-create assertions live beside the existing
`test_graph_edges_table_created_on_fresh_db` / `test_create_tables_creates_*_indexes` style tests in
`db.rs`. Migration builder pattern: `create_v29_database(path)` → write `create_v30_database(path)`
(populated v30 DB: `entries` gate table, `counters(schema_version=30)`, at least one representative
existing table so post-migration data-intact is observable).

## Unit / migration test expectations

**AC-03a — constant bump (grep-verifiable + pinned).**
- `test_current_schema_version_is_at_least_31` — `assert!(CURRENT_SCHEMA_VERSION >= 31)` (mirror the
  existing `test_current_schema_version_is_at_least_30`). Assert `migration.rs:26` reads `31`.

**AC-03b — fresh-create path.**
- `test_fresh_db_creates_cycle_tags_table` — init a v31 DB via `create_tables_if_needed`; assert
  `cycle_tags` exists (`sqlite_master`), PK is `(feature_cycle, tag)` (via `pragma_table_info` /
  `pragma_index_list` — both columns NOT NULL, both in the PK), and `idx_cycle_tags_tag` on `(tag)`
  present (`pragma_index_list`).

**AC-03c — migration path + idempotency + old-DB-safety (R-13).**
- `test_migration_v30_to_v31_creates_cycle_tags` — build a populated v30 DB, migrate; assert
  `cycle_tags` + `idx_cycle_tags_tag` created and `counters.schema_version == 31`.
- `test_migration_v30_to_v31_idempotent` — re-run migration on an already-v31 DB → `Ok`, no error,
  no duplicate DDL (guarded by `sqlite_master`/`pragma_table_info` existence pre-check, canonical
  migration.rs:314-343).
- `test_migration_from_populated_v30_data_intact` — seed rows in a pre-existing table on the v30 DB,
  migrate, assert those rows survive (#378 old-schema-DB coverage).

**AC-03d — pinned/hygiene test + DDL drift guard (#376).**
- Update the pinned schema-version/hygiene test to green at v31.
- `test_fresh_create_and_migration_schemas_identical` — build one DB via fresh-create and one via
  v30→v31 migration; assert the `cycle_tags` DDL (columns, types, NOT NULL, PK, index) is
  structurally identical on both paths (guards DDL drift between the two routes).

## Edge cases
- Migration run against a DB already carrying a stray `cycle_tags` (defensive `IF NOT EXISTS`) → no
  error.
- No FK on `feature_cycle` (free-text, no parent table) — assert none is added (would break the
  free-text contract).

## Assertions summary
Each of AC-03a–d asserted by a DISCRETE test — never one lumped "bump" assertion (R-01 is the
codebase's recurring gate miss #4153/#4373).
