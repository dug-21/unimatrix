# hnsw_rs Research Notes

**Date**: 2026-02-20
**Purpose**: Raw findings and notable details from Track 1A research

---

## Crate Identity

The crate has a naming split worth noting:
- **crates.io name**: `hnsw_rs`
- **GitHub repo name**: `hnswlib-rs` (at jean-pierreBoth/hnswlib-rs)
- This causes confusion when searching. Always search for both names.

---

## Notable API Details

### The `&self` vs `&mut self` Split

hnsw_rs has an interesting thread-safety design:

- **`insert()` and `search()` take `&self`** — concurrent use is safe via internal locks
- **`set_searching_mode()` takes `&mut self`** — requires exclusive access
- **`AnnT` trait takes `&mut self` for insertion** — type-erased interface is more restrictive

This means in practice:
- You can insert and search concurrently (good for an MCP server handling concurrent requests)
- But switching between insert-heavy and search-heavy modes requires exclusive access
- The Unimatrix wrapper will likely need `RwLock<Hnsw<...>>` or careful initialization sequencing

### The `search_possible_filter` Mystery

`search_possible_filter` is just an `#[inline]` wrapper around `search_filter`. They're identical. The naming suggests historical evolution — possibly `search_possible_filter` was the original name and `search_filter` was added later. Use `search_filter` in our code.

### Distance Returns f32 Even for f64 Vectors

The `Distance<T>` trait's `eval` method always returns `f32`:
```rust
fn eval(&self, va: &[T], vb: &[T]) -> f32;
```

The README warns that converting f64 distances to f32 can overflow. Not an issue for us (we'll use f32 vectors), but worth knowing.

### data_dimension Is Vestigial

The `data_dimension` field exists on the struct but:
- Initialized to 0
- Marked `#[allow(unused)]`
- Never set or validated during insert
- `get_data_dimension()` infers from the entry point vector length

This is a footgun — inserting vectors of wrong dimension silently corrupts the index.

---

## Persistence Deep Dive

### Two-File Format

The dump creates:
1. `{name}.hnsw.graph` — topology file with magic headers, neighbor lists, construction params
2. `{name}.hnsw.data` — raw vector bytes with per-point headers (magic, origin_id, length)

Format evolved: v2 used bincode, v3/v4 use raw binary writes. The v3/v4 format enables memory-mapped access.

### The Rename Safety Mechanism

`file_dump()` has a clever safety mechanism: if the target files already exist (e.g., currently mmap'd), it generates a new random basename and returns it. This prevents corrupting a live mmap'd index during re-dump. But it means the caller needs to track which basename was actually used.

### Reload Options

`ReloadOptions` supports:
- `datamap: bool` — memory-map the data file instead of loading into RAM
- `mmap_threshold: usize` — only mmap if more than N items

The mmap approach is valuable for large indexes: "reload" becomes nearly instant since pages are demand-paged. Only the graph file is eagerly loaded.

There's also a `NoData` + `NoDist` combination for loading only the graph topology without any vector data — useful for graph analysis with minimal memory.

### Missing: Deletion

There is NO API for removing individual points from the index. This is a fundamental HNSW limitation (the graph structure doesn't support efficient deletion). Our options:
1. Mark deleted in metadata (redb), filter during search — easy, slight search overhead
2. Periodic full rebuild — expensive but cleans up
3. Both: filter for correctness, rebuild for efficiency

---

## Distance Metric Details

### DistDot vs DistCosine

For L2-normalized vectors (which text embedding models produce):
- `DistCosine` computes: `1 - dot(a,b) / (||a|| * ||b||)` with a sqrt
- `DistDot` computes: `1 - dot(a,b)` directly

Since `||a|| = ||b|| = 1` for normalized vectors, they produce identical results. But `DistDot` is 2-3x faster because it skips the norm computation and sqrt.

`DistDot` has SIMD acceleration (AVX2/SSE2 via simdeez, portable SIMD via std::simd).

### SIMD Feature Flags

Two optional SIMD paths:
- `simdeez_f` — stable Rust, x86_64 AVX2/SSE2 (via simdeez crate)
- `stdsimd` — nightly Rust, portable SIMD (via std::simd)

Neither is enabled by default. For production, we should enable `simdeez_f` on x86_64.

---

## Ecosystem Assessment

### Reverse Dependencies (10 crates)

Notable:
- **aichat** (0.30.0) — popular LLM CLI tool, validates production-grade trust
- **ruvector-core** (2.0.3) — vector database core, similar use case to ours
- **rosella**, **gsearch**, **kmerutils** — bioinformatics tools reflecting author's domain

### Alternatives Comparison

| Crate | Status | Pure Rust | Features |
|-------|--------|-----------|----------|
| **hnsw_rs** | Active (Nov 2025) | Yes | Persistence, filtering, parallel, mmap, SIMD |
| **usearch** | Active (Feb 2026) | No (C++ FFI) | More features, higher raw perf |
| **instant-distance** | Stale (Jun 2023) | Yes | Minimal, no persistence/filter |
| **hnsw** (rust-cv) | Stale (Jul 2021) | Yes | Clean API, no persistence |
| **hora** | Abandoned (Aug 2021) | Yes | Multiple algorithms, not maintained |

**hnsw_rs is clearly the right choice** for pure-Rust embedding. usearch would be the alternative if FFI is acceptable and raw performance is the priority. Our VectorStore trait preserves the migration path.

---

## Performance Data

### Published Benchmarks (i9-13900HX, 24 cores)

| Dataset | Dimensions | Queries/sec | Recall | k |
|---------|-----------|-------------|--------|---|
| Fashion-MNIST | 784 | 62,000 | 0.977 | 10 |
| Ann-Glove-25 | 25 | 12,000 | 0.979 | 100 |
| SIFT1M | 128 | 15,000 | 0.991 | 10 |

### Unpublished but Inferred

- Dump/reload speed: v3/v4 format uses raw binary writes, should be near sequential I/O throughput
- mmap reload: effectively instant for data (demand-paged), graph file read is eager
- Insert throughput: parallel_insert via rayon should saturate available cores
- Filter overhead: per-candidate function call during traversal; overhead depends on filter complexity

---

## Open Issues on GitHub (5 total)

1. **#25** (Jul 2025) — Request for more general dump/reload interfaces
2. **#20** (Jul 2024) — No WASM target support
3. **#19** (Jun 2024) — API design discussion: borrow vs move for HnswIo loading
4. **#9** (May 2023) — Request for graph reordering optimization
5. **#2** (Jun 2020) — SIMD-accelerated Levenshtein distance

None of these are blockers for Unimatrix.

---

## Gaps Requiring Future Investigation

| Gap | How to Resolve | Priority |
|-----|---------------|----------|
| Actual Rust struct memory overhead per entry | Build test harness, measure with jemalloc stats | Low (estimates sufficient for planning) |
| Dump/reload speed at 10K-100K entries | Build test harness, benchmark | Low (expected to be fast) |
| Filter performance impact with redb lookups | Build test harness combining hnsw_rs + redb | Medium (affects search latency) |
| SIMD feature flag interaction with Docker/Linux | Test `simdeez_f` in devcontainer | Low (should work on x86_64) |
| Concurrent insert + search behavior under load | Build test harness with tokio | Medium (affects MCP server design) |

These gaps are acceptable for interface design (Track 3). They can be resolved during implementation with production test harnesses.
