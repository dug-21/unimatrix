# D2: redb Storage Pattern Guide

**Crate**: `redb` v3.1.0 | **Repo**: cberner/redb | **License**: MIT/Apache-2.0
**Engine**: Copy-on-write B-tree | **ACID**: Full | **Pure Rust**: Yes (zero required deps)
**Downloads**: ~3M total, ~327K/month | **Stars**: ~4,200 | **Dependents**: 303 crates
**MSRV**: Rust 1.89 | **Edition**: 2024

---

## Q1: Range Queries on Timestamps

**Answer: Yes.** redb stores keys in sorted B-tree order. Range queries are efficient and first-class.

### Range Query API

```rust
fn range<'a, KR>(&self, range: impl RangeBounds<KR> + 'a) -> Result<Range<'_, K, V>>
```

Accepts all Rust range types: `start..end`, `start..=end`, `start..`, `..end`, `..`.

`Range` implements both `Iterator` and `DoubleEndedIterator` — forward, backward, and reverse iteration all work.

### u64 Timestamps as Keys

`u64` implements `Key` with natural numeric ordering. Unix timestamps work directly:

```rust
const ENTRIES: TableDefinition<u64, &[u8]> = TableDefinition::new("entries");

let table = read_txn.open_table(ENTRIES)?;
for item in table.range(ts_start..=ts_end)? {
    let (key, val) = item?;
    let timestamp: u64 = key.value();
}
```

### Compound Tuple Keys

Tuples up to 12 elements implement `Key` with **lexicographic ordering**. This enables prefix-scan patterns:

```rust
const BY_TIME: TableDefinition<(u64, u64), ()> = TableDefinition::new("by_time");

// All entries in a time window, ordered by (timestamp, entry_id)
for item in table.range((ts_start, 0u64)..=(ts_end, u64::MAX))? {
    let (key, _) = item?;
    let (timestamp, entry_id) = key.value();
}
```

### Convenience Methods

| Method | Returns | Notes |
|--------|---------|-------|
| `first()` | Lowest key entry | O(log n) |
| `last()` | Highest key entry | O(log n) |
| `len()` | Entry count | **O(1)** |
| `iter()` | Full table iterator | Double-ended |

### No Built-In Secondary Indexes

redb has no secondary index mechanism. For multiple access patterns (by time, by tag, by status), maintain **separate tables** updated atomically within a single write transaction.

### Interface Design Implications

- `memory_list` with "entries since X" maps directly to `range(timestamp..)` on a time index table
- Compound keys `(u64_timestamp, u64_entry_id)` give ordered time-series access
- Tag-based filtering uses `MultimapTable<&str, u64>` (tag → entry IDs)
- Status-based filtering uses `TableDefinition<(u8, u64), ()>` (status+id compound key)
- All index tables updated atomically in one write transaction

---

## Q2: Multiple Named Tables in One DB

**Answer: One DB file with multiple named tables is the correct and recommended pattern.**

### Table Definition

Tables are compile-time typed constants:

```rust
const ENTRIES: TableDefinition<u64, &[u8]> = TableDefinition::new("entries");
const TAGS: MultimapTableDefinition<&str, u64> = MultimapTableDefinition::new("tags");
const LIFECYCLE: TableDefinition<u64, u8> = TableDefinition::new("lifecycle");
```

- Each table has independent key/value types
- No limit on number of tables
- `open_table()` creates the table on first use (in write transactions)
- `list_tables()` / `list_multimap_tables()` for dynamic discovery

### Why One DB File

1. **Atomic cross-table updates** — single `WriteTransaction` covers all tables
2. **Single file lock** — simpler concurrency model
3. **Shared via `Arc<Database>`** — multiple components access the same DB
4. **MVCC spans all tables** — readers see consistent snapshots across tables

### Two Table Types

| Type | Definition | Key→Value |
|------|-----------|-----------|
| `TableDefinition<K, V>` | Regular table | One key → one value |
| `MultimapTableDefinition<K, V>` | Multimap table | One key → many values (sorted set) |

Note: Multimap values must implement `Key` (not just `Value`) for ordering.

---

## Q3: Transaction Model

**Answer: Single writer + unlimited concurrent readers, MVCC with serializable isolation.**

### Concurrency Model

