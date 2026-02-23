# nxs-002: Vector Index -- Architecture

## Overview

The Vector Index is a pure-Rust library crate (`unimatrix-vector`) that wraps hnsw_rs to provide vector similarity search for Unimatrix entries. It sits between the storage engine (nxs-001) and the embedding pipeline (nxs-003), consuming pre-computed embeddings and making them searchable via approximate nearest neighbor (ANN) queries.

This crate is synchronous (matching nxs-001's pattern) and is designed to be wrapped by downstream async consumers via `spawn_blocking` with `Arc<VectorIndex>`.

## System Context

```
                    Downstream Consumers

  vnc-002 MCP Tools            nxs-003 Embedding Pipeline
  (context_search,             (ONNX/API, text-to-vec)
   near-dup detection)              |
        |                           | Vec<f32> embeddings
        |                           |
        v                           v
  +---------------------------------------------+
  |         unimatrix-vector (this crate)        |
  |                                              |
  |  +----------+  +-----------+  +-----------+  |
  |  |  index   |  |persistence|  |  filter   |  |
  |  |          |  |           |  |           |  |
  |  |VectorIdx |  |dump/load  |  |EntryIdFilt|  |
  |  |insert    |  |metadata   |  |allow-list |  |
  |  |search    |  |           |  |           |  |
  |  +----------+  +-----------+  +-----------+  |
  |  +----------+  +-----------+                 |
  |  |  config  |  |  error    |                 |
  |  |VectorCfg |  |VectorErr  |                 |
  |  +----------+  +-----------+                 |
  |                                              |
  |          hnsw_rs + anndists                   |
  +---------------------------------------------+
        |                           |
        v                           v
  unimatrix-store              File system
  (VECTOR_MAP, ENTRIES)        (.hnsw.graph, .hnsw.data)
        |
        v
    .redb database
```

## Component Breakdown

### 1. Index Module (`index`)

The core component. Manages the hnsw_rs index and coordinates with unimatrix-store for VECTOR_MAP persistence.

**Primary Type: `VectorIndex`**

```rust
pub struct VectorIndex {
    hnsw: RwLock<Hnsw<'static, f32, DistDot>>,
    store: Arc<Store>,
    config: VectorConfig,
    next_data_id: AtomicU64,
    id_map: RwLock<IdMap>,
}
```

**`IdMap` (internal)**

```rust
struct IdMap {
    data_to_entry: HashMap<u64, u64>,   // hnsw data_id -> entry_id
    entry_to_data: HashMap<u64, u64>,   // entry_id -> hnsw data_id
}
```

The bidirectional map enables:
- Forward lookup: entry_id -> data_id (for building FilterT allow-lists)
- Reverse lookup: data_id -> entry_id (for mapping search results back to entries)

Both directions are maintained atomically on insert and load.

**Public API:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `new` | `(store: Arc<Store>, config: VectorConfig) -> Result<Self>` | Create empty index |
| `insert` | `(&self, entry_id: u64, embedding: &[f32]) -> Result<()>` | Insert vector + VECTOR_MAP |
| `search` | `(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>>` | Unfiltered ANN search |
| `search_filtered` | `(&self, query: &[f32], top_k: usize, ef_search: usize, allowed: &[u64]) -> Result<Vec<SearchResult>>` | Filtered ANN search |
| `point_count` | `(&self) -> usize` | Current index size |
| `contains` | `(&self, entry_id: u64) -> bool` | Check if entry has a vector |

**`SearchResult`:**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f32,    // 0.0 to 1.0, higher = more similar
}
```

Results are sorted by similarity descending (most similar first).

### 2. Persistence Module (`persistence`)

Handles dump and load of the hnsw_rs index files plus coordination with VECTOR_MAP.

**Public API:**

| Method | Signature | Purpose |
|--------|-----------|---------|
| `dump` | `(&self, dir: &Path) -> Result<()>` | Persist index to `.hnsw.graph` + `.hnsw.data` |
| `load` | `(store: Arc<Store>, config: VectorConfig, dir: &Path) -> Result<VectorIndex>` | Reload index + rebuild IdMap from VECTOR_MAP |

**Dump flow:**
1. Acquire read lock on `Hnsw`.
2. Call `hnsw.file_dump(dir, "unimatrix")`.
3. Record the actual basename returned (hnsw_rs may rename to avoid overwriting mmap'd files).
4. Write a metadata file (`unimatrix-vector.meta`) with: basename, point_count, dimension, data_id_counter.

**Load flow:**
1. Read metadata file to recover basename and data_id_counter.
2. Create `HnswIo` from the graph + data files.
3. Call `load_hnsw_with_dist(DistDot)` to reconstruct the `Hnsw` instance.
4. Iterate VECTOR_MAP in redb to rebuild `IdMap`.
5. Set `next_data_id` from the persisted counter.
6. Return `VectorIndex` ready for search.

**Metadata file format (`unimatrix-vector.meta`):**

A simple line-based text file:
```
basename=unimatrix
point_count=1234
dimension=384
next_data_id=1235
```

### 3. Filter Module (`filter`)

Implements hnsw_rs `FilterT` for metadata-filtered semantic search.

**Type: `EntryIdFilter`**

```rust
pub(crate) struct EntryIdFilter {
    allowed_data_ids: Vec<usize>,    // sorted for binary search
}

