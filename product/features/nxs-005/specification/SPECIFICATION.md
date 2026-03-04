# Specification: nxs-005 SQLite Storage Engine

## Objective

Replace the redb storage backend in `crates/unimatrix-store/` with SQLite (via rusqlite with `bundled` feature) while preserving identical behavior across all 10 MCP tools, all 17 tables, and all 34 Store methods. Both backends coexist via a Cargo feature flag; redb remains the default.

## Functional Requirements

### FR-01: SQLite Backend Implementation

Implement a complete SQLite storage backend that provides all 34 Store methods with identical input/output behavior to the redb backend. The backend is activated by the `backend-sqlite` Cargo feature.

### FR-02: Table Schema Parity

Create all 17 tables as SQLite tables with equivalent schemas:
- `entries` (id INTEGER PRIMARY KEY, data BLOB)
- `topic_index` (topic TEXT, entry_id INTEGER, PRIMARY KEY (topic, entry_id))
- `category_index` (category TEXT, entry_id INTEGER, PRIMARY KEY (category, entry_id))
- `tag_index` (tag TEXT, entry_id INTEGER, PRIMARY KEY (tag, entry_id))
- `time_index` (timestamp INTEGER, entry_id INTEGER, PRIMARY KEY (timestamp, entry_id))
- `status_index` (status INTEGER, entry_id INTEGER, PRIMARY KEY (status, entry_id))
- `vector_map` (entry_id INTEGER PRIMARY KEY, hnsw_data_id INTEGER)
- `counters` (name TEXT PRIMARY KEY, value INTEGER)
- `agent_registry` (agent_id TEXT PRIMARY KEY, data BLOB)
- `audit_log` (event_id INTEGER PRIMARY KEY, data BLOB)
- `feature_entries` (feature_id TEXT, entry_id INTEGER, PRIMARY KEY (feature_id, entry_id))
- `co_access` (entry_id_a INTEGER, entry_id_b INTEGER, data BLOB, PRIMARY KEY (entry_id_a, entry_id_b), CHECK (entry_id_a < entry_id_b))
- `outcome_index` (feature_cycle TEXT, entry_id INTEGER, PRIMARY KEY (feature_cycle, entry_id))
- `observation_metrics` (feature_cycle TEXT PRIMARY KEY, data BLOB)
- `signal_queue` (signal_id INTEGER PRIMARY KEY, data BLOB)
- `sessions` (session_id TEXT PRIMARY KEY, data BLOB)
- `injection_log` (log_id INTEGER PRIMARY KEY, data BLOB)

Additional index: `CREATE INDEX idx_co_access_b ON co_access(entry_id_b)`.

### FR-03: WAL Mode Configuration

On database open, set SQLite to WAL mode with: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`, `wal_autocheckpoint=1000`, `cache_size=-16384`, `foreign_keys=OFF`.

### FR-04: Schema Migration Support

The SQLite backend must support the existing schema version chain (v0-v5). Version tracking via the `counters` table (`schema_version` key). Entry rewriting migrations (v0-v3) deserialize bincode, apply field defaults, re-serialize. Table creation migrations (v3-v5) execute CREATE TABLE IF NOT EXISTS.

### FR-05: Feature Flag Coexistence

The `backend-sqlite` Cargo feature selects the SQLite backend. Without the feature, redb is compiled. Both backends share common types (EntryRecord, NewEntry, QueryFilter, Status, etc.) from `schema.rs`. Mutual exclusion at compile time -- only one Store implementation is compiled.

### FR-06: Transaction Type Abstraction

Export `ReadTransaction` and `WriteTransaction` type aliases from the store crate that resolve to the active backend's transaction types. Under redb: `redb::ReadTransaction` / `redb::WriteTransaction`. Under SQLite: thin wrappers providing equivalent table access.

### FR-07: Error Type Extension

Extend `StoreError` with a `Sqlite(rusqlite::Error)` variant under the `backend-sqlite` feature. Existing redb error variants are gated behind `#[cfg(not(feature = "backend-sqlite"))]`.

### FR-08: Data Migration Tooling

Provide a function or binary that reads all 17 tables from a redb database and writes them to a new SQLite database. Per-table row count verification. Corrupt entries logged and skipped (not fatal).

### FR-09: Compact as No-Op

`Store::compact()` under the SQLite backend returns `Ok(())` without performing any operation. SQLite WAL auto-checkpoint handles space management.

### FR-10: Signal Queue Parity

Signal queue operations (insert_signal, drain_signals, signal_queue_len) produce identical behavior:
- Monotonic signal_id allocation via counters table
- 10,000-record cap with oldest-first eviction
- Type-filtered drain with deletion in a single transaction
- Corrupted record cleanup during drain

### FR-11: Session Operations Parity

Session operations (insert_session, update_session, get_session, scan_sessions_by_feature, scan_sessions_by_feature_with_status, gc_sessions) produce identical results including cascade deletion of injection log records.

### FR-12: Injection Log Parity

Injection log operations (insert_injection_log_batch, scan_injection_log_by_session) produce identical results including batch insert atomicity.

## Non-Functional Requirements

### NFR-01: Build Time

Adding rusqlite with `bundled` feature increases build time due to SQLite C compilation. First build may add 15-30 seconds. Incremental builds unaffected.

### NFR-02: Binary Size

SQLite bundled adds ~1-2 MB to the binary. Acceptable for an embedded database engine.

### NFR-03: Thread Safety

Store must remain Send + Sync when using the SQLite backend (achieved via Mutex<Connection> per ADR-002).

### NFR-04: Performance

At current scale (~53 entries, 17 tables), all operations must complete in <10ms. No measurable regression from redb for point lookups, batch writes within transactions, or index scans.

### NFR-05: Disk Space

