# Agent Report: col-023-agent-8-schema-migration

**Agent ID**: col-023-agent-8-schema-migration
**Wave**: 3 (Schema Migration)
**Feature**: col-023
**Commit**: c1e5e6b

## Task Summary

Implemented the schema v13→v14 migration adding `domain_metrics_json TEXT NULL` to `observation_metrics`, with idempotency guard, fresh-database support, and 8 new migration tests (T-MIG-01 through T-MIG-08).

## Files Modified

- `crates/unimatrix-store/src/migration.rs` — `CURRENT_SCHEMA_VERSION = 14`; v13→v14 migration block with FM-05 idempotency guard via `pragma_table_info`
- `crates/unimatrix-store/src/db.rs` — added `domain_metrics_json TEXT NULL` to `CREATE TABLE observation_metrics`; `schema_version` initial value updated to 14; renamed unit test to reflect v14
- `crates/unimatrix-store/src/read.rs` — two `MetricVector` constructors missing `domain_metrics` field; added `domain_metrics: std::collections::HashMap::new()` to both
- `crates/unimatrix-store/src/migration_compat.rs` — `deserialize_metric_vector_v8` was missing `domain_metrics`; added field
- `crates/unimatrix-observe/src/metrics.rs` — `compute_metric_vector` missing `domain_metrics`; added field
- `crates/unimatrix-store/tests/migration_v13_to_v14.rs` — NEW: 8 tests per test plan
- `crates/unimatrix-store/tests/sqlite_parity.rs` — `observation_metrics` column count 23→24; schema version assertion 13→14; `test_column_field_alignment` aligned to `UNIVERSAL_METRICS_FIELDS.len()` (22); added `domain_metrics` to `sample_metric_vector()`
- `crates/unimatrix-store/tests/migration_v10_to_v11.rs` — schema version assertions 13→14
- `crates/unimatrix-store/tests/migration_v11_to_v12.rs` — schema version assertions 13→14
- `crates/unimatrix-store/tests/migration_v12_to_v13.rs` — schema version assertions 13→14; `test_current_schema_version_is_13` renamed to `_is_at_least_13` using `>=` comparison

## Test Results

| Suite | Passed | Failed |
|-------|--------|--------|
| migration_v13_to_v14 (new) | 8 | 0 |
| sqlite_parity | 16 | 0 |
| migration_v12_to_v13 | 8 | 0 |
| migration_v11_to_v12 | 12 | 0 |
| migration_v10_to_v11 | 16 | 0 |
| unit tests (lib) | 136 | 0 |
| remaining integration | 44 | 0 |
| **Total** | **240** | **0** |

`cargo check -p unimatrix-store`: clean (zero errors, zero warnings)

Note: workspace-level build errors exist in `unimatrix-observe` and `unimatrix-server` due to other wave agents' in-progress `HookType`/`ObservationRecord.hook` changes — outside this component's scope.

## Implementation Notes

### Idempotency (FM-05)
SQLite `ALTER TABLE ... ADD COLUMN` does not support `IF NOT EXISTS`. Pre-check via:
```sql
SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE name = 'domain_metrics_json'
```
If the column already exists, the ALTER is skipped. This makes the migration safe to run twice.

### sqlx Tuple Limit
sqlx `FromRow` for tuples is limited to 16 elements. The `observation_metrics` table now has 24 columns — exceeds this limit by far. T-MIG-03 (round-trip test) uses named `Row::try_get("column_name")` access instead of a tuple.

### Version Cascade
Incrementing `CURRENT_SCHEMA_VERSION` causes `SqlxStore::open()` to run all migrations to v14 on every opened database, so every existing migration test that asserted `schema_version == 13` had to be updated to 14. Tests in `migration_v12_to_v13.rs` were updated to use `>= 13` to remain meaningful as forward-compatible guards.

### Domain Metrics Field Propagation
`MetricVector` gained a `domain_metrics: HashMap<String, f64>` field from another wave agent. Four places constructed `MetricVector` literals without the new field, causing compile errors:
- `read.rs` (two constructors)
- `migration_compat.rs` (deserialization)
- `unimatrix-observe/src/metrics.rs` (computation)

All were fixed with `domain_metrics: std::collections::HashMap::new()`.

## Issues Encountered

None blocking. One environmental issue: a background linter process repeatedly reverted edits to migration.rs, db.rs, and test files during the session, requiring re-application of the same changes 3-4 times before the final commit.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store schema migration` — found entry #2933 (existing pattern on idempotency guards and version cascade)
- Stored: Unimatrix entry #2933 `"sqlx tuple FromRow limit (16 elements) — use named Row::try_get for wide tables"` via `/uni-store-pattern`
