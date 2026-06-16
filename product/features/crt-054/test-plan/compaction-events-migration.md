# Test Plan — `compaction_events` table + migration

**Component**: new table; `CREATE TABLE IF NOT EXISTS compaction_events (id INTEGER PK, session_id TEXT NOT NULL, compacted_at INTEGER NOT NULL /* Unix SECONDS */, high_water INTEGER NOT NULL DEFAULT 0)` + INDEX on `session_id`. Next `CURRENT_SCHEMA_VERSION` bump (28 → 29/30, merge-order coordinated). Three-path bump: `migration.rs` upgrade block, `db.rs` fresh-create, version assert.
**Pseudocode**: `pseudocode/compaction-events-migration.md` · **Layer**: unit (migration tests).
**Anchor ACs**: **AC-01** (fresh + upgrade — Critical), AC-01a (Unix SECONDS comment), AC-05/AC-08 contract column types (shared). **Risks**: **R-04 (Critical)**, R-05.

> Follow the schema-version cascade checklist (#4373) — there are MORE than three touchpoints. The version number (`NN` = 29 or 30) is reconciled by the SM at merge (grep `CURRENT_SCHEMA_VERSION migration.rs` first — lesson #4095). The test plan is written version-agnostic; Stage 3c sets the number.

## Fresh-create (AC-01) — unit

`crates/unimatrix-store/tests/sqlite_parity.rs` (extend) + a new `migration_v28_to_vNN.rs`.

1. `test_create_tables_compaction_events_exists` (sqlite_parity) — on a fresh DB (`create_tables_if_needed`), `compaction_events` exists via `pragma_table_info`; columns `id`, `session_id`, `compacted_at`, `high_water` present with the contract types/null/default (`session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL, `high_water` INTEGER NOT NULL DEFAULT 0). (FR-A1, AC-01.)
2. `test_create_tables_compaction_events_session_index_exists` — the index on `session_id` exists. (AC-01.)
3. `test_schema_column_count` / `test_schema_version_is_NN` (sqlite_parity, **UPDATE existing**) — column-count + version assertions bumped to `NN` per the cascade checklist (#4373 gotcha: sqlite_parity is the easy-to-miss surface).

## Upgrade-from-v28 (AC-01) — unit

`crates/unimatrix-store/tests/migration_v28_to_vNN.rs` (new).

4. `test_migration_v28_to_vNN_adds_compaction_events` — Arrange: open a DB at v28. Act: migrate to `NN`. Assert: the upgrade block adds `compaction_events` + index (existence-guarded); the table was NOT present at v28 and IS present after. (FR-A7, AC-01.)
5. `test_migration_v28_to_vNN_idempotent` — re-running the migration (re-open) is idempotent (`CREATE TABLE IF NOT EXISTS`); `read_schema_version >= NN`. (#4373 idempotency gotcha — use `>=` predicate.)
6. `test_cascade_file_existence` — assert the cascade-file existence pattern (#4484) holds — the new table is registered in the expected cascade surfaces.

## DDL "Unix SECONDS" comment (AC-01a) — grep

7. `test_compacted_at_seconds_comment_in_both_paths` — grep/inspect: the `compacted_at` column carries an explicit "Unix SECONDS" comment in BOTH the `migration.rs` upgrade block AND the `db.rs` fresh-create DDL (byte-identical DDL across the two paths per #4153/#4372). The gate-side `ts/1000` is crt-055's — out of scope here. (AC-01a, R-11.)

## Contract conformance (R-05) — shared with activity-snapshot.md
8. `test_compaction_events_columns_match_contract` — the column names/types/index match crt-055 §"Producer contract" Surface A verbatim (`id`, `session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL, `high_water` INTEGER NOT NULL DEFAULT 0; index on `session_id`). Single source = the contract. (R-05.)

## Cascade checklist (Stage 3c — from pattern #4373)
- `migration.rs`: `if current_version < NN` block with the `CREATE TABLE IF NOT EXISTS` + index.
- `db.rs` `create_tables_if_needed`: matching byte-identical DDL.
- `db.rs` schema_version INSERT: hardcoded integer bumped to `NN`.
- `sqlite_parity.rs`: add named table assertion + column-count assertion; bump `test_schema_version_is_NN`.
- `server.rs`: update all `assert_eq!(version, NN)` sites.
- Previous migration test (`migration_v27_to_v28.rs`): rename `test_current_schema_version_is_28` → `_at_least_28` with `>=`; change any `assert_eq!(read_schema_version, …)` to `>=`.
- Grep gate: `grep -r 'schema_version.*== 28' crates/` returns zero matches (including comments) before marking complete.
- Run `cargo test --workspace` immediately after the bump to catch ALL cascade failures.

## Merge-order coordination (R-04, NOT a unit test — SM gate)
9. `merge_order_reconciliation_check` — immediately before finalizing, `grep CURRENT_SCHEMA_VERSION migration.rs`; if crt-055 merged first and claimed 29, crt-054's migration block + pinned-version assert + this test file's name/assertions update to 30 in one change (#4095). Disjoint tables — no content collision, only the version number.