impl FilterT for EntryIdFilter {
    fn hnsw_filter(&self, id: &DataId) -> bool {
        self.allowed_data_ids.binary_search(id).is_ok()
    }
}
```

**Construction:** The caller provides `&[u64]` (entry IDs). The filter module translates these to hnsw data IDs via `IdMap.entry_to_data`, collects into a `Vec<usize>`, sorts it, and wraps in `EntryIdFilter`. Unknown entry IDs (those without vectors) are silently skipped.

Note: hnsw_rs already implements `FilterT` for `Vec<usize>` with binary search. We could use `Vec<usize>` directly instead of a custom struct. The custom struct is preferred for type safety and future extensibility (e.g., adding closure-based filters), but either approach is acceptable.

### 4. Config Module (`config`)

Holds configuration for the vector index.

```rust
#[derive(Debug, Clone)]
pub struct VectorConfig {
    /// Embedding dimension. Default: 384 (all-MiniLM-L6-v2).
    pub dimension: usize,
    /// Max connections per HNSW node. Default: 16.
    pub max_nb_connection: usize,
    /// Construction beam width. Default: 200.
    pub ef_construction: usize,
    /// Pre-allocation hint for max elements. Default: 10_000.
    pub max_elements: usize,
    /// Max HNSW layers. Default: 16.
    pub max_layer: usize,
    /// Default search ef (overridable per-query). Default: 32.
    pub default_ef_search: usize,
}
```

All fields have sensible defaults. The `Default` impl uses values from the product vision and spike research.

### 5. Error Module (`error`)

```rust
#[derive(Debug)]
pub enum VectorError {
    /// Embedding dimension does not match expected (384).
    DimensionMismatch { expected: usize, got: usize },
    /// Error from the underlying store (VECTOR_MAP operations).
    Store(unimatrix_store::StoreError),
    /// Error during index persistence (dump/load).
    Persistence(String),
    /// Index is empty -- cannot search.
    EmptyIndex,
    /// Entry not found in the index.
    EntryNotInIndex(u64),
    /// hnsw_rs internal error.
    Index(String),
}
```

`VectorError` implements `std::error::Error`, `Display`, and `From<StoreError>` for ergonomic `?` propagation.

## Component Interactions

### Insert Data Flow

```
Caller                  VectorIndex              hnsw_rs          Store
  |                         |                       |               |
  | insert(entry_id, emb)  |                       |               |
  |------------------------>|                       |               |
  |                         | validate dim == 384   |               |
  |                         | gen data_id (atomic)  |               |
  |                         |                       |               |
  |                         | write_lock hnsw       |               |
  |                         | insert(emb, data_id)  |               |
  |                         |---------------------->|               |
  |                         | release write_lock    |               |
  |                         |                       |               |
  |                         | put_vector_mapping    |               |
  |                         | (entry_id, data_id)   |               |
  |                         |-------------------------------------->|
  |                         |                       |               |
  |                         | update IdMap          |               |
  |                         |                       |               |
  | Ok(())                  |                       |               |
  |<------------------------|                       |               |
