# nxs-001: Embedded Storage Engine — Specification

**Feature**: nxs-001 (Nexus Phase)
**Status**: Specification
**Source**: SCOPE.md, PRODUCT-VISION.md, ASS-003 D2, ASS-007 Proposal A DATABASE.md/INTERFACE.md

---

## 1. Functional Requirements

### FR-01: Database Lifecycle

**FR-01.1: Open or Create**
The storage engine opens an existing redb database file at a caller-provided path, or creates a new one if the file does not exist. On first open, all 8 named tables are created within the database. Subsequent opens reuse existing tables.

**FR-01.2: Cache Configuration**
The caller may specify a cache size in bytes at open time. Default: 64 MiB. The cache size is passed to redb's `DatabaseBuilder::set_cache_size()`.

**FR-01.3: Compaction**
A `compact()` method rewrites the database file, reclaiming space from COW pages. Intended for clean shutdown. Returns byte count reclaimed or error.

**FR-01.4: Shutdown**
No explicit shutdown method is required — dropping the `Database` handle closes the file. Callers should call `compact()` before drop for space reclamation. The API documents this pattern.

### FR-02: EntryRecord Schema

The primary data structure stored in the ENTRIES table. All fields are defined at schema creation; fields not yet populated by this milestone use `#[serde(default)]` to enable zero-migration schema evolution.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryRecord {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    #[serde(default)]
    pub confidence: f32,
    pub created_at: u64,       // unix timestamp seconds
    pub updated_at: u64,       // unix timestamp seconds
    #[serde(default)]
    pub last_accessed_at: u64,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub supersedes: Option<u64>,
    #[serde(default)]
    pub superseded_by: Option<u64>,
    #[serde(default)]
    pub correction_count: u32,
    #[serde(default)]
    pub embedding_dim: u16,
}
```

**Field semantics:**

| Field | Type | Purpose | Default |
|-------|------|---------|---------|
| `id` | `u64` | Unique entry identifier, monotonically increasing | Generated |
| `title` | `String` | Short human-readable title | Required on insert |
| `content` | `String` | Full markdown content body | Required on insert |
| `topic` | `String` | Primary topic (e.g., "auth", "error-handling") | Required on insert |
| `category` | `String` | Knowledge type (e.g., "convention", "decision", "pattern") | Required on insert |
| `tags` | `Vec<String>` | Cross-cutting labels for filtering | Empty vec |
| `source` | `String` | Provenance (e.g., "agent:architect", "user") | Required on insert |
| `status` | `Status` | Lifecycle state | `Status::Active` |
| `confidence` | `f32` | Cached confidence score (computed by higher layers) | `0.0` |
| `created_at` | `u64` | Creation unix timestamp (seconds) | Set on insert |
| `updated_at` | `u64` | Last modification unix timestamp (seconds) | Set on insert, updated on modify |
| `last_accessed_at` | `u64` | Last read timestamp (updated by higher layers) | `0` |
| `access_count` | `u32` | Read count (updated by higher layers) | `0` |
| `supersedes` | `Option<u64>` | Entry ID this corrects/replaces | `None` |
| `superseded_by` | `Option<u64>` | Entry ID that replaced this | `None` |
| `correction_count` | `u32` | Times this entry has been corrected | `0` |
| `embedding_dim` | `u16` | Embedding dimensions (set by nxs-003) | `0` |

### FR-03: Status Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
}
```

**Lifecycle transitions (enforced by the API):**

| From | To | Trigger |
|------|----|---------|
| Active | Deprecated | `update_status()` call |
| Proposed | Active | `update_status()` call |
| Proposed | Deprecated | `update_status()` call |
| Deprecated | Active | `update_status()` call (re-activation) |

The storage engine enforces that the `status` field in the ENTRIES record and the STATUS_INDEX are always consistent. It does not enforce transition policy — that is the responsibility of higher layers (vnc-002 tools).

### FR-04: Write Operations

