## ADR-001: f64 Scoring Boundary

### Context

The confidence system computes internally in f64 for numerical stability (crt-002 ADR-002) but truncates to f32 at the return boundary. All scoring constants (confidence weights, re-ranking weights, boost caps) are f32. `EntryRecord.confidence` is stored as f32 (4 bytes via bincode). `SearchResult.similarity` is f32. The f32 ceiling limits effective precision to ~7 decimal digits and produces JSON artifacts (e.g., `0.8500000238418579` when a f32 0.85 is serialized to JSON as f64).

Two distinct precision domains exist in the system:
1. **Scoring pipeline**: confidence computation, re-ranking blend, co-access boost -- pure math that benefits from f64.
2. **Embedding/HNSW pipeline**: ONNX model outputs f32 embeddings, hnsw_rs uses `Hnsw<'static, f32, DistDot>` with SIMD-optimized f32 operations, similarity is inherently f32 precision.

Promoting embeddings to f64 would double memory with no precision gain (the ONNX model is the bottleneck). Future embedding scale means quantizing DOWN (int8/binary), not up.

The contradiction detection module (`contradiction.rs`) compares against HNSW similarity scores and uses heuristic weights for conflict detection. These are in the embedding/HNSW domain and do not benefit from f64 promotion.

### Decision

Upgrade the **scoring pipeline** to f64 end-to-end. Leave the **embedding/HNSW pipeline** and **contradiction detection pipeline** at f32.

Specifically:
- `EntryRecord.confidence`: f32 -> f64 (requires schema migration v2->v3)
- `SearchResult.similarity`: f32 -> f64 (cast from f32 hnsw_rs distance at the conversion boundary in `map_neighbours_to_results`)
- All confidence weight constants (W_BASE through W_COAC): f32 -> f64
- `SEARCH_SIMILARITY_WEIGHT`: f32 -> f64
- `compute_confidence` return type: f32 -> f64 (remove `as f32` truncation)
- `rerank_score` signature: f32 -> f64
- `co_access_affinity` signature: f32 -> f64
- `MAX_CO_ACCESS_BOOST`, `MAX_BRIEFING_CO_ACCESS_BOOST`: f32 -> f64
- `compute_search_boost`, `compute_briefing_boost` return maps: `HashMap<u64, f32>` -> `HashMap<u64, f64>`
- `Store::update_confidence`: f32 -> f64

NOT changed:
- All HNSW/embedding types (`Vec<f32>` embeddings, `Hnsw<f32, DistDot>`)
- Contradiction module constants and types (SIMILARITY_THRESHOLD, conflict_score, ContradictionPair fields)
- EmbeddingInconsistency.expected_similarity
- ContradictionConfig fields

The f32-to-f64 boundary lives in `VectorIndex::map_neighbours_to_results`, where `1.0 - n.distance` (f32) is promoted to f64 before being stored in `SearchResult.similarity`.

### Consequences

**Easier:**
- JSON responses emit clean f64 values (no more `0.8500000238418579`)
- ~15 digits of scoring precision enables future pi-based calibration (ass-012)
- No more lossy `as f32` truncation at the confidence return boundary
- Weight sum invariant is exact in f64 (0.35 + 0.30 + ... = 1.0 exactly representable)
- Fine-grained score differentiation between entries with similar relevance at larger scale

**Harder:**
- Schema migration v2->v3 required for stored confidence field
- ~60-80 tests need mechanical type updates (f32 literals to f64)
- All crates except unimatrix-embed are touched
- Mixed precision at the SearchResult boundary (f32 hnsw_rs -> f64 SearchResult) requires explicit cast
