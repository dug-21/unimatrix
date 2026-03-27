# ASS-032: Rust ML Ecosystem Survey

**Generated**: 2026-03-25
**Scope**: Feasibility evaluation of Rust-native ML/neural model options for a self-learning knowledge engine

---

## Unimatrix Baseline (What We Already Have)

```toml
ort = "=2.0.0-rc.9"         # ONNX Runtime — used for sentence embedding today
hnsw_rs = "0.3"             # with simdeez_f — custom HNSW in unimatrix-vector
tokenizers = "0.21"         # HuggingFace tokenizer — already a dependency
```

Plus: hand-rolled ndarray MLP in `unimatrix-learn` (32→64→32→5 SignalClassifier; 32→64→1 ConventionScorer), EWC++ regularization, reservoir sampling, 3-slot model registry.

The `NeuralModel` trait comment in the codebase already reads: **"Designed for future burn/candle implementations behind feature gates"** — the decision point was anticipated.

---

## Priority Matrix

Ranked by (feasibility × impact) for Unimatrix's self-learning goal:

| Rank | Option | Effort | Verdict |
|------|--------|--------|---------|
| **1** | **ort cross-encoder re-ranking** | 1–2 days | **Pursue immediately** — ms-marco-MiniLM-L6 quantized int8, 20–40ms for 20 pairs, already within budget |
| **2** | **petgraph graph-augmented retrieval** | 1–2 days | **Pursue in parallel** — zero new ML, leverages existing CO_ACCESS adjacency |
| **3** | **BM25 sparse-dense hybrid** (`bm25` crate) | 1–2 days | **Pursue** — catches keyword-exact queries dense retrieval misses; no new models |
| 4 | candle | 2–3 days | Defer — fallback if ort C++ dependency causes deployment issues |
| 5 | burn | 3–5 days | Defer — revisit when models > 1M params or GPU path needed |
| 6 | SPLADE (fastembed-rs) | 2–3 days | Defer — meaningful gain only at corpus > 100K entries |
| 7 | usearch | 3–4 days | Defer — no advantage over hnsw_rs until corpus > 500K entries |
| 8 | ColBERT | 7–10 days | **Reject** — 250–700 MB RAM for 10K entries; cross-encoder gives comparable quality |
| 9 | RuVector | 5–10 days | **Reject** — full database system, architecture incompatible |

**Combined top-3 = 3–6 days, no new infrastructure decisions, all within existing dependency tree.**

---

## Option 1: ruvector (ruvnet/RuVector)

### What it is
Full self-learning vector database: HNSW + GNN re-ranking (multi-head attention, 46 variants), PostgreSQL extension, GGUF LLM inference, .rvf cognitive containers, Raft consensus, multi-master replication. CES 2026 Innovation Award. 3,600 GitHub stars.

### Feasibility for Unimatrix
**Incompatible architecture.** RuVector is a full database system. Unimatrix is a deliberately in-process library with SQLite. The `ruvector-core` crate on crates.io is a database interface layer, not a standalone ML library. Extracting the GNN attention layer requires reverse-engineering coupling across 90+ tightly-coupled crates.

### Verdict: **REJECT**
Architecture fundamentally incompatible. The GNN re-ranking idea it demonstrates is better implemented directly in ndarray or candle without the RuVector dependency.

---

## Option 2: burn (tracel-ai/burn)

### What it is
Full deep learning framework: training + inference, backend abstraction (NdArray, CUDA, ROCm, Metal, Vulkan, WebGPU, LibTorch). ONNX models converted to Rust source code at build time via `burn-onnx` (codegen approach, not runtime loading). v0.20.x, pre-1.0. 14,700 GitHub stars.

### Feasibility for Unimatrix
Moderate. NdArray backend is CPU-only, no external dependencies — matches constraints. The codegen approach (model → Rust source) means no runtime model file loading; clean binary deployment. However:
- Pre-1.0 API churn risk
- Higher overhead than ort for inference-only workloads at small model scale
- candle consistently reported faster for CPU inference

### Verdict: **DEFER**
Excellent for GPU-accelerated training or if models grow beyond ~1M params. For current scale (in-process, CPU-only, < 50ms), ort's direct inference is faster and simpler.

