# Agent Report: crt-021-agent-4-store-schema

## Task

Implement the `store-schema` component for crt-021 (Typed Relationship Graph).

File modified: `crates/unimatrix-store/src/db.rs`

## Changes Made

### DDL Added (`create_tables_if_needed`)

Added `graph_edges` table and three indexes after the existing `query_log` block, before counter initialization:

- `graph_edges` table: 10 columns (`id`, `source_id`, `target_id`, `relation_type`, `weight`, `created_at`, `created_by`, `source`, `bootstrap_only`, `metadata`), with `UNIQUE(source_id, target_id, relation_type)` constraint
- `idx_graph_edges_source_id` on `graph_edges(source_id)`
- `idx_graph_edges_target_id` on `graph_edges(target_id)`
- `idx_graph_edges_relation_type` on `graph_edges(relation_type)`

### Counter Initialization Bumped

`schema_version` counter initialization updated from `12` → `13` for fresh databases.

### Tests Added (9 tests)

All in `db::tests` module:

| Test | Covers |
|------|--------|
| `test_graph_edges_table_created_on_fresh_db` | AC-04: table exists, DDL in sqlite_master |
| `test_graph_edges_columns_and_types` | AC-04: all 10 columns, REAL/INTEGER/TEXT types, NOT NULL constraints, metadata nullable |
| `test_graph_edges_unique_constraint_prevents_duplicate` | R-08, AC-04: plain INSERT fails on duplicate triple |
| `test_graph_edges_insert_or_ignore_idempotent` | AC-04: INSERT OR IGNORE leaves exactly one row |
| `test_graph_edges_unique_allows_different_relation_types` | AC-04: same (source_id, target_id) with different types both persist |
| `test_graph_edges_indexes_exist` | AC-04: all three named indexes present in sqlite_master |
| `test_graph_edges_metadata_default_null` | AC-04: metadata column defaults to NULL |
| `test_graph_edges_bootstrap_only_defaults_zero` | AC-08: bootstrap_only defaults to 0 |
| `test_schema_version_initialized_to_13_on_fresh_db` | schema_version = 13 on fresh db |

## Test Results

```
test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

8 of 111 are the new graph_edges tests; all pass.

## Clippy

Zero new warnings introduced. 18 pre-existing warnings in other files (`analytics.rs`, `migration.rs`, `read.rs`, `write.rs`, `write_ext.rs`, `observations.rs`) — none in `db.rs`.

## Commit

`8784ff0` — `impl(store-schema): add GRAPH_EDGES DDL + indexes to create_tables_if_needed (#315)`

## Deviations from Pseudocode

None. Followed pseudocode exactly:
- Table DDL and three indexes match spec verbatim
- Insertion point: after `idx_query_log_ts`, before counter initialization
- Each DDL statement is a separate `sqlx::query(...).execute(&mut *conn).await?` call

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store` — pattern #681 (Create-New-Then-Swap Schema Migration) was returned but not relevant to this DDL-only component; existing `create_tables_if_needed` pattern in `db.rs` was the authoritative reference
- Stored: nothing novel to store — the sqlx DDL pattern (one `sqlx::query` per statement, `execute(&mut *conn).await?` propagation, `IF NOT EXISTS` idempotency) is already established by the 20+ existing table and index blocks in this file. No runtime traps or invisible gotchas encountered.