**FR-04.1: Insert Entry**
Accepts a new entry (all required fields). Within a single write transaction:
1. Read and increment the `"next_entry_id"` counter from COUNTERS.
2. Set the entry's `id` to the generated value.
3. Serialize the `EntryRecord` via bincode and insert into ENTRIES.
4. Insert `(topic, entry_id) -> ()` into TOPIC_INDEX.
5. Insert `(category, entry_id) -> ()` into CATEGORY_INDEX.
6. For each tag, insert `tag -> entry_id` into TAG_INDEX (multimap).
7. Insert `(created_at, entry_id) -> ()` into TIME_INDEX.
8. Insert `(status as u8, entry_id) -> ()` into STATUS_INDEX.
9. Increment the appropriate counter in COUNTERS (`"total_active"` or `"total_deprecated"` or `"total_proposed"` based on initial status).
10. Commit transaction.

Returns the assigned `entry_id` on success. If any step fails, the transaction aborts and no tables are modified.

**FR-04.2: Update Entry**
Accepts an `entry_id` and a complete replacement `EntryRecord`. Within a single write transaction:
1. Read the existing entry from ENTRIES. Error if not found.
2. Compare indexed fields (topic, category, tags, status, created_at) between old and new.
3. For each changed indexed field:
   - Remove the old index entry.
   - Insert the new index entry.
4. Serialize and write the new `EntryRecord` to ENTRIES.
5. Update `updated_at` timestamp.
6. Commit transaction.

The caller provides the full updated record. The engine diffs and handles index maintenance internally.

**FR-04.3: Update Status**
A specialized write that changes only the `status` field. Within a single write transaction:
1. Read the existing entry from ENTRIES. Error if not found.
2. Remove old entry from STATUS_INDEX at `(old_status as u8, entry_id)`.
3. Insert new entry into STATUS_INDEX at `(new_status as u8, entry_id)`.
4. Update `status` field on the `EntryRecord`, set `updated_at`.
5. Serialize and write back to ENTRIES.
6. Adjust COUNTERS: decrement old status counter, increment new status counter.
7. Commit transaction.

**FR-04.4: Vector Map Write**
Insert or update a mapping from `entry_id` to `hnsw_data_id` (both `u64`) in the VECTOR_MAP table. This is a simple key-value write within a write transaction. May be performed as part of a larger transaction or standalone.

**FR-04.5: Delete Entry**
Accepts an `entry_id`. Within a single write transaction:
1. Read the existing entry from ENTRIES. Error if not found.
2. Remove from ENTRIES.
3. Remove `(topic, entry_id)` from TOPIC_INDEX.
4. Remove `(category, entry_id)` from CATEGORY_INDEX.
5. For each tag, remove `entry_id` from TAG_INDEX multimap.
6. Remove `(created_at, entry_id)` from TIME_INDEX.
7. Remove `(status as u8, entry_id)` from STATUS_INDEX.
8. Remove `entry_id` from VECTOR_MAP (if present).
9. Decrement the appropriate status counter in COUNTERS.
10. Commit transaction.

Note: Delete is provided for completeness and test infrastructure. The primary lifecycle mechanism is status transitions (Active → Deprecated), not deletion.

### FR-05: Read Operations

All read operations use redb read transactions (MVCC snapshots). Multiple reads may execute concurrently without blocking writes.

**FR-05.1: Get by ID**
Look up a single entry by `entry_id` in the ENTRIES table. Deserialize and return the `EntryRecord`, or return an error if not found.

**FR-05.2: Query by Topic**
Range scan TOPIC_INDEX for `(topic, 0)..=(topic, u64::MAX)`. Collect entry IDs. Batch fetch and deserialize from ENTRIES. Return `Vec<EntryRecord>`.

**FR-05.3: Query by Category**
Range scan CATEGORY_INDEX for `(category, 0)..=(category, u64::MAX)`. Collect entry IDs. Batch fetch and deserialize from ENTRIES. Return `Vec<EntryRecord>`.

**FR-05.4: Query by Tags**
For each requested tag, look up entry IDs from TAG_INDEX (multimap). Intersect all result sets (entries must match ALL tags). Batch fetch and deserialize from ENTRIES. Return `Vec<EntryRecord>`.

