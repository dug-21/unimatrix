# Agent Report: col-028-agent-5-migration

**Feature**: col-028 — Unified Phase Signal Capture (Read-Side + query_log)
**Component**: Schema Migration v16→v17 (Component 4/5)
**Agent ID**: col-028-agent-5-migration

---

## Summary

Implemented the Schema Migration v16→v17 atomic change unit for col-028. All four
atomic sites (migration.rs, analytics.rs, query_log.rs, migration_v16_to_v17.rs)
were modified in a single commit per C-09/SR-01 requirements.

---

## Files Modified/Created

### unimatrix-store crate (primary deliverables)
- `crates/unimatrix-store/src/migration.rs` — Bumped CURRENT_SCHEMA_VERSION 16→17; added v16→v17 branch with pragma_table_info pre-check, ALTER TABLE query_log ADD COLUMN phase TEXT, CREATE INDEX IF NOT EXISTS idx_query_log_phase, UPDATE counters SET value = 17
- `crates/unimatrix-store/src/analytics.rs` — Added phase: Option<String> to AnalyticsWrite::QueryLog variant; added ?9 bind and column to INSERT; updated test fixture struct literal
- `crates/unimatrix-store/src/query_log.rs` — Added phase: Option<String> field to QueryLogRecord; added phase as final parameter to ::new(); updated both scan_query_log_by_* SELECT statements to include phase; updated row_to_query_log to read index 9; passed record.phase.clone() in insert_query_log
- `crates/unimatrix-store/src/db.rs` — Added phase TEXT column to fresh query_log DDL; added CREATE INDEX IF NOT EXISTS idx_query_log_phase in fresh schema creation
- `crates/unimatrix-store/tests/migration_v16_to_v17.rs` — **New file**: 7 tests (T-V17-01 through T-V17-06 + test_current_schema_version_is_17)

### SR-02 cascade updates
- `crates/unimatrix-store/tests/migration_v15_to_v16.rs` — All assert_eq!(..., 16) → 17; test_current_schema_version_is_16 → _is_17; test_fresh_db_creates_schema_v16 → _v17; T-V16-06 renamed; comments updated
- `crates/unimatrix-server/src/server.rs` — Lines 2059 and 2084: assert_eq!(version, 16) → 17

### Column count cascade fixes (discovered during testing)
- `crates/unimatrix-store/tests/migration_v10_to_v11.rs` — 2 occurrences of column_count query_log == 9 → 10 (fresh DDL change in db.rs caused cascade)
- `crates/unimatrix-store/tests/sqlite_parity.rs` — query_log column count 9 → 10 (two occurrences); test_schema_version_is_14 assertion 16 → 17; column names list updated to include "phase"

---

## Tests

All unimatrix-store tests passing:
- `migration_v16_to_v17.rs`: **7 passed, 0 failed** (T-V17-01..06 + AC-13 constant check)
- `migration_v15_to_v16.rs`: 15 passed, 0 failed
- `migration_v10_to_v11.rs`: 8 passed, 0 failed
- `sqlite_parity.rs`: 44 passed, 0 failed
- `analytics.rs` unit tests: 144 passed, 0 failed
- Full `cargo test -p unimatrix-store --features test-support`: all passed, 0 failed
- `cargo build --workspace`: zero errors

---

## AC-22 Verification

```
grep -rn 'schema_version.*== 16' crates/
```
Returns zero matches. Gate check passes.

---

## Issues / Blockers

None. All constraints satisfied:
- C-02: pragma_table_info pre-check present
- C-05: phase added as ?9, no existing bind index changed
- C-09: all four atomic sites modified in same commit
- SR-01: INSERT, both SELECT statements, and row_to_query_log updated atomically
- SR-02: all schema_version == 16 assertions updated to 17
- SR-03: uds/listener.rs and mcp/tools.rs already had None as final arg (pre-updated by other agents)
- AC-23: cargo build --workspace compiles without error

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-store migration -- found #374, #1263, #836, #375, #365; no specific entry for column-count cascade test updates
- Stored: Attempted via /uni-store-pattern — agent lacks Write capability (anonymous). Pattern to store: "When adding a column to fresh DDL in db.rs, grep for hardcoded column counts in migration_v10_to_v11.rs (~lines 345 and 515) and sqlite_parity.rs — both assert exact query_log column counts and must be updated for every db.rs DDL change. sqlite_parity.rs also has test_schema_version_is_14 asserting the exact current version with assert_eq!(version, N)."