```

### Search Data Flow

```
Caller                  VectorIndex              hnsw_rs          IdMap
  |                         |                       |               |
  | search(query, k, ef)   |                       |               |
  |------------------------>|                       |               |
  |                         | validate dim == 384   |               |
  |                         |                       |               |
  |                         | read_lock hnsw        |               |
  |                         | search(query, k, ef)  |               |
  |                         |---------------------->|               |
  |                         | Vec<Neighbour>        |               |
  |                         |<----------------------|               |
  |                         | release read_lock     |               |
  |                         |                       |               |
  |                         | read_lock IdMap       |               |
  |                         | for each Neighbour:   |               |
  |                         |   data_id -> entry_id |               |
  |                         |-------------------------------------->|
  |                         |   similarity = 1-dist |               |
  |                         | release read_lock     |               |
  |                         |                       |               |
  | Vec<SearchResult>       |                       |               |
  |<------------------------|                       |               |
```

### Filtered Search Data Flow

```
Caller                  VectorIndex              hnsw_rs
  |                         |                       |
  | search_filtered(        |                       |
  |   query, k, ef,         |                       |
  |   allowed_entry_ids)    |                       |
  |------------------------>|                       |
  |                         | validate dim == 384   |
  |                         |                       |
  |                         | translate entry_ids   |
  |                         |   to data_ids via     |
  |                         |   IdMap.entry_to_data |
  |                         | sort -> Vec<usize>    |
  |                         | wrap in EntryIdFilter |
  |                         |                       |
  |                         | read_lock hnsw        |
  |                         | search_filter(        |
  |                         |   query, k, ef,       |
  |                         |   Some(&filter))      |
  |                         |---------------------->|
  |                         | Vec<Neighbour>        |
  |                         |<----------------------|
  |                         | release read_lock     |
  |                         |                       |
  |                         | map results via IdMap |
  |                         |                       |
  | Vec<SearchResult>       |                       |
  |<------------------------|                       |
```

### Persistence Data Flow

```
VectorIndex                 hnsw_rs               File System       Store
  |                            |                      |               |
  | dump(dir)                  |                      |               |
  | read_lock hnsw             |                      |               |
  | file_dump(dir, "unimatrix")|                      |               |
  |--------------------------->|                      |               |
  |                            | write .hnsw.graph    |               |
  |                            | write .hnsw.data     |               |
  |                            |--------------------->|               |
  | actual_basename            |                      |               |
  |<---------------------------|                      |               |
  | release read_lock          |                      |               |
  |                            |                      |               |
  | write metadata file        |                      |               |
  |---------------------------------------------->|               |
  | Ok(())                     |                      |               |


VectorIndex::load(store, config, dir)
  |                            |                      |               |
  | read metadata file         |                      |               |
  |<----------------------------------------------|               |
  |                            |                      |               |
  | HnswIo::new(dir, basename)|                      |               |
  | load_hnsw_with_dist(DistDot)                     |               |
  |--------------------------->|                      |               |
  | Hnsw<f32, DistDot>        |                      |               |
  |<---------------------------|                      |               |
  |                            |                      |               |
  | iterate VECTOR_MAP         |                      |               |
  |-------------------------------------------------------------->|
  | rebuild IdMap              |                      |               |
  |<--------------------------------------------------------------|
  |                            |                      |               |
  | VectorIndex ready          |                      |               |
