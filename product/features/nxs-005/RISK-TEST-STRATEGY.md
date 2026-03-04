# Risk-Based Test Strategy: nxs-005

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | SQLite semantic divergence in edge cases (NULL handling, empty blobs, integer overflow at u64 boundary) | High | Medium | High |
| R-02 | Mutex serialization causes deadlock under concurrent spawn_blocking tasks | High | Low | Medium |
| R-03 | Transaction type abstraction (ADR-001) leaks backend-specific behavior to server code | Medium | Medium | Medium |
| R-04 | Schema migration chain (v0-v5) produces different results on SQLite vs redb due to bincode deserialization order sensitivity | High | Low | Medium |
| R-05 | CO_ACCESS CHECK constraint violation on edge case (entry_id_a == entry_id_b) | Medium | Low | Low |
| R-06 | Signal queue 10K cap eviction order differs between redb sorted iteration and SQLite rowid order | Medium | Medium | Medium |
| R-07 | WAL auto-checkpoint during long write transaction causes unexpected latency spike | Low | Medium | Low |
| R-08 | Feature flag cfg gates miss a code path, causing compilation failure under one backend | Medium | High | High |
| R-09 | Data migration tool produces corrupt SQLite database from partially-written redb source | Medium | Low | Low |
| R-10 | Counter atomicity gap: read-increment-write in SQLite without explicit transaction wrapping | High | Medium | High |

## Risk-to-Scenario Mapping

### R-01: SQLite Semantic Divergence
**Severity**: High
**Likelihood**: Medium
**Impact**: Silent data corruption or incorrect query results across all MCP tools.

**Test Scenarios**:
1. Insert entry with u64::MAX as id, verify get() returns correct record
2. Insert entry with empty string fields, verify roundtrip preserves empty (not NULL)
3. Insert entry with 0-byte blob (empty bincode), verify storage and retrieval
4. Query with all filter fields set to boundary values (empty string topic, status=0, time_range start=0 end=u64::MAX)
5. CO_ACCESS with (0, 1) and (u64::MAX-1, u64::MAX) pairs
6. **infra-001 system-level**: Full harness run (157 tests) against SQLite-backed binary validates all 10 MCP tools return correct results through the entire server stack. The `tools` suite (53 tests) exercises every parameter and response format. The `edge_cases` suite (24 tests) covers unicode, boundary values, and empty DB operations.

**Coverage Requirement**: All 234 existing tests pass plus boundary value tests for u64, empty string, and empty blob. Full infra-001 harness (157 tests) passes against SQLite-backed binary.

### R-02: Mutex Deadlock
**Severity**: High
**Likelihood**: Low
**Impact**: Server hangs permanently, requiring process kill.

**Test Scenarios**:
1. Concurrent insert + query from separate threads (10 threads, 100 operations each)
2. Nested Store method call from within a write operation (if any exist)
3. Store method that panics while holding mutex -- verify poison recovery or clean error
4. **infra-001 system-level**: The `lifecycle` suite (16 tests) runs multi-step flows including store-search-correct chains and restart persistence -- all through the MCP stdio transport where the server handles concurrent requests. The `volume` suite (11 tests) scales to hundreds of entries, exercising sustained lock acquisition under load. Both suites exercise real concurrency through the async server stack.

**Coverage Requirement**: Multi-threaded stress test. Verify no deadlock after 1000 concurrent operations. infra-001 lifecycle + volume suites pass against SQLite-backed binary.

### R-03: Transaction Type Abstraction Leakage
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Server code compiles with redb but fails with SQLite, or vice versa.

**Test Scenarios**:
1. Compile-check the full workspace with `backend-sqlite` feature
2. Server's agent registry read/write operations work identically under both backends
3. Server's audit log append operations work identically under both backends

**Coverage Requirement**: Full workspace compilation under both features. Server integration tests under both.

### R-04: Migration Chain Divergence
**Severity**: High
**Likelihood**: Low
**Impact**: Existing databases fail to open after backend switch, or entries are corrupted during migration.

**Test Scenarios**:
1. Create a database at schema v0 (minimal EntryRecord), migrate to v5, verify all entries have correct field values
2. Create a database at schema v3 (pre-f64 confidence), migrate to v5, verify confidence field is f64
3. Run migration on empty database (no entries), verify schema_version counter is set correctly
4. Run migration on database with entries at each schema version boundary

**Coverage Requirement**: Migration chain test for each version transition, verifying entry field values after migration.

