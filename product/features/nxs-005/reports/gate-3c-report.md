# Gate 3c Report: Risk-Based Validation

**Feature**: nxs-005 (SQLite Storage Engine Migration)
**Gate**: 3c (Final Risk-Based Validation)
**Result**: REWORKABLE FAIL
**Date**: 2026-03-04

## Validation Results

### Test Coverage vs Risk Strategy: PASS (store layer)
- All 10 risks from the Risk-Based Test Strategy are addressed at the store layer
- 87 SQLite tests (41 unit + 46 parity integration) provide comprehensive coverage
- 234 redb regression tests pass (zero regression)
- 1695 workspace tests pass (redb backend)

### Test Coverage vs Risk Strategy: FAIL (server layer)
- R-03 (Transaction Type Abstraction Leakage): Server does not compile under `backend-sqlite`
- AC-03: Full workspace compilation under SQLite: BLOCKED
- AC-16: infra-001 full harness against SQLite binary: BLOCKED

### Risk Strategy Test Scenario Completion

| Risk | Store Layer | Server Layer | Status |
|------|------------|-------------|--------|
| R-01 | COVERED (46 parity + 41 unit) | BLOCKED | Partial |
| R-02 | COVERED (basic) | BLOCKED | Partial |
| R-03 | COVERED (unimatrix-store + core) | FAIL (server 214 errors) | Fail |
| R-04 | COVERED (migration chain) | N/A | Pass |
| R-05 | COVERED (self-pair, CHECK) | N/A | Pass |
| R-06 | COVERED (basic drain) | BLOCKED | Partial |
| R-07 | COVERED (WAL mode) | N/A | Pass |
| R-08 | PASS (store crate) | FAIL (server) | Fail |
| R-09 | NOT STARTED | N/A | Deferred |
| R-10 | COVERED (basic) | BLOCKED | Partial |

### Specific Issues

1. **Server Transaction API Incompatibility**: The server (unimatrix-server) directly imports redb table definition types and uses redb transaction table handle methods. Under `backend-sqlite`, these types are not available. The SQLite transaction wrapper types exist but have different method signatures. This is the single blocking issue.

2. **Root Cause**: The scope document states "unimatrix-server requires zero changes" and "all 34 methods retain identical signatures," but the server bypasses the Store API to directly use redb transaction internals for audit logging, agent registry, and the store/correct/deprecate tool operations. This coupling was not detected during scope or architecture review.

## Recommendation

**REWORKABLE FAIL**: The Store-level implementation is complete, tested, and high quality. The blocking issue is the server-level transaction API compatibility layer. This requires:

1. Adding string-based table name constants to the SQLite export path
2. Adding generic `.insert()`, `.get()`, `.remove()`, `.range()`, `.iter()` methods to `SqliteMutableTableHandle` and `SqliteTableHandle`
3. Adding `next_entry_id()` and `increment_counter()` functions for `SqliteWriteTransaction`
4. Re-exporting these from `lib.rs` under `cfg(feature = "backend-sqlite")`

Estimated effort: Moderate. The transaction wrapper types already exist (txn.rs); they need additional methods with type-erased signatures matching the redb AccessGuard pattern. No server code changes needed if the compatibility layer is correct.

## Files Reviewed
- All 10 files in `crates/unimatrix-store/src/sqlite/`
- 2 parity test files in `crates/unimatrix-store/tests/`
- `crates/unimatrix-store/src/lib.rs` (cfg-gated re-exports)
- `crates/unimatrix-core/src/adapters.rs` (StoreAdapter compatibility)
- `crates/unimatrix-server/src/` (identified 8 files with redb transaction coupling)
