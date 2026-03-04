# Pseudocode Overview: nxs-005

## Component Interaction

```
lib.rs (C4)
  |-- #[cfg(not(backend-sqlite))] mod db; pub use db::Store;
  |-- #[cfg(backend-sqlite)] mod sqlite; pub use sqlite::Store;
  |-- mod schema;   (shared, unchanged)
  |-- mod error;    (extended with cfg-gated variants)
  |-- mod counter;  (redb-only, unchanged -- sqlite has inline counter logic)
  |-- mod hash;     (shared, unchanged)
  |-- mod signal;   (shared types, unchanged)
  |-- mod sessions; (redb impl, unchanged)
  |-- mod injection_log; (redb impl, unchanged)

sqlite/mod.rs (C1 entry point)
  |-- pub struct Store { conn: Mutex<Connection> }
  |-- pub mod db;            (C1: connection, tables, PRAGMAs)
  |-- pub mod write;         (C2: all write operations)
  |-- pub mod read;          (C3: all read operations)
  |-- pub mod signal;        (C4-signal: signal queue ops)
  |-- pub mod sessions;      (C4-sessions: session lifecycle ops)
  |-- pub mod injection_log; (C4-injection: injection log ops)
  |-- pub mod migration;     (C5: schema migration chain)
```

## Data Flow

### Write Path
```
caller -> Store.insert(entry)
  -> conn.lock().unwrap()
  -> tx = conn.execute("BEGIN IMMEDIATE")
  -> read+increment next_entry_id from counters
  -> INSERT into entries (id, bincode blob)
  -> INSERT into topic_index (topic, entry_id)
  -> INSERT into category_index (category, entry_id)
  -> INSERT into tag_index (tag, entry_id) [one per tag]
  -> INSERT into time_index (timestamp, entry_id)
  -> INSERT into status_index (status, entry_id)
  -> increment total_{status} counter
  -> conn.execute("COMMIT")
  -> drop MutexGuard
  -> return id
```

### Read Path
```
caller -> Store.query_by_topic(topic)
  -> conn.lock().unwrap()
  -> SELECT entry_id FROM topic_index WHERE topic = ?
  -> collect ids into HashSet
  -> SELECT data FROM entries WHERE id IN (...)
  -> deserialize each bincode blob
  -> drop MutexGuard
  -> return Vec<EntryRecord>
```

### Transaction Abstraction (ADR-001)

The `begin_read()` and `begin_write()` methods return backend-specific types.
Under redb: `redb::ReadTransaction` / `redb::WriteTransaction`.
Under SQLite: `SqliteReadTransaction<'a>` / `SqliteWriteTransaction<'a>` which wrap
`MutexGuard<'a, Connection>` and provide `open_table()` / `open_multimap_table()` shims.

The server code imports `unimatrix_store::ReadTransaction` (type alias) instead of `redb::ReadTransaction`.

SQLite transaction wrappers expose a `SqliteTableHandle` that implements basic get/insert/remove/iter using SQL statements against the held connection.

## Shared Types (unchanged)

All types in `schema.rs` remain unchanged:
- EntryRecord, NewEntry, QueryFilter, TimeRange, DatabaseConfig, Status
- CoAccessRecord, co_access_key, serialize/deserialize helpers
- Table definition constants (ENTRIES, TOPIC_INDEX, etc.) -- redb-specific, NOT used by SQLite module

All types in `signal.rs`, `sessions.rs` (types only), `injection_log.rs` (types only) remain unchanged.

## Error Mapping

```rust
#[cfg(not(feature = "backend-sqlite"))]
Database(redb::DatabaseError),
Transaction(redb::TransactionError),
Table(redb::TableError),
Storage(redb::StorageError),
Commit(redb::CommitError),
Compaction(redb::CompactionError),

#[cfg(feature = "backend-sqlite")]
Sqlite(rusqlite::Error),
```

The Display and Error impls must handle both variants via cfg.

## Integration Harness Plan

No new integration tests needed. The existing infra-001 harness (157 tests, 8 suites) validates system-level behavior by exercising the compiled binary over MCP stdio. Building with `--features unimatrix-store/backend-sqlite` and running the full harness validates SQLite parity at the system level.

The harness tests exercise: protocol compliance (13), all 10 tools (53), multi-step lifecycles (16), volume/scale (11), security (15), confidence math (13), contradiction detection (12), and edge cases (24).

Run command:
```bash
cargo build --release --features unimatrix-store/backend-sqlite
cd product/test/infra-001
python -m pytest suites/ -v --timeout=60
```