| Property | Behavior |
|----------|----------|
| Concurrent readers | Unlimited, non-blocking |
| Concurrent writers | **One at a time** (second `begin_write()` blocks until first completes) |
| Reader-writer blocking | **None** — readers never block writers, writers never block readers |
| Isolation | Serializable (strongest) |
| Read snapshots | Yes — readers see DB state as of `begin_read()` call |

### Write Transactions

```rust
let write_txn = db.begin_write()?;  // blocks if another write active
{
    let mut t1 = write_txn.open_table(ENTRIES)?;
    let mut t2 = write_txn.open_multimap_table(TAGS)?;
    t1.insert(id, data)?;
    t2.insert("rust", id)?;
}
write_txn.commit()?;  // atomic, durable (fsync by default)
```

**Drop without commit = automatic abort.** Uncommitted writes are never visible.

### Durability

```rust
pub enum Durability {
    Immediate,  // Default. fsync on commit. Survives crashes.
    None,       // Skip fsync. Rolls back to last durable commit on crash.
}
```

### Crash Recovery

redb uses **COW B-trees with double-buffered commit slots** — no WAL needed. Two strategies:

| Strategy | Fsyncs | Recovery | Default |
|----------|--------|----------|---------|
| 1-Phase + Checksum | 1 | Slow (full scan) | Yes |
| 2-Phase Commit | 2 | Fast (instant) | No |

Enable 2PC with `write_txn.set_two_phase_commit(true)`.

### Savepoints

```rust
let sp = write_txn.ephemeral_savepoint()?;    // lost on DB close
let sp_id = write_txn.persistent_savepoint()?; // survives restarts
write_txn.restore_savepoint(&sp)?;              // roll back to savepoint
```

Savepoint creation/restoration is **O(1)**.

### Interface Design Implications

- Concurrent MCP search + insert is fully safe: reads (search) never block writes (store)
- Write serialization is fine — metadata writes are fast (sub-ms at our scale)
- Use `Durability::Immediate` (default) for metadata — crash safety is the whole point
- Savepoints could support "dry run" operations if needed
- Wrap in `tokio::task::spawn_blocking` for async compatibility

---

## Q4: Typed Tables and Structured Metadata

**Answer: Rich type system with multiple approaches for structured data.**

### Built-In Key/Value Types

| Category | Types |
|----------|-------|
| Integers | `u8`–`u128`, `i8`–`i128` |
| Floats | `f32`, `f64` |
| Strings | `&str`, `String` |
| Bytes | `&[u8]`, `Vec<u8>`, `[u8; N]` |
| Tuples | Up to 12 elements where each element implements Key/Value |
| Arrays | `[T; N]` where T: Key/Value |
| Option | `Option<T>` where T: Key/Value |
| Unit | `()` (useful for set-like tables) |

### Storing Structured Metadata

**Three approaches, from simplest to most flexible:**

**1. Tuple values (simplest for small schemas):**
```rust
// (created_at, updated_at, status, confidence)
const META: TableDefinition<u64, (u64, u64, u8, u32)> = TableDefinition::new("meta");
```

**2. redb_derive macros (for proper structs):**
```rust
use redb_derive::{Key, Value};

#[derive(Debug, Key, Value, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct EntryMetadata {
    created_at: u64,
    updated_at: u64,
    status: u8,
    confidence: u32,
}
const META: TableDefinition<u64, EntryMetadata> = TableDefinition::new("meta");
```

**3. Serde serialization (most flexible):**
```rust
// Serialize via bincode/postcard, store as &[u8]
const META: TableDefinition<u64, &[u8]> = TableDefinition::new("meta");
```

### Custom Key/Value Implementations

Implement the `Value` trait directly:
```rust
pub trait Value: Debug {
    type SelfType<'a>: Debug where Self: 'a;
    type AsBytes<'a>: AsRef<[u8]> + 'a where Self: 'a;
    fn fixed_width() -> Option<usize>;
    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>;
    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>;
    fn type_name() -> TypeName;
}
```

For `Key`, additionally implement `fn compare(data1: &[u8], data2: &[u8]) -> Ordering`.

### AccessGuard Pattern

All reads return `AccessGuard<V>` for zero-copy access:
```rust
let guard: Option<AccessGuard<'_, &str>> = table.get("key")?;
if let Some(g) = guard {
    let value: &str = g.value();  // zero-copy borrow
}
// data released when guard drops
```

### Interface Design Implications