**FR-05.5: Query by Time Range**
Range scan TIME_INDEX for `(start_ts, 0)..=(end_ts, u64::MAX)`. Collect entry IDs. Batch fetch and deserialize from ENTRIES. Return `Vec<EntryRecord>`.

**FR-05.6: Query by Status**
Range scan STATUS_INDEX for `(status as u8, 0)..=(status as u8, u64::MAX)`. Collect entry IDs. Batch fetch and deserialize from ENTRIES. Return `Vec<EntryRecord>`.

**FR-05.7: Combined Query (QueryFilter)**
Accept a `QueryFilter` struct with optional fields. For each present field, execute the corresponding index query (FR-05.2 through FR-05.6). Intersect all result sets. Batch fetch from ENTRIES. Return `Vec<EntryRecord>`.

If all fields are `None`, return all entries with `Status::Active` (default behavior).

**FR-05.8: Vector Map Lookup**
Look up `hnsw_data_id` by `entry_id` in VECTOR_MAP. Return `Option<u64>`.

**FR-05.9: Entry Existence Check**
Check whether an `entry_id` exists in the ENTRIES table without deserializing the value. Return `bool`.

### FR-06: Counter and ID Generation

**FR-06.1: Next Entry ID**
Read `"next_entry_id"` from COUNTERS, return current value, write `current + 1`. This operation is always performed within a write transaction, guaranteeing monotonic increase. The first ID generated is `1` (counter starts at `0`, but the first call reads `0`, stores `1`, and returns `1` — or alternatively reads missing key, defaults to `1`, stores `2`, returns `1`).

Design decision: The first entry ID is `1` (not `0`). ID `0` is reserved as a sentinel/invalid value.

**FR-06.2: Counter Read**
Read any named counter from COUNTERS. Keys include: `"next_entry_id"`, `"total_active"`, `"total_deprecated"`, `"total_proposed"`. Returns `u64`. Missing keys return `0`.

**FR-06.3: Counter Increment/Decrement**
Atomic read-modify-write on a named counter within a write transaction. Used internally by insert/delete/status-change operations to maintain accurate counts.

### FR-07: Index Maintenance

All index maintenance is automatic and internal. Callers never directly manipulate index tables.

**FR-07.1: Insert Indexing**
On entry insert (FR-04.1), all five index tables (TOPIC, CATEGORY, TAG, TIME, STATUS) are populated atomically.

**FR-07.2: Update Indexing**
On entry update (FR-04.2), the engine reads the old record, compares indexed fields, removes stale index entries, and inserts new index entries — all within one transaction. Only changed fields trigger index updates.

**FR-07.3: Delete Indexing**
On entry delete (FR-04.5), all index entries for the deleted record are removed atomically.

**FR-07.4: Status Index Migration**
On status change (FR-04.3), the old `(status, entry_id)` is removed from STATUS_INDEX and the new `(status, entry_id)` is inserted, within one transaction.

---

## 2. Non-Functional Requirements

### NFR-01: Performance

| Operation | Target | Scale | Basis |
|-----------|--------|-------|-------|
| Point read (by ID) | < 1 ms | 1K–100K entries | B-tree depth 2–3 (ASS-003 D2) |
| Index range scan | < 1 ms | 1K–100K entries | B-tree range iteration |
| Tag intersection (3 tags) | < 2 ms | 1K–100K entries | Three multimap lookups + set intersection |
| Combined QueryFilter | < 5 ms | 1K–100K entries | Multiple index scans + intersection + batch fetch |
| Single entry insert | < 5 ms | 1K–100K entries | Write transaction with 6 table writes + fsync |
| Entry update | < 5 ms | 1K–100K entries | Read old + diff + write transaction + fsync |
| Status change | < 3 ms | 1K–100K entries | Two STATUS_INDEX ops + ENTRIES write + fsync |
| Database open | < 100 ms | Up to 200 MB file | File open + table validation |
| Compaction | < 5 s | Up to 200 MB file | Full file rewrite |

These targets are derived from ASS-003 benchmarks at Unimatrix scale. They are design targets, not hard SLA guarantees. Performance depends on underlying storage hardware.

### NFR-02: Crash Safety

