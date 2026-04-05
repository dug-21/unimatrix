# Agent Report: 509-gate-bugfix

> Agent ID: 509-gate-bugfix
> Gate: Bug Fix Validation
> Feature: bugfix-509
> Date: 2026-04-05
> Result: REWORKABLE FAIL

## What I Validated

Bug fix for GH #509 — missing compound index `idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)` causing O(K) linear scans in the S1 tag co-occurrence self-join in `graph_enrichment_tick.rs`.

## Checks Executed

All checks from the bug fix gate check set were run:

1. **Root cause addressed** — PASS. Index present in all 3 DDL paths: db.rs (fresh), migration.rs (v22→v23 live block), migration.rs v5→v6 legacy array.
2. **No stubs/placeholders** — PASS. Clean across all changed files.
3. **All tests pass** — PASS. 2764 server tests, 5 new migration_v22_to_v23 tests, 0 failures.
4. **No new clippy warnings** — PASS. Existing clippy failures are pre-existing in unimatrix-engine and unimatrix-observe, not in changed files.
5. **No unsafe code** — PASS.
6. **Fix is minimal** — PASS. Exactly 5 files, all on-target.
7. **Tests catch the bug** — PASS. MIG-V23-U-04 uses `pragma_index_info` to verify the index physically exists and has `tag` as the leading column. A false positive (version bumped but index absent) would fail.
8. **Integration smoke tests** — PASS. 22/22.
9. **No xfail markers** — PASS.
10. **Schema version in both paths** — PASS.
11. **server.rs assertions committed** — FAIL. See below.
12. **Test file line count** — WARN. 508 lines (8 over 500-line limit).

## Blocking Issue

`crates/unimatrix-server/src/server.rs` has working-tree changes updating `assert_eq!(version, 22)` to `assert_eq!(version, 23)` in `test_migration_v7_to_v8_backfill`, but these are **not committed**. The committed state of the repository (`a439b345`) still asserts version 22. Checking out the branch fresh would produce 2 failing tests in `unimatrix-server`.

Fix: commit the working-tree server.rs changes.

## Knowledge Stewardship

- Stored: nothing novel to store -- uncommitted file in a fix commit is a one-off execution error, not a recurring pattern with cross-feature value.
