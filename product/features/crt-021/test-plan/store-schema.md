# Test Plan: store-schema (unimatrix-store/src/db.rs)

Covers: `create_tables_if_needed` — GRAPH_EDGES DDL, three indexes, UNIQUE constraint,
`metadata TEXT DEFAULT NULL` column

Risks addressed: R-09 (indirect), AC-04, AC-08

---

## Unit / Integration Tests

All tests in this plan are integration tests that open a fresh in-memory SQLite database
via `SqlxStore::open(":memory:", PoolConfig::test_default())` or `open_test_store` helper.

---

## Test: GRAPH_EDGES table exists on fresh database (AC-04)

### `test_graph_edges_table_created_on_fresh_db`
- Arrange: open a fresh database with `create_tables_if_needed`
- Act: query `sqlite_master` for table `graph_edges`
- Assert: table exists
- Assert: row from `sqlite_master` contains `CREATE TABLE` DDL string

### `test_graph_edges_columns_and_types`
- Arrange: fresh database
- Act: query `pragma_table_info('graph_edges')` for all column names
- Assert: all ten columns present: `id`, `source_id`, `target_id`, `relation_type`,
  `weight`, `created_at`, `created_by`, `source`, `bootstrap_only`, `metadata`
- Assert: `weight` has type `REAL`
- Assert: `bootstrap_only` has type `INTEGER`
- Assert: `metadata` has type `TEXT`
- Assert: `source_id`, `target_id`, `relation_type` are `NOT NULL`

---

## Test: UNIQUE constraint enforced (R-08, AC-04)

### `test_graph_edges_unique_constraint_prevents_duplicate`
- Arrange: fresh database with `graph_edges` table
- Act: insert `(source_id=1, target_id=2, relation_type='Supersedes', weight=1.0, ...)`
- Act: attempt to insert the same `(1, 2, 'Supersedes')` triple again
- Assert: second insert returns a constraint violation error (without `OR IGNORE`)

### `test_graph_edges_insert_or_ignore_idempotent`
- Arrange: fresh database
- Act: `INSERT OR IGNORE` the same `(source_id, target_id, relation_type)` triple twice
- Assert: table contains exactly one row after two inserts, no error

### `test_graph_edges_unique_allows_different_relation_types`
- Arrange: fresh database
- Act: insert `(1, 2, 'Supersedes')` then `(1, 2, 'CoAccess')`
- Assert: both rows present — same `(source_id, target_id)` pair allowed for different types

---

## Test: Three indexes exist (AC-04)

### `test_graph_edges_indexes_exist`
- Arrange: fresh database
- Act: query `sqlite_master WHERE type='index' AND tbl_name='graph_edges'`
- Assert: three indexes found with names:
  - `idx_graph_edges_source_id`
  - `idx_graph_edges_target_id`
  - `idx_graph_edges_relation_type`

---

## Test: metadata column is NULL by default (AC-04)

### `test_graph_edges_metadata_default_null`
- Arrange: insert a row without specifying `metadata`
- Act: query the row back
- Assert: `metadata` column value is `NULL`

---

## Test: bootstrap_only defaults to 0 (AC-08)

### `test_graph_edges_bootstrap_only_defaults_zero`
- Arrange: insert a row without specifying `bootstrap_only`
- Act: query the row back
- Assert: `bootstrap_only = 0`

---

## CI Gate: sqlx-data.json regenerated (R-09, AC-19)

This is not a Rust unit test — it is a CI shell gate.

### `ci_gate_sqlx_offline_build_succeeds`
- Command: `cargo build --workspace` with environment variable `SQLX_OFFLINE=true`
- Assert: exits with code 0
- Blocking: **do not merge crt-021 PR without this gate passing**

---

## Test Module Location

Tests live in `crates/unimatrix-store/src/db.rs` `#[cfg(test)]` module, following the
existing pattern of opening a test store and querying `sqlite_master`/`pragma_table_info`.
Use `open_test_store` from `test_helpers.rs` (fresh tempdir DB) or pass `":memory:"` path
directly with `PoolConfig::test_default()`.