```

## Technology Decisions

| Technology | Decision | Rationale | ADR |
|------------|----------|-----------|-----|
| hnsw_rs v0.3.3 | ANN index library | Pure Rust, persistence, FilterT, SIMD, production-validated (aichat). See ADR-001. | [ADR-001](ADR-001-hnsw-rs-vector-index.md) |
| DistDot | Distance metric | 2-3x faster than DistCosine for pre-normalized embeddings. SIMD-accelerated. See ADR-002. | [ADR-002](ADR-002-distdot-distance-metric.md) |
| RwLock wrapper | Concurrency model | hnsw_rs `insert`/`search` take `&self` but `set_searching_mode` takes `&mut self`. RwLock provides safe concurrent reads with exclusive mode transitions. See ADR-003. | [ADR-003](ADR-003-rwlock-concurrency-model.md) |
| Bidirectional IdMap | ID mapping strategy | Entry IDs and hnsw data IDs are independent. Bidirectional map enables O(1) lookup in both directions. Rebuilt from VECTOR_MAP on load. See ADR-004. | [ADR-004](ADR-004-bidirectional-id-map.md) |
| Synchronous API | Concurrency boundary | Matches nxs-001 pattern. No tokio dependency. Consumers wrap with `spawn_blocking`. See nxs-001 ADR-004. | nxs-001 ADR-004 |

## Cargo Workspace Integration

```
/                               Cargo workspace root
+-- Cargo.toml                  [workspace] members = ["crates/*"]
+-- crates/
|   +-- unimatrix-store/        nxs-001: storage engine (existing)
|   +-- unimatrix-vector/       nxs-002: vector index (this crate)
|       +-- Cargo.toml
|       +-- src/
|           +-- lib.rs
|           +-- index.rs
|           +-- persistence.rs
|           +-- filter.rs
|           +-- config.rs
|           +-- error.rs
+-- (future crates)
    +-- unimatrix-core/         nxs-004: storage traits
    +-- unimatrix-embed/        nxs-003: embedding pipeline
    +-- unimatrix-mcp/          vnc-001: MCP server
```

**unimatrix-vector Cargo.toml:**

```toml
[package]
name = "unimatrix-vector"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = ["unimatrix-store/test-support"]

[dependencies]
unimatrix-store = { path = "../unimatrix-store" }
hnsw_rs = { version = "0.3", features = ["simdeez_f"] }
anndists = "0.1"

[dev-dependencies]
tempfile = "3"
rand = "0.9"
```

**Workspace Cargo.toml additions:**

```toml
[workspace.dependencies]
# existing:
redb = "3.1"
serde = { version = "1", features = ["derive"] }
bincode = { version = "2", features = ["serde"] }
# new:
hnsw_rs = { version = "0.3", features = ["simdeez_f"] }
anndists = "0.1"
```

Note: `hnsw_rs` and `anndists` may be added as workspace dependencies or as direct crate dependencies. Either pattern is acceptable. The direct crate dependency approach is simpler since only `unimatrix-vector` uses these.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `Store::put_vector_mapping(entry_id: u64, hnsw_data_id: u64) -> Result<()>` | Store method | unimatrix-store write.rs |
| `Store::get_vector_mapping(entry_id: u64) -> Result<Option<u64>>` | Store method | unimatrix-store read.rs |
| `Store::exists(entry_id: u64) -> Result<bool>` | Store method | unimatrix-store read.rs |
| `Store::get(entry_id: u64) -> Result<EntryRecord>` | Store method | unimatrix-store read.rs |
| `Hnsw::new(max_nb_connection, max_elements, max_layer, ef_construction, dist)` | hnsw_rs constructor | hnsw_rs lib |
| `Hnsw::insert_slice(&self, (&[f32], usize))` | hnsw_rs insert | hnsw_rs lib |
| `Hnsw::search_filter(&self, &[f32], knbn, ef_arg, Option<&dyn FilterT>) -> Vec<Neighbour>` | hnsw_rs search | hnsw_rs lib |
| `Hnsw::set_searching_mode(&mut self, bool)` | hnsw_rs mode | hnsw_rs lib |
| `Hnsw::file_dump(&self, &Path, &str) -> Result<String>` | hnsw_rs persist | hnsw_rs lib |
| `Hnsw::get_nb_point() -> usize` | hnsw_rs info | hnsw_rs lib |
| `HnswIo::load_hnsw_with_dist(&self, D) -> Result<Hnsw<T,D>>` | hnsw_rs reload | hnsw_rs lib |
| `FilterT::hnsw_filter(&self, &DataId) -> bool` | hnsw_rs trait | hnsw_rs lib |
| `Neighbour { d_id: DataId, distance: f32, p_id: PointId }` | hnsw_rs result | hnsw_rs lib |

### Downstream Integration (vnc-002 MCP Tools)

vnc-002 will use `VectorIndex` for:

1. **`context_search`**: Call `search(query_embedding, top_k, ef_search)` or `search_filtered(...)` with metadata-filtered allow-list.
2. **Near-duplicate detection**: Call `search(new_embedding, 1, ef_search)` and check if `similarity >= 0.92`.
3. **Result hydration**: After search, use `Store::get(entry_id)` to fetch full `EntryRecord` for each result.

### Downstream Integration (nxs-003 Embedding Pipeline)

nxs-003 produces `Vec<f32>` embeddings and calls `VectorIndex::insert(entry_id, &embedding)`. The insert flow is:
1. nxs-003 computes embedding from `EntryRecord.title + EntryRecord.content`.
2. nxs-003 calls `vector_index.insert(entry_id, &embedding)`.
3. nxs-002 handles hnsw_rs insertion + VECTOR_MAP update.

### Downstream Integration (vnc-001 MCP Server)

vnc-001 wraps `VectorIndex` for async access:

```rust
let vector_index = Arc::new(VectorIndex::new(store.clone(), config)?);

