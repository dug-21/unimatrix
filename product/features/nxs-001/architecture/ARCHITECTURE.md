# nxs-001: Embedded Storage Engine — Architecture

## Overview

The Embedded Storage Engine is the foundational persistence layer for Unimatrix. It provides a synchronous Rust library (`unimatrix-store`) backed by redb, exposing atomic multi-table transactions across 8 named tables with bincode v2 serialization.

This crate has **zero async runtime dependencies**. It is designed to be wrapped by downstream consumers (vnc-001 MCP server) via `tokio::task::spawn_blocking` with `Arc<Database>`.

## System Context

```
┌─────────────────────────────────────────────────┐
│                 Downstream Consumers             │
│                                                  │
│  vnc-001 MCP Server    nxs-002 Vector Index      │
│  (async, spawn_blocking)  (hnsw_rs, VECTOR_MAP)  │
│        │                       │                 │
└────────┼───────────────────────┼─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────────────────────────────────────┐
│              unimatrix-store (this crate)        │
│                                                  │
│  ┌──────────┐ ┌───────────┐ ┌────────────────┐  │
│  │  schema   │ │ database  │ │   error        │  │
│  │          │ │           │ │                │  │
│  │EntryRecord│ │open/close │ │StoreError      │  │
│  │Status    │ │compact    │ │                │  │
│  │tables    │ │config     │ │                │  │
│  └──────────┘ └───────────┘ └────────────────┘  │
│  ┌──────────┐ ┌───────────┐ ┌────────────────┐  │
│  │  write   │ │  read     │ │   counter      │  │
│  │          │ │           │ │                │  │
│  │insert    │ │get_by_id  │ │next_entry_id   │  │
│  │update    │ │query_*    │ │read_counter    │  │
│  │delete_idx│ │query()    │ │                │  │
│  └──────────┘ └───────────┘ └────────────────┘  │
│                                                  │
│                    redb v3.1.x                   │
└─────────────────────────────────────────────────┘
         │
         ▼
    ┌──────────┐
    │ .redb    │  Single file on disk
    │ database │  (COW B-tree, ACID, fsync)
    └──────────┘
```

## Component Breakdown

### 1. Schema Module (`schema`)

Defines all data types and table constants.

**Types:**
- `EntryRecord` — primary data struct, serialized to bincode for ENTRIES table values
- `Status` — `Active(0)`, `Deprecated(1)`, `Proposed(2)` as `#[repr(u8)]` enum
- `QueryFilter` — optional-field struct for composable multi-index queries

**Table Constants (8 total):**

| Constant | redb Type | Key | Value | Purpose |
|----------|-----------|-----|-------|---------|
| `ENTRIES` | `TableDefinition<u64, &[u8]>` | entry_id | bincode bytes | Primary store |
| `TOPIC_INDEX` | `TableDefinition<(&str, u64), ()>` | (topic, entry_id) | unit | Topic prefix scan |
| `CATEGORY_INDEX` | `TableDefinition<(&str, u64), ()>` | (category, entry_id) | unit | Category prefix scan |
| `TAG_INDEX` | `MultimapTableDefinition<&str, u64>` | tag | entry_id set | Tag intersection |
| `TIME_INDEX` | `TableDefinition<(u64, u64), ()>` | (timestamp, entry_id) | unit | Temporal range |
| `STATUS_INDEX` | `TableDefinition<(u8, u64), ()>` | (status_byte, entry_id) | unit | Status filter |
| `VECTOR_MAP` | `TableDefinition<u64, u64>` | entry_id | hnsw_data_id | Vector bridge |
| `COUNTERS` | `TableDefinition<&str, u64>` | counter_name | value | ID gen + stats |

**Counter Keys:** `"next_entry_id"`, `"total_active"`, `"total_deprecated"`

### 2. Database Module (`database`)

Manages lifecycle of the redb `Database` instance.

**Public API:**
- `open(path: &Path, config: DatabaseConfig) -> Result<Database>` — opens existing or creates new database, ensures all 8 tables exist
- `compact(db: &Database) -> Result<()>` — triggers redb compaction (call on shutdown)

**`DatabaseConfig`:**
```rust
pub struct DatabaseConfig {
    pub cache_size_bytes: usize,  // default: 64 * 1024 * 1024 (64 MB)
}
```

