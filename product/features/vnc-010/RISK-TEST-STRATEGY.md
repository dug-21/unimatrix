# vnc-010: Risk-Test Strategy

## Risk Registry

### R-01: Migration Column Addition Failure (from SR-01)

**Description**: ALTER TABLE ADD COLUMN could fail if the column already exists (re-run scenario) or if the database is locked.
**Severity**: High
**Likelihood**: Low
**Mitigation**: Guard with pragma_table_info check before ALTER. SQLite ADD COLUMN is atomic within a transaction.
**Test**: Integration test: open store at v7, verify migration adds column, re-open to verify idempotency.

### R-02: Backfill Correctness (from SR-01)

**Description**: Existing quarantined entries get pre_quarantine_status=0 (Active). If any were not actually quarantined from Active (impossible under current code, but defensive), the backfill is wrong.
**Severity**: Medium
**Likelihood**: Very Low
**Mitigation**: Backfill is correct by construction (old code only allowed Active->Quarantined). Audit log can be consulted for manual correction.
**Test**: Integration test: create quarantined entry at v7 schema, migrate, verify pre_quarantine_status=0.

### R-03: Counter Drift on Non-Active Quarantine (from SR-02)

**Description**: Quarantining a Deprecated entry must decrement total_deprecated and increment total_quarantined. If the counter logic uses hardcoded status names instead of the generic status_counter_key(), counters drift.
**Severity**: Medium
**Likelihood**: Low (existing code is generic)
**Mitigation**: The existing `change_status_with_audit` uses `status_counter_key(old_status)` which is fully generic. No code change needed for counters.
**Test**: Integration test: quarantine Deprecated entry, verify total_deprecated decremented and total_quarantined incremented. Restore, verify reverse.

### R-04: Invalid pre_quarantine_status on Restore (from SR-04)

**Description**: If pre_quarantine_status contains a value not in {0,1,2}, Status::try_from will fail. The restore must handle this gracefully.
**Severity**: Medium
**Likelihood**: Very Low
**Mitigation**: ADR-002 specifies fallback to Active. The try_from error is caught, Active is used, and the audit log records the fallback.
**Test**: Unit test: manually set pre_quarantine_status=99, restore, verify entry becomes Active.

### R-05: Regression in Existing Quarantine Behavior

**Description**: The Active->Quarantined->Active path must continue to work identically to the current implementation.
**Severity**: High
**Likelihood**: Low
**Mitigation**: All existing quarantine tests must pass. The Active path now additionally sets pre_quarantine_status=0, which is additive.
**Test**: Run all existing quarantine/restore tests unchanged.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Test Coverage |
|------------|-------------------|---------------|
| SR-01: Migration backfill | R-01, R-02 | Migration integration tests |
| SR-02: Counter bookkeeping | R-03 | Counter integrity integration tests |
| SR-03: Correct operation | N/A (no change) | Existing correction tests |
| SR-04: Restore target integrity | R-04 | Invalid status fallback unit test |
| SR-05: Concurrent race | N/A (existing SQLite) | N/A (existing serialization) |

## Test Plan

### Unit Tests (unimatrix-store)

| Test | Covers |
|------|--------|
| EntryRecord with pre_quarantine_status serialization round-trip | Schema field |
| EntryRecord with pre_quarantine_status=None serialization | Nullable handling |
| entry_from_row with NULL pre_quarantine_status | SQL mapping |
| entry_from_row with valid pre_quarantine_status | SQL mapping |

### Integration Tests (unimatrix-server)

| Test | Covers | Risk |
|------|--------|------|
| Quarantine Active entry -> pre_quarantine_status=0 | AC-3 | R-05 |
| Quarantine Deprecated entry -> pre_quarantine_status=1 | AC-1 | R-03 |
| Quarantine Proposed entry -> pre_quarantine_status=2 | AC-2 | R-03 |
| Restore to Deprecated (round-trip) | AC-4 | R-03 |
| Restore to Proposed (round-trip) | AC-4 | R-03 |
| Restore with NULL pre_quarantine_status -> Active fallback | AC-5 | R-04 |
| Restore with invalid pre_quarantine_status -> Active fallback | AC-10 | R-04 |
| Idempotent quarantine (already quarantined) | AC-6 | R-05 |
| Counter integrity: Deprecated quarantine round-trip | AC-8 | R-03 |
| Migration v7->v8: column added, backfill correct | AC-7 | R-01, R-02 |
| Migration idempotency: re-open at v8 | AC-7 | R-01 |
| Audit log contains pre_quarantine_status detail | AC-9 | - |

### Existing Tests (Must Pass)

All tests in:
- `crates/unimatrix-server/src/mcp/tools.rs` (quarantine params tests)
- `crates/unimatrix-store/src/schema.rs` (status round-trip tests)
- Any integration tests exercising quarantine/restore

## Risk Summary

| Risk | Severity | Test Coverage |
|------|----------|---------------|
| R-01: Migration failure | High | Integration |
| R-02: Backfill correctness | Medium | Integration |
| R-03: Counter drift | Medium | Integration |
| R-04: Invalid restore target | Medium | Unit + Integration |
| R-05: Regression | High | Existing tests |

**Top 3 by severity**: R-01, R-05, R-03