- Use **serde + bincode** for the main entry metadata (most flexible, supports schema evolution)
- Use **tuple keys** for index tables (compound key ordering)
- Use **MultimapTable** for tag indexes
- `redb_derive` is available but less flexible than serde for evolving schemas
- `type_name()` provides runtime schema validation — catches type mismatches on open

---

## Q5: Practical Size Limits

**Answer: No practical size limits at Unimatrix scale. Performance concerns start in the multi-GB range.**

### Benchmarks Summary (Ryzen 9950X3D, NVMe)

| Operation | redb | Best Competitor | Notes |
|-----------|------|----------------|-------|
| Individual writes | **920ms** | lmdb: 1,598ms | redb wins — lowest per-txn fsync overhead |
| Batch writes | 1,595ms | fjall: 353ms | redb loses — 4.5x slower |
| Random reads (1 thread) | 1,138ms | lmdb: 637ms | redb ~1.8x slower |
| Random reads (32 threads) | 410ms | lmdb: 125ms | redb ~3.3x slower, but good scaling |
| Range reads | 1,174ms | lmdb: 565ms | redb ~2x slower |
| Removals | 23,297ms | fjall: 6,004ms | redb loses — slowest |
| File size (compacted) | 1.69 GiB | rocksdb: 455 MiB | redb loses — largest |

### At Unimatrix Scale (1K-100K entries, ~50 MB data)

| Metric | Assessment |
|--------|-----------|
| Read latency | Sub-millisecond (B-tree depth 2-3) |
| Write latency | Sub-millisecond per individual transaction |
| File size | 50-200 MB (with periodic compaction) |
| Memory | Entire DB fits in default 1 GiB cache |
| B-tree depth | 2-3 levels at 100K entries |

**None of redb's weaknesses matter at this scale.** The benchmarks above are for datasets orders of magnitude larger.

### File Size Management

- **No automatic compaction** — file grows over time due to COW
- `Database::compact()` reclaims space (~58% reduction in benchmarks)
- Minimum file size: ~50 KiB (v3 format)
- Recommend: compact on server shutdown or periodically

### Known Issues (All Fixed)