Table creation happens inside an initial write transaction on open — all 8 tables are opened (creating them if absent), then committed. This guarantees tables exist for all subsequent operations.

### 3. Write Operations Module (`write`)

All write operations execute within a single redb `WriteTransaction`, ensuring atomicity across the ENTRIES table and all index tables.

**Public API:**
- `insert_entry(db: &Database, record: &EntryRecord) -> Result<u64>` — full insert with ID generation and all index writes
- `update_entry(db: &Database, record: &EntryRecord) -> Result<()>` — reads old record, diffs indexed fields, removes stale index entries, inserts new ones, writes updated record
- `update_status(db: &Database, entry_id: u64, new_status: Status) -> Result<()>` — atomic status transition with STATUS_INDEX migration and counter updates
- `put_vector_mapping(db: &Database, entry_id: u64, hnsw_data_id: u64) -> Result<()>` — insert/update VECTOR_MAP entry (used by nxs-002)
- `delete_vector_mapping(db: &Database, entry_id: u64) -> Result<()>` — remove VECTOR_MAP entry

**Insert transaction flow:**
1. Begin write transaction
2. Read and increment `"next_entry_id"` from COUNTERS
3. Serialize `EntryRecord` via bincode v2, insert into ENTRIES
4. Insert `(topic, entry_id) -> ()` into TOPIC_INDEX
5. Insert `(category, entry_id) -> ()` into CATEGORY_INDEX
6. For each tag: insert `tag -> entry_id` into TAG_INDEX (multimap)
7. Insert `(created_at, entry_id) -> ()` into TIME_INDEX
8. Insert `(status as u8, entry_id) -> ()` into STATUS_INDEX
9. Increment appropriate counter (`"total_active"` or `"total_deprecated"`)
10. Commit transaction

**Update transaction flow (topic change example):**
1. Begin write transaction
2. Read old EntryRecord from ENTRIES (deserialize)
3. Diff indexed fields: detect that topic changed from "auth" to "security"
4. Remove old index entry: delete `("auth", entry_id)` from TOPIC_INDEX
5. Insert new index entry: insert `("security", entry_id)` into TOPIC_INDEX
6. Repeat for category, tags, status, timestamp if changed
7. Serialize and write updated EntryRecord to ENTRIES
8. Commit transaction

If the transaction is dropped without commit (panic, early return, `?` propagation), all changes are automatically rolled back.

### 4. Read Operations Module (`read`)

All read operations use redb `ReadTransaction` for MVCC snapshot isolation. Multiple reads execute concurrently without blocking writes.

**Individual index queries (building blocks):**
- `get_by_id(db: &Database, entry_id: u64) -> Result<Option<EntryRecord>>` — direct ENTRIES lookup, O(log n)
- `query_by_topic(db: &Database, topic: &str) -> Result<Vec<u64>>` — TOPIC_INDEX range scan on `(topic, 0)..=(topic, u64::MAX)`
- `query_by_category(db: &Database, category: &str) -> Result<Vec<u64>>` — CATEGORY_INDEX range scan
- `query_by_tags(db: &Database, tags: &[&str]) -> Result<Vec<u64>>` — TAG_INDEX multimap lookup per tag, then set intersection
- `query_by_time_range(db: &Database, start: u64, end: u64) -> Result<Vec<u64>>` — TIME_INDEX range scan
- `query_by_status(db: &Database, status: Status) -> Result<Vec<u64>>` — STATUS_INDEX range scan
- `get_vector_mapping(db: &Database, entry_id: u64) -> Result<Option<u64>>` — VECTOR_MAP point lookup
- `batch_get(db: &Database, ids: &[u64]) -> Result<Vec<EntryRecord>>` — batch ENTRIES fetch for a set of entry IDs

**Combined query:**
- `query(db: &Database, filter: &QueryFilter) -> Result<Vec<EntryRecord>>` — executes individual index queries for each present filter field, intersects result ID sets, batch-fetches matching records

**QueryFilter struct:**
```rust
pub struct QueryFilter {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<Status>,
    pub time_range: Option<(u64, u64)>,
}
```

When all fields are `None`, returns all active entries (implicit `status: Some(Status::Active)` default).

**Query composition strategy:**

