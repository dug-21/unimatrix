# nxs-005: SQLite Storage Engine

## Problem Statement

Unimatrix's storage backend (redb) requires manual secondary index maintenance (5 of 17 tables exist solely as application-managed indexes), forces full table scans for reverse lookups (co-access, session GC), and makes multi-table correlation queries impractical (client-side joins across separate read transactions). These friction points compound as the system grows: the retrospective analytics evolution (col-002 structured events path) needs multi-table JOINs (SESSIONS x INJECTION_LOG x ENTRIES) that are trivial in SQL but require full scans and client-side assembly in KV. Schema migrations require scan-and-rewrite of every entry; SQL uses ALTER TABLE.

The system currently has ~53 active entries across 17 tables (schema v5). While redb performs adequately at this scale, the architectural friction is real and growing at ~2.5 tables per milestone. Replacing redb with SQLite (rusqlite, `bundled` feature) in the unimatrix-store crate eliminates these friction points while preserving every observable behavior.

## Goals

1. Replace redb with SQLite as the storage backend in `crates/unimatrix-store/`
2. Preserve identical behavior across all 10 MCP tools -- zero functional change
3. Preserve all 17 tables as SQLite tables with equivalent schemas
4. Maintain bincode blob serialization for all record types (EntryRecord, AgentRecord, AuditRecord, etc.)
5. Keep HNSW vector index in-memory with VECTOR_MAP bridge table in SQLite (same architecture)
6. Preserve schema v5 migration chain (v0->v5 with entry rewriting for v0->v3, table creation for v3->v5)
7. Validate migration via dual-backend parity testing -- all 234 existing store tests pass against both backends before cutover
8. Provide data migration tooling (redb -> SQLite export/import with row count verification)

## Non-Goals

