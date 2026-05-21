# FINDINGS: RuVector Re-Evaluation — Targeted Learning for a Knowledge Lifecycle Engine

**Spike**: ass-052
**Date**: 2026-04-21
**Approach**: unimatrix-first code audit + external code audit
**Confidence**: empirical — all capability claims code-verified; README-only claims flagged

---

## Decision-Facing Summary

### §7 — Fit-Gate Triage

| Mechanism | Gate 1 (content type) | Gate 2 (lifecycle) | Gate 3 (schema-agnostic) | Gate 4 (arch fit) | Gate 5 (no prior rejection) | Verdict |
|---|---|---|---|---|---|---|
| GNN query learning | FAIL | FAIL | FAIL | FAIL | FAIL | **REJECT** |
| Graph-augmented retrieval | FAIL | FAIL | N/A | FAIL | FAIL | **REJECT** |
| HNSW index adoption | FAIL | N/A | N/A | FAIL | FAIL | **REJECT** |
| Embedding coexistence | N/A | N/A | N/A | FAIL | N/A | **REJECT** |
| EWC from ruvector-gnn | UNCERTAIN | N/A | PASS | UNCERTAIN | PASS | **UNCERTAIN** |

No mechanisms passed the fit gate as HIGH_FIT.

### §8 — Adoption Map

**Empty.** No HIGH_FIT mechanisms exist.

The one UNCERTAIN result (EWC from ruvector-gnn) has an evaluation protocol. Per the scope's anti-chasing rule, the evaluation protocol is the deliverable — not an adoption recommendation. The protocol cannot execute until W3-1 training infrastructure exists (post-Wave 2).

**EWC evaluation protocol**: When W3-1 training infrastructure is implemented, run a single comparison on a synthetic class-imbalanced corpus (10% high-access decision-like + 90% low-access lesson-learned-like): (a) no regularization, (b) current unimatrix-adapt EwcState, (c) ruvector-gnn EWC (~220 lines, MIT), (d) DER++ (ASS-032 recommendation). Metric threshold: CC@5 must improve ≥0.05 over current EwcState to justify adoption. Effort: S/M once training harness exists.

---

## §0 — Unimatrix Architecture Baseline