Each present filter field produces a `HashSet<u64>` of matching entry IDs. The engine intersects all sets to produce the final candidate list, then batch-fetches from ENTRIES. This approach:
- Allows each index to be independently testable (AC-07 through AC-11)
- Composes naturally — adding a filter field narrows results
- Extends cleanly — future milestones add fields (feature, project, usage threshold) to QueryFilter without changing the intersection logic

### 5. Counter Module (`counter`)

Atomic ID generation and statistical counters.

**Public API:**
- `next_entry_id(txn: &WriteTransaction) -> Result<u64>` — read-increment-write within caller's transaction (not a standalone transaction, to ensure the ID is used atomically with the insert)
- `read_counter(db: &Database, key: &str) -> Result<u64>` — read a counter value in a read transaction
- `increment_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()>` — increment within caller's write transaction
- `decrement_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()>` — decrement within caller's write transaction

`next_entry_id` takes a `WriteTransaction` reference (not `&Database`) because it must execute within the same transaction as the ENTRIES insert. This prevents ID gaps from failed inserts.

### 6. Error Module (`error`)

Single `StoreError` enum covering all failure modes:

```rust
#[derive(Debug)]
pub enum StoreError {
    /// redb database errors (I/O, corruption, lock contention)
    Database(redb::DatabaseError),
    /// redb table errors (table not found, type mismatch)
    Table(redb::TableError),
    /// redb transaction errors
    Transaction(redb::TransactionError),
    /// redb storage errors (disk full, permission denied)
    Storage(redb::StorageError),
    /// bincode serialization/deserialization failure
    Serialization(bincode::error::EncodeError),
    Deserialization(bincode::error::DecodeError),
    /// Entry not found for a given ID
    EntryNotFound(u64),
    /// Counter key not found
    CounterNotFound(String),
}
```

All public functions return `Result<T, StoreError>`. No panics in the public API.

`StoreError` implements `std::error::Error` and `Display` with descriptive messages. `From` impls provide ergonomic `?` usage for redb and bincode errors.

## Data Flow

### Write Path

```
Caller                  unimatrix-store                    redb
  │                          │                               │
  │  insert_entry(db, rec)   │                               │
  │─────────────────────────►│                               │
  │                          │  begin_write()                │
  │                          │──────────────────────────────►│
  │                          │  open COUNTERS, read+inc      │
  │                          │──────────────────────────────►│
  │                          │  bincode::encode(record)      │
  │                          │  open ENTRIES, insert          │
  │                          │──────────────────────────────►│
  │                          │  open TOPIC_INDEX, insert      │
  │                          │──────────────────────────────►│
  │                          │  open CATEGORY_INDEX, insert   │
  │                          │──────────────────────────────►│
  │                          │  open TAG_INDEX, insert(×N)   │
  │                          │──────────────────────────────►│
  │                          │  open TIME_INDEX, insert       │
  │                          │──────────────────────────────►│
  │                          │  open STATUS_INDEX, insert     │
  │                          │──────────────────────────────►│
  │                          │  inc total_active counter      │
  │                          │──────────────────────────────►│
  │                          │  commit() [fsync]             │
  │                          │──────────────────────────────►│
  │  Ok(entry_id)            │                               │
  │◄─────────────────────────│                               │
```

### Read Path (Combined Query)

```
Caller                  unimatrix-store                    redb
  │                          │                               │
  │  query(db, filter)       │                               │
  │─────────────────────────►│                               │
  │                          │  begin_read()                 │
  │                          │──────────────────────────────►│
  │                          │  [if topic set]                │
  │                          │  TOPIC_INDEX range scan        │
  │                          │──────────────────────────────►│
  │                          │  ◄── Set A: {id1, id3, id7}   │
  │                          │  [if category set]             │
  │                          │  CATEGORY_INDEX range scan     │
  │                          │──────────────────────────────►│
  │                          │  ◄── Set B: {id1, id5, id7}   │
  │                          │  [if tags set]                 │
  │                          │  TAG_INDEX lookup per tag      │
  │                          │──────────────────────────────►│
  │                          │  ◄── Set C: {id1, id7, id9}   │
  │                          │                               │
  │                          │  intersect(A, B, C) → {id1,id7}│
  │                          │                               │
  │                          │  ENTRIES batch get(id1, id7)  │
  │                          │──────────────────────────────►│
  │                          │  bincode::decode × 2          │
  │  Ok(vec![rec1, rec7])    │                               │
  │◄─────────────────────────│                               │
```

