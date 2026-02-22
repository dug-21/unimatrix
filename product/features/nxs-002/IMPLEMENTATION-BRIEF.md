# nxs-002: Vector Index -- Implementation Brief

**Handoff**: Session 1 (Design) -> Session 2 (Delivery)
**Date**: 2026-02-22

---

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-002/SCOPE.md |
| Architecture | product/features/nxs-002/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-002/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nxs-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-002/ALIGNMENT-REPORT.md |

---

## Feature Summary

nxs-002 implements the vector similarity search layer for Unimatrix: a synchronous Rust library crate (`unimatrix-vector`) that wraps hnsw_rs (`Hnsw<f32, DistDot>`) with 384-dimension embeddings, 16 max connections, and ef_construction=200. It provides insert, unfiltered search, metadata-filtered search, index persistence (dump/load), and bidirectional ID mapping between entry IDs and hnsw data IDs via the VECTOR_MAP table in unimatrix-store. The crate has zero async runtime dependencies and is the foundation for semantic search in all downstream features (nxs-003 embedding pipeline, vnc-002 context_search tool).

---

## Component Map

### C1: crate-setup

**Purpose**: Cargo crate scaffolding for unimatrix-vector.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/Cargo.toml`, `crates/unimatrix-vector/src/lib.rs` |
| Modules | N/A (configuration only) |
| Dependencies | None (configuration) |

**Details**:
- Workspace member: `crates/unimatrix-vector/`
- Dependencies: `unimatrix-store = { path = "../unimatrix-store" }`, `hnsw_rs = { version = "0.3", features = ["simdeez_f"] }`, `anndists = "0.1"`
- Dev-dependencies: `tempfile = "3"`, `rand = "0.9"`
- Feature: `test-support = ["unimatrix-store/test-support"]`
- Crate-level `#![forbid(unsafe_code)]`

### C2: error

**Purpose**: Typed error enum for all vector index operations.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/error.rs` |
| Modules | `VectorError`, `Result<T>` type alias |
| Dependencies | `unimatrix-store` (StoreError) |

**Variants**: `DimensionMismatch { expected: usize, got: usize }`, `Store(StoreError)`, `Persistence(String)`, `EmptyIndex`, `EntryNotInIndex(u64)`, `Index(String)`, `InvalidEmbedding(String)`.

Implements `Display`, `Error`, and `From<StoreError>`.

### C3: config

**Purpose**: Configuration struct for VectorIndex construction.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/config.rs` |
| Modules | `VectorConfig` |
| Dependencies | None |

**Details**:
- `VectorConfig { dimension: usize, max_nb_connection: usize, ef_construction: usize, max_elements: usize, max_layer: usize, default_ef_search: usize }`
- Default: `{ 384, 16, 200, 10_000, 16, 32 }`

### C4: index

**Purpose**: Core `VectorIndex` struct with insert and search operations.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/index.rs` |
| Modules | `VectorIndex`, `SearchResult`, internal `IdMap` |
| Dependencies | C2 (error), C3 (config), hnsw_rs, anndists, unimatrix-store |

**Public API**:
- `VectorIndex::new(store: Arc<Store>, config: VectorConfig) -> Result<Self>`
- `VectorIndex::insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()>`
- `VectorIndex::search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>>`
- `VectorIndex::search_filtered(&self, query: &[f32], top_k: usize, ef_search: usize, allowed_entry_ids: &[u64]) -> Result<Vec<SearchResult>>`
- `VectorIndex::point_count(&self) -> usize`
- `VectorIndex::contains(&self, entry_id: u64) -> bool`
- `VectorIndex::stale_count(&self) -> usize`

**Internal**:
- `IdMap { data_to_entry: HashMap<u64, u64>, entry_to_data: HashMap<u64, u64> }`
- `RwLock<Hnsw<'static, f32, DistDot>>` for thread-safe index access
- `AtomicU64` for data ID generation
- Dimension validation on insert and search
- NaN/infinity validation on insert and search (W2 acceptance)
- Similarity = `1.0 - distance`

### C5: filter

**Purpose**: FilterT implementation for metadata-filtered search.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/filter.rs` |
| Modules | `EntryIdFilter` (pub(crate)) |
| Dependencies | hnsw_rs (FilterT trait) |

**Details**:
- `EntryIdFilter { allowed_data_ids: Vec<usize> }` -- sorted for binary search
- Implements `FilterT` for `EntryIdFilter`
- Construction: entry IDs -> data IDs via IdMap -> sort -> wrap

### C6: persistence

**Purpose**: Dump and load hnsw_rs index with metadata tracking.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/persistence.rs` |
| Modules | `VectorIndex::dump`, `VectorIndex::load` |
| Dependencies | C2 (error), C3 (config), C4 (index -- VectorIndex struct), hnsw_rs (file_dump, HnswIo), unimatrix-store (VECTOR_MAP iteration) |

