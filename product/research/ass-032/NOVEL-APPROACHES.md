# ASS-032: Novel Self-Learning Knowledge Engine Approaches

**Generated**: 2026-03-25
**Scope**: Research survey of seven novel directions for self-learning knowledge engines with Rust implementation feasibility assessment

---

## Synthesis First: Recommended Architecture

### Two Core Principles from the Literature

1. **Signal is the bottleneck, not algorithms.** Mem0 and Zep/Graphiti both outperform ML-heavy systems at current corpus scales using clean signal + simple aggregation. The research uniformly shows this beats sophisticated models on sparse feedback.

2. **Graph topology outperforms vector-only at retrieval time.** HippoRAG's 20% improvement over standard RAG on multi-hop queries comes from Personalized PageRank over an entity graph, not from a better embedding model.

### Top 3 Novel Approaches by Expected Impact

**Rank 1 — Graph-Traversal Retrieval via Personalized PageRank**
- Evidence: HippoRAG 20% multi-hop improvement. Graph already exists in `GRAPH_EDGES`. No new ML, no training data, no new dependencies beyond `petgraph` (already used).
- Change surface: post-HNSW scoring step only.
- Implementation: ~150 lines Rust.

**Rank 2 — Phase-Conditioned Retrieval via Contextual Frequency Table**
- Evidence: Contextual bandit literature confirms phase+category is among the most predictive categorical context features. RA-DIT demonstrates the retrain-retriever-from-feedback loop.
- Self-improves with every query with zero training.
- Implementation: ~150 lines Rust, background tick rebuild.

**Rank 3 — Active Gap Detection via Query Cluster Confidence Analysis**
- The only approach that identifies *missing* knowledge rather than improving surfacing of *existing* knowledge.
- Mechanism: cluster queries by (phase, category), flag clusters with mean_similarity < 0.55 and count ≥ 5 as gaps.
- All raw data already present in QUERY_LOG and injection_log.
- Implementation: tick-based, no schema migration.

---

## Task 1: Active Learning for Knowledge Gap Detection

### Key Finding

Production RAG systems detect knowledge gaps through three compounding signals: (1) low-confidence retrieval (top-k similarity scores below threshold), (2) iterative query rephrasing by the same agent in a short window, and (3) downstream error correlation — retrieval that precedes a low-helpfulness mark or agent correction. The most capable framework (Knowledge-Aware Iterative Retrieval, arxiv:2503.13275, Song et al. 2025) tracks "unresolved gaps" as explicit items and reruns the cycle until no gaps remain.

For Unimatrix, the query space is discrete `(agent_id, phase, category)` rather than continuous free text — this makes clustering vastly more tractable. Gap candidates are `GapCandidate { phase, category, query_centroid: Vec<f32>, mean_similarity: f64, query_count: u32 }`.

### Citations
- **Knowledge-Aware Iterative Retrieval for Multi-Agent Systems** — Song et al., arxiv:2503.13275, 2025. https://arxiv.org/abs/2503.13275
- **FLARE: Active Retrieval Augmented Generation** — Jiang et al., EMNLP 2023. https://arxiv.org/abs/2305.06983

### Applicability: HIGH
Unimatrix already writes QUERY_LOG and helpful_count/unhelpful_count. Only the aggregation pass is missing. The discrete (phase, category) key makes this far cheaper than general NLP gap detection.

### Rust Feasibility
Pure arithmetic over existing SQL tables. No new crate dependencies. Use `Arc<RwLock<Vec<GapCandidate>>>` rebuilt each tick (existing pattern). Gap candidates surfaced via `context_status` response without schema migration.

---

## Task 2: Self-Supervised Contrastive Learning for Knowledge Embeddings

### Key Finding

SimCSE (Gao, Yao, Chen — EMNLP 2021, arxiv:2104.08821): in unsupervised form, uses same sentence with two dropout masks as positive pair; all other sentences as negatives. +4.2 Spearman correlation on STS benchmarks over BERT-base. Domain adaptation: meaningful embedding shift with as few as 1,000–10,000 in-domain sentences; diminishing returns above ~100K.

Unimatrix already possesses positive/negative pair signals:
- Correction chains: original + correction = hard negative pair
- Co-access pairs: co-retrieved entries = soft positives
- Contradiction edges: `Contradicts` edges = hard negatives
- Supersession chains: entry + successor = positive pair (same concept, newer phrasing)

