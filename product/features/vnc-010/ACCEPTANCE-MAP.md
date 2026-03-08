# vnc-010: Acceptance Map

## Acceptance Criteria to Implementation Mapping

| AC | Description | Chunk | Files | Test Type |
|----|-------------|-------|-------|-----------|
| AC-1 | Quarantine from Deprecated | C2 | tools.rs, server.rs | Integration |
| AC-2 | Quarantine from Proposed | C2 | tools.rs, server.rs | Integration |
| AC-3 | Quarantine from Active (existing) | C2 | tools.rs, server.rs | Integration (existing + new) |
| AC-4 | Restore to pre-quarantine status | C2 | server.rs | Integration |
| AC-5 | Restore fallback (NULL pre_quarantine) | C2 | server.rs | Integration |
| AC-6 | Idempotent quarantine | C2 | tools.rs | Integration (existing) |
| AC-7 | Schema migration v7->v8 | C1, C3 | migration.rs, db.rs | Integration |
| AC-8 | Status counter integrity | C2 | server.rs | Integration |
| AC-9 | Audit trail | C2 | server.rs | Integration |
| AC-10 | Invalid pre_quarantine fallback | C2 | server.rs | Unit + Integration |

## Risk to Acceptance Criteria Mapping

| Risk | AC Coverage | Confidence |
|------|-------------|------------|
| R-01: Migration failure | AC-7 | High |
| R-02: Backfill correctness | AC-7 | High |
| R-03: Counter drift | AC-8 | High |
| R-04: Invalid restore target | AC-5, AC-10 | High |
| R-05: Regression | AC-3, AC-6 | High |

## Verification Checklist

- [ ] All 10 acceptance criteria have at least one test
- [ ] All 5 risks have test coverage
- [ ] Existing quarantine/restore tests pass unchanged
- [ ] Migration tested on v7 database
- [ ] Counter integrity verified for Deprecated round-trip
- [ ] Fallback behavior verified for NULL and invalid pre_quarantine_status
- [ ] Audit log entries include pre_quarantine_status information