**Details**:
- `dump(&self, dir: &Path) -> Result<()>`: file_dump + metadata file
- `load(store: Arc<Store>, config: VectorConfig, dir: &Path) -> Result<VectorIndex>`: HnswIo reload + IdMap rebuild from VECTOR_MAP
- Metadata file: `unimatrix-vector.meta` (basename, point_count, dimension, next_data_id)
- Requires `Store::iter_vector_mappings()` (to be added to unimatrix-store)

### C7: store-extension

**Purpose**: Add `iter_vector_mappings` to unimatrix-store for VECTOR_MAP iteration.

| Attribute | Value |
|-----------|-------|
| Modifies | `crates/unimatrix-store/src/read.rs` |
| Modules | `Store::iter_vector_mappings() -> Result<Vec<(u64, u64)>>` |
| Dependencies | unimatrix-store internals (VECTOR_MAP table) |

**Details**:
- Iterates all entries in VECTOR_MAP, returns `Vec<(entry_id, data_id)>`.
- Read-only operation using ReadTransaction.
- Required for C6 persistence load path.
- W1 alignment acceptance: minor extension to nxs-001.

### C8: lib

**Purpose**: Crate root with public re-exports.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-vector/src/lib.rs` |
| Modules | Re-exports from all other modules |
| Dependencies | All above modules |

**Re-exports**: `VectorIndex`, `SearchResult`, `VectorConfig`, `VectorError`, `Result`.

### C9: test-infra

**Purpose**: Reusable test infrastructure for this and downstream features.

| Attribute | Value |
|-----------|-------|
| Creates | Test helpers module (accessible via `#[cfg(test)]` internally, `test-support` feature flag for downstream) |
| Modules | `TestVectorIndex`, `random_normalized_embedding`, `assert_search_contains`, `seed_vectors` |
| Dependencies | All above components, `tempfile`, `rand` |

**Details**:
- `TestVectorIndex`: creates temp store + VectorIndex, implements Drop for cleanup.
- `random_normalized_embedding(dim: usize) -> Vec<f32>`: generates random L2-normalized vector.
- `assert_search_contains(results, entry_id)`: verify entry appears in search results.
- `seed_vectors(vi, count)`: insert count random vectors, return entry IDs.

---

## Component Map -- Stage 3a File Paths

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: crate-setup | pseudocode/crate-setup.md | test-plan/crate-setup.md |
| C2: error | pseudocode/error.md | test-plan/error.md |
| C3: config | pseudocode/config.md | test-plan/config.md |
| C4: index | pseudocode/index.md | test-plan/index.md |
| C5: filter | pseudocode/filter.md | test-plan/filter.md |
| C6: persistence | pseudocode/persistence.md | test-plan/persistence.md |
| C7: store-extension | pseudocode/store-extension.md | test-plan/store-extension.md |
| C8: lib | pseudocode/lib.md | test-plan/lib.md |
| C9: test-infra | pseudocode/test-infra.md | test-plan/test-infra.md |

Overview documents:
- Pseudocode: pseudocode/OVERVIEW.md
- Test Plan: test-plan/OVERVIEW.md

---

## Implementation Order

```
Phase 1: Foundation (no internal dependencies)
  C1: crate-setup     -- workspace + Cargo.toml
  C2: error           -- VectorError enum
  C3: config          -- VectorConfig struct

Phase 2: Core (depends on Phase 1)
  C7: store-extension -- Store::iter_vector_mappings (nxs-001 extension)
  C5: filter          -- EntryIdFilter (needs hnsw_rs FilterT only)
  C4: index           -- VectorIndex (needs error, config, filter, hnsw_rs, store)

Phase 3: Persistence (depends on Phase 2)
  C6: persistence     -- dump/load (needs index, store-extension)

Phase 4: Assembly (depends on all above)
  C8: lib             -- re-exports
  C9: test-infra      -- helpers, builders, assertions
```

**Critical path**: C1 -> C2/C3 -> C4 -> C6. The index module (C4) is the largest component and contains the highest-risk code (R-01, R-02, R-03).

---

## Critical Decisions Constraining Implementation

### From ADRs

| ADR | Constraint | Implementation Impact |
|-----|-----------|----------------------|
| ADR-001 | hnsw_rs v0.3.3 with simdeez_f | Cargo.toml: `hnsw_rs = { version = "0.3", features = ["simdeez_f"] }` |
| ADR-002 | DistDot distance metric, fixed | `Hnsw::new(...)` with `DistDot` from anndists. Similarity = 1.0 - distance. |
| ADR-003 | RwLock concurrency model | `RwLock<Hnsw>` for thread-safe access. Write lock for insert, read lock for search. |
| ADR-004 | Bidirectional IdMap | In-memory `HashMap<u64, u64>` x2. Rebuilt from VECTOR_MAP on load. |

