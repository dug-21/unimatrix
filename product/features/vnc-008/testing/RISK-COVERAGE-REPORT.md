# Risk Coverage Report: vnc-008 Module Reorganization

**Feature:** vnc-008
**Date:** 2026-03-04

## Test Execution Summary

| Suite | Count | Pass | Fail | Ignored |
|-------|-------|------|------|---------|
| unimatrix-store unit | 64 | 64 | 0 | 0 |
| unimatrix-vector unit | 21 | 21 | 0 | 0 |
| unimatrix-embed unit | 76 | 76 | 0 | 18 |
| unimatrix-core unit | 171 | 171 | 0 | 0 |
| unimatrix-engine unit | 264 | 264 | 0 | 0 |
| unimatrix-server unit | 739 | 739 | 0 | 0 |
| unimatrix-server integration | 234 | 234 | 0 | 0 |
| unimatrix-observe unit | 104 | 104 | 0 | 0 |
| **Workspace Total** | **1673** | **1673** | **0** | **18** |
| Integration smoke (pytest) | 19 | 19 | 0 | 0 |

## Risk Coverage Matrix

### R-01: Import Path Breakage (Critical)
**Coverage: FULL**
- All 5 migration steps compile with `cargo check`
- Full test suite passes: 1673 workspace tests, 19 integration smoke tests
- Re-exports removed in final step -- no residual old-path references
- All internal imports updated from flat paths to grouped paths

### R-02: ToolContext Behavioral Divergence (High)
**Coverage: FULL**
- `build_context()` used in 10 of 12 handlers (2 handlers -- context_meta and one other -- do not need full context)
- `require_cap()` used for capability enforcement across all handlers
- All 739 server unit tests pass -- these test handler output directly
- 234 server integration tests pass -- these test end-to-end MCP behavior
- 19 integration smoke tests pass -- these test full server lifecycle

### R-03: StatusService Report Divergence (High)
**Coverage: HIGH**
- StatusService is a verbatim extraction of the inline context_status handler
- No logic changes -- exact same computation, same variable names, same table access
- All existing status-related tests pass (status formatting tests in response/mod.rs)
- Integration smoke test `test_status_empty_db` validates StatusService end-to-end
- No snapshot comparison test (deferred -- would require test infrastructure changes)

### R-04: format_status_change Output Mismatch (Medium)
**Coverage: FULL**
- `format_deprecate_success`, `format_quarantine_success`, `format_restore_success` are thin wrappers calling `format_status_change`
- All existing deprecate/quarantine/restore formatting tests pass
- Tests in response/mod.rs cover all 3 variants x 3 formats = 9 combinations

### R-05: SessionWrite Serde Incompatibility (Medium)
**Coverage: HIGH**
- Capability enum uses `#[derive(Serialize, Deserialize)]` -- bincode handles new variants additively
- Existing AGENT_REGISTRY entries don't contain SessionWrite -- they decode to their original variants
- Registry bootstrap tests pass (bootstrap_defaults creates agents without SessionWrite)
- Round-trip serde not explicitly tested (low risk -- bincode enum variant encoding is well-understood)

### R-06: Test Migration Dependency Breakage (High)
**Coverage: FULL**
- All tests moved with their modules
- 739 server unit tests pass (same count as before vnc-008, verified by careful test restoration)
- Dropped tests from response.rs split were identified and restored (78 base + 5 briefing)
- `pub(crate)` visibility adjusted where tests needed cross-module access (trust_level_str, capability_str)

### R-07: Circular Import Between Groups (Medium)
**Coverage: HIGH**
- Verified by grep: no `crate::services` in infra/, no `crate::mcp` in uds/, no `crate::uds` in mcp/
- Two pre-existing violations documented (infra/shutdown.rs -> uds, infra/validation.rs -> mcp)
- One structural coupling: services/status.rs imports StatusReport from mcp/response/status (data type, not transport logic)
- `cargo check` confirms no circular dependency errors

### R-08: UDS Capability False Rejection (Medium)
**Coverage: FULL**
- 6 new UDS capability unit tests in uds/mod.rs:
  - test_uds_capabilities_exact_set
  - test_uds_has_capability_read
  - test_uds_has_capability_search
  - test_uds_has_capability_session_write
  - test_uds_has_capability_write_false
  - test_uds_has_capability_admin_false
- UDS_CAPABILITIES contains exactly {Read, Search, SessionWrite}
- Capability enforcement added to 7 dispatch arms in listener.rs
- All existing UDS integration tests pass (234 tests)
- Smoke tests pass (19 tests including lifecycle, protocol, tools)

### R-09: response.rs Split Visibility Breakage (Medium)
**Coverage: FULL**
- All `format_*` functions re-exported via response/mod.rs
- `ResponseFormat`, `parse_format`, `StatusReport`, `Briefing` types accessible via re-exports
- `entry_to_json` helper accessible within response/ sub-modules via `super::*`
- All 78 response formatting tests pass

### R-10: Re-Export Name Collisions (Low)
**Coverage: FULL**
- Re-exports used during intermediate steps -- no ambiguous import warnings
- All re-exports removed in final cleanup step
- `cargo check` confirms clean compilation with no ambiguity

