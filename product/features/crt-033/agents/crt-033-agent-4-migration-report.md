# Agent Report: crt-033-agent-4-migration

## Task
Implement the full schema v17→v18 migration cascade. All 7 touchpoints mandatory.

## Files Modified

1. `crates/unimatrix-store/src/migration.rs` — bumped `CURRENT_SCHEMA_VERSION` 17→18; added `if current_version < 18` block with `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL + `UPDATE counters SET value = 18`; updated comment on final INSERT OR REPLACE
2. `crates/unimatrix-store/src/db.rs` — added `cycle_review_index` DDL to `create_tables_if_needed()` before the counters INSERT block; schema_version INSERT already uses `CURRENT_SCHEMA_VERSION` constant (no literal to change)
3. `crates/unimatrix-store/tests/sqlite_parity.rs` — updated `test_schema_version_is_14` assertion from `== 17` to `== 18`; added `test_create_tables_cycle_review_index_exists` and `test_create_tables_cycle_review_index_schema` tests
4. `crates/unimatrix-server/src/server.rs` — updated both `assert_eq!(version, 17)` assertions (lines ~2137, ~2162) to `== 18`
5. `crates/unimatrix-store/tests/migration_v16_to_v17.rs` — renamed `test_current_schema_version_is_17` → `test_current_schema_version_is_at_least_17` with `>= 17` predicate; updated all `read_schema_version(&store).await == 17` assertions to `>= 17`
6. `crates/unimatrix-store/tests/migration_v15_to_v16.rs` — updated all `== 17` assertions (constant check + counter checks) to `>= 16` predicates (discovered via grep gate — not in the original 7-touchpoint list but caught by `grep -r 'schema_version.*== 17' crates/`)

## Files Created

7. `crates/unimatrix-store/tests/migration_v17_to_v18.rs` — 6 integration tests: MIG-U-01 (`test_current_schema_version_is_18`), MIG-U-02 (`test_fresh_db_creates_schema_v18`), MIG-U-03 (`test_v17_to_v18_migration_creates_table`), MIG-U-04 (`test_v17_to_v18_migration_table_has_five_columns`), MIG-U-05 (`test_v17_to_v18_migration_preserves_existing_data`), MIG-U-06 (`test_v17_to_v18_migration_idempotent`)

## Tests

All migration and schema tests: **18 passed, 0 failed**

Breakdown:
- `migration_v17_to_v18.rs`: 6 passed (MIG-U-01 through MIG-U-06)
- `migration_v16_to_v17.rs`: 4 passed (updated assertions)
- `migration_v15_to_v16.rs`: 3 passed (updated assertions)
- `sqlite_parity.rs` schema tests: 6 passed (including 2 new cycle_review_index tests)

Pre-existing flaky test `test_wal_mode_creates_wal_file` (unrelated to this work) fails under full parallel run but passes in isolation — confirmed pre-existing by stash verification.

## Gate Check Result

```
grep -r 'schema_version.*== 17' crates/ 2>/dev/null
exit_code=1  (no matches)
```

**PASS — zero matches.**

## Issues

**Extra touchpoint discovered:** `migration_v15_to_v16.rs` also contained `== 17` assertions (not listed in the original 7-touchpoint spec). The grep gate caught them. Per entry #3539, this is a known cascade pattern. All fixed. The original 7-touchpoint spec was complete for the primary files; the grep gate is the authoritative final check.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #3539 (schema cascade checklist), #2937 (server.rs test maintenance). Entry #3539 was directly applicable.
- Stored: Attempted `context_correct` to extend entry #3539 with the stacked-older-files gotcha — failed with `Agent 'anonymous' lacks Write capability`. Pattern to document: when adding v18, both migration_v15_to_v16.rs AND migration_v16_to_v17.rs had `== 17` literals that needed updating, not just the N-1 file. The grep gate `grep -r 'schema_version.*== 17' crates/` is the authoritative catch-all and should be the first step of cascade verification.
