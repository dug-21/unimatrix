# Agent Report: crt-044-agent-3-migration

**Agent ID**: crt-044-agent-3-migration
**Component**: migration_v19_v20
**Date**: 2026-04-03

---

## Files Modified

| File | Action |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Bumped `CURRENT_SCHEMA_VERSION` 19→20; added `if current_version < 20` block with Statement A (S1+S2 Informs) and Statement B (S8 CoAccess) back-fill SQL, plus in-transaction version bump |
| `crates/unimatrix-store/tests/migration_v19_v20.rs` | Created — 11 new integration tests per test plan |
| `crates/unimatrix-store/tests/migration_v18_to_v19.rs` | Updated 6 stale exact-version assertions (== 19) to >= 19 / CURRENT_SCHEMA_VERSION |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | Updated `test_schema_version_is_14` assertion from 19 to 20 |

---

## Constraints Satisfied

| ID | Status | Evidence |
|----|--------|----------|
| C-01 | PASS | `WHERE g.source IN ('S1', 'S2')` and `WHERE g.source = 'S8'` — no `created_by` in WHERE |
| C-02 | PASS | `INSERT OR IGNORE` in both statements |
| C-03 | PASS | Two separate statements with distinct `relation_type` predicates |
| C-04 | PASS | `source IN ('S1','S2')` implicitly excludes nli/cosine_supports |
| C-05 | PASS | `NOT EXISTS (SELECT 1 FROM graph_edges rev ...)` in both statements |
| C-08 | PASS | Block header is `if current_version < 20` |

---

## Tests

**11 new tests in `migration_v19_v20.rs`** (1 sync + 10 async):

| Test | AC/Risk |
|------|---------|
| `test_current_schema_version_is_20` | R-10 |
| `test_fresh_db_creates_schema_v20` | R-10 |
| `test_v19_to_v20_back_fills_s1_informs_edge` | R-01, AC-09 |
| `test_v19_to_v20_back_fills_s2_informs_edge` | R-01, AC-09 |
| `test_v19_to_v20_back_fills_s8_coaccess_edge` | R-01, AC-09 |
| `test_v19_to_v20_s1_s2_count_parity_after_migration` | AC-01 |
| `test_v19_to_v20_s8_count_parity_after_migration` | AC-02 |
| `test_v19_to_v20_excludes_excluded_sources` | R-06, R-07, C-04 |
| `test_v19_to_v20_migration_idempotent_clean_state` | AC-07, R-09 |
| `test_v19_to_v20_migration_idempotent_with_preexisting_reverse` | AC-14, R-09 |
| `test_v19_to_v20_empty_graph_edges_is_noop` | edge case |

**All workspace tests**: `cargo test --workspace` — zero failures across all packages.

---

## Issues

None. No deviations from pseudocode.

**Notable**: The v19→v20 back-fill source filters (`S1`, `S2`, `S8`) are disjoint from the v18→v19
filters (`co_access`), so row-count assertions in `migration_v18_to_v19.rs` did NOT require
updating (no cross-contamination). Only the schema-version assertions in that file were stale.

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced entry #3889 (back-fill pattern),
  #4079 (crt-044 ADR-001 two-statement strategy), #4078 (S8 gap pattern), #3900 (migration
  procedure). All applied directly.
- **Stored**: attempted `context_correct` on entry #3894 (Schema Version Cascade checklist) to
  add the crt-044 finding — blocked by anonymous agent Write capability restriction. Pattern to
  add manually:

  > **Gotcha (crt-044)**: When a second data-only back-fill migration is added to the same table
  > as a prior one, all historical migration test exact-version assertions (`assert_eq!(version, N)`)
  > go stale again. Fix strategy: convert ALL historical migration test post-migration version
  > assertions from `== N` to `>= N`; convert `test_current_schema_version_is_N` to
  > `assert!(... >= N)`; convert fresh-DB checks to `assert_eq!(version, CURRENT_SCHEMA_VERSION as i64)`.
  > Keep `sqlite_parity.rs` test exact (it is a spec assertion for the current version).
  > Row-count cross-contamination is avoided when back-fill source filters are disjoint.
  > Files affected at crt-044: `migration_v18_to_v19.rs` (6 assertions), `sqlite_parity.rs` (1).
