# FINDINGS-RAW: RuVector Re-Evaluation — Targeted Learning for a Knowledge Lifecycle Engine

**Spike**: ass-052
**Date**: 2026-04-21
**Approach**: unimatrix-first code audit + external code audit (empirical)
**Confidence**: empirical — all capability claims code-verified; README-only claims flagged

---

## §0 — Unimatrix Architecture Baseline

Queried Unimatrix via context_briefing and context_search across five mechanism areas before reading any RuVector code. The following table is the reference against which all RuVector findings are evaluated.

| Mechanism Area | Decision Made | Reason Recorded | Open Question / Revisit Flag |
|---|---|---|---|
| **Vector index (HNSW)** | hnsw_rs v0.3.3 with anndists DistDot | Pure Rust (no FFI), FilterT trait for pre-filtering, active maintenance, 280K+ downloads, built-in persistence. usearch (C++ FFI) rejected; instant-distance (stale Jun 2023) rejected; hora (abandoned Aug 2021) rejected. | Deletion limitation known at decision time: "Harder: No deletion API." Accepted cost, not an open gap. |
| **Compaction strategy (ADR-004)** | Build-new-then-swap, VECTOR_MAP-first ordering, re-embedding from content | hnsw_rs has no point deletion API. Full rebuild required. If in-place mutation fails mid-way, old index destroyed. Re-embedding from content uses current model; raw vector retrieval API not exposed by hnsw_rs. | "Narrow crash window between VECTOR_MAP write and in-memory swap has graceful degradation." No open question — design accepted. |
| **Co-access / graph signal** | Co-access signal fully migrated to PPR graph topology (crt-032, entry #3785). GRAPH_EDGES table (StableGraph, analytics.db) with 5 typed edge types. w_coac=0.0 default. PPR expander: HNSW(k=20) → BFS expand (max_candidates=200) → PPR → top-k. MRR +0.0122 vs. baseline. | Direct co-access boost (w_coac=0.10) was measured redundant with PPR: zero CC@5/ICD difference at 4349+4467 scenarios. petgraph StableGraph now in production — the "no graph primitives" gap from ASS-022 is closed. | Co-access promotion cycle bug fixed (#476). Open carry-forwards: #477, #471, #510 — non-blocking for this spike. |
| **Scoring / learning pipeline** | conf-boost-c formula: w_sim=0.50, w_conf=0.35, w_nli=0.00. Phase signals via PhaseFreqTable (crt-050). PPR expander enabled. W3-1 GNN deferred — after Wave 2 + ASS-029 architecture spike + behavioral data accumulates. Thompson Sampling deferred — after PPR ICD measured. | ASS-031 designed a 5121-param graph-feature-enriched MLP. ASS-032 identified the feedback loop failure: training labels come from entries the current formula surfaced only. Bandit preferred for Modes 1/2 (built-in exploration). W3-1 architecture designed but delivery blocked pending Wave 2 completion. | Extension point pre-designed in dsn-001 (entry #3247): load_learned_weights() hook at top of resolve_confidence_params(). |
| **Embedding boundary (ONNX)** | Raw ort + tokenizers, no fastembed wrapper. sentence-transformers/all-MiniLM-L6-v2, 384-dim, f32. Lazy-load with EmbedServiceHandle state machine. | fastembed pins exact ort version (conflict risk), limited model catalog, no fine-tuning control. Inference-only (no training). ONNX boundary is stable production dependency. | SimCSE fine-tuning deferred: corpus needs ≥2,000 active entries; requires offline Python step. |
| **Lifecycle / schema** | Active/Deprecated/Quarantined/Proposed states. SHA-256 hash chain. Prune pass removes quarantined HNSW points every maintenance tick. Heal pass re-embeds embedding_dim=0 entries. Restore must re-insert into HNSW. | Correction chains are cryptographically verifiable. State transitions affect HNSW index membership. Prune-before-heal-before-compact tick ordering enforced (bugfix-444). | No open lifecycle gaps relevant to this spike. |

**Critical architecture shift since ASS-022**: The co-access graph is no longer a scalar boost. As of crt-042/044/045 (PPR expander), Unimatrix has a live typed graph (GRAPH_EDGES, petgraph StableGraph) with 5 edge types, BFS expansion, and Personalized PageRank — all in production, eval-validated at MRR +0.0122. ASS-022's conclusion that "graph primitives" were a gap is no longer accurate.

---

## §1 — RuVector State-of-Repository Snapshot (2026-04-21)

**Repository metadata (code-verified)**:
- Version: 2.2.0 (released April 20, 2026, one day before this spike)
- License: MIT (code-verified from LICENSE file and Cargo.toml workspace)
- MSRV: 1.77, Edition 2021
- 130+ member crates, 100 directories enumerated in crates/
- Core dependencies: hnsw_rs (same library as Unimatrix), redb (former Unimatrix dependency), ndarray, tokio, rayon, ort + tokenizers (same stack as Unimatrix embed)
- Recent commits (April 18–21, 2026): ruvector-graph updates (batch edge deletion, FloatArray for zero-copy embedding storage, fused callback API for edge retrieval). No GNN-specific commits visible in this window.
- Note: changelog is stale — latest documented entry is v2.0.5 (Feb 26, 2026) despite v2.2.0 release. Cannot code-verify what changed between v2.0.5 and v2.2.0 without reading every commit.

**GNN capability inventory (ruvector-gnn crate, code-verified)**:

Implemented and verified in code:
- Five neural network components: Linear, LayerNorm, MultiHeadAttention, GRUCell, RuvectorLayer
- Standard EWC (not EWC++) with diagonal Fisher approximation — structurally identical to unimatrix-adapt's existing EwcState
- Reservoir sampling replay buffer with Welford distribution stats — structurally identical to Unimatrix's TrainingReservoir
- Adam optimizer with momentum and bias correction
- InfoNCE contrastive loss, local contrastive loss, MSE, SCE for GraphMAE
- GraphMAE (masked autoencoder for graph data)
- Four query modes: VectorSearch, NeuralSearch, SubgraphExtraction, DifferentiableSearch
- Differentiable search: soft-attention exhaustive cosine similarity over candidates (NOT HNSW-backed — exhaustive O(n) scan)

NOT implemented (code-verified absent):
- No HNSW integration in the GNN forward pass. layer.rs accepts pre-computed neighbor embeddings as inputs; it does not query the HNSW graph.
- No query pattern learning. training.rs has no mechanism for learning from query access patterns.
- No lifecycle awareness. No Active/Deprecated/Quarantined concept.
- No category diversity mechanism. replay.rs tracks statistical distribution of query embedding vectors; no categorical diversity tracking.

README-only claims (not backed by audited code):
- "GNN re-ranks neighbors using learned attention weights" — attention in layer.rs operates on pre-computed embeddings, not on HNSW neighbors. No HNSW wiring present.
- "Updates happen in under 1ms" — no benchmark verifies this. Usage example in lib.rs is marked `ignore` (will not compile).
- INT8/FP16 quantization — mentioned in README feature table; no implementation found in audited source.
- LSTM aggregation modes — listed as available; no LSTM layer found in layer.rs.
- Batch processing with Rayon — claimed but not present in training or forward-pass code paths audited.

**Graph capability inventory (ruvector-graph crate, code-verified)**:

Implemented:
- redb-backed persistence (NODES, EDGES, HYPEREDGES, METADATA tables)
- Basic single-hop traversal: get_outgoing_edges, get_incoming_edges, get_edges_for_nodes (fused callback), has_edge
- Batch edge deletion: delete_edges_batch (added April 19, 2026)
- PropertyValue::FloatArray for zero-copy embedding storage (added April 19, 2026)
- Cypher module present in module tree (VectorCypherParser exported from lib.rs)
- ACID transactions via redb

NOT implemented (code-verified absent):
- No multi-hop traversal. API provides no path search beyond depth-1.
- No Personalized PageRank or any graph ranking algorithm.
- No integration with HNSW vector index in any search path.
- No combined graph+vector query in a single operation.

**HNSW index (ruvector-core crate, code-verified)**:
- Same hnsw_rs library as Unimatrix
- Deletion: "hnsw_rs doesn't support direct deletion. We remove from our mappings but the graph structure remains. This is a known limitation of HNSW." — from index/hnsw.rs comment (code-verified)
- No compaction. Deleted vectors remain in the HNSW graph permanently.
- No filtered/predicate search. Search accepts query vector and k only.
- No quantization. Full-precision Vec<f32>.
- Parameters exposed: m, ef_construction, ef_search (per-query override), max_elements — same parameter surface as Unimatrix's VectorConfig.

**Embedding pipeline (ruvector-core crate, code-verified)**:
- EmbeddingProvider trait: embed(text: &str) → Vec<f32> — takes text, not vectors
- Four providers: HashEmbedding (non-semantic placeholder), OnnxEmbedding, ApiEmbedding, CandleEmbedding (stub)
- OnnxEmbedding uses same ort + tokenizers stack as Unimatrix
- No mechanism to inject externally-generated embeddings into the VectorDB pipeline. Insert path requires text input.

---

## §2 — GNN Query Learning: Verdict

**Verdict**: (C) Solves a different problem from what was described in ASS-022; not applicable to Unimatrix's injection pipeline.

**What RuVector's GNN actually does (code-verified)**: Provides ML training primitives — message passing layers, EWC regularization, replay buffer, Adam optimizer — for general graph representation learning tasks (node classification, link prediction, GraphMAE). It does not analyze HNSW query patterns, does not consume query access logs, and has no connection to retrieval ranking.

**Does it avoid the ASS-031 feedback loop failure?** No. RuVector's training loop is supervised binary classification or contrastive learning on labeled pairs. Both inherit the same selection bias: training data comes only from entries the current formula surfaced. replay.rs tracks statistical distribution shift in query embedding statistics (Welford mean/variance) but not categorical diversity. The 6% access penetration in lesson-learned entries would be invisible to this sampling strategy. The feedback loop failure mode from ASS-031 and ASS-032 applies identically.

**Fit-gate triage**: REJECT — all five gates fail (see §7).

---

## §3 — Graph-Augmented Retrieval: Verdict

**Verdict**: (B) Unimatrix's existing petgraph + PPR is the correct solution; RuVector's graph component is incompatible and offers no improvement.

**Critical context**: ASS-022 identified "graph primitives" as a gap. That gap was closed by crt-042/044/045. The PPR expander is live, eval-validated (MRR +0.0122, 2,096 scenarios), and co-access signal has already been migrated into it. The w_coac scalar boost was zeroed (entry #3785) precisely because PPR subsumes it.

**What RuVector's graph provides vs. what Unimatrix already has**:
- Unimatrix: petgraph StableGraph, BFS expansion, Personalized PageRank, 5 typed edge types (Informs/Supports/CoAccess/Contradicts/SupersededBy), behavioral edge emission, goal-conditioned briefing blending — eval-validated
- RuVector: redb-backed depth-1 traversal, no PPR, no multi-hop, no vector integration, no eval baseline

RuVector's graph layer is a foundational storage primitive. It does not address Unimatrix's specific multi-hop co-access traversal need, cannot integrate with an external SQLite-persisted graph (Unimatrix's GRAPH_EDGES table), and uses an incompatible persistence backend (redb vs. SQLite).

**Fit-gate triage**: REJECT — all applicable gates fail (see §7).

---

## §4 — HNSW Comparison: Verdict

**Verdict**: No migration opportunity exists. Unimatrix's VectorIndex is strictly more capable than RuVector's HnswIndex in every dimension relevant to Unimatrix.

**Comparison table**:

| Capability | Unimatrix VectorIndex | RuVector HnswIndex |
|---|---|---|
| HNSW library | hnsw_rs v0.3.3 | hnsw_rs (same) |
| Deletion handling | Tombstone-only + full compaction (build-new-then-swap, VectorIndex::compact(), VECTOR_MAP-first ordering, atomically swaps in-memory graph) — production-proven | Tombstone-only ("graph structure remains") — no compaction |
| Filtered/predicate search | Yes — FilterT trait, EntryIdFilter with allow-list of data IDs. Pre-filter during graph traversal. Code-verified in search_filtered(). | Not supported. Search accepts query vector and k only. |
| ef_construction, M, ef_search | Configurable via VectorConfig. ef_search overridable per query. | Configurable: m, ef_construction, ef_search (per-query), max_elements. Same parameters. |
| Quantization | Not implemented (f32 only) | Not implemented (f32 only) |
| Bidirectional IdMap | Yes — entry_to_data + data_to_entry HashMaps, O(1) both directions. VECTOR_MAP is crash-safe source of truth. | Internal mapping; no crash-safe SQLite/redb source of truth found in audit. |

Both systems use the same underlying hnsw_rs library with the same deletion limitation. Unimatrix has already solved the deletion/compaction problem that RuVector has not. Filtered search (needed because Active entries are a minority of total indexed entries after Deprecated/Quarantined accumulation) is present in Unimatrix but absent in RuVector.

**Fit-gate triage**: REJECT — all applicable gates fail (see §7).

---

## §5 — Embedding Pipeline Coexistence: Verdict

**Verdict**: No native path for external embedding consumption in the standard VectorDB pipeline. The EmbeddingProvider trait could theoretically be wrapped for passthrough, but no utility path exists and the integration cost exceeds implementing the equivalent in Unimatrix's existing infrastructure.

**For the GNN specifically**: RuvectorLayer.forward() accepts pre-computed embeddings (Array2/Array3 inputs). At the mathematical level, 384-dim f32 vectors from Unimatrix's ONNX pipeline could be passed in. However, there is no integration path from Unimatrix's GRAPH_EDGES + HNSW candidate set to the RuvectorLayer input format. Assembling the neighbor_embeddings Array3 and edge_weights Array2 from Unimatrix's graph state would require building all the integration plumbing that RuVector does not provide — equivalent work to implementing the same functionality natively using Unimatrix's existing ndarray infrastructure.

**Fit-gate triage**: REJECT — Gate 4 fails (see §7).

---

## §6 — Targeted Learning Assessment Against Unimatrix's Specific Characteristics

**Corpus scale (1K–10K entries)**: No RuVector techniques optimized for small high-precision corpora found. Differentiable search is exhaustive O(n) — acceptable at this scale but provides no benefit over Unimatrix's HNSW + PPR pipeline.

**Knowledge lifecycle**: RuVector has no lifecycle concept. No mechanism for Active/Deprecated/Quarantined state transitions. The prune-heal-compact tick ordering requirement (bugfix-444) has no analogue.

**Category imbalance (6% access penetration in lesson-learned, 0.43 confidence gap)**: RuVector's replay.rs tracks distribution shift via Welford statistics over query embedding vectors, not categorical diversity. The imbalance would be invisible. Same feedback loop failure mode applies.

**Correction chains**: No concept in RuVector. Graph versioning is implicit redb key-value updates. No SHA-256 hash linking, no correction_count signal.

**Domain pack configurability**: RuVector's GNN and graph layer make no category/phase assumptions. Neutral — provides nothing, breaks nothing.

---

## §7 — Fit-Gate Triage Summary

| Mechanism | Gate 1 (content type) | Gate 2 (lifecycle) | Gate 3 (schema-agnostic) | Gate 4 (arch fit) | Gate 5 (no prior rejection) | Verdict |
|---|---|---|---|---|---|---|
| GNN query learning | FAIL | FAIL | FAIL | FAIL | FAIL | **REJECT** |
| Graph-augmented retrieval | FAIL | FAIL | N/A | FAIL | FAIL | **REJECT** |
| HNSW index adoption | FAIL | N/A | N/A | FAIL | FAIL | **REJECT** |
| Embedding coexistence | N/A | N/A | N/A | FAIL | N/A | **REJECT** |
| EWC from ruvector-gnn | UNCERTAIN | N/A | PASS | UNCERTAIN | PASS | **UNCERTAIN** |

### EWC Evaluation Protocol (the only UNCERTAIN result)

**What it is**: Standard EWC (not EWC++), diagonal Fisher, consolidate() + penalty() + gradient(). ~220 lines of clean Rust, MIT license. Structurally similar to unimatrix-adapt/src/regularization.rs EwcState.

**Why UNCERTAIN**:
- Gate 1 uncertainty: Cannot confirm without measurement whether ruvector-gnn's EWC performs differently from Unimatrix's existing EwcState on the class-imbalance scenario. ASS-032 noted EWC (standard) is insufficient for high-confidence dominant-category distributions because high-confidence entries generate near-zero FIM, failing to protect underrepresented category weights. This applies to both implementations equally — the uncertainty is whether EWC is the right algorithm at all, not which implementation is better.
- Gate 4 uncertainty: Taking a crate dependency brings ndarray 0.17.2 (version compatibility with unimatrix-learn TBD) + ruvector-core as transitive dependency. Alternatively, copy the 220 lines (MIT license permits this).

**Evaluation protocol**:
- What to measure: On a synthetic corpus with 10% high-access (decision-like) + 90% low-access (lesson-learned-like), compare W3-1 training loss convergence and CC@5 using: (a) no regularization, (b) current unimatrix-adapt EwcState, (c) ruvector-gnn EWC, (d) DER++ (ASS-032 recommendation)
- Metric threshold: CC@5 must improve ≥0.05 over (b) to justify any adoption over the current implementation
- Experiment requires: W3-1 RelevanceScorer training infrastructure (does not yet exist — blocked on Wave 2 + ASS-029)
- Investment: S/M (< 1 day hands-on once W3-1 training harness exists)
- Pre-condition: Cannot execute until W3-1 training infrastructure ships. When W3-1 training is being implemented, include ruvector-gnn EWC alongside DER++ in a single comparison run rather than a separate spike.

---

## §8 — Adoption Map

**No mechanisms passed the §7 fit gate as HIGH_FIT.**

The one UNCERTAIN result (EWC from ruvector-gnn) has an evaluation protocol defined in §7. Per the scope's anti-chasing rule, the evaluation protocol is the deliverable — not an adoption recommendation. The protocol cannot execute until W3-1 training infrastructure exists (post-Wave 2).

---

## Unanswered Questions

**Q: What specifically changed in RuVector v2.0.5 → v2.2.0 (mid-March to April 2026)?**
The changelog was not updated for v2.2.0. Visible commit messages (April 18–21) show ruvector-graph improvements, no GNN learning changes. A full audit of every commit in this range would be needed to be exhaustive. Not pursued — the mechanism-specific code audits in §2–5 are sufficient for the evaluation, and no GNN changes were found that would change the REJECT verdicts.

---

## Out-of-Scope Discoveries

**1. The primary ASS-022 gaps are closed**: The five "learning opportunities" identified in ASS-022 map to production state as of 2026-04-21. Graph primitives: done (GRAPH_EDGES, petgraph, PPR expander — eval-validated). Edge packaging: done (5 typed edge types). WASM: deferred (not a gap for this use case). Performance at scale: by design out of scope (single-node). GNN-based query learning: designed (W3-1), delivery deferred post-Wave 2. This confirms that ASS-022's "complementary" framing was accurate and that the right research question is now W3-1 architecture specifics, not RuVector adoption. A future spike for RuVector is not warranted unless RuVector ships a dedicated knowledge-lifecycle-specific learning mechanism or a production-validated query pattern adaptation system.

**2. DER++ vs. EWC remains unresolved for W3-1**: ASS-032 recommended DER++ (Dark Experience Replay) over EWC for Unimatrix's class-imbalance scenario. This spike confirms ruvector-gnn implements standard EWC (not EWC++, not DER++). The DER++ direction from ASS-032 remains the stronger recommendation and should be included in the EWC comparison protocol above when W3-1 training infrastructure is built.

**3. ruvector-graph PropertyValue::FloatArray (added April 19, 2026)**: Zero-copy embedding storage as a graph node/edge property. Interesting pattern for embedding-graph co-location but uses redb, incompatible with Unimatrix's SQLite architecture. File as an industry direction observation only.

**4. ruvector-gnn GraphMAE**: Self-supervised masked autoencoder for graph representation learning (no labels needed). Not applicable to Unimatrix's text-entry knowledge base, but the label-free self-supervised approach is relevant context for the SimCSE embedding fine-tuning direction identified in ASS-032. If SimCSE is evaluated, GraphMAE-style masking over text content could be a complementary approach. Future spike candidate only.

---

## Recommendations Summary

- **GNN query learning**: REJECT. RuVector's GNN is graph ML research infrastructure (message passing, GraphMAE) that operates on pre-computed node embeddings and requires supervised/contrastive training labels. It does not analyze HNSW query patterns, has no lifecycle awareness, and inherits the same selection bias feedback loop that disqualified ASS-031. The ASS-022 finding that RuVector's GNN is "more sophisticated than Unimatrix's current co-access boosting" was based on README claims not backed by code. The current PPR expander (eval-validated, MRR +0.0122) is already more capable than anything in ruvector-gnn's retrieval path.

- **Graph-augmented retrieval**: REJECT. Unimatrix already ships the graph-augmented retrieval capability ASS-022 identified as a gap (PPR expander, crt-042/044/045). RuVector's graph layer is a redb-backed depth-1 traversal store with no PPR, no multi-hop, no vector integration, and an incompatible persistence backend. Replacing Unimatrix's petgraph + PPR would be a regression.

- **HNSW index adoption**: REJECT. Both systems use hnsw_rs. Unimatrix's VectorIndex is strictly more capable (compaction via build-new-then-swap, FilterT pre-filtering, bidirectional IdMap). No migration provides any benefit.

- **Embedding coexistence**: REJECT. No native path for external embedding consumption in RuVector's standard pipeline. Integration cost exceeds implementing equivalent functionality natively.

- **EWC from ruvector-gnn**: UNCERTAIN. Valid algorithm, MIT license, ~220 lines. Cannot recommend adoption without the W3-1 training comparison harness. When W3-1 training infrastructure is implemented (post-Wave 2), run a single comparison: no regularization vs. current EwcState vs. ruvector-gnn EWC vs. DER++, on a synthetic class-imbalanced corpus.

- **Overall verdict**: No RuVector mechanisms pass the fit gate for adoption. The self-learning pipeline next step is W3-1 delivery after Wave 2 + ASS-029 architecture spike, not further RuVector evaluation. The research debt from ASS-022 and ASS-032 is fully discharged by this spike and by the production features already shipped.