- All write transactions use redb's default `Durability::Immediate` (fsync on commit).
- redb's COW B-tree with double-buffered commit slots ensures ACID guarantees without a WAL.
- Uncommitted transactions are automatically aborted on drop (panic, early return, or `?` propagation).
- The database self-recovers on next open after a crash. No manual intervention required.

### NFR-03: Memory Usage

- Default cache: 64 MiB (configurable at open time).
- At 100K entries with ~1 KB average serialized size: ~100 MB on disk, comfortably within cache.
- No additional memory pools or allocators. Standard Rust allocator only.

### NFR-04: No Unsafe Code

The `unimatrix-store` crate must contain zero `unsafe` blocks. redb and bincode are safe Rust. The `#![forbid(unsafe_code)]` attribute is set at the crate level.

### NFR-05: No Async Runtime Dependency

The crate has no dependency on tokio, async-std, or any async runtime. All functions are synchronous. Downstream consumers (vnc-001) wrap calls with `tokio::task::spawn_blocking` and `Arc<Database>`.

### NFR-06: Concurrency Model

- Single writer at a time (redb enforces this — second `begin_write()` blocks until first completes).
- Unlimited concurrent readers (MVCC snapshots).
- Readers never block writers; writers never block readers.
- The `Database` type is `Send + Sync`, shareable via `Arc<Database>`.

### NFR-07: Data Integrity

- Bincode serialization round-trips are verified: `deserialize(serialize(record)) == record` for all valid `EntryRecord` values.
- Index tables are always consistent with the ENTRIES table (enforced by atomic transactions).
- Counter values are always consistent with actual table contents (enforced by atomic transactions).

### NFR-08: File Size

- At 100K entries: estimated 50–200 MB database file.
- Compaction reclaims ~58% of freed space (per ASS-003 benchmarks).
- No automatic compaction. Manual `compact()` call on shutdown.

---

## 3. Domain Model

### 3.1 Core Types

#### EntryRecord

See FR-02 for full field listing. The canonical schema struct stored in the ENTRIES table via bincode serialization.

#### Status

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
}
```

Supports `From<u8>` and `Into<u8>` conversions. Unknown byte values return an error (not a panic).

#### QueryFilter

```rust
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<Status>,
    pub time_range: Option<TimeRange>,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    pub start: u64,  // inclusive, unix timestamp seconds
    pub end: u64,    // inclusive, unix timestamp seconds
}
```

When all fields are `None`, the query returns all entries with `Status::Active`. When one or more fields are set, results are the intersection of all individual index queries.

The `QueryFilter` is designed for extensibility. Future milestones add fields (`feature: Option<String>`, `min_confidence: Option<f32>`, `project_id: Option<String>`) without changing existing callers — new fields default to `None` via `#[derive(Default)]`.

#### DatabaseConfig

```rust
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub cache_size: usize,  // bytes, default 64 * 1024 * 1024
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            cache_size: 64 * 1024 * 1024, // 64 MiB
        }
    }
}
```

### 3.2 Table Definitions

Eight named redb tables, defined as typed constants:

| Constant | redb Type | Key Type | Value Type | Purpose |
|----------|-----------|----------|------------|---------|
| `ENTRIES` | `TableDefinition<u64, &[u8]>` | `u64` (entry_id) | `&[u8]` (bincode) | Primary entry storage |
| `TOPIC_INDEX` | `TableDefinition<(&str, u64), ()>` | `(&str, u64)` (topic, entry_id) | `()` | Topic prefix scan |
| `CATEGORY_INDEX` | `TableDefinition<(&str, u64), ()>` | `(&str, u64)` (category, entry_id) | `()` | Category prefix scan |
| `TAG_INDEX` | `MultimapTableDefinition<&str, u64>` | `&str` (tag) | `u64` (entry_id) | Tag set intersection |
| `TIME_INDEX` | `TableDefinition<(u64, u64), ()>` | `(u64, u64)` (timestamp, entry_id) | `()` | Temporal range queries |
| `STATUS_INDEX` | `TableDefinition<(u8, u64), ()>` | `(u8, u64)` (status byte, entry_id) | `()` | Status filtering |
| `VECTOR_MAP` | `TableDefinition<u64, u64>` | `u64` (entry_id) | `u64` (hnsw_data_id) | Bridge to vector index |
| `COUNTERS` | `TableDefinition<&str, u64>` | `&str` (counter name) | `u64` (value) | ID generation + stats |