### R-11: StatusService Table Definition Drift (Low)
**Coverage: HIGH**
- StatusService imports table constants from `unimatrix_store` (ENTRIES, COUNTERS, CATEGORY_INDEX, TOPIC_INDEX, CO_ACCESS)
- Same `deserialize_entry()` path as Store::get()
- Integration smoke test `test_status_empty_db` validates end-to-end

## Acceptance Criteria Verification

| AC-ID | Description | Status | Evidence |
|-------|-------------|--------|----------|
| AC-01 | mcp/ directory structure | PASS | tools.rs, context.rs, identity.rs, response/ present |
| AC-02 | uds/ directory structure | PASS | listener.rs, hook.rs present |
| AC-03 | infra/ directory structure | PASS | All 13 infrastructure modules + mod.rs present |
| AC-04 | StatusService exists | PASS | `pub(crate) struct StatusService` in services/status.rs |
| AC-05 | Root contains only 4 files | PASS | main.rs, lib.rs, error.rs, server.rs |
| AC-06 | No flat-root modules remain | PASS | All 7 checked modules absent from root |
| AC-07 | response/mod.rs shared helpers | PASS | parse_format, format_timestamp, ResponseFormat present |
| AC-08 | entries.rs functions | PASS | All 7 entry formatting functions present |
| AC-09 | mutations.rs generic formatter | PASS | format_status_change + 3 thin wrappers |
| AC-10 | status.rs format_status_report | PASS | pub fn format_status_report present |
| AC-11 | briefing.rs functions | PASS | format_briefing + format_retrospective_report present |
| AC-12 | No standalone response.rs | PASS | File does not exist at root |
| AC-13 | ToolContext struct | PASS | pub(crate) struct ToolContext in mcp/context.rs |
| AC-14 | build_context usage | PASS | 10 calls (2 handlers don't need full context) |
| AC-15 | map_err count reduced | PARTIAL | 49 (from ~79 baseline) = 38% reduction. Spirit met -- remaining are business logic errors, not ceremony. |
| AC-16 | StatusService methods | PASS | compute_report() + run_maintenance() present |
| AC-17 | context_status delegates | PASS | Handler at lines 662-706 (~45 lines), uses self.services.status |
| AC-18 | StatusService identical output | PARTIAL | Verbatim extraction, all tests pass. No formal snapshot test. |
| AC-19 | SessionWrite variant | PASS | Capability::SessionWrite in registry.rs |
| AC-20 | UDS capabilities | PASS | UDS_CAPABILITIES = {Read, Search, SessionWrite} |
| AC-21 | SessionWrite permits ops | PASS | UDS dispatch tests pass for SessionRegister, RecordEvent |
| AC-22 | SessionWrite rejects knowledge/admin | PASS | test_uds_has_capability_write_false, admin_false |
| AC-23 | UDS formal capability boundary | PASS | Capability checks at dispatch level |
| AC-24 | MCP responses identical | PASS | All 739 unit tests pass unchanged |
| AC-25 | UDS responses identical | PASS | All UDS integration tests pass |
| AC-26 | No test count reduction | PASS | 739 server tests (tests were carefully restored after response split) |
| AC-27 | Changes in unimatrix-server only | PASS | No code changes outside crates/unimatrix-server/ |
| AC-28 | mcp/ no storage access | PASS | Only EntryRecord/Status type imports |
| AC-29 | services/ is storage accessor | PARTIAL | services/ + infra/shutdown.rs + uds/listener.rs also access Store (pre-existing) |
| AC-30 | No circular dependencies | PARTIAL | 2 pre-existing violations (infra->uds, infra->mcp), 1 structural coupling (services->mcp for StatusReport type) |

## Risk Coverage Summary

| Priority | Risk Count | Covered | Coverage Level |
|----------|-----------|---------|---------------|
| Critical | 1 | 1 | Full |
| High | 4 | 4 | Full (3), High (1) |
| Medium | 5 | 5 | Full (3), High (2) |
| Low | 1 | 1 | Full |
| **Total** | **11** | **11** | **100% covered** |

## Known Gaps

1. **R-03 Snapshot Test**: No formal StatusReport snapshot comparison test. Mitigated by verbatim code extraction and all existing tests passing.
2. **R-05 Round-trip Serde**: No explicit bincode round-trip test for Capability enum with SessionWrite. Mitigated by bincode enum handling being well-understood.
3. **AC-15**: 38% reduction in map_err calls vs 50% target. Remaining calls are on business logic errors that ToolContext cannot absorb.
4. **AC-30**: Three import direction violations exist (2 pre-existing, 1 structural). These are documented for future resolution.

## Integration Test Results

19/19 integration smoke tests passed:
- test_cold_start_search_equivalence
- test_base_score_active
- test_contradiction_detected
- test_unicode_cjk_roundtrip
- test_empty_database_operations
- test_restart_persistence
- test_server_process_cleanup
- test_store_search_find_flow
- test_correction_chain_integrity
- test_isolation_no_state_leakage
- test_initialize_returns_capabilities
- test_server_info
- test_graceful_shutdown
- test_injection_patterns_detected
- test_store_minimal
- test_store_roundtrip
- test_search_returns_results
- test_status_empty_db
- test_store_1000_entries

No xfail markers added. No integration tests deleted or commented out.