SQLite database file should be comparable to or smaller than the redb database for equivalent data. WAL file adds temporary overhead (~4MB max before auto-checkpoint).

### NFR-06: Crash Safety

SQLite WAL with synchronous=NORMAL guarantees: committed transactions survive process crash. Last uncommitted transaction may be lost on OS-level crash (power failure). This matches redb's practical behavior.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | All 17 tables exist as SQLite tables with equivalent schemas | Schema introspection test: query sqlite_master, verify table names and column types |
| AC-02 | All 234 existing store tests pass against SQLite backend | `cargo test -p unimatrix-store --features backend-sqlite` |
| AC-03 | All 10 MCP tools return identical results with SQLite backend | Full workspace test suite with `backend-sqlite` feature propagated |
| AC-04 | HNSW vector index operates identically | Vector mapping roundtrip test: put_vector_mapping, get_vector_mapping, iter_vector_mappings |
| AC-05 | Schema migration chain v0-v5 executes correctly | Migration test: create database at each schema version, verify upgrade to v5 |
| AC-06 | Data migration exports redb to SQLite with verified row counts | Migration test: populate redb with sample data, export, verify per-table counts |
| AC-07 | SQLite WAL mode enabled and concurrent access works | Test: write in one thread while reading in another, no SQLITE_BUSY errors |
| AC-08 | Bincode roundtrip verified for all record types | Existing serialization tests (schema.rs, signal.rs, sessions.rs) pass unchanged |
| AC-09 | CO_ACCESS ordering enforced by CHECK constraint | Test: attempt INSERT with entry_id_a > entry_id_b, verify constraint violation |
| AC-10 | Counter operations atomic within transactions | Test: concurrent counter increments produce sequential values |
| AC-11 | Signal queue operations produce identical results | Existing signal queue tests (db.rs) pass against SQLite |
| AC-12 | Session operations produce identical results | Existing session tests (sessions.rs) pass against SQLite |
| AC-13 | Injection log operations produce identical results | Existing injection log tests (injection_log.rs) pass against SQLite |
| AC-14 | Both backends available via feature flag | Test: `cargo check -p unimatrix-store` (redb) and `cargo check -p unimatrix-store --features backend-sqlite` (SQLite) both succeed |
| AC-15 | No code changes outside `crates/unimatrix-store/` except import path for transaction types | Git diff verification: only store crate files modified, plus minimal server import adjustments for ADR-001 |

## Domain Models

### Store (modified)

The Store struct's internal representation changes based on feature flag:

```
Store {
  #[cfg(not(backend-sqlite))] db: redb::Database
  #[cfg(backend-sqlite)]      conn: Mutex<rusqlite::Connection>
}
```

All public methods remain identical. Store is Send + Sync in both configurations.

### StoreError (extended)

```
StoreError {
  EntryNotFound(u64)          -- unchanged
  #[cfg(not(backend-sqlite))]
  Database(redb::DatabaseError)
  Transaction(redb::TransactionError)
  Table(redb::TableError)
  Storage(redb::StorageError)
  Commit(redb::CommitError)
  Compaction(redb::CompactionError)
  #[cfg(backend-sqlite)]
  Sqlite(rusqlite::Error)     -- new: covers all SQLite errors
  Serialization(String)       -- unchanged
  Deserialization(String)     -- unchanged
  InvalidStatus(u8)           -- unchanged
}
```

### Shared Types (unchanged)

EntryRecord, NewEntry, QueryFilter, TimeRange, DatabaseConfig, Status, CoAccessRecord, SignalRecord, SignalType, SignalSource, SessionRecord, SessionStatus, InjectionLogRecord -- all defined in shared modules, used by both backends without modification.

## User Workflows

### Developer: Build with SQLite Backend

```bash
# Build with SQLite
cargo build -p unimatrix-store --features backend-sqlite

# Test with SQLite
cargo test -p unimatrix-store --features backend-sqlite

# Full workspace with SQLite
cargo test --workspace --features unimatrix-store/backend-sqlite
```

### Operator: Migrate Existing Data

```bash
# Run migration tool (exact CLI TBD by architect)
cargo run --bin migrate-redb-to-sqlite -- \
  --source ~/.unimatrix/{project_hash}/unimatrix.redb \
  --dest ~/.unimatrix/{project_hash}/unimatrix.db
```

### CI: Test Both Backends

```yaml
# Test redb (default)
- cargo test -p unimatrix-store
# Test SQLite
- cargo test -p unimatrix-store --features backend-sqlite
```

## Constraints

- rusqlite `bundled` feature required (statically links SQLite C source)
- SQLite WAL mode only -- no other journal modes
- All write operations in explicit transactions
- No new public API methods on Store
- No changes to bincode serialization format
- Test infrastructure extends existing helpers -- no isolated scaffolding

## Dependencies

| Dependency | Version | Feature | Purpose |
|-----------|---------|---------|---------|
| rusqlite | ~0.34 | bundled | SQLite bindings with static linking |
| redb | workspace | (existing) | Retained as default backend |
| bincode | workspace | (existing) | Serialization unchanged |
| serde | workspace | (existing) | Derive unchanged |
| sha2 | 0.10 | (existing) | Content hashing unchanged |
| tempfile | 3 | test/optional | Test database creation |

## NOT in Scope

- Schema normalization (index table elimination) -- nxs-006
- HNSW replacement with sqlite-vec -- separate feature, evaluated and rejected
- bincode to JSON migration -- orthogonal concern
- injection_log session_id column -- nxs-006 schema enhancement
- redb removal from Cargo.toml -- nxs-006
- Runtime backend selection -- compile-time only via feature flag
- SQLite FTS5 integration -- future feature
- Connection pooling -- over-engineering for current scale