---

## Option 3: candle (huggingface/candle)

### What it is
Minimalist ML framework from HuggingFace, targeting serverless inference. Loads safetensors + GGUF quantized models. Native support for BERT, T5, LLaMA, Phi, Mistral, and dozens more. Uses `tokenizers` crate (already a dependency). 19,800 GitHub stars.

### Feasibility for Unimatrix
High for cross-encoder reranking. Can run `cross-encoder/ms-marco-MiniLM-L-12-v2` in-process on CPU. Memory: quantized int8 6-layer MiniLM ≈ 75–100 MB total RAM overhead. Acceptable for persistent MCP server.

**Latency caveat:** Community benchmarks show candle is 2–3x slower than ort for the same ONNX models on CPU inference. Since ort is already present, candle is the backup path.

### Verdict: **DEFER as fallback**
Use if ort's C++ dynamic library linkage causes deployment issues. For current environment where ort works, candle is the second choice.

---

## Option 4: ort (ONNX Runtime — existing dependency)

### What it is
`ort = "=2.0.0-rc.9"` — already in the codebase, running the sentence embedding model today. Latest release: v2.0.0-rc.12 (March 5, 2026). 2,100 GitHub stars, used by ~3,600 Rust projects.

### Can ort run cross-encoder/re-ranker models?
**Yes.** Any ONNX-exported model works. Pre-quantized cross-encoder models available:
- `Xenova/ms-marco-MiniLM-L-6-v2` — quantized ONNX files in `onnx/` folder on HuggingFace (avx2, avx512, avx512_vnni variants)
- `cross-encoder/ms-marco-MiniLM-L2-v2` — 2-layer, ~7M params, 3–5ms for 20 pairs

### Latency for 20 document-query pairs on CPU
- ms-marco-MiniLM-L6 int8 ONNX, modern x86: **20–40 ms** (within 50ms budget, minimal headroom)
- ms-marco-MiniLM-L2 int8 ONNX: **3–5 ms** (well within budget, some accuracy tradeoff)
- Session overhead: 0–2 ms per call

### Supplementary: fastembed-rs
`fastembed-rs` (v5.x, 810 stars) wraps ort to provide a one-line API for cross-encoder reranking. Supported rerankers: `BAAI/bge-reranker-base` (0.22 GB), `jinaai/jina-reranker-v1-turbo-en` (0.15 GB), ms-marco MiniLM variants. Reduces integration from 1–2 days to 0.5 days. Model auto-download must be gated for offline-first operation.

### Verdict: **PURSUE IMMEDIATELY**
Lowest cost, highest impact. Unimatrix already has ort running. Adding a cross-encoder session follows the identical pattern to the existing embedding session. This is extension of existing infrastructure, not a new dependency decision.

---

## Option 5: SPLADE / Sparse-Dense Hybrid

### SPLADE in Rust
No standalone pure-Rust SPLADE crate. Options:
- **fastembed-rs** (ort-based): `SparseTextEmbedding` with `SparseModel::SPLADEPPV1` — `prithivida/Splade_PP_en_v1` (0.532 GB ONNX). Produces sparse vectors as `(indices, values)`.
- **embed_anything** (1,200 stars): SPLADE + dense + ColBERT via ort.

### Storage overhead: sparse vs. dense
- Dense (384-dim, fp32): 1,536 bytes/doc
- SPLADE (30K-dim vocabulary, ~174 non-zero): ~1,392 bytes/doc (similar storage, completely different index structure)
- Requires inverted index over 30K vocabulary tokens — HNSW is irrelevant

### BM25 alternative (no ML)
- `bm25` crate (crates.io): in-memory BM25 engine, zero external dependencies
- `tantivy` (crates.io, Lucene-inspired, 10K+ stars): BM25 default ranking, production-battle-tested (used by Quickwit)
- BM25 + dense hybrid via Reciprocal Rank Fusion (RRF): practical, no new model downloads

### Verdict: **Pursue BM25 hybrid, defer SPLADE**
BM25 + RRF: 1–2 days, zero new ML model downloads. SPLADE adds 530 MB model + 10–30ms latency overhead — meaningful only at corpus > 100K entries.

