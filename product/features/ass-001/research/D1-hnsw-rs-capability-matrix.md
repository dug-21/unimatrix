# D1: hnsw_rs Capability Matrix

**Crate**: `hnsw_rs` v0.3.3 | **Repo**: jean-pierreBoth/hnswlib-rs | **License**: MIT/Apache-2.0
**Companion crate**: `anndists` v0.1.4 (distance metrics)
**Rust edition**: 2024 | **Key deps**: rayon, parking_lot, serde, bincode, hashbrown, mmap-rs

---

## Q1: FilterT — Pre-Filtering During Search

**Answer: Yes.** hnsw_rs supports filtering during HNSW graph traversal, not as a post-processing step.

### FilterT Trait

```rust
pub trait FilterT {
    fn hnsw_filter(&self, id: &DataId) -> bool;
}
```

Built-in implementations:
- `Vec<usize>` — sorted allow-list, uses binary search
- `Fn(&DataId) -> bool` — any closure

### Filtered Search API

```rust
pub fn search_filter(
    &self, data: &[T], knbn: usize, ef_arg: usize,
    filter: Option<&dyn FilterT>,
) -> Vec<Neighbour>
```

(`search_possible_filter` is an `#[inline]` alias that delegates to `search_filter`.)

### Filtering Semantics

The filter is applied **during traversal** — candidates that fail the filter are excluded from the result set. However, distance IS computed for filtered-out candidates for graph navigation purposes. This is the standard HNSW filtered-search approach: the filter prevents candidates from entering results, not from being evaluated for routing.

### Interface Design Implications

- `memory_search` CAN take inline filter params (phase, status, tags)
- We implement `FilterT` with a closure that checks our metadata store (redb) against the filter criteria
- No separate filtered-search flow needed — single code path with optional filter
- Filter operates on `DataId` (usize), so we need a fast `DataId -> metadata` lookup (redb)
- No built-in `parallel_search_filter` — we'd use rayon manually if needed

---

## Q2: Search Return Types

### Neighbour Struct

```rust
#[repr(C)]
pub struct Neighbour {
    pub d_id: DataId,       // usize — user-provided origin ID
    pub distance: f32,      // distance to query (always f32)
    pub p_id: PointId,      // internal graph ID (layer, rank) — not needed externally
}
```

Implements `Copy`, `Clone`, `Ord` (ordered by distance ascending).

### Search Signatures

| Method | Signature | Returns |
|--------|-----------|---------|
| `search` | `(&self, data: &[T], knbn: usize, ef_arg: usize)` | `Vec<Neighbour>` |
| `search_filter` | `(&self, data: &[T], knbn: usize, ef_arg: usize, filter: Option<&dyn FilterT>)` | `Vec<Neighbour>` |
| `parallel_search` | `(&self, datas: &[Vec<T>], knbn: usize, ef: usize)` | `Vec<Vec<Neighbour>>` |

Parameters:
- `knbn` — k, max results to return
- `ef_arg` — search beam width (higher = more accurate, slower; should be >= knbn)

### Interface Design Implications

- `memory_search` response maps directly: each `Neighbour.d_id` is our entry ID, `Neighbour.distance` becomes similarity score
- Distance is always `f32` — our response schema should use `f32` for similarity
- Results are pre-sorted by distance ascending (closest first)
- May return fewer than `knbn` results (if index is small or filter is restrictive)
- For cosine distance: similarity = 1.0 - distance (since `DistCosine` returns `1 - cos_sim`)

---

## Q3: Parallel Insertion

**Answer: Yes.** `parallel_insert` works reliably via rayon.

### API

```rust
pub fn parallel_insert(&self, datas: &[(&Vec<T>, usize)])    // Vec references
pub fn parallel_insert_slice(&self, datas: &Vec<(&[T], usize)>)  // slices
```

Takes `&self` (not `&mut self`) — concurrent insertion is safe.

### Thread Safety

- `parking_lot::RwLock` on `PointIndexation` (layers, entry point, point count)
- `Arc<RwLock<...>>` on each point's neighbour lists
- Rayon for work-stealing parallelism

### AnnT Trait Alternative

```rust
fn parallel_insert_data(&mut self, data: &[(&Vec<Self::Val>, usize)]);
```

Note: AnnT takes `&mut self` (type-erased interface), while direct methods take `&self`.

### Interface Design Implications

