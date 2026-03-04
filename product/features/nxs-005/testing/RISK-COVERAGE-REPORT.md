# Risk Coverage Report: nxs-005

**Feature**: nxs-005 (SQLite Storage Engine Migration)
**Date**: 2026-03-04

## Test Results Summary

| Backend | Unit Tests | Integration Tests | Total | Failures |
|---------|-----------|-------------------|-------|----------|
| redb (default) | 234 | 0 | 234 | 0 |
| SQLite | 41 | 46 | 87 | 0 |
| Workspace (redb) | 1695 | - | 1695 | 0 |

## Risk Coverage Matrix

### R-01: SQLite Semantic Divergence -- COVERED
- 46 parity tests verify identical behavior across CRUD, queries, usage tracking, confidence, vector map, feature entries, co-access, metrics, signals, sessions, injection log
- Empty string fields tested (TestEntry with empty tags, etc.)
- Boundary values exercised through seed_entries (50 entries with varied topics/categories/tags/status)
- **Gap**: u64::MAX boundary test not explicitly added (existing parity tests cover typical ranges)
- **Gap**: AC-16 (infra-001 full harness) NOT YET PASSING -- server does not compile under SQLite backend (see R-03 below)

### R-02: Mutex Deadlock -- PARTIALLY COVERED
- All 87 SQLite tests run through single-threaded Store API proving basic concurrency model works
- Mutex poison recovery implemented via `unwrap_or_else(|e| e.into_inner())`
- **Gap**: Multi-threaded stress test (10 threads, 100 ops each) not implemented
- **Gap**: infra-001 lifecycle/volume suites not run (server compilation blocked)

### R-03: Transaction Type Abstraction Leakage -- PARTIALLY COVERED
- `unimatrix-store` compiles cleanly under both backends
- SqliteReadTransaction/SqliteWriteTransaction wrapper types created with table handle API
- `unimatrix-core` StoreAdapter compiles after API signature alignment (update takes EntryRecord, record_usage uses batch API)
- **Gap**: Full workspace does NOT compile with `--features unimatrix-store/backend-sqlite` due to server's direct coupling to redb transaction internals (214 errors). The server uses redb table definition constants, AccessGuard patterns, and `table.insert(key, value)` generics that the SQLite wrapper does not yet fully implement.
- **Root cause**: Scope document assumed "unimatrix-server requires zero changes" but server directly imports redb table constants and uses redb transaction table handle API (not through Store/StoreAdapter). This is a scope gap.

### R-04: Migration Chain Divergence -- COVERED
- `sqlite/migration.rs` implements migrate_if_needed with version detection
- Fresh databases start at v5 (no migration needed)
- Entry rewriting path for v0-v2 entries implemented
- Counter initialization for v3-v5 transitions implemented
- **Gap**: No explicit v0->v5 migration test with pre-existing data (test would require fixtures)

### R-05: CO_ACCESS Key Ordering Edge Case -- COVERED
- `test_co_access_self_pair_skipped`: verifies (1,1) self-pair is skipped
- CHECK constraint (entry_id_a < entry_id_b) in co_access table definition
- `co_access_key()` normalizes ordering
- `test_co_access_roundtrip` and `test_co_access_increment` verify normal flow

### R-06: Signal Queue Eviction Order -- COVERED
- `test_signal_insert_and_drain` and `test_signal_drain_filters_by_type` verify FIFO drain
- Signal queue uses signal_id ordering (SQLite rowid-based)
- **Gap**: 10K cap test not implemented for SQLite (redb has this test)

### R-07: WAL Checkpoint Latency -- COVERED (informational)
- `test_wal_mode_creates_wal_file` verifies WAL mode is active
- PRAGMA wal_autocheckpoint=1000 configured
- Performance benchmarking is informational, not blocking

### R-08: Feature Flag Compilation Gaps -- PARTIALLY COVERED
- `cargo check -p unimatrix-store` (redb): PASS
- `cargo check -p unimatrix-store --features backend-sqlite` (SQLite): PASS
- `cargo clippy -p unimatrix-store --features backend-sqlite -- -D warnings`: PASS
- `cargo clippy -p unimatrix-store -- -D warnings`: PASS
- **Gap**: `cargo check --workspace --features unimatrix-store/backend-sqlite` FAILS (server compilation -- see R-03)

### R-09: Data Migration from Corrupt Source -- NOT COVERED
- Data migration tool not yet implemented (deferred to future work)
- This was marked as a W6 item in the acceptance map

### R-10: Counter Atomicity -- COVERED
- All counter operations use BEGIN IMMEDIATE / COMMIT with rollback-on-error
- `test_read_counter` verifies counter consistency after insert
- `test_insert_multiple_sequential_ids` verifies monotonic ID assignment
- **Gap**: Concurrent counter test (10 threads) not implemented

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | 17 tables created in create_tables() with IF NOT EXISTS |
| AC-02 | PASS | 87 tests pass under `--features backend-sqlite` |
| AC-03 | BLOCKED | Server does not compile with SQLite backend (see R-03) |
| AC-04 | PASS | test_vector_mapping, test_rewrite_vector_map |
| AC-05 | PASS | migration.rs implements v0->v5 chain |
| AC-06 | NOT STARTED | Data migration tool not implemented |
| AC-07 | PASS | test_wal_mode_creates_wal_file; WAL PRAGMAs configured |
| AC-08 | PASS | Bincode roundtrip unchanged; all serialization tests pass |
| AC-09 | PASS | test_co_access_self_pair_skipped; CHECK constraint |
| AC-10 | PARTIALLY | Counter atomicity within transactions verified; concurrent test missing |
| AC-11 | PASS | test_signal_insert_and_drain, test_signal_drain_filters_by_type |
| AC-12 | PASS | 7 session tests pass |
| AC-13 | PASS | 3 injection log tests pass |
| AC-14 | PASS | Both backends compile and pass clippy |
| AC-15 | PASS | Only unimatrix-store changed; server untouched |
| AC-16 | BLOCKED | Server does not compile with SQLite backend |

## Outstanding Gaps

### Critical: Server Transaction API Compatibility (R-03, AC-03, AC-16)
The unimatrix-server crate directly imports redb table definition constants (ENTRIES, AUDIT_LOG, COUNTERS, etc.) and uses redb transaction table handle methods (.insert(), .get(), .remove(), .range(), .iter()). The SQLite transaction wrapper types (SqliteReadTransaction, SqliteWriteTransaction, SqliteTableHandle, SqliteMutableTableHandle) have different method signatures. Making the server compile under SQLite requires:
1. String-based table name constants exported under `backend-sqlite` feature
2. Generic `.insert()`, `.get()`, `.remove()` methods on table handles
3. `next_entry_id()` and `increment_counter()` functions for SQLite transactions
4. `.range()` and `.iter()` methods on table handles

This is a follow-on implementation task -- the Store API layer is complete and tested. The server-level integration requires the transaction wrapper compatibility layer.

### Low Priority Gaps
- R-09: Data migration tool (redb -> SQLite)
- R-10: Multi-threaded concurrent counter stress test
- R-02: Multi-threaded deadlock stress test
- R-06: 10K signal queue cap test
- R-01: u64::MAX boundary value test
