## ADR-001: hnsw_rs as Vector Index Library

### Context

Unimatrix needs an approximate nearest neighbor (ANN) index for semantic search over 384-dimension text embeddings. The index must be pure Rust (no FFI), support persistence, support filtered search, and integrate cleanly into the existing Cargo workspace (edition 2024, MSRV 1.89).

Alternatives considered:
- **usearch**: Higher raw performance, more features, but requires C++ FFI. Breaks the pure-Rust guarantee.
- **instant-distance**: Pure Rust, minimal API, but stale (last release Jun 2023), no persistence, no filtered search.
- **hora**: Pure Rust, multiple algorithms, but abandoned (Aug 2021).

### Decision

Use `hnsw_rs` v0.3.3 with `anndists` v0.1.4 for distance metrics.

Key capabilities validated by ASS-001 spike research:
- `FilterT` trait for pre-filtering during graph traversal (not post-filtering).
- `file_dump` / `HnswIo::load_hnsw_with_dist` for persistence.
- `parallel_insert` via rayon for batch loading.
- `insert` and `search` take `&self` (concurrent use safe via internal locks).
- SIMD acceleration via `simdeez_f` feature flag.
- 280K+ downloads, actively maintained, used by aichat (popular LLM CLI).

Enable `simdeez_f` feature by default for SIMD-accelerated distance computation on x86_64.

### Consequences

- **Easier**: Pure Rust dependency, no cross-compilation complexity, safe Rust throughout.
- **Easier**: FilterT enables metadata-filtered semantic search without a separate code path.
- **Easier**: Built-in persistence means we don't need to implement serialization ourselves.
- **Harder**: No deletion API -- deprecated entries accumulate in the index until rebuild.
- **Harder**: `set_searching_mode(&mut self)` requires exclusive access, necessitating `RwLock` wrapper.
- **Harder**: Persistence is NOT atomic -- crash during dump corrupts files. Must rely on redb VECTOR_MAP as crash-safe source of truth.
- **Harder**: Dimension validation is our responsibility (hnsw_rs does not validate).
