# nxs-002: Vector Index

## Problem Statement

Unimatrix has a storage engine (nxs-001) that can persist and query entries by metadata (topic, category, tags, status, time range), but has no semantic search capability. Agents need to find contextually relevant knowledge using natural language queries, not just exact metadata matches. Without a vector similarity index, the `context_search` tool (vnc-002) cannot function, and Unimatrix's core value proposition -- delivering the right context to the right agent at the right moment -- remains unrealized.

nxs-002 bridges entry storage and semantic retrieval by integrating hnsw_rs as a vector similarity search index. It takes pre-computed embedding vectors (produced by the downstream nxs-003 embedding pipeline) and makes them searchable, while coordinating with the redb-backed VECTOR_MAP table that nxs-001 already provides.

## Goals

1. Create a `unimatrix-vector` library crate that wraps hnsw_rs (`Hnsw<f32, DistDot>`) with 384-dimension embeddings, 16 max connections, and ef_construction=200.
2. Provide `insert_vector(entry_id, embedding)` -- inserts into the hnsw_rs index AND writes the entry_id-to-hnsw_data_id mapping via `Store::put_vector_mapping`.
3. Provide `search(query_embedding, top_k, ef_search)` -- searches the hnsw_rs index and returns results with entry IDs and similarity scores.
4. Provide filtered search: `search_filtered(query_embedding, top_k, ef_search, allowed_ids)` -- searches with a pre-computed allow-list of entry IDs (converted to hnsw data IDs), enabling metadata-filtered semantic search.
5. Persist the hnsw_rs index to disk on demand (`dump`) and reload from disk on startup (`load`), using the hnsw_rs two-file format (`.hnsw.graph` + `.hnsw.data`).
6. Validate embedding dimensions on every insert -- reject vectors that are not exactly 384 dimensions.
7. Manage the hnsw_rs lifecycle: creation of new indexes, mode transitions (insert mode to search mode), and graceful handling of empty indexes.
8. Provide a reverse lookup from hnsw data ID back to entry ID, enabling result mapping after hnsw_rs search returns `Neighbour` structs.
9. Expose the index as a synchronous Rust API (matching nxs-001's pattern) suitable for wrapping with `spawn_blocking` in downstream async consumers.

## Non-Goals

- **No embedding generation.** This crate receives pre-computed `Vec<f32>` embeddings. The embedding pipeline (ONNX runtime, model loading, text-to-vector conversion) is nxs-003's responsibility.
- **No MCP server exposure.** MCP tools (`context_search`, `context_store`) are vnc-001/vnc-002. This crate provides the underlying search primitive.
- **No async API.** Matches nxs-001's synchronous pattern. Async wrapping is the consumer's responsibility.
- **No near-duplicate detection logic.** vnc-002's `context_store` tool uses search to detect near-duplicates at a 0.92 threshold, but the detection logic and threshold comparison live in vnc-002, not here.
- **No confidence scoring.** The `confidence` field on `EntryRecord` is computed by higher-level logic (vnc-002, crt-002). This crate returns raw similarity scores (distance-based).
- **No entry deletion from index.** hnsw_rs has no deletion API. Deprecated entries are excluded via filtered search using the allow-list pattern. Periodic index rebuild (full re-insert from store) may be added in a future feature.
- **No automatic embedding on insert.** When a caller inserts a vector, the embedding must already be computed. The write path is: nxs-003 computes embedding -> nxs-002 inserts vector + VECTOR_MAP -> done.
- **No SIMD feature selection logic.** The `simdeez_f` feature for hnsw_rs is enabled at compile time via Cargo.toml, not at runtime. This is a build configuration decision, not runtime logic.
- **No index rebuild or migration tooling.** Periodic full rebuild (for garbage collection of deprecated entries) and dimension migration (for switching embedding models) are future concerns.

## Background Research

### Prior Spike Research (Heavily Leveraged)

**ASS-001 (hnsw_rs Capability Spike)** at `product/research/ass-001/`:

- Confirmed hnsw_rs v0.3.3 as the strongest pure-Rust HNSW implementation.
- `DistDot` is 2-3x faster than `DistCosine` for pre-normalized text embeddings (SIMD-accelerated).
- `FilterT` trait supports pre-filtering during search: `Vec<usize>` (sorted allow-list) or closures.
- `search_filter(&self, data, knbn, ef_arg, filter)` returns `Vec<Neighbour>` with `d_id` (our data ID), `distance` (f32), `p_id` (internal, not needed).
- `parallel_insert(&self, datas)` enables batch loading via rayon with `&self` (thread-safe).
- `set_searching_mode(&mut self, true)` required before searching (requires exclusive access).
- Persistence: `file_dump(&self, path, basename)` produces two files. NOT atomic -- redb VECTOR_MAP is the crash-safe fallback.
- `HnswIo::load_hnsw_with_dist(&self, dist)` reloads with explicit distance metric.
- No deletion API -- deprecated entries filtered via `FilterT`.
- `data_dimension` field is vestigial and not validated by hnsw_rs -- dimension validation is our responsibility.
- Memory at 384d: ~1.8 MB / 1K entries, ~18 MB / 10K, ~183 MB / 100K.

### nxs-001 Integration Surface

The implemented `unimatrix-store` crate provides:

- `Store::put_vector_mapping(entry_id: u64, hnsw_data_id: u64)` -- writes to VECTOR_MAP table.
- `Store::get_vector_mapping(entry_id: u64) -> Option<u64>` -- reads from VECTOR_MAP.
- `Store::query_by_status(status) -> Vec<EntryRecord>` -- for building active entry allow-lists.
- `Store::get(entry_id) -> EntryRecord` -- for result hydration.
- `Store::exists(entry_id) -> bool` -- for validation.
- `EntryRecord.embedding_dim: u16` -- `#[serde(default)]` field, currently unused. nxs-002 should set this to 384 when inserting vectors.

The `Store` struct is `Send + Sync`. VECTOR_MAP already exists (created on `Store::open`). The `test-support` feature exposes `TestDb` and `TestEntry` helpers.

### Key Technical Constraints

- **hnsw_rs data IDs are `usize`; VECTOR_MAP stores `u64`.** Cast at boundary: `u64 as usize` / `usize as u64`. Lossless on 64-bit platforms (all target platforms).
- **hnsw_rs `insert` takes `(&[f32], usize)`.** The `usize` is the user-provided data ID. We use a monotonically increasing counter (separate from entry IDs) to assign hnsw data IDs.
- **`set_searching_mode(&mut self)` requires exclusive access.** The `VectorIndex` wrapper must use interior mutability (`RwLock`) or enforce lifecycle phases.
- **hnsw_rs index is NOT part of redb.** It lives as separate files on disk. The redb VECTOR_MAP table serves as the source of truth for entry_id-to-data_id mappings. If hnsw_rs files are lost, the index can be rebuilt by re-inserting all vectors (embeddings stored elsewhere or re-computed by nxs-003).
- **`file_dump` may rename files** if target files are mmap'd. The caller must track the actual basename returned.
- **Distance metric is fixed at index creation** (`DistDot`). Cannot be changed without full rebuild.
- **Similarity score = 1.0 - distance** for `DistDot` on normalized vectors. Returns 0.0-1.0 where 1.0 = identical.

### Crate Workspace Context

The Cargo workspace at the repo root uses `edition = "2024"`, `rust-version = "1.89"`, `resolver = "3"`. The new crate will live at `crates/unimatrix-vector/` alongside `crates/unimatrix-store/`. It depends on `unimatrix-store` for VECTOR_MAP operations and `hnsw_rs` + `anndists` for the vector index.

## Proposed Approach

### Crate Structure

Create a `unimatrix-vector` library crate within the existing Cargo workspace:

```
crates/unimatrix-vector/
  Cargo.toml
  src/
    lib.rs         -- Public re-exports, crate-level #![forbid(unsafe_code)]
    index.rs       -- VectorIndex struct, lifecycle, insert, search
    persistence.rs -- dump/load for hnsw_rs index files
    error.rs       -- VectorError enum
    config.rs      -- VectorConfig (dimension, M, ef_construction, ef_search defaults)
    filter.rs      -- FilterT implementation for metadata-filtered search
```

### VectorIndex Wrapper

```rust
pub struct VectorIndex {
    hnsw: RwLock<Hnsw<'static, f32, DistDot>>,
    store: Arc<Store>,
    config: VectorConfig,
    next_data_id: AtomicU64,          // monotonic hnsw data ID generator
    data_id_to_entry_id: RwLock<HashMap<u64, u64>>,  // reverse lookup
}
```

- `RwLock<Hnsw>` allows concurrent reads (searches) with exclusive write access for `set_searching_mode` and mode transitions.
- `Arc<Store>` for VECTOR_MAP operations.
- `AtomicU64` for lock-free data ID generation.
- In-memory reverse map (`data_id -> entry_id`) for fast result translation. Rebuilt from VECTOR_MAP on load.

### Insert Flow

1. Validate embedding dimension == 384.
2. Acquire write lock on `Hnsw`.
3. Generate next data ID via `AtomicU64`.
4. Insert `(&embedding, data_id as usize)` into hnsw_rs.
5. Write `(entry_id, data_id)` to VECTOR_MAP via `store.put_vector_mapping()`.
6. Insert `(data_id, entry_id)` into reverse map.
7. Update `EntryRecord.embedding_dim = 384` via store.
8. Return `Ok(())`.

### Search Flow

1. Acquire read lock on `Hnsw`.
2. Call `hnsw.search_filter(&query_embedding, top_k, ef_search, filter)`.
3. Map each `Neighbour.d_id` (usize -> u64) to entry_id via reverse map.
4. Compute similarity = 1.0 - distance for each result.
5. Return `Vec<SearchResult>` sorted by similarity descending.

### Filtered Search

The caller (vnc-002) builds an allow-list of entry IDs based on metadata filters (status, topic, category). nxs-002 translates entry IDs to hnsw data IDs via the reverse map, produces a sorted `Vec<usize>`, and passes it as the `FilterT` to `search_filter`.

### Persistence

- **Dump**: Call `hnsw.file_dump(path, basename)`. Track the returned actual basename. Store dump metadata (basename, point count, dimension) in COUNTERS or a dedicated metadata entry.
- **Load**: Use `HnswIo::load_hnsw_with_dist(DistDot::default())`. Rebuild the reverse map by iterating VECTOR_MAP.
- **Crash recovery**: If hnsw_rs files are corrupt or missing, rebuild from VECTOR_MAP (caller must re-provide embeddings or nxs-003 must re-compute them). The VECTOR_MAP in redb is crash-safe.

## Acceptance Criteria

- AC-01: A `unimatrix-vector` library crate compiles within the Cargo workspace with `cargo build`.
- AC-02: `VectorIndex::new(store, config)` creates a new empty hnsw_rs index with the specified parameters (dimension=384, max_nb_connection=16, ef_construction=200, DistDot).
- AC-03: `VectorIndex::insert(entry_id, embedding)` inserts the vector into the hnsw_rs index, writes the VECTOR_MAP entry via `Store::put_vector_mapping`, and updates the reverse map. Returns an error if embedding dimension is not 384.
- AC-04: `VectorIndex::search(query_embedding, top_k, ef_search)` returns up to `top_k` results as `Vec<SearchResult>` where each result contains `entry_id: u64` and `similarity: f32` (range 0.0-1.0, higher = more similar). Results are sorted by similarity descending.
- AC-05: `VectorIndex::search_filtered(query_embedding, top_k, ef_search, allowed_entry_ids)` returns results restricted to entries in the allow-list. Entries not in the allow-list do not appear in results.
- AC-06: Dimension validation rejects embeddings with length != 384. Attempting to insert a 128-d or 512-d vector returns `VectorError::DimensionMismatch`.
- AC-07: Searching an empty index returns an empty result set (no panic).
- AC-08: Inserting the same entry_id twice (re-embedding) updates the VECTOR_MAP mapping and inserts a new point in hnsw_rs. The old point becomes unreachable but does not corrupt the index.
- AC-09: `VectorIndex::dump(path)` persists the hnsw_rs index to disk as `.hnsw.graph` and `.hnsw.data` files.
- AC-10: `VectorIndex::load(store, config, path)` reloads a previously dumped index and rebuilds the reverse map from VECTOR_MAP. Search works correctly after reload.
- AC-11: `VectorIndex::point_count()` returns the number of vectors currently in the index.
- AC-12: The similarity score for identical vectors is approximately 1.0 (within floating-point tolerance). The similarity score for orthogonal vectors is approximately 0.0.
- AC-13: After inserting N vectors and searching, the correct entry appears in the top-k results for its own embedding (self-search returns the entry as the closest match).
- AC-14: All public API functions return typed `Result` errors (no panics). Error types cover dimension mismatch, persistence failures, store errors, and index errors.
- AC-15: `VectorIndex` is `Send + Sync`, shareable via `Arc<VectorIndex>`.
- AC-16: Test infrastructure extends nxs-001's patterns: `TestVectorIndex` builder, helper functions for generating random normalized embeddings, assertion helpers for search result validation.
- AC-17: `#![forbid(unsafe_code)]` at crate level.
- AC-18: The reverse map (data_id -> entry_id) is consistent with VECTOR_MAP after insert, dump, and load operations.

## Constraints

- **Rust edition 2024** (workspace setting, required by redb v3.1.0).
- **Dependencies**: hnsw_rs (v0.3.3), anndists (v0.1.4, for DistDot), unimatrix-store (workspace path dependency). No async runtime dependency.
- **No unsafe code.** Both hnsw_rs and unimatrix-store are safe Rust. `#![forbid(unsafe_code)]` at crate level.
- **Fixed dimension: 384.** The all-MiniLM-L6-v2 model produces 384-d embeddings. Dimension is validated on every insert and enforced at index creation. Changing dimension requires full index rebuild (future feature).
- **Fixed distance metric: DistDot.** Optimal for pre-normalized text embeddings. Fixed at index creation. Cannot be changed without rebuild.
- **hnsw_rs lifecycle constraint.** `set_searching_mode(&mut self)` requires exclusive access. The wrapper must handle mode transitions safely.
- **VECTOR_MAP is source of truth.** If hnsw_rs index files are lost, VECTOR_MAP entries survive (crash-safe in redb). Rebuild requires re-computing embeddings (nxs-003 responsibility).
- **No async runtime dependency.** The crate is synchronous. Matches nxs-001's pattern.
- **Single-index-per-store model.** One hnsw_rs index per store instance. Multi-project isolation (dsn-002) uses separate store instances, each with their own vector index.

## Resolved Decisions

1. **Wrapper pattern**: `VectorIndex` wraps `Hnsw` with `RwLock` for safe concurrent access (reads in parallel, exclusive write for mode transitions and inserts).
2. **Data ID strategy**: Monotonically increasing `AtomicU64` for hnsw data IDs, independent of entry IDs. Stored in VECTOR_MAP. Rebuilt from VECTOR_MAP on load.
3. **Reverse map**: In-memory `HashMap<u64, u64>` (data_id -> entry_id) for O(1) result translation. Rebuilt from VECTOR_MAP on load. Acceptable memory overhead (~16 bytes per entry).
4. **Similarity formula**: `1.0 - distance` for DistDot on normalized vectors. Range 0.0-1.0.
5. **Filtered search interface**: Caller provides `Vec<u64>` of allowed entry IDs. The crate translates to sorted `Vec<usize>` of hnsw data IDs for `FilterT`.
6. **No internal mode toggling**: The caller is responsible for ensuring the index is in the correct mode. The wrapper enforces this through the API design (insert operations set insert mode, search operations set search mode internally via `RwLock`).

## Resolved Open Questions

- **OQ-1: Re-embedding behavior.** RESOLVED: Leave old hnsw_rs points in place. Track stale count. Rebuild when stale ratio exceeds a threshold (future feature). Simple, no corruption risk.
- **OQ-2: Dump trigger strategy.** RESOLVED: Explicit `dump()` call only, matching nxs-001's `compact()` pattern. **Integration note**: vnc-001 (MCP Server) will need a shutdown/graceful-exit handler that coordinates both `Store::compact()` and `VectorIndex::dump()`. This is a vnc-001 design concern, not nxs-002. See PRODUCT-VISION.md vnc-001 note.
- **OQ-3: Mode transition overhead.** RESOLVED: Accept current `RwLock`-based design. Add a micro-benchmark test during implementation (insert N vectors, time `set_searching_mode` at 100/1K/10K index sizes). Document findings. If expensive, add explicit batch-insert-then-transition-once semantics in a follow-up.
- **OQ-4: Max elements hint.** RESOLVED: Make `max_elements` a configurable field in `VectorConfig` (default 10,000). It is a pre-allocation hint, not a hard cap — the index grows dynamically beyond it. Over-provisioning wastes memory; under-provisioning triggers reallocation. Document as a tuning knob.
- **OQ-5: Feature flag for SIMD.** RESOLVED: Enable `simdeez_f` by default. Compile-time optimization with no runtime cost on non-x86_64 platforms.

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/2
