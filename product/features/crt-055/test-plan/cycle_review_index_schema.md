# Test Plan — cycle_review_index schema v5 (columns + CycleReviewRecord + migration)

**Component**: `unimatrix-store/src/cycle_review_index.rs`, `migration.rs`, `db.rs`
**Risks**: R-10 (three-path migration drift), R-18 (version handshake), R-09 (column type), R-13 (no token column)
**ACs**: AC-02, AC-03 (+ AC-20 column-type, AC-10 no-token-column)

## Unit tests

### Migration / pragma (R-10, AC-02)
- `test_v5_migration_adds_all_columns_fresh_db` — fresh DB created via the `db.rs` fresh-create path; `pragma_table_info('cycle_review_index')` lists ALL 16 new v5 columns. Assert each: correct name, type `INTEGER` (or `TEXT` for `signal_class_counts_json`), `NOT NULL`, `DEFAULT 0` (`'{}'` for the JSON column).
- `test_v5_migration_adds_all_columns_upgraded_db` — start a v4 DB, apply the v5 ALTER block; assert identical `pragma_table_info` result to the fresh-create path (the two paths AGREE — #4153 three-path).
- `test_v5_migration_idempotent` — re-run the v5 ALTER block on an already-migrated DB; assert no error and no duplicate columns (pragma-guarded ALTERs are no-ops).
- `test_context_reload_pct_column_is_integer_not_real` (AC-20) — assert `pragma_table_info` reports `context_reload_pct` type `INTEGER`, never `REAL`. Structural guard: no `REAL`/`f64`-typed metric column anywhere in the v5 set.

### Version pinning (R-10, AC-03)
- `test_summary_schema_version_is_5` — assert `SUMMARY_SCHEMA_VERSION == 5`.
- `test_pinned_migration_version_assertion_updated` — the `migration.rs` pinned `CURRENT_SCHEMA_VERSION` test moves in the same change (assert the migration-version test reflects the new sequential number).
- `test_cascade_file_exists` (#4484) — the prior `migration_vN_to_vN+1.rs` cascade file exists before the new one (file-existence guard).

### Token-field guard (R-13, AC-10)
- `test_no_token_named_column` — structural/grep guard: no column on `cycle_review_index` contains "token"; throughput unit is bytes.

## Integration tests

- `test_cycle_review_index_v5_columns_present` (harness, AC-02/03) — extend the `_compaction_events_columns` pragma pattern to `cycle_review_index`. Boot the binary fresh, then restart (upgrade path), assert every v5 column present with correct type/default on BOTH. Cross-references AC-02 fresh-vs-upgrade agreement through the real binary.

## Version handshake (R-18, AC-03) — SM coordination, asserted at merge
- `test_crt054_crt055_distinct_schema_versions` — crt-054 (`compaction_events`) and crt-055 (`cycle_review_index` v5) hold DISTINCT sequential `CURRENT_SCHEMA_VERSION` numbers (#4095). Both migrations apply cleanly in EITHER merge order (disjoint tables — no ALTER collision).
- `test_crt054_does_not_bump_summary_schema_version` — boundary check: crt-054 leaves `SUMMARY_SCHEMA_VERSION` untouched (crt-055 owns 4→5 alone).

## Edge cases
- Pre-v5 row read after migration → returns column DEFAULTS (0 / '{}') until guarded recompute refreshes them (NFR-03); not an error.
- Migration re-run on a partially-migrated DB → pragma-guard makes each individual ALTER idempotent.

## Expected behaviors / assertions summary
- Every v5 column: `INTEGER NOT NULL DEFAULT 0` (or `TEXT NOT NULL DEFAULT '{}'`), pragma-guarded.
- `SUMMARY_SCHEMA_VERSION == 5`; pinned-version test moved in the same change.
- Fresh-create and upgrade paths produce byte-identical schema; idempotent re-run is a no-op.
- No `REAL` metric column; no token-named column.
