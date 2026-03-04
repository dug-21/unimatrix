# Implementation Brief: nxs-005 SQLite Storage Engine

## Summary

Replace the redb storage backend in `crates/unimatrix-store/` with SQLite (rusqlite, `bundled` feature). Zero functional change. Both backends coexist via Cargo feature flag `backend-sqlite`. redb remains default.

## Resolved Decisions

| Decision | ADR | Unimatrix ID | Summary |
|----------|-----|-------------|---------|
| Transaction type abstraction | ADR-001 | #327 | Type aliases behind cfg select redb or SQLite transaction types. Server imports change from `redb::*` to `unimatrix_store::*`. |
| Mutex<Connection> for Sync | ADR-002 | #328 | Wrap rusqlite::Connection in std::sync::Mutex. Serializes all access. Acceptable at current scale. |
| WAL mode + auto-checkpoint | ADR-003 | #329 | journal_mode=WAL, synchronous=NORMAL, auto-checkpoint at 1000 pages. compact() becomes no-op. |
| Feature flag strategy | ADR-004 | #330 | `backend-sqlite` Cargo feature. Compile-time mutual exclusion. Shared types in schema.rs. |

## Implementation Waves

### Wave 1: Foundation (sqlite/db.rs, error.rs, lib.rs, Cargo.toml)

**Goal**: SQLite Store struct compiles and creates all 17 tables.

1. Add `rusqlite = { version = "0.34", features = ["bundled"], optional = true }` to Cargo.toml
2. Add `backend-sqlite = ["dep:rusqlite"]` feature
3. Create `src/sqlite/mod.rs` with Store struct wrapping `Mutex<Connection>`
4. Create `src/sqlite/db.rs` with `open()`, `open_with_config()`, table creation, WAL PRAGMAs
5. Extend `error.rs` with cfg-gated `Sqlite(rusqlite::Error)` variant
6. Update `lib.rs` with cfg-gated module selection and re-exports
7. Implement `compact()` as no-op
8. Export transaction type aliases (ADR-001)

**Verification**: `cargo check -p unimatrix-store --features backend-sqlite` succeeds. `test_open_creates_all_tables` equivalent passes.

### Wave 2: Write Operations (sqlite/write.rs)

**Goal**: All write methods work against SQLite.

1. Implement `insert()` -- ENTRIES + 5 index tables + COUNTERS in single transaction
2. Implement `update()` -- ENTRIES + index diff (delete old, insert new)
3. Implement `update_status()` -- ENTRIES + STATUS_INDEX update
4. Implement `delete()` -- ENTRIES + all 5 index table cleanup
5. Implement `record_usage()`, `record_usage_with_confidence()`
6. Implement `update_confidence()`
7. Implement `put_vector_mapping()`, `rewrite_vector_map()`
8. Implement `record_feature_entries()`
9. Implement `record_co_access_pairs()`, `cleanup_stale_co_access()`
10. Implement `store_metrics()`

**Verification**: write.rs tests pass under `backend-sqlite`.

### Wave 3: Read Operations (sqlite/read.rs)

**Goal**: All read methods work against SQLite.

1. Implement `get()`, `exists()`
2. Implement `query_by_topic()`, `query_by_category()`, `query_by_tags()`, `query_by_time_range()`, `query_by_status()`
3. Implement `query()` (multi-filter intersection via QueryFilter)
4. Implement `get_vector_mapping()`, `iter_vector_mappings()`
5. Implement `read_counter()`
6. Implement `get_co_access_partners()`, `co_access_stats()`, `top_co_access_pairs()`
7. Implement `get_metrics()`, `list_all_metrics()`

**Verification**: read.rs tests pass under `backend-sqlite`.

### Wave 4: Specialized Operations (signal, sessions, injection_log)

**Goal**: Signal queue, session lifecycle, and injection log work against SQLite.

1. Implement `insert_signal()`, `drain_signals()`, `signal_queue_len()` in sqlite module
2. Implement `insert_session()`, `update_session()`, `get_session()`, `scan_sessions_by_feature()`, `scan_sessions_by_feature_with_status()`, `gc_sessions()`
3. Implement `insert_injection_log_batch()`, `scan_injection_log_by_session()`

**Verification**: db.rs signal tests, sessions.rs tests, injection_log.rs tests all pass under `backend-sqlite`.

### Wave 5: Migration and Schema (migration.rs adaptation)

**Goal**: Schema migration chain works on SQLite.

