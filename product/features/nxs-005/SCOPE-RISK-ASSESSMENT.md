# Scope Risk Assessment: nxs-005

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | rusqlite `bundled` feature introduces C compilation dependency; build times increase and cross-compilation becomes harder | Low | High | Architect should verify CI build times. Accept the tradeoff -- SQLite's C code is battle-tested and `bundled` is the standard approach. |
| SR-02 | SQLite WAL mode creates sidecar files (-wal, -shm) that PidGuard and backup tooling may not expect | Low | Medium | Architect should verify PidGuard, compact(), and any file-level operations handle multi-file database correctly. |
| SR-03 | bincode positional encoding is fragile across schema versions; SQLite blob storage preserves this fragility rather than resolving it | Medium | Low | Accept for nxs-005 (zero-change scope). Flag that nxs-006 schema normalization is the resolution path. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Feature flag dual-backend increases test matrix (every test runs 2x); CI time may double for store crate | Medium | High | Spec writer should define whether parity tests run in CI on every commit or only on release. Architect should design test macro to minimize duplication. |
| SR-05 | "Zero functional change" is hard to verify exhaustively; subtle semantic differences (NULL handling, integer overflow, collation order) may hide in edge cases | High | Medium | Architect should design parity test harness that compares outputs byte-for-byte where possible. 234 tests are necessary but may not be sufficient. |
| SR-06 | Scope excludes index table elimination (nxs-006) but keeps 5 manual index tables as SQLite tables -- this is intentional duplication that adds no SQL benefit yet | Low | Low | Accept. The zero-change constraint is more valuable than premature optimization. Document clearly for nxs-006. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | StoreAdapter in unimatrix-core wraps Arc<Store>; if Store struct changes shape (e.g., Connection vs Database), adapter may need adjustment despite "no changes outside store crate" claim | Medium | Medium | Architect should verify StoreAdapter coupling. If Store's public type changes, the adapter import path changes even if the trait boundary holds. |
| SR-08 | redb::ReadTransaction/WriteTransaction have different lifetime semantics than rusqlite transactions; methods borrowing from transactions may not translate directly | Medium | High | Architect should map every transaction pattern in read.rs/write.rs to rusqlite equivalents early. This is the highest-effort translation. |
| SR-09 | Data migration tool must handle partially-written redb databases (crash during previous write) and corrupt entries | Medium | Low | Migration tool should report and skip corrupt entries rather than failing entirely. |

## Assumptions

1. **SQLite WAL concurrency matches redb MVCC** (Goals section) -- SQLite WAL allows concurrent readers during writes, but `SQLITE_BUSY` can occur if a reader holds a snapshot too long. redb never returns busy errors. If the server holds long-lived read transactions, this assumption could fail.
2. **EntryStore trait boundary holds completely** (Non-Goals) -- assumes StoreAdapter only depends on Store's public methods, not on redb-specific types leaking through. Needs verification.
3. **234 tests are sufficient for parity** (AC-02) -- assumes existing tests cover all behavioral edge cases. New edge cases from SQLite semantics (NULL, empty blob, max integer) may not be covered.

## Design Recommendations

1. **SR-05, SR-08**: Architect should prioritize transaction semantics mapping as the first design decision. Document every redb transaction pattern and its SQLite equivalent.
2. **SR-04**: Architect should design a `#[test_both_backends]` macro or similar to avoid 234 tests being copy-pasted.
3. **SR-07**: Architect should verify Store's public interface is backend-agnostic before committing to "zero changes outside store crate."
4. **SR-09**: Data migration tool should be fault-tolerant with per-table verification and skip-on-error semantics.