### R-05: CO_ACCESS Key Ordering Edge Case
**Severity**: Medium
**Likelihood**: Low
**Impact**: Duplicate pairs or CHECK constraint errors on self-referencing entries.

**Test Scenarios**:
1. Attempt co_access_key(5, 5) -- verify (5, 5) returned
2. Attempt INSERT with entry_id_a == entry_id_b -- verify CHECK constraint behavior (should it allow equal?)
3. Reverse lookup for entry that appears only as entry_id_b

**Coverage Requirement**: Explicit test for equal IDs and CHECK constraint boundary.

### R-06: Signal Queue Eviction Order
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Wrong signal dropped during cap enforcement, leading to incorrect confidence updates.

**Test Scenarios**:
1. Insert 10,001 signals, verify the first (signal_id=0) is evicted
2. Insert signals with non-sequential IDs (if possible via direct table manipulation), verify oldest by signal_id is evicted
3. Drain after eviction, verify remaining signals are correct

**Coverage Requirement**: Existing cap test (test_signal_queue_cap_at_10001_drops_oldest) must pass identically.

### R-07: WAL Checkpoint Latency
**Severity**: Low
**Likelihood**: Medium
**Impact**: Occasional >100ms write latency during auto-checkpoint.

**Test Scenarios**:
1. Batch insert 1000 entries, measure p99 write latency
2. Verify auto-checkpoint does not cause SQLITE_BUSY for concurrent readers

**Coverage Requirement**: Performance benchmark (informational, not blocking).

### R-08: Feature Flag Compilation Gaps
**Severity**: Medium
**Likelihood**: High
**Impact**: Build failure under one backend configuration.

**Test Scenarios**:
1. `cargo check -p unimatrix-store` (redb) succeeds
2. `cargo check -p unimatrix-store --features backend-sqlite` (SQLite) succeeds
3. `cargo check --workspace` (redb) succeeds
4. `cargo check --workspace --features unimatrix-store/backend-sqlite` (SQLite) succeeds
5. `cargo test -p unimatrix-store` and `cargo test -p unimatrix-store --features backend-sqlite` both pass

**Coverage Requirement**: CI must run both configurations. Missing cfg gate = build failure = caught immediately.

### R-09: Data Migration from Corrupt Source
**Severity**: Medium
**Likelihood**: Low
**Impact**: Migration tool crashes or produces incomplete SQLite database.

**Test Scenarios**:
1. Migrate a healthy redb database, verify all row counts match
2. Migrate a database with one corrupt entry blob, verify tool skips and reports
3. Migrate an empty database, verify empty SQLite is valid

**Coverage Requirement**: Migration test with healthy and degenerate inputs.

### R-10: Counter Atomicity
**Severity**: High
**Likelihood**: Medium
**Impact**: Duplicate entry IDs, signal IDs, or audit event IDs leading to data overwrite.

**Test Scenarios**:
1. Concurrent next_entry_id calls from 10 threads, verify all IDs are unique
2. insert() followed by read_counter("next_entry_id"), verify counter reflects insert
3. Transaction rollback: begin write, increment counter, rollback -- verify counter unchanged

**Coverage Requirement**: Concurrent counter test proving uniqueness. Transaction rollback test.

## Integration Risks

| Risk | Components | Impact | Test Approach |
|------|-----------|--------|--------------|
| StoreAdapter wraps wrong Store variant | unimatrix-core, unimatrix-store | Compilation fails or wrong backend used | Compile-check workspace under both features |
| Server imports redb transaction types directly | unimatrix-server, unimatrix-store | Compilation fails under backend-sqlite | ADR-001 type aliases resolve this; verify with compile-check |
| VectorIndex expects specific iter_vector_mappings ordering | unimatrix-vector, unimatrix-store | HNSW graph built with wrong ID mappings | Verify iter_vector_mappings returns same pairs in same order |
| Full MCP stack with SQLite backend | unimatrix-server + all crates | Behavioral divergence undetected by unit tests | infra-001 full harness (157 tests) against SQLite-backed binary |

## Edge Cases