### Citations
- **SimCSE** — Gao, Yao, Chen, EMNLP 2021. https://arxiv.org/abs/2104.08821
- **COCO-LM** — Meng et al., NeurIPS 2021. https://arxiv.org/abs/2102.08473
- **SBERT Domain Adaptation** — https://www.sbert.net/examples/sentence_transformer/domain_adaptation/README.html

### Applicability: MEDIUM (becomes HIGH at 2,000+ entries)
Current corpus too small for meaningful fine-tuning. SimCSE needs ~thousands of in-domain pairs. Signals already exist; corpus needs to grow.

### Rust Feasibility
**Blocked by ONNX inference-only constraint.** ONNX Runtime does not support fine-tuning at runtime. Path: export pairs from store → offline Python fine-tuning (SBERT + SimCSE loss) → re-export to ONNX → hot-swap model file. Periodic offline operation, not an online loop. Would require a dedicated `unimatrix-adapt-embed` pipeline distinct from the existing MicroLoRA adapter. Feasible but requires a Python build step.

---

## Task 3: Epistemic Neural Networks / Uncertainty-Aware Retrieval

### Key Finding

Epinet framework (Osband, Wen et al., NeurIPS 2023, arxiv:2107.08924): small additive subnetwork that takes base network's penultimate activations + random epistemic index vector z and outputs a prediction delta. Sampling different z values generates approximate posterior without full ensembles. Matches 100-member ensemble at ~1/10th compute.

For Unimatrix's `ConventionScorer` (32→32→1 MLP): epinet is a secondary 32→K→1 network, K=10–30 (typical epistemic index dimension). The key distinction from existing confidence composite:
- **Confidence scoring**: calibration (did this entry help historically?)
- **Epistemic uncertainty**: exploration (have we tested this ranking decision enough?)

The combination lets the system distinguish "low confidence because entry is weak" from "low confidence because this query type is rare and needs more data."

### Citations
- **Epistemic Neural Networks** — Osband et al., NeurIPS 2023. https://arxiv.org/abs/2107.08924
- **Approximate Thompson Sampling via ENNs** — Osband et al., UAI 2023. https://proceedings.mlr.press/v216/osband23a.html
- **Epinet for Content Cold Start (Meta Reels)** — arxiv:2412.04484

### Applicability: MEDIUM (premature at current scale)
Becomes relevant when query diversity across phases and agents is high enough to generate meaningful joint uncertainty estimates. Architecturally clean for small MLP.

### Rust Feasibility
Epinet: `input (32) → epistemic_index z (K=16) → concat (48) → hidden (32) → delta (1)`. ~1,600 parameters. Fits existing ndarray infrastructure. <1ms inference overhead. Training uses same reservoir feedback buffer in `unimatrix-learn`. No new crate dependencies. Main challenge: generating random epistemic index z at inference time and accumulating posterior samples.

---

## Task 4: RAG Literature on Self-Improvement

### Key Finding

Three canonical papers define the state-of-the-art:

**Self-RAG** (Asai et al., 2023, arxiv:2310.11511): trains LM with four "reflection tokens" — `[Retrieve]`, `[IsRel]`, `[IsSup]`, `[IsUse]` — inserted inline to decide whether to retrieve, whether results are relevant, whether generation is supported, whether output is useful. Outperforms ChatGPT + Llama2-chat on open-domain QA at 7B parameters.

**FLARE** (Jiang et al., EMNLP 2023, arxiv:2305.06983): generate speculatively, identify spans with low token probability, use those spans as retrieval queries, then regenerate. Low probability = uncertainty = retrieval trigger. For proactive injection: phase is a strong prior; FLARE's confidence threshold maps to `mean_similarity < threshold` gap signal.

**RA-DIT** (Lin, Chen et al., ICLR 2024, arxiv:2310.01352): two-pass fine-tuning — (1) teach LM to use retrieved context better, (2) retrain retriever based on what the LM actually used. Bi-directional signal: LM tells retriever what was useful, retriever tells LM what is retrievable. Direct analogue to Unimatrix's helpful/unhelpful feedback → scorer training loop.