**COUNTERS keys:**
- `"next_entry_id"` — next ID to assign (starts at 1)
- `"total_active"` — count of entries with `Status::Active`
- `"total_deprecated"` — count of entries with `Status::Deprecated`
- `"total_proposed"` — count of entries with `Status::Proposed`

**Index key design rationale:**
- TOPIC_INDEX and CATEGORY_INDEX use `(&str, u64)` compound keys — the string prefix enables range scans (`"auth", 0)..=("auth", u64::MAX)`), and the entry_id suffix ensures uniqueness without a separate value.
- TIME_INDEX uses `(u64, u64)` — timestamp first for temporal ordering, entry_id second for uniqueness.
- STATUS_INDEX uses `(u8, u64)` — status byte first for filtering, entry_id second for uniqueness.
- TAG_INDEX is a `MultimapTable` because one tag maps to many entries — the natural one-to-many relationship.

### 3.3 Error Types

```rust
#[derive(Debug)]
pub enum StoreError {
    /// Entry with the given ID was not found.
    EntryNotFound(u64),

    /// Underlying redb database error.
    Database(redb::DatabaseError),

    /// redb transaction error.
    Transaction(redb::TransactionError),

    /// redb table error.
    Table(redb::TableError),

    /// redb storage error (I/O, corruption).
    Storage(redb::StorageError),

    /// redb compaction error.
    Compaction(redb::CompactionError),

    /// Bincode serialization failed.
    Serialization(String),

    /// Bincode deserialization failed.
    Deserialization(String),

    /// Invalid status byte (not 0, 1, or 2).
    InvalidStatus(u8),
}
```

**Error handling contract:**
- All public functions return `Result<T, StoreError>`. No panics.
- `StoreError` implements `std::fmt::Display` and `std::error::Error`.
- redb error types are wrapped, not re-exported. Callers do not depend on redb directly.
- Serialization/deserialization errors include descriptive context (which operation, which entry_id if known).

---

## 4. Acceptance Criteria

Restated from SCOPE.md with verification methods, grouped by functional area.

### Workspace and Compilation

**AC-01**: A Cargo workspace exists at the repository root with a `unimatrix-store` library crate that compiles with `cargo build`.
- **Verification**: `cargo build --workspace` succeeds with zero errors and zero warnings. `Cargo.toml` at repo root defines `[workspace]` with `unimatrix-store` as a member. Crate is `edition = "2024"`.

### Schema

**AC-02**: EntryRecord round-trip serialization via bincode.
- **Verification**: Unit test creates an `EntryRecord` with all fields populated, serializes to bytes via bincode, deserializes back, and asserts equality (`==`). Test covers edge cases: empty strings, empty tags vec, `Option::None` fields, max u64 timestamps.

**AC-16**: Schema evolution — old serialized data deserializes with new fields.
- **Verification**: Unit test serializes an `EntryRecord` without `#[serde(default)]` fields (simulated by serializing a reduced struct or by manually crafting bytes), then deserializes into the full struct. New fields must default to their `serde(default)` values (`0`, `None`, `false`, empty).

### Table Creation

**AC-03**: All 8 tables are defined and created on database open.
- **Verification**: Integration test opens a new database, then verifies all 8 tables exist by opening each in a read transaction. Test uses `list_tables()` and `list_multimap_tables()` to confirm names.

### Write Operations

**AC-04**: Atomic multi-table insert.
- **Verification**: Integration test inserts an entry, then reads from each index table to verify presence. A second test simulates a failure mid-transaction (by dropping the transaction before commit) and verifies no tables were modified.

**AC-05**: Monotonic entry ID generation from COUNTERS.
- **Verification**: Integration test inserts 100 entries and asserts each returned ID is strictly greater than the previous. Second test: after inserting entries, reads `"next_entry_id"` counter and asserts it equals `last_assigned_id + 1`.

