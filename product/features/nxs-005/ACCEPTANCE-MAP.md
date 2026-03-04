# Acceptance Map: nxs-005 SQLite Storage Engine

## AC to Specification to Risk Traceability

| AC-ID | Spec Requirement | Risk Coverage | Wave | Verification |
|-------|-----------------|---------------|------|-------------|
| AC-01 | FR-02: Table Schema Parity | R-01, R-08 | W1 | Schema introspection test queries sqlite_master for all 17 table names and column types |
| AC-02 | FR-01: SQLite Backend Implementation | R-01, R-06 | W6 | `cargo test -p unimatrix-store --features backend-sqlite` -- 234 tests pass |
| AC-03 | FR-01 (integration) | R-03 | W6 | `cargo test --workspace --features unimatrix-store/backend-sqlite` |
| AC-04 | FR-02 (vector_map table) | R-01 | W3 | put_vector_mapping + get_vector_mapping + iter_vector_mappings roundtrip test |
| AC-05 | FR-04: Schema Migration | R-04 | W5 | Migration chain test: create at v0, migrate to v5, verify entries |
| AC-06 | FR-08: Data Migration Tooling | R-09 | W6 | Migration test: populate redb, export to SQLite, compare row counts per table |
| AC-07 | FR-03: WAL Mode | R-02, R-07 | W1 | Test: verify PRAGMA journal_mode returns 'wal'. Concurrent read+write test. |
| AC-08 | FR-01 (serialization) | R-01 | W2-W4 | Existing bincode roundtrip tests in schema.rs, signal.rs, sessions.rs pass unchanged |
| AC-09 | FR-02 (co_access CHECK) | R-05 | W2 | INSERT with entry_id_a > entry_id_b fails with constraint violation |
| AC-10 | FR-01 (counters) | R-10 | W2 | Concurrent next_entry_id test: 10 threads, all IDs unique |
| AC-11 | FR-10: Signal Queue Parity | R-06 | W4 | All signal queue tests (db.rs) pass under backend-sqlite |
| AC-12 | FR-11: Session Parity | R-01 | W4 | All session tests (sessions.rs) pass under backend-sqlite |
| AC-13 | FR-12: Injection Log Parity | R-01 | W4 | All injection_log tests pass under backend-sqlite |
| AC-14 | FR-05: Feature Flag | R-08 | W1 | `cargo check` (redb) + `cargo check --features backend-sqlite` (SQLite) both succeed |
| AC-15 | FR-06: Transaction Abstraction | R-03 | W1, W6 | Git diff: only store crate + minimal server import changes |
| AC-16 | FR-13: System-Level Validation | R-01, R-02, R-03 | W6 | Build binary with `--features unimatrix-store/backend-sqlite`, run full infra-001 harness (`python -m pytest suites/ -v --timeout=60` in `product/test/infra-001/`). All 157 tests pass across 8 suites (protocol, tools, lifecycle, volume, security, confidence, contradiction, edge_cases). |

## Coverage Matrix

| Wave | ACs Covered | Risk Scenarios Covered |
|------|------------|----------------------|
| W1: Foundation | AC-01, AC-07, AC-14 | R-01 (partial), R-08 |
| W2: Writes | AC-08 (partial), AC-09, AC-10 | R-01, R-05, R-10 |
| W3: Reads | AC-04, AC-08 (partial) | R-01 |
| W4: Specialized | AC-08, AC-11, AC-12, AC-13 | R-01, R-06 |
| W5: Migration | AC-05 | R-04 |
| W6: Parity + Migration Tool + System Validation | AC-02, AC-03, AC-06, AC-15, AC-16 | R-01, R-02, R-03, R-08, R-09 |

## Uncovered Areas

| Gap | Risk Level | Mitigation |
|-----|-----------|-----------|
| Performance benchmarking (NFR-04) | Low | Informational measurement, not a gate. At ~53 entries, any backend is fast. |
| Binary size impact (NFR-02) | Low | Measure after build, accept ~1-2MB increase. |
| WAL checkpoint latency (R-07) | Low | Performance benchmark, not a blocking test. |

## Gate Criteria

All of the following must be true before nxs-005 is considered complete:

- [ ] AC-01 through AC-16 verified (all PASS)
- [ ] 234/234 store tests pass on redb (regression check)
- [ ] 234/234 store tests pass on SQLite (parity check)
- [ ] Full workspace tests pass with `--features unimatrix-store/backend-sqlite`
- [ ] 157/157 infra-001 integration tests pass against SQLite-backed binary (AC-16)
- [ ] Data migration tool verified with sample data
- [ ] No code changes outside `crates/unimatrix-store/` except server import adjustments (ADR-001)
- [ ] CI configured to test both backend configurations