- **Do not normalize the schema** -- keep entries as bincode blobs. Index table elimination (5 tables -> CREATE INDEX) is nxs-006.
- **Do not replace index tables with SQL CREATE INDEX** -- keep the 5 index tables as SQLite tables. Same query code paths. nxs-006 scope.
- **Do not replace HNSW with sqlite-vec** -- HNSW stays in-memory. sqlite-vec was evaluated and rejected (brute-force only, no ANN, i64 row IDs vs u64, no atomic compact equivalent). See product vision vector architecture note.
- **Do not change bincode serialization to JSON** -- serialization format is orthogonal to storage engine.
- **Do not add injection_log session_id column** -- that is a schema enhancement (nxs-006).
- **Do not change any code outside crates/unimatrix-store/** -- the EntryStore trait boundary (unimatrix-core) holds completely. unimatrix-core StoreAdapter, unimatrix-engine, unimatrix-server, unimatrix-vector require zero changes.
- **Do not change the public Store API surface** -- all 34 methods retain identical signatures.

## Background Research

### Prior Research (ASS-016)

Two research documents inform this feature:

1. **storage-assessment.md** (2026-03-03): Comprehensive evaluation of redb vs alternatives. Concluded SQLite is the strategic long-term target. Identified 7 friction points with redb (manual secondary indexes, co-access reverse lookup full scan, schema scan-and-rewrite migrations, session GC cascade scan, no compression, no secondary indexes, no async API).

2. **retrospective-data-architecture.md** (2026-03-04): Analyzed retrospective analytics evolution. Identified that multi-table correlation queries (entry effectiveness scoring) are the primary driver for SQLite. Provided complete SQLite schema mapping, file-by-file change analysis, risk catalog (7 risks), and phased migration strategy.

### Codebase Analysis

- **Store crate**: 7,452 lines across 14 source files. Files requiring rewrite: db.rs (532 lines, 100%), write.rs (1,939 lines, 80-90%), read.rs (924 lines, 80-90%), counter.rs (56 lines, 100%), query.rs (318 lines, ~50%). Files with minimal change: migration.rs (1,421 lines, ~20%), schema.rs (656 lines, ~10%).
- **Test coverage**: 234 passing tests in unimatrix-store. These become the parity validation suite.
- **Abstraction boundary**: Store struct wraps redb::Database. StoreAdapter (unimatrix-core) wraps Arc<Store> and implements EntryStore trait. All consumers (server, engine, vector) go through this trait boundary. Only the Store implementation changes.
- **Dependencies**: redb (workspace dep) -> rusqlite with `bundled` feature. SQLite WAL mode provides equivalent concurrency model (single writer, concurrent readers).

### Key Technical Findings

1. **Transaction semantics match**: SQLite WAL mode has the same MVCC-like model -- one writer, concurrent readers. No semantic divergence expected.
2. **Bincode blobs transfer directly**: BLOB columns store identical bytes. No serialization changes needed.
3. **CO_ACCESS key ordering**: Current code enforces min < max. SQLite CHECK constraint provides equivalent guarantee.
4. **Multimap tables (TAG_INDEX, FEATURE_ENTRIES)**: redb MultimapTable -> SQLite table with composite PK. Same client-side HashSet intersection logic.
5. **Counter atomicity**: `UPDATE counters SET value = value + 1` is atomic within a transaction, simpler than redb's read-increment-write pattern.
6. **File locking**: SQLite WAL mode creates `-wal` and `-shm` sidecar files. PidGuard (vnc-004) guards the process, not the database file -- no change needed.

## Proposed Approach

### Phase 0: Dual-Backend Test Harness

Create infrastructure to run all 234 store tests against both redb and SQLite backends. This is the primary risk reduction mechanism -- if parity tests pass, the migration is safe.

### Phase 1: SQLite Store Implementation

Implement SQLite storage alongside redb in a `sqlite/` submodule within the store crate:
- `sqlite/mod.rs` -- module root
- `sqlite/db.rs` -- Connection management, table creation, WAL configuration
- `sqlite/read.rs` -- All read operations as SQL queries
- `sqlite/write.rs` -- All write operations as SQL INSERT/UPDATE/DELETE

Keep redb implementation untouched during this phase. Both implementations coexist.

### Phase 2: Parity Testing

Run all 234 tests against both backends. Every test must pass on both. Fix any divergences.

### Phase 3: Data Migration Tooling

Export/import utility: open redb database, read every table, write to SQLite, verify row counts match.

### Phase 4: Cutover

Make SQLite the default backend. Remove redb dependency.

### Phase 5: Cleanup

Remove old redb code. Flatten sqlite module into main store if desired.

## Acceptance Criteria

- AC-01: All 17 tables exist as SQLite tables with equivalent schemas (verified by schema introspection test)
- AC-02: All 234 existing store tests pass against the SQLite backend with zero modification to test logic
- AC-03: All 10 MCP tools return identical results when backed by SQLite (integration test via server)
- AC-04: HNSW vector index operates identically -- VECTOR_MAP bridge table in SQLite, in-memory HNSW graph unchanged
- AC-05: Schema migration chain (v0->v5) executes correctly on SQLite, including entry rewriting for v0->v3
- AC-06: Data migration tooling successfully exports a redb database to SQLite with verified row counts per table
- AC-07: SQLite WAL mode is enabled and concurrent read/write operations work correctly
- AC-08: Bincode serialization roundtrip is verified for every record type (EntryRecord, AgentRecord, AuditRecord, SessionRecord, InjectionLogRecord, SignalRecord, MetricVector, CoAccessRecord)
- AC-09: CO_ACCESS key ordering invariant (entry_id_a < entry_id_b) is enforced by both application code and SQLite CHECK constraint
- AC-10: Counter operations (next_entry_id, next_data_id, next_signal_id, etc.) are atomic within transactions
- AC-11: Signal queue operations (drain, cap-at-limit) produce identical results
- AC-12: Session lifecycle operations (create, update, GC sweep) produce identical results
- AC-13: Injection log operations (batch append, cascade delete by session) produce identical results
- AC-14: Both backends available via Cargo feature flag (`backend-sqlite` enables SQLite, redb remains default). Dual-backend coexistence verified.
- AC-15: No code changes outside `crates/unimatrix-store/` (EntryStore trait boundary holds)

## Constraints

- **Dependency**: rusqlite with `bundled` feature (statically links SQLite). Adds a C compilation step to the build.
- **Concurrency model**: SQLite WAL mode only. Do not use journal_mode=DELETE or other modes.
- **Transaction pattern**: All write operations must be wrapped in explicit transactions to match redb's atomicity guarantees.
- **No new public API**: The Store struct's public method signatures must not change. Internal implementation is free to restructure.
- **Test infrastructure is cumulative**: Extend existing test_helpers.rs with parity test support. Do not create isolated scaffolding.
- **File paths**: Database file changes from `.redb` to `.db` extension. The `-wal` and `-shm` sidecar files are expected SQLite behavior.

## Open Questions (Resolved)

1. **Feature flag approach confirmed.** Use a Cargo feature flag to keep both backends. redb stays as the default during migration (safe backout). Cleanup/removal of redb is a separate feature (nxs-006).
2. **Migration tool is one-time.** The redb->SQLite export tool ships with nxs-005 but is temporary. Removed in nxs-006 along with the redb backend.
3. **WAL checkpoint: SQLite auto-checkpoint.** No checkpoint-on-compact(). Rely on SQLite's built-in auto-checkpoint. Architect may override if there is a strong technical reason.

## Tracking

GitHub Issue will be created during Session 1 synthesis phase.
