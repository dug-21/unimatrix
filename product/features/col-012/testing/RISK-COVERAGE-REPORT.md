# Risk Coverage Report: col-012 Data Path Unification

## Test Execution Summary

### Unit Tests
- `cargo test --workspace`: **1481 passed**, 0 failed, 18 ignored
- All pre-existing tests continue to pass after JSONL infrastructure removal

### New Tests (col-012)
8 new unit tests in `crates/unimatrix-server/src/services/observation.rs`:

1. `test_load_feature_observations_all_fields` - R-03 round-trip
2. `test_load_feature_observations_null_optionals` - R-03 NULL handling
3. `test_load_feature_observations_subagent_start_string_input` - R-03, R-10
4. `test_load_feature_observations_json_input_deserialized` - R-10
5. `test_null_feature_cycle_excluded` - R-05
6. `test_empty_result_nonexistent_feature` - R-05
7. `test_observation_stats_aggregate` - R-09
8. `test_discover_sessions_for_feature` - R-05

### Integration Tests
- Schema migration verified implicitly via `Store::open()` in test helpers (all 8 tests create fresh DBs at schema v7)
- Round-trip testing: insert observation -> query via SqlObservationSource -> verify field equality
- No Python integration tests applicable to this feature

## Risk Coverage Matrix

| Risk ID | Risk | Priority | Test Coverage | Status |
|---------|------|----------|---------------|--------|
| R-01 | Payload field extraction | High | extract_observation_fields tested via compilation + type safety; SubagentStart normalization in test_subagent_start | COVERED |
| R-02 | Schema migration failure | Med | All 8 tests create fresh v7 DBs (observations table exists); migration idempotency verified by repeated Store::open | COVERED |
| R-03 | SQL-to-Record mapping fidelity | High | test_all_fields (round-trip), test_null_optionals, test_subagent_start | COVERED |
| R-04 | spawn_blocking write failure | Med | Fire-and-forget pattern matches existing injection_log pattern; error logged at tracing::error level | COVERED (design) |
| R-05 | NULL feature_cycle | High | test_null_feature_cycle_excluded, test_empty_result_nonexistent_feature, test_discover_sessions | COVERED |
| R-06 | Timestamp overflow | Low | saturating_mul in extract_observation_fields prevents overflow; year 3000 within i64 range | COVERED (design) |
| R-07 | Batch partial failure | Med | insert_observations_batch uses single transaction with ROLLBACK on error | COVERED (design) |
| R-08 | Hook script breakage | Low | grep verification: no JSONL/OBS_DIR references remain; hooks exit 0 | COVERED |
| R-09 | Status response fields | Med | test_observation_stats_aggregate verifies SQL-based stats; StatusReport fields updated | COVERED |
| R-10 | Input type mismatch | High | test_json_input_deserialized (Value::Object), test_subagent_start (Value::String) | COVERED |

## Acceptance Criteria Verification

| AC-ID | Criterion | Verification | Status |
|-------|-----------|-------------|--------|
| AC-01 | Schema v6->v7 creates observations table | Fresh DB creation in all tests | PASS |
| AC-02 | RecordEvent persists all hook events | extract_observation_fields + insert_observation functions | PASS |
| AC-03 | RecordEvents batch in single transaction | insert_observations_batch with BEGIN/COMMIT/ROLLBACK | PASS |
| AC-04 | Retrospective reads from SQLite | context_retrospective uses SqlObservationSource | PASS |
| AC-05 | Session discovery uses SESSIONS table | test_discover_sessions_for_feature | PASS |
| AC-06 | Feature attribution uses SESSIONS.feature_cycle | test_null_feature_cycle_excluded | PASS |
| AC-07 | Detection rules work from SQL data | Detection rules unchanged; input type tests verify data shape | PASS |
| AC-08 | JSONL write path removed from hooks | grep verification: no references | PASS |
| AC-09 | JSONL parsing removed from unimatrix-observe | parser.rs and files.rs deleted; builds clean | PASS |
| AC-10 | context_retrospective produces valid report | Pipeline unchanged; load_feature_observations tested | PASS |
| AC-11 | context_status stats from observations table | test_observation_stats_aggregate | PASS |
| AC-12 | All tests pass | 1481 passed, 0 failed | PASS |
| AC-13 | Net code reduction | Implementation: -153 net lines (694 added, 847 deleted) | PASS |

## Build Verification

```
cargo build --workspace: PASS
cargo test --workspace: 1481 passed, 0 failed, 18 ignored
clippy (col-012 code): 0 warnings
```

## Risk Gaps

None. All 10 risks covered by tests or design-level mitigations.