1. Adapt `migrate_if_needed()` for SQLite -- version detection via counters table
2. Implement entry rewriting migrations (v0-v3) using SQLite reads/writes
3. Implement table creation migrations (v3-v5) using CREATE TABLE IF NOT EXISTS
4. Test full migration chain from v0 to v5

**Verification**: Migration tests pass under `backend-sqlite`.

### Wave 6: Parity Testing, Migration Tooling, and System Validation

**Goal**: All 234 unit tests pass on both backends. Data migration tool works. Full infra-001 harness passes against SQLite-backed binary.

1. Update `test_helpers.rs` to create Store with active backend
2. Run full `cargo test -p unimatrix-store --features backend-sqlite`
3. Implement redb-to-SQLite migration function
4. Test migration with sample data
5. Run `cargo test --workspace --features unimatrix-store/backend-sqlite`
6. Build SQLite-backed binary: `cargo build --release --features unimatrix-store/backend-sqlite`
7. Run full infra-001 harness against SQLite-backed binary: `cd product/test/infra-001 && python -m pytest suites/ -v --timeout=60`
8. All 157 integration tests (8 suites) must pass. This is a **gate requirement** (AC-16).

The infra-001 harness validates system-level behavior that `cargo test` cannot: MCP protocol compliance, multi-step lifecycle flows (store-search-correct chains), restart persistence, scale to hundreds of entries, security defenses, confidence math end-to-end, contradiction detection, and edge cases (unicode, boundary values, concurrent ops). Per the USAGE-PROTOCOL: "Schema or storage changes -> run lifecycle, volume suites." Since nxs-005 is a complete backend replacement, the full suite is required. See `product/test/infra-001/USAGE-PROTOCOL.md` for suite details and running instructions.

**Verification**: 234/234 unit tests pass on SQLite. Migration tool verified with row counts. 157/157 infra-001 integration tests pass against SQLite-backed binary.

## Key Files

| File | Action | Estimated Lines |
|------|--------|----------------|
| `Cargo.toml` | Edit -- add rusqlite, feature flag | ~5 |
| `src/lib.rs` | Edit -- cfg-gated module selection | ~20 |
| `src/error.rs` | Edit -- cfg-gated Sqlite error variant | ~30 |
| `src/sqlite/mod.rs` | **New** -- Store struct, re-exports | ~50 |
| `src/sqlite/db.rs` | **New** -- Connection, tables, PRAGMAs, transactions | ~200 |
| `src/sqlite/read.rs` | **New** -- All read operations | ~600 |
| `src/sqlite/write.rs` | **New** -- All write operations | ~1200 |
| `src/sqlite/signal.rs` | **New** -- Signal queue operations | ~150 |
| `src/sqlite/sessions.rs` | **New** -- Session lifecycle operations | ~400 |
| `src/sqlite/injection_log.rs` | **New** -- Injection log operations | ~150 |
| `src/sqlite/migration.rs` | **New** -- SQLite-specific migration | ~200 |
| `src/test_helpers.rs` | Edit -- backend-aware store creation | ~20 |
| `src/migrate_redb_to_sqlite.rs` | **New** -- One-time migration tool | ~200 |

**Estimated total new code**: ~3,200 lines (SQLite implementation) + ~50 lines (edits to shared modules).

## Constraints for Implementers

1. **Do not modify existing redb code**. All redb files (db.rs, read.rs, write.rs, etc.) remain untouched.
2. **All SQL must use parameterized queries** (`?` placeholders). No string interpolation.
3. **Every write operation must be in an explicit transaction** (BEGIN/COMMIT). Do not rely on SQLite autocommit.
4. **Use `conn.lock().unwrap()` consistently**. The Mutex<Connection> lock must be held for the entire transaction. Do not release and reacquire mid-transaction.
5. **Match redb iteration order**. Where redb returns results in key order (sorted), SQLite queries must ORDER BY the same key columns.
6. **Test with both backends in CI**. Every PR must pass `cargo test -p unimatrix-store` AND `cargo test -p unimatrix-store --features backend-sqlite`.

## Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| R-01 (semantic divergence) | Boundary value tests for u64::MAX, empty strings, empty blobs |
| R-02 (mutex deadlock) | Never hold lock across await points. All store ops are synchronous. |
| R-08 (cfg gaps) | CI builds both configurations. Compilation failure = immediate detection. |
| R-10 (counter atomicity) | All counter reads and increments within same SQL transaction. |