**AC-12**: Atomic status update with index migration.
- **Verification**: Integration test inserts an entry with `Status::Active`, then calls `update_status(id, Status::Deprecated)`. Asserts: STATUS_INDEX no longer contains `(0, id)`, STATUS_INDEX contains `(1, id)`, ENTRIES record shows `status == Deprecated`, `"total_active"` counter decremented, `"total_deprecated"` counter incremented.

**AC-13**: VECTOR_MAP insert and lookup.
- **Verification**: Integration test writes `(entry_id=42, hnsw_data_id=7)` to VECTOR_MAP. Reads back and asserts `hnsw_data_id == 7`. Overwrites with `hnsw_data_id=99`, reads back, asserts `99`.

**AC-18**: Atomic update with index migration on field change.
- **Verification**: Integration test inserts entry with `topic="auth"`, then updates to `topic="security"`. Asserts: TOPIC_INDEX range scan for `"auth"` returns empty, TOPIC_INDEX range scan for `"security"` returns the entry_id. Same pattern for category change and tag addition/removal.

### Read Operations

**AC-06**: Point lookup by entry ID.
- **Verification**: Integration test inserts an entry, retrieves by ID, asserts all fields match the inserted values. Test also asserts that looking up a non-existent ID returns `StoreError::EntryNotFound`.

**AC-07**: Topic index query.
- **Verification**: Integration test inserts 5 entries across 3 topics. Queries for topic `"auth"` and asserts exactly the entries with that topic are returned (correct count, correct IDs).

**AC-08**: Category index query.
- **Verification**: Same pattern as AC-07 but on CATEGORY_INDEX. Inserts across multiple categories, queries one, asserts correct set returned.

**AC-09**: Tag intersection query.
- **Verification**: Integration test inserts entries with overlapping tag sets. Queries for `["rust", "error"]` and asserts only entries with BOTH tags are returned. Edge case: query with a tag that matches no entries returns empty vec.

**AC-10**: Time range query.
- **Verification**: Integration test inserts entries with timestamps at 1000, 2000, 3000, 4000, 5000. Queries range `2000..=4000` and asserts exactly 3 entries returned. Edge cases: empty range, single-point range.

**AC-11**: Status index query.
- **Verification**: Integration test inserts entries with different statuses. Queries for `Status::Active` and asserts only active entries returned.

### Combined Query

**AC-17**: QueryFilter with multiple fields.
- **Verification**: Integration test inserts entries varying in topic, category, tags, status, and time. Applies a QueryFilter with `topic="auth"` + `status=Active` + `tags=["jwt"]`. Asserts only the intersection is returned. Second test: empty QueryFilter returns all active entries.

### Error Handling

**AC-15**: Typed Result errors, no panics.
- **Verification**: Unit tests verify each error variant is constructible and displays meaningful messages. Integration tests verify: `get(nonexistent_id)` returns `EntryNotFound`, corrupt bytes in ENTRIES cause `Deserialization` error (test by raw-writing invalid bytes). `#![forbid(unsafe_code)]` is set at crate level (verified by compilation).

### Database Lifecycle

**AC-14**: Open/create, cache config, compact.
- **Verification**: Integration test: (1) Open creates new file — assert file exists on disk. (2) Close and reopen — assert previously inserted entries are still present. (3) Open with custom cache size (128 MiB) — no error. (4) Call `compact()` — no error, file size does not increase.

### Test Infrastructure

**AC-19**: Reusable test infrastructure.
- **Verification**: Test helpers are defined in a `test_utils` module (or `tests/common` module) providing: `TestDb` struct that creates a temp directory and database, implements `Drop` for cleanup; `sample_entry()` and `sample_entry_with(...)` factory functions; assertion helpers for verifying index consistency. These are public to downstream crates via `#[cfg(test)]` or a `test-support` feature flag.

---

## 5. Constraints and Dependencies

### Hard Constraints