| Edge Case | Expected Behavior | Risk ID |
|-----------|------------------|---------|
| Database file does not exist | Created automatically with all 17 tables | R-08 |
| Database file exists but is empty (0 bytes) | Treated as new database, tables created | R-01 |
| Entry with u64::MAX as ID | Stored and retrievable | R-01 |
| Empty string as topic/category/tag | Stored in index tables, queryable | R-01 |
| 0-length bincode blob | Deserialization error, handled gracefully | R-01 |
| Concurrent open of same database file | Second open gets SQLITE_BUSY after timeout | R-02 |
| Counter overflow (u64::MAX + 1) | Rust wrapping behavior -- should never happen at realistic scale | R-10 |

## Security Risks

This feature does not introduce new external input surfaces. All data entering SQLite comes through the existing Store API, which has upstream validation (content scanning, input validation in unimatrix-server). The security posture is unchanged because:

- SQLite parameterized queries prevent SQL injection (no string interpolation in queries)
- Bincode blob storage means no user-controlled SQL is ever constructed
- File permissions on the database are OS-level, same as redb
- No new network surfaces -- SQLite is embedded, same as redb

**Residual risk**: If a malicious bincode blob is stored (bypassing upstream validation), deserialization could panic. This is identical to the redb risk and is mitigated by the existing content scanning pipeline.

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| SQLITE_BUSY (write contention) | rusqlite::Error with SQLITE_BUSY code | busy_timeout PRAGMA retries for 5s; if still busy, return StoreError::Sqlite |
| Corrupt database file | SQLite integrity_check fails | Restore from backup or re-export from redb |
| WAL file grows unbounded | WAL file size > 100MB | Manual PRAGMA wal_checkpoint(TRUNCATE) -- should not happen with auto-checkpoint |
| Mutex poisoned (thread panic while holding lock) | PoisonError on lock() | Return StoreError; caller can decide recovery. Consider unwrap_or_else pattern from vnc-004 CategoryAllowlist. |
| Migration interrupted mid-table | Incomplete SQLite database | Re-run migration (idempotent -- drops and recreates destination tables) |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (C compilation dependency) | R-08 | Feature flag makes C compilation opt-in. Default build unaffected. |
| SR-02 (WAL sidecar files) | -- | Verified: PidGuard is process-level, not file-level. No architecture impact. |
| SR-03 (bincode fragility) | R-04 | Migration chain tested end-to-end. Bincode serialization unchanged. |
| SR-04 (dual-backend test matrix) | R-08 | CI runs both: `cargo test` (redb) + `cargo test --features backend-sqlite` (SQLite). |
| SR-05 (subtle semantic differences) | R-01 | Primary risk. 234 parity tests + boundary value tests + concurrent access tests + infra-001 full harness (157 system-level tests). |
| SR-06 (intentional duplication of index tables) | -- | Accepted. Zero-change scope. Documented for nxs-006. |
| SR-07 (StoreAdapter coupling) | R-03 | ADR-001 type aliases minimize coupling. Compile-check under both features. |
| SR-08 (transaction lifetime semantics) | R-02, R-10 | Mutex<Connection> serializes access. Counter atomicity via SQL transactions. |
| SR-09 (migration of corrupt data) | R-09 | Migration tool skips corrupt entries with logging. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-08, R-10) | 16 scenarios |
| Medium | 4 (R-02, R-03, R-04, R-06) | 14 scenarios |
| Low | 3 (R-05, R-07, R-09) | 7 scenarios |
| **Total** | **10** | **37 scenarios** |

### System-Level Validation (infra-001)

The infra-001 integration harness provides cross-cutting system-level validation that complements unit-level parity testing. Building the `unimatrix-server` binary with `--features unimatrix-store/backend-sqlite` and running the full harness (157 tests, 8 suites) validates:

| Suite | Tests | Risks Covered |
|-------|-------|--------------|
| `protocol` | 13 | R-03, R-08 -- MCP handshake, JSON-RPC compliance, tool discovery work through full stack |
| `tools` | 53 | R-01 -- every tool parameter, valid/invalid inputs, all response formats |
| `lifecycle` | 16 | R-01, R-02 -- multi-step flows, correction chains, restart persistence |
| `volume` | 11 | R-02, R-07 -- scale to hundreds of entries, sustained load |
| `security` | 15 | R-01 -- content scanning, capability enforcement at system level |
| `confidence` | 13 | R-01 -- 6-factor composite formula validated end-to-end |
| `contradiction` | 12 | R-01 -- detection pipeline through full stack |
| `edge_cases` | 24 | R-01, R-05 -- unicode, boundary values, empty DB, concurrent ops |

The harness is a gate requirement (AC-16). All 157 tests must pass against the SQLite-backed binary.