- Slow DB open on macOS for large databases (#386) — not relevant at our scale
- Non-durable transaction file growth — fixed in v3.0.0
- Slow aborts on large writes — fixed in v3.0.2

### Interface Design Implications

- No size warnings needed until well beyond 100K entries
- `status` tool could report DB file size and entry count
- Compact on clean shutdown (cheap, ensures small file)
- Default 1 GiB cache is overkill — reduce to 64-128 MB to save memory

---

## Async (Tokio) Integration

redb has a **synchronous API only**. Standard pattern for Tokio:

```rust
let db = Arc::new(Database::create("unimatrix.redb")?);

// Read path (non-blocking MVCC — safe to run many concurrently)
let db_clone = db.clone();
let result = tokio::task::spawn_blocking(move || {
    let txn = db_clone.begin_read()?;
    let table = txn.open_table(ENTRIES)?;
    table.get(key)?.map(|g| g.value().to_vec())
}).await??;

// Write path (serialized — only one at a time)
let db_clone = db.clone();
tokio::task::spawn_blocking(move || {
    let txn = db_clone.begin_write()?;
    {
        let mut table = txn.open_table(ENTRIES)?;
        table.insert(key, value)?;
    }
    txn.commit()
}).await??;
```

This pattern is **production-proven** — Iroh (p2p data sync) uses exactly this approach.

`Database` is `Send + Sync`, shareable via `Arc<Database>`.

---

## Recommended Table Layout for Unimatrix

```rust
use redb::{Database, TableDefinition, MultimapTableDefinition};

// === Primary Storage ===

// Entry metadata: entry_id -> serialized metadata (bincode)
// Contains: content, category, phase, status, confidence, timestamps, content_hash
const ENTRIES: TableDefinition<u64, &[u8]> = TableDefinition::new("entries");

// === Index Tables ===

// Time index: (created_at_ts, entry_id) -> ()
// Enables: "entries since X" range queries for memory_list
const TIME_INDEX: TableDefinition<(u64, u64), ()> = TableDefinition::new("time_idx");

// Tag index: tag_string -> entry_id (one-to-many)
// Enables: "entries with tag X" lookups for memory_search filter
const TAG_INDEX: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("tag_idx");

// Status index: (status_u8, entry_id) -> ()
// Enables: "all active entries", "all deprecated entries" for lifecycle queries
const STATUS_INDEX: TableDefinition<(u8, u64), ()> =
    TableDefinition::new("status_idx");

// Phase index: (phase_u8, entry_id) -> ()
// Enables: "entries in architecture phase" for memory_search phase filter
const PHASE_INDEX: TableDefinition<(u8, u64), ()> =
    TableDefinition::new("phase_idx");

// === Cross-Reference ===

// Vector mapping: entry_id -> hnsw_data_id
// Bridges redb metadata to hnsw_rs vector index
const VECTOR_MAP: TableDefinition<u64, u64> = TableDefinition::new("vector_map");

// === Sequence Generator ===

// Counter: "next_entry_id" -> u64
// Monotonically increasing entry ID generator
const COUNTERS: TableDefinition<&str, u64> = TableDefinition::new("counters");
```

### Write Pattern (Atomic Multi-Table Insert)

```rust
let write_txn = db.begin_write()?;
{
    // Get next ID
    let mut counters = write_txn.open_table(COUNTERS)?;
    let next_id = counters.get("next_entry_id")?.map(|g| g.value()).unwrap_or(0);
    counters.insert("next_entry_id", next_id + 1)?;

    // Insert entry
    let mut entries = write_txn.open_table(ENTRIES)?;
    entries.insert(next_id, serialized_metadata)?;

    // Update indexes
    let mut time_idx = write_txn.open_table(TIME_INDEX)?;
    time_idx.insert((timestamp, next_id), ())?;

    let mut tag_idx = write_txn.open_multimap_table(TAG_INDEX)?;
    for tag in tags {
        tag_idx.insert(tag.as_str(), next_id)?;
    }

    let mut status_idx = write_txn.open_table(STATUS_INDEX)?;
    status_idx.insert((STATUS_ACTIVE, next_id), ())?;

    let mut vector_map = write_txn.open_table(VECTOR_MAP)?;
    vector_map.insert(next_id, hnsw_data_id)?;
}
write_txn.commit()?;  // ALL tables updated atomically
```

### Read Pattern (FilterT Integration with hnsw_rs)

```rust
// Build filter from redb for hnsw_rs search
let read_txn = db.begin_read()?;
let status_idx = read_txn.open_table(STATUS_INDEX)?;

// Collect active entry IDs into a sorted Vec for FilterT
let active_ids: Vec<usize> = status_idx
    .range((STATUS_ACTIVE, 0u64)..=(STATUS_ACTIVE, u64::MAX))?
    .map(|item| {
        let (key, _) = item.unwrap();
        let (_, entry_id) = key.value();
        entry_id as usize
    })
    .collect();

// Pass to hnsw_rs as FilterT (Vec<usize> implements FilterT via binary_search)
let results = hnsw.search_filter(&query_vec, k, ef, Some(&active_ids));
```

---

## Notable Details

### Drop Safety
WriteTransaction auto-aborts on drop if not committed. This means panics, early returns, and `?` operator all safely roll back.

### File Format Stability
v3.x format is stable. Databases created with v3.0.0 work with v3.1.0. The `Database::upgrade()` method handles v2→v3 migration.

### Compaction Strategy
```rust
// On clean shutdown:
db.compact()?;  // rewrites file, reclaims freed pages
drop(db);       // closes file
```

### Cache Tuning
```rust
let db = Database::builder()
    .set_cache_size(64 * 1024 * 1024)  // 64 MB (plenty for <100K entries)
    .create("unimatrix.redb")?;
```

Default 1 GiB cache is overkill. 64-128 MB is sufficient for Unimatrix's expected data size.

---

## Design Decisions Enabled by This Research

| Decision | Recommendation | Confidence |
|----------|---------------|------------|
| Storage layout | Single redb file, multiple named tables | High |
| Index strategy | Manual multi-table indexes, atomic updates | High |
| Metadata format | Serde + bincode stored as `&[u8]` values | High |
| Timestamp queries | Compound tuple keys `(u64, u64)` with range scans | High |
| Tag storage | MultimapTable for one-to-many relationships | High |
| Async integration | `tokio::task::spawn_blocking` with `Arc<Database>` | High |
| Crash safety | Default `Durability::Immediate`, no extra work needed | High |
| Cache size | 64-128 MB (reduce from 1 GiB default) | Medium |
| Compaction | Manual on clean shutdown | High |
| hnsw_rs bridge | VECTOR_MAP table mapping entry_id → hnsw_data_id | High |
| FilterT integration | Build sorted `Vec<usize>` from redb index, pass to hnsw_rs | High |