### Citations
- **Self-RAG** — Asai, Wu, Wang et al., arxiv:2310.11511. https://arxiv.org/abs/2310.11511
- **FLARE** — Jiang, Xu, Gao et al., EMNLP 2023. https://arxiv.org/abs/2305.06983
- **RA-DIT** — Lin, Chen et al., ICLR 2024. https://arxiv.org/abs/2310.01352

### Applicability: HIGH
RA-DIT's bi-directional signal is exactly what the existing helpful/unhelpful flags provide. The feedback loop is partially closed; what is missing is the outer training pass that updates retriever weights based on accumulated feedback. FLARE's anticipation mechanism maps to proactive injection by phase.

### Rust Feasibility
RA-DIT's classifier training (stage 1) maps directly to the existing `ConventionScorer` training loop in `unimatrix-learn`. The feedback routing path (agent marks entry unhelpful → labeled training example for scorer) is the missing piece. Implementing the closed loop (accumulate → retrain → persist → hot-reload) is feasible entirely in Rust. Stage 2 (ONNX model fine-tuning) requires offline Python.

---

## Task 5: Knowledge Graph + Vector Hybrid Systems

### Key Finding

**HippoRAG** (Gutierrez et al., NeurIPS 2024, arxiv:2405.14831): builds knowledge graph offline, extracts named entities from query at runtime, links to graph nodes, runs Personalized PageRank to surface strongly connected context. **20% improvement over standard RAG on multi-hop QA**. 10–30x cheaper than iterative retrieval methods. PageRank propagates relevance through the graph — an entry strongly connected to many highly-retrieved entries gets a score boost even if its own embedding similarity is moderate.

**GraphRAG** (Edge et al., arxiv:2404.16130): builds hierarchical entity knowledge graph via LLM, applies Leiden community detection. Substantial improvement on "global sensemaking" queries. Overhead makes it unsuitable for real-time indexing.

For Unimatrix: `GRAPH_EDGES` table with five edge types (Supersedes, Contradicts, Supports, CoAccess, Prerequisite) is already populated. The current `graph_penalty` uses the graph for penalties only, not retrieval boosting. **Supports and Prerequisite edges are currently invisible to retrieval** — activating them as positive propagation edges is the key opportunity.

Minimum graph density for community detection: ~50 nodes + 100 edges. Below that, depth-1 BFS along Supports + CoAccess edges is sufficient. Full community detection (Leiden/Louvain) worthwhile above ~500 active entries.

### Citations
- **HippoRAG** — Gutierrez et al., NeurIPS 2024. https://arxiv.org/abs/2405.14831
- **GraphRAG** — Edge et al., 2024. https://arxiv.org/abs/2404.16130
- **Graph RAG Survey** — ACM TOIS. https://dl.acm.org/doi/10.1145/3777378
- **RAG vs GraphRAG Systematic Evaluation** — arxiv:2502.11371.

### Applicability: HIGH
Unimatrix already has the graph and `petgraph` as a dependency. The extension to retrieval boosting via neighbor-weighted PageRank is a contained change to the search pipeline. The current `MAX_CO_ACCESS_BOOST = 0.03` cap is precisely the kind of neighbor-based signal HippoRAG formalizes.

### Rust Feasibility
`petgraph` (already a dependency) provides the graph substrate. Personalized PageRank: power iteration over sparse adjacency matrix — O(|E| * iterations), ~20 iterations to converge. Personalization vector initialized from HNSW scores. For sub-1000 entry counts, per-query is feasible; above that, background pre-computation preferred. No new crate dependencies.

---

## Task 6: Continual Learning Beyond EWC++

### Key Finding

Replay-based methods **consistently outperform regularization-based (EWC, SI) for class-incremental settings** — the exact setting Unimatrix faces (heavy class imbalance; rare negative signals).

**Dark Experience Replay DER++** (Buzzega et al., NeurIPS 2020): stores logits (not just labels) from past forward passes in a replay buffer. Training minimizes both current task loss and a distillation loss toward stored logits. Matches GEM accuracy at a fraction of GEM's compute. For class imbalance: **prioritized sampling** (oversample rare negatives from buffer) + **focal loss** (down-weight well-classified majority examples) are the highest-return interventions.

