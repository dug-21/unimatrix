# Risk Coverage Report: bugfix-509

## Summary

Bugfix-509 adds a compound index `idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)` and bumps
schema version from 22 to 23 in `migration.rs` and `db.rs`. The fix is a pure additive DDL change —
no existing rows, columns, or logic were modified.

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| MIG-V23-U-01 | CURRENT_SCHEMA_VERSION constant is >= 23 | `test_current_schema_version_is_at_least_23` | PASS | Full |
| MIG-V23-U-02 | Fresh DB initializes directly to v23 with compound index present | `test_fresh_db_creates_schema_v23` | PASS | Full |
| MIG-V23-U-03 | v22→v23 migration creates `idx_entry_tags_tag_entry_id` | `test_v22_to_v23_migration_creates_compound_index` | PASS | Full |
| MIG-V23-U-04 | Compound index has correct column ordering (tag first, entry_id second) | `test_v22_to_v23_compound_index_has_correct_columns` | PASS | Full |
| MIG-V23-U-05 | Re-opening a v23 database is a no-op (idempotency) | `test_v23_migration_idempotent` | PASS | Full |
| REG-01 | Existing schema migration chain (v7→current) still advances correctly | `test_migration_v7_to_v8_backfill` (fixed assertion) | PASS | Full |
| INT-01 | Restart persistence works with v23 schema | `test_data_persistence_across_restart` (infra-001 lifecycle) | PASS | Full |
| INT-02 | Store/search/lookup operations unaffected by new index | `test_store_roundtrip`, `test_store_minimal`, `test_search_returns_results`, `test_lookup_by_topic` (infra-001 tools) | PASS | Full |

---

## Test Results

### Unit Tests (cargo test --workspace)

- Total: 2,764+ across all crates
- Passed: all
- Failed: 0

Note: `test_migration_v7_to_v8_backfill` in `unimatrix-server` had a hardcoded
`assert_eq!(version, 22)` assertion that was made stale by this schema bump. This was a bad test
assertion caused directly by the fix (updating the version constant from 22 to 23). Updated both
occurrences to `assert_eq!(version, 23)` — the test now passes.

### New Bug-Specific Tests

Command: `cargo test -p unimatrix-store --test migration_v22_to_v23 --features test-support`

| Test | Result |
|------|--------|
| `test_current_schema_version_is_at_least_23` | PASS |
| `test_fresh_db_creates_schema_v23` | PASS |
| `test_v22_to_v23_migration_creates_compound_index` | PASS |
| `test_v22_to_v23_compound_index_has_correct_columns` | PASS |
| `test_v23_migration_idempotent` | PASS |

All 5 passed in 0.13s.

### Integration Tests

**Smoke gate** (`pytest -m smoke --timeout=60`):
- Total: 22
- Passed: 22
- Failed: 0
- Time: 191s

**Targeted lifecycle** (`test_data_persistence_across_restart`):
- Passed: 1/1

**Targeted tools** (`test_store_roundtrip`, `test_store_minimal`, `test_search_returns_results`, `test_lookup_by_topic`):
- Passed: 4/4

Note: Full `test_lifecycle.py` and `test_tools.py` suite runs exceeded the system timeout (~300s)
due to ONNX model initialization overhead per fixture. This is a pre-existing harness performance
characteristic unrelated to this fix. The smoke subset (which covers all critical paths for
store/search/lifecycle/restart) fully passed.

---

## Clippy

`cargo clippy --workspace -- -D warnings` reports `collapsible_if` errors in:
- `crates/unimatrix-engine/src/auth.rs` (2 errors)
- `crates/unimatrix-engine/src/event_queue.rs`
- `crates/unimatrix-observe/src/attribution.rs`, `baseline.rs`, `detection/*.rs`

All are pre-existing, confirmed in the spawn prompt. None are in `unimatrix-store` or any file
modified by bugfix-509. Not caused by this fix.

---

## Gaps

None. All risks identified for the schema version bump have test coverage.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: Schema version is 23 after migration | PASS | `test_current_schema_version_is_at_least_23` + `test_fresh_db_creates_schema_v23` |
| AC-02: Compound index `idx_entry_tags_tag_entry_id` exists on fresh DB | PASS | `test_fresh_db_creates_schema_v23` (sqlite_master query) |
| AC-03: v22→v23 migration creates the compound index on upgrade | PASS | `test_v22_to_v23_migration_creates_compound_index` |
| AC-04: Index column order is `tag` first, `entry_id` second | PASS | `test_v22_to_v23_compound_index_has_correct_columns` (PRAGMA index_info) |
| AC-05: Migration is idempotent | PASS | `test_v23_migration_idempotent` |
| AC-06: No regressions in store/search/persistence behavior | PASS | Smoke 22/22 + targeted tools/lifecycle tests |

---

## Test Assertion Fix

`crates/unimatrix-server/src/server.rs::test_migration_v7_to_v8_backfill` had two hardcoded
`assert_eq!(version, 22)` assertions and a stale comment referencing "crt-046 goal_clusters table."
These were made incorrect by the schema bump in this fix. Updated both to `assert_eq!(version, 23)`
and updated the comment to reference "bugfix-509 compound index." This is categorized as a bad test
assertion caused by the fix, not a pre-existing issue.