| Mechanism Area | Decision Made | Reason Recorded | Open Question / Revisit Flag |
|---|---|---|---|
| **Vector index (HNSW)** | hnsw_rs v0.3.3 with anndists DistDot | Pure Rust (no FFI), FilterT trait for pre-filtering, active maintenance, 280K+ downloads. usearch (C++ FFI) rejected; instant-distance (stale Jun 2023) rejected; hora (abandoned Aug 2021) rejected. | Deletion limitation known at decision time: "Harder: No deletion API." Accepted cost, not an open gap. |
| **Compaction strategy (ADR-004)** | Build-new-then-swap, VECTOR_MAP-first ordering, re-embedding from content | hnsw_rs has no point deletion API. Full rebuild required. | Narrow crash window has graceful degradation. No open question — design accepted. |
| **Co-access / graph signal** | Co-access signal fully migrated to PPR graph topology (crt-032, entry #3785). GRAPH_EDGES table (petgraph StableGraph, analytics.db) with 5 typed edge types. w_coac=0.0 default. PPR expander: HNSW(k=20) → BFS expand (max_candidates=200) → PPR → top-k. MRR +0.0122 vs. baseline. | Direct co-access boost (w_coac=0.10) measured redundant with PPR. petgraph StableGraph now in production — ASS-022's "graph primitives gap" is closed. | Open carry-forwards: #477, #471, #510 — non-blocking for this spike. |
| **Scoring / learning pipeline** | w_sim=0.50, w_conf=0.35, w_nli=0.00. Phase signals via PhaseFreqTable (crt-050). PPR expander enabled. W3-1 GNN deferred — post-Wave 2 + ASS-029. Thompson Sampling deferred — post-PPR ICD measurement. | ASS-031 GNN disqualified: feedback loop (training labels from entries current formula surfaced only). Bandit preferred for Modes 1/2. | Extension point pre-designed in dsn-001 (entry #3247): load_learned_weights() hook. |
| **Embedding boundary (ONNX)** | Raw ort + tokenizers. all-MiniLM-L6-v2, 384-dim, f32. Lazy-load EmbedServiceHandle. | fastembed pins ort version (conflict risk). ONNX boundary is stable production dependency. | SimCSE fine-tuning deferred: corpus needs ≥2,000 active entries. |
| **Lifecycle / schema** | Active/Deprecated/Quarantined/Proposed states. SHA-256 hash chain. Prune-before-heal-before-compact tick ordering (bugfix-444). | Correction chains are cryptographically verifiable. State transitions affect HNSW index membership. | No open lifecycle gaps relevant to this spike. |

**Critical architecture shift since ASS-022**: The co-access graph is no longer a scalar boost. As of crt-042/044/045 (PPR expander), Unimatrix has a live typed graph with 5 edge types, BFS expansion, and Personalized PageRank — production-validated at MRR +0.0122. ASS-022's "graph primitives gap" conclusion is no longer accurate.

---

## Findings

### Q: Does RuVector's GNN formulation avoid the feedback loop failure mode that made the ASS-031 GNN unsuitable for Unimatrix's injection pipeline?

**Answer**: No. RuVector's GNN does not analyze HNSW query patterns and has no retrieval ranking connection. It is graph ML research infrastructure (message passing, GraphMAE, contrastive learning). The feedback loop failure from ASS-031 applies identically.

**Evidence (code-verified)**:
- `layer.rs`: Five components (Linear, LayerNorm, MultiHeadAttention, GRUCell, RuvectorLayer). Attention operates on pre-computed input embeddings, not on HNSW neighbors.
- `training.rs`: No mechanism for learning from query access patterns.
- `replay.rs`: Tracks Welford mean/variance of query embedding vectors — no categorical diversity tracking. 6% access penetration in lesson-learned would be invisible.
- Training is supervised binary classification or contrastive learning on labeled pairs — inherits identical selection bias.
- README claim "GNN re-ranks neighbors using learned attention weights" is not backed by code: no HNSW wiring in any audited file.
- README claim "updates in under 1ms" has no benchmark; usage example in `lib.rs` is marked `#[ignore]`.

**Recommendation**: REJECT GNN for Unimatrix injection pipeline. Thompson Sampling and W3-1 remain the correct architecture. Do not revisit RuVector's GNN unless it ships a query-pattern adaptation system with an external retrieval integration path.

---

### Q: Is RuVector's graph component worth adopting for Unimatrix's co-access graph?

**Answer**: No. Unimatrix already ships the graph-augmented retrieval capability ASS-022 identified as a gap. RuVector's graph layer does not approach Unimatrix's current capability.

**Evidence (code-verified)**:
- ruvector-graph API: `get_outgoing_edges`, `get_incoming_edges`, `get_edges_for_nodes`, `has_edge`, `delete_edges_batch`. No multi-hop traversal, no PPR, no path search beyond depth-1.
- Unimatrix PPR expander: petgraph StableGraph, BFS to max_candidates=200, Personalized PageRank, 5 typed edge types, goal-conditioned briefing blending. MRR +0.0122, 2,096 scenarios.
- ruvector-graph uses redb persistence — incompatible with Unimatrix's SQLite-first GRAPH_EDGES table.
- w_coac=0.10 scalar boost was zeroed (entry #3785) because PPR subsumes it. Replacing PPR with depth-1 traversal would be a regression.

**Recommendation**: REJECT ruvector-graph. petgraph + PPR is in production and eval-validated. The graph-augmented retrieval gap from ASS-022 is closed.

---

### Q: Does RuVector's HNSW implementation offer a meaningful improvement over Unimatrix's VectorIndex?

**Answer**: No. Both use the same hnsw_rs library. Unimatrix's VectorIndex is strictly more capable in every evaluated dimension.

**Evidence (code-verified)**:

| Capability | Unimatrix VectorIndex | RuVector HnswIndex |
|---|---|---|
| HNSW library | hnsw_rs v0.3.3 | hnsw_rs (same) |
| Deletion handling | Tombstone + full compaction (build-new-then-swap, VectorIndex::compact(), VECTOR_MAP-first ordering, atomically swaps in-memory graph) — production-proven | Tombstone-only ("graph structure remains" — from index/hnsw.rs comment, code-verified). No compaction. |
| Filtered/predicate search | Yes — FilterT trait, EntryIdFilter with allow-list. Pre-filter during graph traversal. Code-verified in search_filtered(). | Not supported. Search accepts query vector and k only. |
| ef_construction, M, ef_search | Configurable via VectorConfig; ef_search overridable per query. | Same parameters exposed. |
| Quantization | f32 only | f32 only — README claims INT8/FP16; no implementation found. |
| Crash-safe bidirectional IdMap | Yes — entry_to_data + data_to_entry HashMaps, VECTOR_MAP as SQLite source of truth. | Internal mapping; no crash-safe external source of truth found. |

**Recommendation**: REJECT migration. No capability improvement exists. Unimatrix already solved the deletion/compaction problem (ADR-004) that RuVector has not addressed.

---

### Q: Can RuVector components consume externally generated embeddings?

**Answer**: Not through the standard VectorDB pipeline. The EmbeddingProvider trait requires text input. The GNN layer accepts pre-computed embeddings at the type level, but building the integration plumbing from Unimatrix's GRAPH_EDGES + HNSW candidate set to the required Array3 input format is equivalent to implementing the functionality natively.

**Evidence (code-verified)**:
- EmbeddingProvider trait signature: `embed(text: &str) → Vec<f32>`. Four providers: HashEmbedding, OnnxEmbedding, ApiEmbedding, CandleEmbedding (stub). No passthrough/precomputed-vector provider.
- RuvectorLayer.forward() accepts ndarray Array2/Array3 — 384-dim f32 vectors are type-compatible, but assembling neighbor_embeddings and edge_weights arrays from Unimatrix's graph state requires all integration plumbing RuVector does not provide.

**Recommendation**: REJECT adoption via crate dependency. No benefit over implementing equivalent functionality using Unimatrix's existing ndarray infrastructure.

---

### Q: Which RuVector mechanisms, if any, pass the fit gate for Unimatrix adoption?

**Answer**: None pass as HIGH_FIT. One mechanism (EWC from ruvector-gnn) is UNCERTAIN. Evaluation protocol defined in §7/§8 above.

**Recommendation**: No adoption. The EWC evaluation protocol should be folded into the W3-1 training harness comparison (post-Wave 2) as a one-line addition, not a separate spike.

---

## §1 — RuVector State-of-Repository Snapshot (2026-04-21)

**Repository metadata (code-verified)**:
- Version: 2.2.0 (released April 20, 2026, one day before this spike)
- License: MIT (code-verified — ASS-022 recorded Apache-2.0; that record is stale)
- MSRV: 1.77, Edition 2021
- 130+ member crates, 100 directories
- Core dependencies: hnsw_rs (same as Unimatrix), redb (former Unimatrix dependency), ndarray, tokio, rayon, ort + tokenizers (same ONNX stack as unimatrix-embed)
- Recent commits (April 18–21): ruvector-graph improvements (batch edge deletion, FloatArray for zero-copy embedding storage, fused callback API). No GNN learning changes in this window.
- Changelog stale: latest documented entry is v2.0.5 (Feb 26, 2026) despite v2.2.0 release.

**README-only claims not backed by audited code**:
- "GNN re-ranks neighbors using learned attention weights" — no HNSW wiring in layer.rs
- "Updates happen in under 1ms" — no benchmark; usage example in lib.rs marked `#[ignore]`
- INT8/FP16 quantization — no implementation found
- LSTM aggregation modes — no LSTM layer in layer.rs
- Batch processing with Rayon — not present in training or forward-pass code paths

---

## §6 — Targeted Learning Assessment Against Unimatrix's Specific Characteristics

**Corpus scale (1K–10K entries)**: No RuVector techniques optimized for small high-precision corpora. Differentiable search is exhaustive O(n) — provides no benefit over Unimatrix's HNSW + PPR pipeline.

**Knowledge lifecycle**: RuVector has no lifecycle concept. No mechanism for Active/Deprecated/Quarantined state transitions. The prune-heal-compact tick ordering requirement (bugfix-444) has no analogue.

**Category imbalance (6% access penetration in lesson-learned, 0.43 confidence gap)**: replay.rs tracks Welford statistics over query embedding vectors, not categorical diversity. The imbalance would be invisible. Same feedback loop failure mode as ASS-031.

**Correction chains**: No concept in RuVector. Graph versioning is implicit redb key-value updates. No SHA-256 hash linking, no correction_count signal.

**Domain pack configurability**: RuVector's GNN and graph layer make no category/phase assumptions — neutral. Provides nothing, breaks nothing.

---

## Unanswered Questions

**Q: What specifically changed in RuVector v2.0.5 → v2.2.0?**

The changelog was not updated for v2.2.0. Visible commits (April 18–21) show ruvector-graph improvements, no GNN learning changes. A full commit-by-commit audit was not pursued — the mechanism-specific code audits in §2–5 are sufficient and no GNN changes were found that would alter the REJECT verdicts.

---

## Out-of-Scope Discoveries

**1. ASS-022 gaps are fully closed**: The five "learning opportunities" from ASS-022 map to production state as of 2026-04-21. Graph primitives: done (GRAPH_EDGES, petgraph, PPR expander). Edge packaging: done (5 typed edge types). WASM: deferred (not a gap). Performance at scale: out of scope by design (single-node). GNN-based query learning: designed (W3-1), delivery deferred post-Wave 2. A future RuVector spike is not warranted unless RuVector ships a dedicated knowledge-lifecycle-specific learning mechanism.

**2. DER++ vs. EWC remains unresolved for W3-1**: ASS-032 recommended DER++ over EWC for the class-imbalance scenario. ruvector-gnn implements standard EWC (not EWC++, not DER++). Include all four options (no regularization, current EwcState, ruvector-gnn EWC, DER++) in the W3-1 training harness comparison run.

**3. ruvector-graph PropertyValue::FloatArray**: Zero-copy embedding storage as a graph node/edge property (added April 19, 2026). Incompatible with Unimatrix's SQLite architecture. Industry direction observation only.

**4. ruvector-gnn GraphMAE**: Self-supervised masked autoencoder (no labels required). Not applicable to Unimatrix's text-entry knowledge base. Relevant context for SimCSE fine-tuning direction (ASS-032): GraphMAE-style masking over text content could complement SimCSE. Future spike candidate only.

**5. License discrepancy**: ASS-022 recorded RuVector as Apache-2.0. Code-verified as MIT as of v2.2.0. Both are compatible with Unimatrix. The ASS-022 license record is stale.

---

## Recommendations Summary

- **GNN query learning**: REJECT. Solves graph ML research problems, not query pattern adaptation. No HNSW wiring. Same selection bias feedback loop as ASS-031. ASS-022's "more sophisticated" claim was based on README copy not backed by code.
- **Graph-augmented retrieval**: REJECT. Unimatrix already ships the capability ASS-022 identified as a gap (PPR expander, crt-042/044/045). ruvector-graph is depth-1, no PPR, incompatible persistence backend. Adoption would be a regression.
- **HNSW index adoption**: REJECT. Both systems use hnsw_rs. Unimatrix's VectorIndex is strictly more capable (compaction, FilterT pre-filtering, crash-safe VECTOR_MAP). No migration benefit.
- **Embedding coexistence**: REJECT. No native path for external embedding consumption in standard pipeline. Integration cost equals implementing natively.
- **EWC from ruvector-gnn**: UNCERTAIN. Valid algorithm, MIT license, ~220 lines. Fold into the DER++ comparison run when W3-1 training infrastructure ships post-Wave 2. No separate spike needed.
- **Overall verdict**: No RuVector mechanisms pass the fit gate. The self-learning pipeline next step is W3-1 delivery after Wave 2 + ASS-029 architecture spike. The research debt from ASS-022 and ASS-032 is fully discharged by this spike and by the production features already shipped.
