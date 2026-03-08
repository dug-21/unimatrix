# vnc-010: Scope Risk Assessment

## SR-01: Migration Backfill Assumption

**Risk**: Existing quarantined entries (created under v7 schema) have no `pre_quarantine_status` value. The migration must backfill these. Since the old code only allowed quarantine from Active status, backfilling as `Active` (0) is correct — but if any entries were quarantined via direct SQL manipulation outside the tool handler, the assumption may be wrong.

**Severity**: Medium
**Likelihood**: Low (all quarantine operations go through the tool handler)
**Mitigation**: Backfill as Active (0). Document the assumption in migration comments. The audit_log can be consulted post-migration if any entry's pre-quarantine status needs manual correction.

## SR-02: Status Counter Bookkeeping for Non-Active Sources

**Risk**: The `change_status_with_audit` method decrements the source status counter and increments the target status counter. Currently only Active->Quarantined is tested. Adding Deprecated->Quarantined and Proposed->Quarantined paths means `total_deprecated` and `total_proposed` counters must decrement correctly. On restore, the reverse must happen (e.g., increment `total_deprecated` not `total_active`).

**Severity**: Medium
**Likelihood**: Low (the existing code already uses `status_counter_key(old_status)` generically)
**Mitigation**: The existing generic counter logic should work without changes. Add integration tests for each status transition to verify.

## SR-03: Interaction with Correct Operation

**Risk**: The `correct` operation rejects both Deprecated and Quarantined entries. Now that Deprecated entries can be quarantined, there's a question: can a Deprecated entry that was quarantined and then restored be corrected? Answer: No — correction rejects Deprecated entries regardless. No behavior change needed.

**Severity**: Low
**Likelihood**: N/A (no change required)
**Mitigation**: Document in specification that correction eligibility is unchanged.

## SR-04: Restore Target Status Integrity

**Risk**: If `pre_quarantine_status` contains an invalid status value (e.g., corruption, future status enum variant), the restore operation could fail or set an unexpected status.

**Severity**: Medium
**Likelihood**: Very Low
**Mitigation**: Validate `pre_quarantine_status` via `Status::try_from()` during restore. Fall back to Active if the value is invalid, with a warning in the audit log.

## SR-05: Concurrent Quarantine/Restore Race

**Risk**: Two concurrent quarantine or restore operations on the same entry could create inconsistent state. This is an existing risk not introduced by this feature — the `change_status_with_audit` method runs in a transaction.

**Severity**: Low
**Likelihood**: Very Low (single-writer SQLite, serialized via spawn_blocking)
**Mitigation**: No change needed. SQLite serializes writes.

## Summary

| ID | Risk | Severity | Likelihood | Action |
|----|------|----------|------------|--------|
| SR-01 | Migration backfill assumption | Medium | Low | Backfill as Active, document |
| SR-02 | Counter bookkeeping | Medium | Low | Verify existing generic logic, add tests |
| SR-03 | Correct operation interaction | Low | N/A | Document only |
| SR-04 | Restore target integrity | Medium | Very Low | Validate + fallback |
| SR-05 | Concurrent race | Low | Very Low | No change (existing SQLite serialization) |

**Top 3 risks for architect attention**: SR-01, SR-02, SR-04