| Constraint | Source | Impact |
|------------|--------|--------|
| Rust edition 2024 | redb v3.1.0 MSRV 1.89 | Workspace `Cargo.toml` sets `edition = "2024"` |
| Synchronous API only | redb has no async API | All functions are blocking; no `async fn` |
| Single writer | redb transaction model | API documents this; no internal writer pooling |
| No unsafe code | Project convention | `#![forbid(unsafe_code)]` at crate level |
| Caller-provided file path | Architecture decision | No default paths, no environment variable lookups |
| No CLI, no MCP | Scope boundary | Pure library crate, no binary targets |

### Crate Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `redb` | `3.1.x` | Embedded key-value database |
| `serde` | `1.x` | Serialization framework (derive macros) |
| `bincode` | `2.x` | Binary serialization codec |

**Dev dependencies** (test only):
| Dependency | Version | Purpose |
|------------|---------|---------|
| `tempfile` | `3.x` | Temporary directories for test databases |

No other runtime dependencies. No feature flags required for core functionality.

### Cargo Workspace Layout

```
/Cargo.toml                    # workspace root
/crates/
  unimatrix-store/
    Cargo.toml                 # library crate
    src/
      lib.rs                   # crate root, re-exports
      schema.rs                # EntryRecord, Status, table definitions
      db.rs                    # Database open/create/compact, DatabaseConfig
      write.rs                 # insert, update, update_status, delete
      read.rs                  # get, query_by_topic, query_by_category, etc.
      query.rs                 # QueryFilter, combined query logic
      counter.rs               # next_id, read_counter, inc/dec
      error.rs                 # StoreError enum
```

---

## 6. API Surface

### Module: `schema`

```rust
/// Primary entry record stored in the ENTRIES table.
pub struct EntryRecord { /* FR-02 fields */ }

/// Entry lifecycle status.
pub enum Status { Active, Deprecated, Proposed }

/// Combined query filter for multi-index intersection.
pub struct QueryFilter { /* optional topic, category, tags, status, time_range */ }

/// Time range for temporal queries.
pub struct TimeRange { pub start: u64, pub end: u64 }

/// Database configuration.
pub struct DatabaseConfig { pub cache_size: usize }
```

### Module: `error`

```rust
/// All errors returned by the storage engine.
pub enum StoreError { /* see §3.3 */ }

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, StoreError>;
```

### Module: `db`

```rust
/// The storage engine handle. Wraps a redb::Database.
/// Send + Sync. Shareable via Arc<Store>.
pub struct Store { /* private fields */ }

impl Store {
    /// Open or create a database at the given path with default config.
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;

    /// Open or create with custom configuration.
    pub fn open_with_config(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self>;

    /// Compact the database file, reclaiming space.
    pub fn compact(&self) -> Result<()>;
}
```

### Module: `write` (methods on `Store`)

```rust
impl Store {
    /// Insert a new entry. Returns the assigned entry_id.
    /// All index tables are updated atomically.
    pub fn insert(&self, entry: NewEntry) -> Result<u64>;

    /// Update an existing entry. Indexes are diffed and updated atomically.
    pub fn update(&self, entry: EntryRecord) -> Result<()>;

    /// Change the status of an entry. Migrates STATUS_INDEX atomically.
    pub fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()>;

    /// Delete an entry and all its index entries.
    pub fn delete(&self, entry_id: u64) -> Result<()>;

    /// Write a vector map entry (entry_id -> hnsw_data_id).
    pub fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()>;
}

/// Fields required to create a new entry (id, created_at, updated_at assigned by engine).
pub struct NewEntry {
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
}
```

### Module: `read` (methods on `Store`)

```rust
impl Store {
    /// Get a single entry by ID.
    pub fn get(&self, entry_id: u64) -> Result<EntryRecord>;

    /// Check if an entry exists.
    pub fn exists(&self, entry_id: u64) -> Result<bool>;

    /// Query entries by topic.
    pub fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>>;

    /// Query entries by category.
    pub fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>>;

    /// Query entries matching ALL specified tags.
    pub fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>>;

    /// Query entries within a time range (inclusive).
    pub fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>>;

    /// Query entries with a given status.
    pub fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>>;

    /// Combined query with set intersection across all specified filters.
    pub fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>>;

    /// Look up the hnsw_data_id for an entry.
    pub fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>>;

    /// Read a named counter value. Returns 0 if counter does not exist.
    pub fn read_counter(&self, name: &str) -> Result<u64>;
}
```