EWC++ is solving the wrong problem for a knowledge store: the entries themselves do not catastrophically forget. EWC++ failure mode in biased settings (from prior research): high confidence on decision entries → vanishing FIM → fails to protect the weights that should be preserved.

PackNet and Progressive Networks: inapplicable (PackNet requires known task boundaries; ProgNN requires growing model size).

### Citations
- **DER: Dark Experience Replay** — Buzzega et al., NeurIPS 2020.
- **Uncertainty-Aware Enhanced DER** — Applied Intelligence 2024. https://dl.acm.org/doi/10.1007/s10489-024-05488-w
- **GEM** — Lopez-Paz, Ranzato, NeurIPS 2017. https://arxiv.org/abs/1706.08840
- **Continual Learning Survey 2024** — https://arxiv.org/html/2403.05175v1

### Applicability: MEDIUM
Addresses real problem (class imbalance in scorer training signal) but is a correctness improvement, not a capability improvement. Should be done, but is not top-3 by expected impact on retrieval quality.

### Rust Feasibility
DER++: extend `TrainingReservoir<T>` to store `(input, label, stored_logit)` — a type-level change. Distillation term: MSE between current logit and stored logit, added to BCE loss — two lines. Focal loss: replace BCE with `-(t*(1-y)^γ*log(y) + (1-t)*y^γ*log(1-y))` where γ=2.0. No new crates. **Recommendation: replace EWC++ with DER++ before activating W3-1 training.**

---

## Task 7: Phase-Aware and Context-Aware Retrieval

### Key Finding

**Contextual bandits literature** (arxiv:2312.14037): most predictive features in production recommendation systems are: (a) item-context interaction features (not just item and context independently), (b) recency of context signal, (c) **categorical identity of context** — sparse one-hot phase and category features perform comparably to dense neural approaches at much lower compute.

**SASRec / BERT4Rec**: model item sequences as Transformer inputs where position encodes session order. For Unimatrix: phase sequence (design → delivery → bugfix) is analogous to a session sequence. A first-order Markov table over (current_phase, last_retrieved_entries) → next_entries captures phase-transition patterns without a full Transformer.

**CARS (Context-Aware Recommender Systems)**: formalizes the problem as `relevance(entry | query, context)` rather than `relevance(entry | query)`. RouteRAG (arxiv:2512.09487) uses RL to learn when to use which retrieval path given query complexity — the "path" being analogous to Unimatrix's phase.

The current search pipeline ignores phase entirely at retrieval time. Phase is used for feature tracking but not scoring.

### Citations
- **Neural Contextual Bandits** — arxiv:2312.14037. https://arxiv.org/html/2312.14037v1
- **SASRec** — Kang, McAuley, ICDM 2018. https://arxiv.org/abs/1808.09781
- **BERT4Rec** — Sun et al., CIKM 2019. https://arxiv.org/abs/1904.06690
- **RouteRAG** — arxiv:2512.09487. https://arxiv.org/html/2512.09487

### Applicability: HIGH
Unimatrix's schema already records `phase`, `agent_id`, `feature` per query. The `(agent_id, phase, category)` tuple is exactly the contextual feature theory identifies as highest-signal. Adding a phase-conditioned score multiplier from QUERY_LOG history is the single cheapest intervention with highest expected return.

### Rust Feasibility
Phase-conditioned frequency table: `Arc<RwLock<HashMap<(Phase, Category), Vec<(u64, f32)>>>>` rebuilt each tick from QUERY_LOG. ~100 lines Rust. Full SASRec requires a Transformer (not in current stack). First-order Markov approximation (boost co-access partners in phase N+1 for entries retrieved in phase N) is already approximated by CoAccess boost logic and needs only a phase-conditioning filter.

---

## Deferred Approaches

| Approach | Reason for Deferral |
|---|---|
| SimCSE embedding fine-tuning | Requires offline Python step; corpus too small today |
| Epinet uncertainty heads | Premature until query diversity across agents is sufficient |
| Full GraphRAG (LLM entity extraction) | LLM cost at index time; unsuitable for real-time entry storage |
| SASRec / BERT4Rec session model | Transformer not in stack; Markov approximation sufficient for now |
| EWC++ replacement with DER++ | Correctness improvement, not a capability one; medium priority |
