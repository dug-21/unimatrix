# Agent Report: vnc-014-agent-4-migration

## Task
Implement v24→v25 schema migration in `migration.rs`, update `db.rs` DDL parity, update parity and cascade tests for vnc-014 (ASS-050 audit_log 4-column migration).

## Files Modified

- `crates/unimatrix-store/src/migration.rs` — bumped `CURRENT_SCHEMA_VERSION` to 25; added v24→v25 migration block with 4 pragma pre-checks (all before any ALTER per ADR-004), 4 conditional ALTERs, 2 new indexes, 2 append-only triggers, version bump to 25
- `crates/unimatrix-store/src/db.rs` — updated `audit_log` DDL in `create_tables_if_needed()` to 12 columns (added `credential_type`, `capability_used`, `agent_attribution`, `metadata`); added matching indexes and append-only triggers (byte-identical to migration DDL per R-11)
- `crates/unimatrix-store/tests/sqlite_parity.rs` — updated schema version assertion to 25; added 3 new tests: `test_audit_log_column_count_is_12`, `test_audit_log_new_columns_present_and_correct`, `test_audit_log_append_only_triggers_exist`
- `crates/unimatrix-store/tests/migration_v23_to_v24.rs` — updated 3 hardcoded `assert_eq!(version, 24)` to `assert!(version >= 24)` to remain valid as schema advances past v24

## Files Created

- `crates/unimatrix-store/tests/migration_v24_to_v25.rs` — 12 migration tests per component test plan (MIG-V25-U-01 through MIG-V25-U-10, plus parity and trigger tests); includes `create_v24_database()` helper building an 8-column audit_log at schema v24

## Tests

- 291 lib tests (unimatrix-store): all pass
- 52 sqlite_parity tests: all pass
- 12 migration_v24_to_v25 tests: all pass
- 0 new failures introduced

## Deviations from Pseudocode

None. Implementation follows validated pseudocode exactly.

## Issues Encountered

- Two format string compile errors in initial `migration_v24_to_v25.rs`: `DEFAULT must be '{}'` was interpreted as a Rust format placeholder. Fixed by escaping to `'{{}}'`.
- Pre-existing failures in `audit.rs` unit tests (`test_audit_event_serde_default_*`) were discovered during investigation. These are owned by agent-3-audit-event (AuditEvent struct) and pass once their feature flag and struct fields are in place. Not introduced by migration changes.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced pattern #4125 (schema version cascade checklist) and ADR-004 (pragma pre-check ordering rule). Both applied directly.
- Stored: nothing novel to store -- the pragma pre-check pattern, IF NOT EXISTS idempotency, and cascade checklist are already captured in Unimatrix entries #4125 and relevant ADRs. No new traps discovered.