---

## Option 6: petgraph (Graph-Augmented Retrieval)

### What it is
petgraph: mature Rust graph library, 3,800 stars, stable 0.8 release. Provides directed/undirected graphs with BFS/DFS, Dijkstra, Bellman-Ford, A*, MST (Kruskal/Prim), topological sort, connected components, SCC.

### Feasibility for Unimatrix
High. Unimatrix already has CO_ACCESS table as adjacency data and GRAPH_EDGES with five typed edge types (Supersedes, Contradicts, Supports, CoAccess, Prerequisite). The existing `graph_penalty` traversal already uses typed graph logic — it only computes penalties, not retrieval boosts. The Supports and Prerequisite edges are currently dead weight in retrieval scoring.

### Performance at 10K entries
- BFS/DFS over 10K nodes: < 1 ms
- Building graph from CO_ACCESS at startup: 1–5 ms
- Memory: ~50K edges × 16 bytes ≈ 800 KB — negligible

### graphrag-rs (automataIA/graphrag-rs)
221 GitHub stars. Claims Leiden community detection + Personalized PageRank + cross-encoder reranking. Low adoption; documentation reads promotional. Not recommended as a dependency — implement the algorithms directly with petgraph.

### Verdict: **PURSUE**
petgraph is already conceptually compatible with Unimatrix's data model. CO_ACCESS is the adjacency structure; Supports/Prerequisite edges are the relevance propagation edges. Personalized PageRank initialized from HNSW scores converts the existing graph from penalty-only to positive relevance amplifier.

---

## Option 7: usearch (unum-cloud)

### What it is
High-performance ANN library with Rust bindings via CXX bridge. Supports HNSW, quantized storage (INT8, FP16, binary). 4,000 GitHub stars, v2.24.0.

### vs. hnsw_rs
At 10K entries: no measurable performance difference. Advantages only at > 500K entries:
- Quantized storage: INT8 reduces memory 4x
- Throughput: 131K–274K QPS vs hnsw_rs ~100K QPS (at different scales)
- Serialization: more mature

**Disadvantage**: Adds C++ build-time dependency via CXX. hnsw_rs is pure Rust with SIMD (simdeez_f already in Cargo.toml).

### Verdict: **DEFER**
No functional benefit at current corpus size. Pure-Rust hnsw_rs has no external build dependency. Revisit when corpus exceeds ~500K entries.

---

## Option 8: ColBERT (Late Interaction)

### Token-level storage at 10K entries
- ColBERT: 50 tokens × 128 dims × 4 bytes = 25 KB/document → **250 MB for 10K docs**
- colbert-small (96-dim): **192 MB for 10K docs**
- Model weight: 0.44 GB (colbertv2.0) or 0.13 GB (colbert-small)
- **Total: 320–700 MB RAM overhead**

Unimatrix's entire SQLite database for 10K entries is likely under 50 MB. ColBERT's index would be 4–5x the entire rest of the database.

### MaxSim complexity
Q=32 query tokens × D=50 doc tokens × 128 dims = ~200K multiply-adds per document × 10K docs = ~2B operations per query. Even with SIMD ndarray: 20–100 ms for a full corpus scan. Requires offline pre-indexing.

### ONNX models available
colbertv2.0 (0.44 GB), answerai-colbert-small-v1 (0.13 GB), jina-colbert-v2 — all available via ort. `embed_anything` and fastembed-rs provide ColBERT support.

### Verdict: **REJECT at this scale**
Memory footprint disproportionate to corpus size. Cross-encoder re-ranking (Task 4) provides comparable late-interaction retrieval quality at a fraction of memory cost. Revisit if Unimatrix moves to a dedicated server deployment with > 100K entries.

---

## Key Finding

> Unimatrix is one ort session away from neural re-ranking.

The `NeuralModel` trait already anticipates this. The `tokenizers` crate is already present. The CO_ACCESS table already holds the graph structure for petgraph. The maximum ROI path — cross-encoder re-ranking via ort + petgraph graph expansion + BM25 sparse hybrid — requires only new logic, no new infrastructure decisions, all within the existing dependency tree, at a combined cost of **3–6 days**.