### From Alignment Warnings (Accepted)

**W1: Store::iter_vector_mappings (ACCEPTED)**
Add `Store::iter_vector_mappings() -> Result<Vec<(u64, u64)>>` to unimatrix-store. Required for IdMap rebuild during load. Single read-only method, no architectural changes.

**W2: NaN/Infinity Validation (ACCEPTED)**
Add NaN/infinity validation alongside dimension validation in insert and search paths. Add `VectorError::InvalidEmbedding(String)` variant.

### From Specification

- VectorConfig default: dimension=384, max_nb_connection=16, ef_construction=200, max_elements=10_000, max_layer=16, default_ef_search=32.
- SearchResult: `{ entry_id: u64, similarity: f32 }`, sorted descending.
- Empty index search returns Ok(empty vec), not error.
- Re-embedding leaves old point in hnsw_rs, updates VECTOR_MAP to new data_id.
- Data IDs from AtomicU64 starting at 0.
- `#![forbid(unsafe_code)]` at crate level.

---

## Integration Constraints

| Consumer | Integration Surface | Constraint |
|----------|-------------------|------------|
| nxs-003 (Embedding Pipeline) | `VectorIndex::insert(entry_id, &embedding)` | Embedding must be exactly 384-d, L2-normalized. |
| vnc-002 (MCP Tools) | `search`, `search_filtered` | Returns `Vec<SearchResult>` with entry_id + similarity. |
| vnc-001 (MCP Server) | `Arc<VectorIndex>` + `spawn_blocking` | VectorIndex is Send + Sync. Dump on shutdown. |
| unimatrix-store (nxs-001) | `put_vector_mapping`, `get_vector_mapping`, `iter_vector_mappings` | VECTOR_MAP is crash-safe source of truth. |

---

## Risk Hotspots (Test First)

Ranked by priority from RISK-TEST-STRATEGY.md:

| Priority | Risk | Component | What to Test First |
|----------|------|-----------|-------------------|
| 1 | R-02: Dimension mismatch | C4 (index) | Insert wrong-dimension vectors. Verify error. **Write this test FIRST.** |
| 2 | R-01: IdMap desync with VECTOR_MAP | C4 (index) | Insert 100 vectors, verify IdMap matches VECTOR_MAP for every entry. |
| 3 | R-03: Filtered search correctness | C4 (index) | Insert 10, filter to 3, verify only those 3 returned. Exclusion test. |
| 4 | R-06: Re-embedding stale points | C4 (index) | Re-embed entry, verify latest vector found, stale count correct. |
| 5 | R-04: Persistence round-trip | C6 (persistence) | Insert, dump, load, search. Verify same results. |

---

## Resolved Questions

| Question | Resolution | Source |
|----------|-----------|--------|
| Distance metric | DistDot (fixed, SIMD) | ADR-002 |
| Concurrency model | RwLock on Hnsw | ADR-003 |
| ID mapping strategy | Bidirectional in-memory HashMap | ADR-004 |
| Data ID generation | AtomicU64, independent of entry IDs | Architecture |
| Similarity formula | 1.0 - distance | Specification FR-07 |
| Re-embedding behavior | Leave old point, track stale count | SCOPE OQ-1 |
| Dump trigger | Explicit only | SCOPE OQ-2 |
| Max elements | Configurable in VectorConfig (default 10K) | SCOPE OQ-4 |
| SIMD feature | simdeez_f enabled by default | SCOPE OQ-5 |
| VECTOR_MAP iteration | Add Store::iter_vector_mappings() | W1 acceptance |
| NaN validation | Add to insert/search paths | W2 acceptance |

---

## Alignment Status

**Overall**: 4 PASS, 2 WARN, 0 VARIANCE, 0 FAIL.

- W1 (Store::iter_vector_mappings): Accepted. Minor nxs-001 extension.
- W2 (NaN/infinity validation): Accepted. Safety addition beyond SCOPE AC-06.

No variances requiring human approval remain.

---

## Open Questions Remaining

**OQ-1: Hnsw Lifetime Parameter**
`Hnsw<'static, f32, DistDot>` -- verify during implementation that owned data satisfies the `'static` requirement. If not, may need `Box::leak` or arena allocation pattern.

**OQ-2: hnsw_rs set_searching_mode Timing**
The RwLock write lock in `insert` must also handle mode transition. Determine during implementation whether to call `set_searching_mode(false)` before every insert and `set_searching_mode(true)` before every search, or to track mode state and only transition when needed. Recommendation: track mode state to minimize transitions.