### Downstream Integration (nxs-002 Vector Index)

nxs-002 uses `unimatrix-store` as a dependency. Its integration points:

1. **VECTOR_MAP writes** — after embedding an entry and inserting into hnsw_rs, nxs-002 calls `put_vector_mapping(db, entry_id, hnsw_data_id)` to persist the mapping
2. **VECTOR_MAP reads** — during search, nxs-002 reads `get_vector_mapping(db, entry_id)` to translate between entry IDs and hnsw_data_id values
3. **STATUS_INDEX reads** — nxs-002 builds a `FilterT` allow-list by scanning STATUS_INDEX for active entries, collecting their hnsw_data_ids via VECTOR_MAP, and passing the sorted `Vec<usize>` to `hnsw_rs::search_filter`
4. **Batch reads** — after hnsw_rs returns `Vec<Neighbour>`, nxs-002 maps `d_id` back to entry_id via VECTOR_MAP and calls `batch_get` for full records

### Downstream Integration (vnc-001 MCP Server)

vnc-001 wraps `unimatrix-store` for async access:

```rust
let db = Arc::new(database::open(&path, config)?);

// In MCP tool handler:
let db = db.clone();
let result = tokio::task::spawn_blocking(move || {
    read::query(&db, &filter)
}).await??;
```

The `Arc<Database>` pattern is production-proven (used by Iroh). redb's MVCC ensures reads never block writes. The single-writer constraint is naturally serialized by `spawn_blocking` — concurrent write attempts block at `begin_write()`, not in Tokio's async runtime.

## Technology Decisions

| Technology | Decision | Rationale | ADR |
|------------|----------|-----------|-----|
| redb v3.1.x | Embedded database | Pure Rust, ACID, zero deps, sub-ms at our scale. See ADR-001. |  [ADR-001](ADR-001-redb-embedded-database.md) |
| bincode v2 + serde | Serialization | Compact binary format, `#[serde(default)]` for zero-migration schema evolution. See ADR-002. | [ADR-002](ADR-002-bincode-v2-schema-evolution.md) |
| Manual secondary indexes | Index strategy | redb has no secondary index mechanism. Separate tables updated atomically. See ADR-003. | [ADR-003](ADR-003-manual-secondary-indexes.md) |
| Synchronous API | Concurrency boundary | Matches redb's sync API. Async wrapping deferred to consumers. See ADR-004. | [ADR-004](ADR-004-synchronous-api.md) |
| Compound tuple keys | Index key design | Lexicographic ordering enables prefix scans. See ADR-005. | [ADR-005](ADR-005-compound-tuple-keys.md) |

## Cargo Workspace Structure

```
/                               Cargo workspace root
├── Cargo.toml                  [workspace] members = ["crates/*"]
├── crates/
│   └── unimatrix-store/
│       ├── Cargo.toml          name = "unimatrix-store"
│       └── src/
│           ├── lib.rs          Public re-exports
│           ├── schema.rs       EntryRecord, Status, QueryFilter, table defs
│           ├── database.rs     open, compact, DatabaseConfig
│           ├── write.rs        insert_entry, update_entry, update_status, vector_map ops
│           ├── read.rs         get_by_id, query_by_*, query, batch_get
│           ├── counter.rs      next_entry_id, read_counter, increment/decrement
│           └── error.rs        StoreError enum
└── (future crates)
    ├── unimatrix-core/         nxs-004: storage traits
    ├── unimatrix-vector/       nxs-002: hnsw_rs integration
    ├── unimatrix-embed/        nxs-003: embedding pipeline
    └── unimatrix-mcp/          vnc-001: MCP server
```

**Workspace Cargo.toml:**
```toml
[workspace]
members = ["crates/*"]
resolver = "3"

[workspace.package]
edition = "2024"
rust-version = "1.89"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
redb = "3.1"
serde = { version = "1", features = ["derive"] }
bincode = "2"
```