- `memory_import` (batch store) can use `parallel_insert` for fast bulk loading
- Individual `memory_store` calls use `insert` (also thread-safe via `&self`)
- After bulk insertion, call `set_searching_mode(true)` before searching (requires `&mut self`)

---

## Q4: Persistence — file_dump() and Reload

### Dump

```rust
fn file_dump(&self, path: &Path, file_basename: &str) -> Result<String>
```

Produces two files:
- `{basename}.hnsw.graph` — topology (point IDs, layers, neighbor lists, distances, construction params)
- `{basename}.hnsw.data` — vector data (raw binary, dimension metadata, magic headers)

Returns the actual basename used (may differ if files already exist to avoid overwriting mmap'd files).

Requires: `T: Serialize + DeserializeOwned + Clone + Send + Sync`

### Reload

```rust
// HnswIo struct
pub fn load_hnsw<'b, 'a, T, D>(&'a mut self) -> Result<Hnsw<'b, T, D>>
    where D: Distance<T> + Default + Send + Sync
pub fn load_hnsw_with_dist<'b, 'a, T, D>(&'a self, f: D) -> Result<Hnsw<'b, T, D>>
    where D: Distance<T> + Send + Sync
```

**ReloadOptions:**
- `datamap: bool` — memory-map data file instead of loading into RAM
- `mmap_threshold: usize` — only mmap above N items (default 0 = always mmap)

### Format

Custom binary format (v3/v4), NOT bincode. Raw byte writes via `BufWriter<File>` with magic number headers. This enables memory-mapped access for the data file.

### Atomicity

**NOT atomic.** No write-ahead log, journaling, or temp-file-rename pattern. If interrupted mid-write, files are corrupt. Avoids overwriting existing mmap'd files by generating new random basenames.

### State Preservation

Full state preserved: all vectors, all graph topology, construction parameters, distance metric name, data type name. Index is fully functional for search and further insertion after reload.

### Interface Design Implications

- **redb IS needed for crash-safe metadata** — hnsw_rs persistence is not atomic
- Persistence strategy: hnsw_rs for vector index (dump on clean shutdown), redb for metadata (transactional, crash-safe)
- On startup: reload hnsw_rs index + verify against redb metadata
- Consider: write-to-temp + rename wrapper for safer index persistence
- mmap reload option is valuable for large indexes (reduces startup memory)
- No incremental persistence — full snapshot on every dump

---

## Q5: Dimensionality

**Answer: Fixed by convention, NOT enforced by the library.**

- `data_dimension` field is initialized to `0` in `new()` and marked `#[allow(unused)]`
- No dimension parameter in the constructor
- No validation on insert — mismatched dimensions produce nonsensical distances or panics
- `get_data_dimension()` infers from the entry point vector length (returns 0 if empty)

### Interface Design Implications

- **Switching embedding models (OpenAI 1536d → local 384d) REQUIRES full index rebuild**
- Dimension consistency is OUR responsibility — validate at the Unimatrix layer
- Store expected dimension in redb metadata, validate every insert
- `project_create` should capture embedding model + dimension as immutable config
- Migration path: create new index with new dimension, re-embed and re-insert all entries

---

## Q6: Memory Profile

### Formula

```
memory_per_entry ≈ (4 * D) + (2 * M * 8) + ~80 bytes overhead
```

Where:
- `D` = vector dimension
- `M` = max connections per node (hnsw_rs parameter `max_nb_connection`)
- `4 * D` = vector data (f32)
- `2 * M * 8` = graph connections (layer 0 has 2M neighbors, ~8 bytes each)
- ~80 bytes = Rust struct overhead (Arc, hash entries, RwLock)

### Estimates (M=16)

**384 dimensions (all-MiniLM-L6-v2):**

| Entries | Vector | Graph | Overhead | Total |
|---------|--------|-------|----------|-------|
| 1,000 | 1.5 MB | 0.25 MB | 0.08 MB | **~1.8 MB** |
| 10,000 | 15 MB | 2.5 MB | 0.8 MB | **~18 MB** |
| 100,000 | 150 MB | 25 MB | 8 MB | **~183 MB** |

**1536 dimensions (OpenAI text-embedding-3-small):**

| Entries | Vector | Graph | Overhead | Total |
|---------|--------|-------|----------|-------|
| 1,000 | 6 MB | 0.25 MB | 0.08 MB | **~6.3 MB** |
| 10,000 | 60 MB | 2.5 MB | 0.8 MB | **~63 MB** |
| 100,000 | 600 MB | 25 MB | 8 MB | **~633 MB** |

### Key Observations

- At 384d+, **vector data dominates** (>80% of total memory)
- Graph overhead (M-dependent) is a relatively small fraction at high dimensions
- 100K entries at 1536d = ~633 MB — approaching practical limits for local-first single-user
- 100K entries at 384d = ~183 MB — very manageable
- mmap reload can reduce resident memory significantly for large indexes

### Memory Reduction Options

1. **mmap data on reload** — OS pages in only needed vectors
2. **FlatNeighborhood** — topology-only representation, discards Arc wrappers
3. **NoData reload** — graph without vectors (for topology analysis)
4. **Lower M** — reduces graph memory (M=8 halves graph overhead vs M=16)
5. **Quantization** — not built into hnsw_rs; would need custom implementation

### Interface Design Implications

- Per-project resource limits should be based on entry count + dimension
- 384d local model is strongly preferred for resource reasons (3.5x less memory than 1536d)
- At 100K entries / 384d, ~183 MB is acceptable for local-first
- At 100K entries / 1536d, ~633 MB may warrant warnings or limits
- Consider: `memory_count` or `status` tool should report current memory usage
- Quantization planning is NOT needed for initial versions (< 100K entries)

---

## Q7: Distance Metrics — DistCosine vs DistL2

### Available Metrics

| Struct | Description | SIMD | Best For |
|--------|-------------|------|----------|
| `DistCosine` | `1 - cos_sim(a, b)` | No | General text embeddings |
| `DistDot` | `1 - dot(a, b)` (pre-normalized) | Yes | Pre-normalized text embeddings (fastest) |
| `DistL2` | Squared euclidean | Yes | When you need euclidean |
| `DistL1` | Manhattan | Yes | Sparse-like data |

Full list includes: DistHamming, DistJaccard, DistHellinger, DistJeffreys, DistJensenShannon, DistLevenshtein, NoDist

### Cosine vs L2 for Text Embeddings

For **pre-normalized vectors** (unit length, which most text embedding models produce):
- Cosine distance = `1 - dot(a, b)`
- Squared L2 = `2 - 2 * dot(a, b)` = `2 * cosine_distance`
- **Rankings are identical** — only absolute values differ

### Recommended: DistDot for Text Embeddings

`DistDot` is optimal for pre-normalized text embeddings:
- Computes `1 - dot(a, b)` directly
- Skips norm computation that `DistCosine` performs (already normalized)
- Has SIMD acceleration (AVX2/SSE2 on x86_64)
- **~2-3x faster** than DistCosine per comparison

### Distance Metric Is Fixed at Index Creation

The metric `D` is a generic type parameter on `Hnsw<T, D>`, set at construction time. It's recorded in the dump and must match on reload. Changing the metric requires rebuilding the index.

### Interface Design Implications

- **Distance metric should NOT be user-configurable per-project** — use `DistDot` for all text embeddings
- Require/verify that embedding models produce L2-normalized vectors
- If we ever support non-text data, metric configurability could be added then
- Similarity score in `memory_search` response = `1.0 - distance` (gives 0-1 range where 1 = identical)

---

## Construction Parameters Reference

| Parameter | hnsw_rs Name | Typical Value | Effect |
|-----------|-------------|---------------|--------|
| Max connections | `max_nb_connection` | 16-48 | Higher = better recall, more memory. 16 is good default. |
| Max elements | `max_elements` | N (estimate) | Pre-allocation hint. Over-provisioning wastes memory. |
| Max layers | `max_layer` | 16 | Cap on hierarchy depth. 16 is fine for all practical sizes. |
| Construction ef | `ef_construction` | 200-400 | Higher = better index quality, slower build. No memory impact. |
| Search ef | `ef_arg` (per-query) | 32-200 | Higher = better recall, slower search. Tunable at query time. |
| Level scale | `modify_level_scale` | 0.2-1.0 | Controls layer distribution. Default works well. |

### Recommended Defaults for Unimatrix

```
max_nb_connection: 16     (good recall/memory balance for < 100K entries)
max_layer: 16             (standard, sufficient for millions)
ef_construction: 200      (good quality, fast build)
ef_search: 32-64          (tunable per query; 32 for fast, 64 for thorough)
distance: DistDot         (fastest for normalized text embeddings)
```

---

## Complete API Surface

### Insertion

| Method | Signature | Thread Safety |
|--------|-----------|---------------|
| `insert` | `(&self, (&[T], usize))` | Safe (`&self`, internal locks) |
| `insert_slice` | `(&self, (&[T], usize))` | Safe |
| `parallel_insert` | `(&self, &[(&Vec<T>, usize)])` | Safe (rayon) |
| `parallel_insert_slice` | `(&self, &Vec<(&[T], usize)>)` | Safe (rayon) |

### Search

| Method | Signature | Returns |
|--------|-----------|---------|
| `search` | `(&self, &[T], knbn, ef_arg)` | `Vec<Neighbour>` |
| `search_filter` | `(&self, &[T], knbn, ef_arg, Option<&dyn FilterT>)` | `Vec<Neighbour>` |
| `parallel_search` | `(&self, &[Vec<T>], knbn, ef)` | `Vec<Vec<Neighbour>>` |

### Persistence

| Method | Signature | Notes |
|--------|-----------|-------|
| `file_dump` | `(&self, &Path, &str) -> Result<String>` | Two-file dump |
| `HnswIo::load_hnsw` | `(&mut self) -> Result<Hnsw<T,D>>` | Requires D: Default |
| `HnswIo::load_hnsw_with_dist` | `(&self, D) -> Result<Hnsw<T,D>>` | Explicit distance |

### Configuration

| Method | Signature | Effect |
|--------|-----------|--------|
| `set_searching_mode` | `(&mut self, bool)` | Toggle insert/search mode |
| `set_keeping_pruned` | `(&mut self, bool)` | Keep pruned vectors |
| `set_extend_candidates` | `(&mut self, bool)` | Enforce ef candidates |
| `modify_level_scale` | `(&mut self, f64)` | Adjust layer distribution |

### Info

| Method | Returns | Notes |
|--------|---------|-------|
| `get_nb_point()` | `usize` | Current entry count |
| `get_data_dimension()` | `usize` | Inferred from entry point (0 if empty) |
| `get_max_level_observed()` | `u8` | Highest populated layer |
| `get_ef_construction()` | `usize` | Construction parameter |
| `get_max_nb_connection()` | `u8` | M parameter |
| `dump_layer_info()` | (prints) | Per-layer point distribution |

---

## Known Limitations

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| No atomic persistence | Crash during dump corrupts files | Write-to-temp + rename; redb for crash-safe metadata |
| No point deletion | Cannot remove individual entries | Mark as deprecated in redb metadata; filter from search; rebuild index periodically |
| No incremental persistence | Full dump every time | Acceptable for < 100K entries; dump on clean shutdown only |
| Dimension not validated | Wrong dimension = silent corruption | Validate at Unimatrix layer; store dimension in redb |
| Distance always f32 | Precision loss for f64 vectors | Not an issue for f32 text embeddings |
| No WASM target | Cannot run in browser | Not relevant for local-first MCP server |
| `set_searching_mode` requires `&mut self` | Awkward with shared references | Wrap in RwLock at Unimatrix layer, or call once during initialization |
| Max 16 layers | Limits hierarchy depth | Sufficient for millions of entries |
| Max 256 connections (u8) | Limits M parameter | M=16-48 is optimal anyway |

---

## Ecosystem Context

- **280K total downloads, 90K recent** — actively growing adoption
- **26 versions published** — actively maintained
- **Notable dependents**: aichat (LLM CLI), ruvector-core (vector DB), multiple bioinformatics tools
- **Alternatives**: usearch (C++ FFI, slightly higher downloads but not pure Rust), instant-distance (stale), hora (abandoned)
- **hnsw_rs is the strongest pure-Rust HNSW implementation** for embedding into a Rust project

---

## Design Decisions Enabled by This Research

| Decision | Recommendation | Confidence |
|----------|---------------|------------|
| Filter approach | Single `search_filter` path with closure-based FilterT | High |
| Response schema | Map Neighbour directly: d_id → entry_id, distance → similarity | High |
| Batch import | Use `parallel_insert` with rayon | High |
| Persistence model | hnsw_rs dump for index + redb for metadata (crash-safe) | High |
| Dimension handling | Validate at Unimatrix layer; rebuild index on model change | High |
| Distance metric | `DistDot` for pre-normalized text embeddings | High |
| Default M | 16 (good recall/memory for < 100K entries) | High |
| Configurable metric | No — fix to DistDot for text embeddings | Medium |
| Entry deletion | Metadata-only (filter in search); periodic index rebuild | High |
| Memory limits | Warn at 50K entries (1536d) or 100K entries (384d) | Medium |
