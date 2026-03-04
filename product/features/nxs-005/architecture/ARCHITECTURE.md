# Architecture: nxs-005 SQLite Storage Engine

## System Overview

nxs-005 replaces the redb storage backend in `crates/unimatrix-store/` with SQLite (via rusqlite). The change is confined to a single crate by the existing `EntryStore` trait boundary (Unimatrix entry #71, ADR-001 from nxs-004). All consumers -- unimatrix-core (StoreAdapter), unimatrix-engine, unimatrix-server, unimatrix-vector -- interact through trait objects and see no change.

```
unimatrix-server ──┐
unimatrix-engine ──┼──> EntryStore trait ──> StoreAdapter ──> Store
unimatrix-vector ──┘    (unimatrix-core)    (unimatrix-core)  (unimatrix-store)
                                                                  │
                                                    ┌─────────────┴─────────────┐
                                                    │ redb backend (current)    │
                                                    │ SQLite backend (nxs-005)  │
                                                    └───────────────────────────┘
```

The feature flag `backend-sqlite` selects the SQLite backend. redb remains the default for safe backout.

## Component Breakdown

### C1: SQLite Connection Manager (`sqlite/db.rs`)

Owns the `rusqlite::Connection`, table creation (17 CREATE TABLE + CREATE INDEX statements), WAL mode configuration, and PRAGMA tuning.

**Responsibilities:**
- Open/create SQLite database at a given path
- Configure WAL mode, synchronous=NORMAL, journal_size_limit
- Create all 17 tables on first open (idempotent via IF NOT EXISTS)
- Run schema migration chain (delegates to migration module)
- Provide transaction wrappers

**Key design**: Unlike redb where `Database` is `Send + Sync` (internally uses Arc<RwLock>), `rusqlite::Connection` is `Send` but NOT `Sync`. The Store struct must use `Mutex<Connection>` to provide `Sync`. This is acceptable because redb already serializes write transactions -- the contention model is unchanged.

### C2: SQLite Write Operations (`sqlite/write.rs`)

All insert, update, delete, and index maintenance operations.

**Responsibilities:**
- Entry CRUD with index synchronization (same 5-index pattern as redb, using SQL INSERT/DELETE)
- Usage recording, confidence updates
- Vector mapping writes
- Co-access pair recording and cleanup
- Feature entry recording
- Metric storage

### C3: SQLite Read Operations (`sqlite/read.rs`)

All query and lookup operations.

**Responsibilities:**
- Entry point lookups (get, exists)
- Index-based queries (by topic, category, tags, time range, status)
- Multi-filter intersection (query with QueryFilter)
- Vector mapping reads
- Counter reads
- Co-access partner lookups, stats, top pairs
- Metric reads

### C4: Feature Flag Integration (`lib.rs`, `db.rs`)

Cargo feature `backend-sqlite` controls which backend is active.

**Responsibilities:**
- Conditional compilation: `#[cfg(feature = "backend-sqlite")]` selects SQLite modules
- Default (no feature flag): redb backend unchanged
- Store struct contains either `redb::Database` or `Mutex<rusqlite::Connection>` based on feature

### C5: Migration Tooling (`migrate_redb_to_sqlite.rs`)

One-time data export from redb to SQLite.

**Responsibilities:**
- Open source redb database (read-only)
- Open destination SQLite database (write)
- Copy all 17 tables row by row
- Verify row counts match per table
- Report any corrupt/skipped entries (SR-09)

### C6: Parity Test Harness (`test_helpers.rs` extension)

Test infrastructure for running the same tests against both backends.

**Responsibilities:**
- `test_with_store` macro or helper that creates a Store with the active backend
- All 234 existing tests run unchanged against whichever backend is compiled

## Component Interactions

### Transaction Flow

**redb (current):**
```
Store.insert() -> db.begin_write() -> txn.open_table(ENTRIES) -> table.insert()
                                   -> txn.open_table(TOPIC_INDEX) -> table.insert()
                                   -> ... (5 more index tables)
                                   -> txn.commit()
```

**SQLite (new):**
```
Store.insert() -> conn.lock() -> conn.execute("BEGIN")
              -> conn.execute("INSERT INTO entries ...")
              -> conn.execute("INSERT INTO topic_index ...")
              -> ... (5 more index tables)
              -> conn.execute("COMMIT")
```

The logical flow is identical. The difference is:
1. redb: typed table handles opened per transaction, compile-time table definitions
2. SQLite: SQL strings executed against a connection, runtime schema

### Error Mapping

redb error types (DatabaseError, TransactionError, TableError, StorageError, CommitError, CompactionError) map to SQLite's single `rusqlite::Error`. The StoreError enum must be updated:

- Keep `EntryNotFound`, `Serialization`, `Deserialization`, `InvalidStatus` unchanged
- Under `backend-sqlite` feature: replace 6 redb variants with `Sqlite(rusqlite::Error)`
- Under default (redb) feature: keep existing variants

### Public API Surface

The Store struct's public API (34 methods) remains identical. No signature changes:

| Method Group | Count | Signature Change |
|-------------|-------|-----------------|
| open, open_with_config, compact | 3 | None (compact becomes VACUUM) |
| begin_read, begin_write | 2 | **Return type changes** (see ADR-001) |
| insert, update, update_status, delete | 4 | None |
| get, exists, query, query_by_* | 8 | None |
| Vector mapping ops | 3 | None |
| read_counter | 1 | None |
| Co-access ops | 4 | None |
| Usage recording | 2 | None |
| Confidence update | 1 | None |
| Feature entries, metrics | 3 | None |
| Signal queue ops | 3 | None |
| Session ops | 5 | None |
| Injection log ops | 2 | None |

**Critical**: `begin_read()` and `begin_write()` currently return `redb::ReadTransaction` and `redb::WriteTransaction`. These leak redb types through the public API and are used by unimatrix-server directly (agent registry, audit log). See ADR-001 for resolution.

## Technology Decisions

### ADR-001: Abstract Transaction Types (see `ADR-001-abstract-transaction-types.md`)

The `begin_read()` / `begin_write()` methods return redb-specific types. Under the feature flag, these must return SQLite equivalents. Resolution: introduce backend-specific type aliases behind cfg.

### ADR-002: Mutex<Connection> for Sync (see `ADR-002-mutex-connection-sync.md`)

rusqlite::Connection is Send but not Sync. Store must be Sync (shared via Arc across async handlers). Resolution: wrap Connection in Mutex.

### ADR-003: WAL Mode and Auto-Checkpoint (see `ADR-003-wal-mode-auto-checkpoint.md`)

SQLite journal mode selection and checkpoint strategy.

### ADR-004: Feature Flag Strategy (see `ADR-004-feature-flag-strategy.md`)

How the dual-backend coexistence works via Cargo features.

## Integration Points

| Component | Interface | Impact |
|-----------|-----------|--------|
| unimatrix-core StoreAdapter | `Arc<Store>` wrapping | None -- StoreAdapter wraps Arc<Store>, all methods delegate. Store's internal type changes are invisible. |
| unimatrix-server (agent_registry, audit_log) | `store.begin_read()`, `store.begin_write()` | **ADR-001**: These methods return backend-specific transaction types. Server code that opens AGENT_REGISTRY and AUDIT_LOG tables directly must be abstracted behind cfg or moved to Store methods. |
| unimatrix-vector VectorIndex | `store.iter_vector_mappings()`, `store.put_vector_mapping()` | None -- these are Store methods that return `Vec<(u64, u64)>` and take primitive args. |
| unimatrix-server PidGuard | File locking on PID file | None -- PidGuard guards the process, not the database file. SQLite's additional -wal/-shm files are irrelevant. |

## Integration Surface

| Integration Point | Type/Signature | Source | Change |
|-------------------|---------------|--------|--------|
| `Store::open(path)` | `fn open(impl AsRef<Path>) -> Result<Self>` | `db.rs` | None |
| `Store::begin_read()` | `fn begin_read(&self) -> Result<redb::ReadTransaction>` | `db.rs` | **Return type becomes cfg-dependent** (ADR-001) |
| `Store::begin_write()` | `fn begin_write(&self) -> Result<redb::WriteTransaction>` | `db.rs` | **Return type becomes cfg-dependent** (ADR-001) |
| `Store::compact()` | `fn compact(&mut self) -> Result<()>` | `db.rs` | SQLite: no-op or VACUUM |
| `StoreAdapter::new(Arc<Store>)` | `fn new(store: Arc<Store>) -> Self` | `adapters.rs` | None |
| `StoreError` enum | 10 variants | `error.rs` | 6 redb variants replaced by 1 Sqlite variant under feature flag |
| `DatabaseConfig` | `struct { cache_size: usize }` | `schema.rs` | May add SQLite-specific fields behind cfg |

## Testing Architecture

Validation operates at two levels:

1. **Unit-level parity** (`cargo test`): All 234 store tests run against whichever backend is compiled. The feature flag selects the Store implementation; tests are backend-agnostic. This validates individual method behavior.

2. **System-level parity** (infra-001 harness): The infra-001 integration test harness (`product/test/infra-001/`) exercises the compiled `unimatrix-server` binary over MCP JSON-RPC stdio -- the exact interface agents use. Building the binary with `--features unimatrix-store/backend-sqlite` and running the full harness (157 tests, 8 suites) validates that the SQLite backend produces correct behavior through the entire server stack: protocol compliance, multi-step lifecycle flows, restart persistence, scale behavior, security defenses, confidence math, contradiction detection, and edge cases. This catches integration issues that unit tests cannot (e.g., transaction lifetime mismatches under async request handling, HNSW rebuild with SQLite VECTOR_MAP). See `product/test/infra-001/USAGE-PROTOCOL.md` for suite descriptions and running instructions.

## SQLite Schema

All 17 tables map 1:1 from redb. See `product/research/ass-016/retrospective-data-architecture.md` section 2.4 for the complete CREATE TABLE schema. Key design decisions:

1. **BLOB columns preserve bincode**: No deserialization change.
2. **Composite primary keys replace tuple keys**: `(topic TEXT, entry_id INTEGER)` replaces redb's `(&str, u64)` key.
3. **MultimapTable -> regular table**: TAG_INDEX and FEATURE_ENTRIES become tables with composite PKs instead of redb multimap tables.
4. **CO_ACCESS CHECK constraint**: `CHECK (entry_id_a < entry_id_b)` mirrors the application-level `co_access_key()` ordering.
5. **Single additional index**: `idx_co_access_b ON co_access(entry_id_b)` for reverse lookups.

## SQLite PRAGMA Configuration

```sql
PRAGMA journal_mode = WAL;          -- Required: concurrent readers during writes
PRAGMA synchronous = NORMAL;        -- Balanced durability: fsync on checkpoint, not every commit
PRAGMA wal_autocheckpoint = 1000;   -- Default: auto-checkpoint after 1000 pages (~4MB)
PRAGMA foreign_keys = OFF;          -- No FK relationships in our schema
PRAGMA busy_timeout = 5000;         -- 5s wait before SQLITE_BUSY (addresses SR assumption #1)
PRAGMA cache_size = -16384;         -- 16MB page cache (negative = KB)
```