**unimatrix-store Cargo.toml:**
```toml
[package]
name = "unimatrix-store"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
redb = { workspace = true }
serde = { workspace = true }
bincode = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

Dependencies are minimal by design: redb, serde, bincode. No async runtime. No other runtime dependencies.

## Error Handling Strategy

1. **No panics in public API.** Every public function returns `Result<T, StoreError>`. Internal assertions use `debug_assert!` only.

2. **Error categorization.** `StoreError` variants map to distinct failure domains:
   - `Database`, `Table`, `Transaction`, `Storage` — redb infrastructure failures (disk, permissions, corruption)
   - `Serialization`, `Deserialization` — bincode encode/decode failures (schema mismatch, corrupt data)
   - `EntryNotFound`, `CounterNotFound` — application-level constraint violations

3. **Ergonomic conversion.** `From` impls for all redb and bincode error types enable `?` propagation throughout the crate.

4. **Transaction safety.** redb auto-aborts uncommitted write transactions on drop. Combined with `?` propagation, any error during a multi-table write cleanly rolls back all changes. No partial writes are possible.

5. **No retry logic.** The storage engine does not retry failed operations. Retry policy is the caller's responsibility (appropriate for a library crate).

## Concurrency Model

**Single-writer, multiple-reader (MVCC):**

- redb enforces exactly one `WriteTransaction` at a time. A second `begin_write()` blocks until the first commits or aborts.
- `ReadTransaction` instances are unlimited and non-blocking. Readers see a consistent MVCC snapshot from the moment `begin_read()` is called.
- Readers never block writers. Writers never block readers.

**Sharing pattern:**

The `Database` type is `Send + Sync`. Downstream consumers wrap it in `Arc<Database>` and clone the `Arc` for each operation. This is the production-proven pattern used by Iroh (p2p data sync framework).

```rust
// Downstream async consumer (vnc-001)
let db: Arc<Database> = Arc::new(database::open(&path, config)?);

// Multiple concurrent reads — all proceed in parallel
let db1 = db.clone();
let r1 = spawn_blocking(move || read::get_by_id(&db1, 42));
let db2 = db.clone();
let r2 = spawn_blocking(move || read::query(&db2, &filter));

// Writes are serialized by redb — second write blocks until first completes
let db3 = db.clone();
let w1 = spawn_blocking(move || write::insert_entry(&db3, &record));
```

**Why synchronous (not async):**

redb's API is synchronous. Wrapping it in async at the storage layer would add complexity without benefit — every operation would just be `spawn_blocking` internally. By keeping the API synchronous, we:
- Match redb's natural API shape
- Avoid a tokio dependency in the storage crate
- Let each consumer choose their own async strategy
- Keep the crate testable without an async runtime

## Integration Points Summary

| Downstream Feature | Integration Surface | Notes |
|-------------------|---------------------|-------|
| nxs-002 (Vector Index) | `put_vector_mapping`, `get_vector_mapping`, `delete_vector_mapping`, `query_by_status` for FilterT construction, `batch_get` for result hydration | VECTOR_MAP table is created by nxs-001 but primarily written by nxs-002 |
| nxs-003 (Embedding Pipeline) | Reads `EntryRecord.content` and `EntryRecord.title` for embedding input | Read-only consumer of ENTRIES |
| nxs-004 (Core Traits) | Will define `EntryStore` trait that `unimatrix-store` implements | Trait defined in core crate, impl in store crate |
| vnc-001 (MCP Server) | Full API via `Arc<Database>` + `spawn_blocking` | Async wrapper around all read/write operations |
| crt-001 (Usage Tracking) | Extends `QueryFilter` with usage fields; adds `last_accessed_at`/`access_count` updates | `#[serde(default)]` on EntryRecord fields supports this without migration |
| col-001 (Outcome Tracking) | New tables (OUTCOME_INDEX) added in separate crate, same database file | redb supports adding tables to existing databases |

## Open Questions

1. **VECTOR_MAP value type: `u64` vs `usize`.** hnsw_rs uses `usize` for `DataId`, but redb requires fixed-width types. The SCOPE resolves this as "store as `u64`, cast to `usize` at the boundary." The architecture follows this — VECTOR_MAP stores `u64`. nxs-002 performs the `u64 as usize` / `usize as u64` casts. On 64-bit platforms (all target platforms), this is lossless.

2. **Owned vs borrowed string keys.** TOPIC_INDEX and CATEGORY_INDEX are defined with `(&str, u64)` keys in the redb table definition (redb handles the borrowing internally). The public API accepts `&str` parameters. redb's `Key` implementation for `&str` stores owned data in the B-tree, so there is no lifetime complexity in the public API despite the `&str` type parameter.