// In MCP tool handler (async context):
let vi = vector_index.clone();
let results = tokio::task::spawn_blocking(move || {
    vi.search(&query_embedding, top_k, ef_search)
}).await??;

// Graceful shutdown:
let vi = vector_index.clone();
tokio::task::spawn_blocking(move || {
    vi.dump(&index_dir)
}).await??;
store_handle.compact()?;
```

## Error Handling Strategy

1. **No panics in public API.** Every public function returns `Result<T, VectorError>`.
2. **Dimension validation at boundary.** Every `insert` and `search` call validates the embedding dimension before touching hnsw_rs.
3. **Store errors propagate.** `VectorError::Store(StoreError)` wraps all unimatrix-store errors with `From` impl.
4. **Empty index is not an error for search.** Searching an empty index returns an empty `Vec<SearchResult>`, not an error. This enables graceful startup before any vectors are inserted.
5. **Persistence errors are recoverable.** If dump fails, the in-memory index is still valid. If load fails, the caller can create a new empty index and trigger a re-embedding pass.

## Concurrency Model

**RwLock on Hnsw:**
- Read lock: `search` and `search_filtered` (concurrent readers, non-blocking).
- Write lock: `insert` (exclusive access for hnsw_rs insert + potential mode transition).
- Write lock: `dump` (exclusive for file_dump to avoid concurrent modification during serialization). Note: actually `file_dump` takes `&self`, so a read lock suffices for dump. Write lock is only needed for `set_searching_mode`.

**RwLock on IdMap:**
- Read lock: result mapping in search, filter construction in search_filtered.
- Write lock: insert (add new entry to both maps).

**AtomicU64 for next_data_id:**
- Lock-free increment via `fetch_add(1, Ordering::Relaxed)`. No contention.

**Sharing pattern:**

`VectorIndex` is `Send + Sync` (all interior state is behind locks or atomic). Downstream consumers wrap in `Arc<VectorIndex>`:

```rust
let vi: Arc<VectorIndex> = Arc::new(VectorIndex::new(store, config)?);
// Clone Arc for concurrent access in spawn_blocking
```

## Open Questions

1. **Hnsw lifetime parameter.** `Hnsw<'static, f32, DistDot>` uses `'static` because the index outlives any individual operation. This requires that all data inserted (embeddings) is owned, not borrowed. Verify this works with hnsw_rs API during implementation.

2. **VECTOR_MAP iteration for IdMap rebuild.** unimatrix-store does not currently expose a "scan all VECTOR_MAP entries" method. The `load` path needs this. Options: (a) add a `Store::iter_vector_mappings()` method, or (b) add a public method that returns the redb `ReadTransaction` for direct table access. Recommendation: (a) -- add a dedicated method to unimatrix-store.

3. **Stale point tracking.** When an entry is re-embedded, the old data_id remains in hnsw_rs but is no longer the "current" mapping. The IdMap correctly tracks only the latest mapping, so search results will use the latest entry_id. But the old point still exists in hnsw_rs memory. Track stale count via `point_count() - id_map.len()`.
