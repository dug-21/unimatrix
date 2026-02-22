# nxs-001 Test Plan Overview

## Strategy

Tests are organized by risk priority. High-severity risks get dedicated test suites with comprehensive edge case coverage. All tests use the TestDb fixture for isolated, reproducible test databases.

## Risk-to-Test Mapping

| Risk | Priority | Test Location | Coverage Level |
|------|----------|---------------|----------------|
| R2: Update Path Stale Index Orphaning | 1 (CRITICAL) | tests in write.rs (update section) | Exhaustive: every indexed field individually + combinations |
| R1: Index-Entry Desync | 2 (CRITICAL) | tests in write.rs (insert section) + read.rs | Per-index verification after every write path |
| R7: QueryFilter Intersection | 3 (HIGH) | tests in query.rs | All filter combinations + property tests |
| R4: Schema Evolution | 4 (HIGH) | tests in schema.rs | Hardcoded byte fixtures simulating version skew. FIRST TEST WRITTEN. |
| R8: Status Transition Atomicity | 5 (HIGH) | tests in write.rs (status section) | Every transition + counter consistency |
| R5: Monotonic ID | 6 (HIGH) | tests in write.rs (insert section) | 100 sequential inserts |
| R3: Serialization Round-Trip | 7 (HIGH) | tests in schema.rs | All field types + edge values |
| R6: Transaction Atomicity | 8 (HIGH) | tests in write.rs | Drop-without-commit verification |
| R9: Tag Index Operations | 9 (MEDIUM) | tests in read.rs + write.rs | Single/multi-tag, add/remove |
| R10: Database Lifecycle | 10 (MEDIUM) | tests in db.rs | Create/open/compact cycle |
| R12: Error Types | 11 (MEDIUM) | tests in error.rs | Each variant constructible + Display |
| R11: VECTOR_MAP Bridge | 12 (MEDIUM) | tests in write.rs + read.rs | CRUD + boundary values |

## Test Organization

All tests are in-crate `#[cfg(test)]` modules within each source file. This allows testing `pub(crate)` functions directly.

## Cross-Component Integration Tests

1. **Insert + Read roundtrip**: Insert via C6, read via C7, verify all fields match (AC-06)
2. **Insert + All index queries**: Insert via C6, query each index via C7 (AC-07 through AC-11)
3. **Insert + Combined query**: Insert varied entries via C6, query via C8 (AC-17)
4. **Update + Index verification**: Update via C6, verify old indexes cleared + new indexes populated (AC-18)
5. **Status change + Query**: Change status via C6, query by status via C7 (AC-12)
6. **Lifecycle + Data persistence**: Open, insert, close, reopen, verify data via C7 (AC-14)
