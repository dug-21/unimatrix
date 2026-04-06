# Agent Report: crt-047-agent-4-migration

**Agent ID:** crt-047-agent-4-migration
**Component:** migration v23→v24
**GH Issue:** #529
**Branch:** feature/crt-047

---

## Summary

Implemented the v23→v24 schema migration for the crt-047 curation health metrics feature.
Added seven new `INTEGER NOT NULL DEFAULT 0` columns to `cycle_review_index` with full
`pragma_table_info` pre-check idempotency per ADR-004. Updated the fresh-schema DDL in
`db.rs`. Updated all cascade test assertions. Created the new integration test file.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Bumped `CURRENT_SCHEMA_VERSION` 23→24; added `if current_version < 24` block with 7 pragma_table_info pre-checks and 7 conditional ALTER TABLE statements |
| `crates/unimatrix-store/src/db.rs` | Updated `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL to include all 7 new columns |
| `crates/unimatrix-server/src/server.rs` | Updated two `assert_eq!(version, 23)` to `assert_eq!(version, 24)` |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | Updated schema version assertion (23→24) and cycle_review_index column count assertion (5→12 with all 7 new columns named) |
| `crates/unimatrix-store/tests/migration_v22_to_v23.rs` | Relaxed 3 exact `== 23` assertions to `>= 23` |
| `crates/unimatrix-store/tests/migration_v17_to_v18.rs` | Relaxed cycle_review_index column count `== 5` to `>= 5` (was broken by v24 adding 7 columns) |

## Files Created

| File | Change |
|------|--------|
| `crates/unimatrix-store/tests/migration_v23_to_v24.rs` | New integration test file: MIG-V24-U-01 through MIG-V24-U-05 |

---

## Test Results

```
cargo test -p unimatrix-store --features test-support
All test suites: PASS
migration_v22_to_v23: 5 passed, 0 failed
migration_v23_to_v24: 5 passed, 0 failed
sqlite_parity:        49 passed, 0 failed
```

Total across unimatrix-store: 0 failures.

---

## Pre-Delivery Checks

- `grep -r 'schema_version.*== 23' crates/` — zero matches
- `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` — shows `24`

---

## Issues / Blockers

None.

One unexpected cascade fix was required:

`migration_v17_to_v18.rs` contained `assert_eq!(col_count, 5, "cycle_review_index must have exactly 5 columns after v17→v18 migration")`. This breaks because `Store::open()` runs all migrations to the current version — a v17 DB opened after v24 is deployed will have 12 columns, not 5. Changed to `assert!(col_count >= 5, ...)`. This follows the same relaxation pattern applied to `schema_version` equality checks in all prior migration tests.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — entry #4153 found covering schema bump cascade
  (schema_version exact equality → `>= N`). Does not cover column count assertion relaxation.
- Stored: Write capability unavailable from this agent context. Pattern to store:
  "Prior migration tests asserting exact column count (`assert_eq!(col_count, N)`) on tables
  that receive new columns in later migrations must be relaxed to `assert!(col_count >= N)` —
  same reasoning as schema_version equality relaxation. Discovered in crt-047 when
  migration_v17_to_v18 asserted cycle_review_index == 5 and v24 made it 12."
  Recommend SM or next retrospective agent supersede entry #4153 to include this case.