### Re-exports (crate root `lib.rs`)

```rust
pub use schema::{EntryRecord, Status, QueryFilter, TimeRange, DatabaseConfig, NewEntry};
pub use db::Store;
pub use error::{StoreError, Result};
```

All internal modules (`write`, `read`, `counter`, `query`) expose their functionality through methods on `Store`. Users interact with a single `Store` type.

---

## 7. Open Questions

**OQ-1: `NewEntry` vs direct `EntryRecord` for insert.**
The specification introduces a `NewEntry` struct for insert (without `id`, `created_at`, `updated_at`, which are engine-assigned). Alternative: accept a partial `EntryRecord` with id=0 and overwrite. The `NewEntry` approach is type-safer — the engine can never receive a caller-assigned ID by accident.
**Resolution**: Use `NewEntry`. Type safety outweighs the minor duplication.

**OQ-2: Test infrastructure exposure mechanism.**
AC-19 requires reusable test helpers. Options: (a) `pub mod test_utils` behind `#[cfg(test)]` (only visible within crate), (b) a `test-support` feature flag that exports helpers for downstream crates, (c) a separate `unimatrix-store-test-utils` crate. Option (b) is lightest; option (c) is cleanest for cross-crate use.
**Recommendation**: Start with (b) `test-support` feature flag. Migrate to (c) if test utility scope grows significantly in later milestones.

**OQ-3: Batch fetch optimization.**
Read operations (FR-05.2 through FR-05.7) collect entry IDs from indexes, then batch-fetch from ENTRIES. The batch fetch could be implemented as individual `get()` calls in a loop, or as a range scan with skip logic. Individual gets are simpler; range scans may be faster for large result sets. At Unimatrix scale (1K–100K entries, result sets typically < 100), individual gets are likely sufficient.
**Recommendation**: Implement as individual gets initially. Optimize to range scan only if profiling shows a bottleneck.

---

## 8. Key Specification Decisions

1. **First entry ID is 1, not 0.** ID 0 is reserved as a sentinel value. This avoids ambiguity in `Option<u64>` vs "no entry" scenarios.

2. **`NewEntry` struct for inserts.** Separates caller-provided fields from engine-assigned fields (`id`, `created_at`, `updated_at`). Prevents callers from accidentally supplying IDs.

3. **VECTOR_MAP stores `u64`, not `usize`.** redb requires fixed-width types. hnsw_rs uses `usize` internally. The boundary conversion (`u64 as usize` / `usize as u64`) happens in nxs-002, not in the storage engine.

4. **Engine-managed index diff on update.** Callers provide the full updated `EntryRecord`. The engine reads the old record, identifies which indexed fields changed, and performs the minimal set of index removals and insertions. This prevents orphaned index entries and simplifies the caller's responsibility.

5. **`QueryFilter` defaults to active-only.** An empty filter returns all `Status::Active` entries, matching the most common query pattern (context_lookup/context_search default behavior per ASS-007 INTERFACE.md).

6. **No limit/offset on query results.** At Unimatrix scale (< 100K entries, typical result sets < 100), pagination is unnecessary. Future milestones may add `limit` to `QueryFilter` if needed.

7. **Three status counters, not a single total.** COUNTERS tracks `total_active`, `total_deprecated`, and `total_proposed` separately. This avoids full STATUS_INDEX scans for basic stats (used by `context_status` tool).

8. **`#![forbid(unsafe_code)]` at crate level.** Not just convention — compiler-enforced. Any `unsafe` block in the crate or any future contribution is a compile error.

9. **bincode v2.** Adopted over v1 for better `#[serde(default)]` handling and forward-compatible API. This is a greenfield crate with no legacy bincode v1 data.

10. **Delete operation included.** The primary lifecycle mechanism is status transitions, not deletion. Delete is provided for test cleanup, data corruption recovery, and completeness. The public API includes it but documentation emphasizes status transitions as the standard pattern.
