# ASS-052: RuVector Re-Evaluation — Targeted Learning for a Knowledge Lifecycle Engine

**Date**: 2026-04-21
**Tier**: 2 — informs crt-phase and col-phase feature roadmap; no active delivery blocked
**Feeds**: self-learning pipeline (W3-1 follow-on), graph-augmented retrieval direction, HNSW tuning decisions
**Related**: ASS-022 §02 (prior broad comparison, March 2026), ASS-032 §6.2 (ruvector as one of nine Rust ML libs surveyed), ASS-031 (W3-1 GNN architecture)

---

## Question

ASS-022 established that RuVector and Unimatrix are complementary, not head-to-head competitors, and identified five high-level learning areas (graph primitives, edge packaging, WASM, performance at scale, GNN-based query learning). ASS-032 briefly surveyed ruvector as part of a broader Rust ML ecosystem scan.

Neither spike asked the targeted question: **given what we now know about Unimatrix's production characteristics and its specific design as a knowledge lifecycle engine — not a general vector database — which specific RuVector mechanisms are worth adopting, adapting, or discarding, and what would adoption actually cost?**

Since ASS-022 (March 2026), Unimatrix has evolved substantially: we now have live production data on corpus shape (2982 active entries, 76% lesson-learned with 6% access penetration, 0.43 confidence gap between categories), a Thompson Sampling injection direction replacing the original GNN approach, a co-access graph with 368 pairs, and a concrete self-learning pipeline gap in Modes 1/2 (proactive injection and briefing). RuVector has also continued to develop. The prior comparison is stale in both directions.

**This spike answers: given Unimatrix's actual production characteristics and its specific use case — knowledge lifecycle management for multi-agent dev orchestration — which RuVector mechanisms are worth adopting, which are architectural mismatches, and what are the precise implementation costs for the applicable ones?** The output is a prioritized, evidence-based adoption map, not a general comparison.

---

## Why It Matters

The prior ASS-022 comparison identified GNN-based query learning as the most promising learning opportunity ("more sophisticated than Unimatrix's current co-access boosting"). ASS-031 then designed a W3-1 GNN that was subsequently replaced by Thompson Sampling in ASS-032, on the grounds that the training data distribution was too biased for supervised learning to improve on the bandit. This is a partial validation of the ASS-022 finding — but it leaves open whether RuVector's specific GNN formulation (which operates on query patterns, not on entry-level helpfulness votes) would have the same failure mode.

Additionally, ASS-032 surfaced three novel directions (graph-augmented retrieval, late interaction models, self-supervised contrastive embeddings) as unanswered research questions. RuVector has native graph primitives and a GNN layer — it may have already solved, or partially solved, the graph-augmented retrieval problem in a way that is directly applicable to Unimatrix's co-access graph.

Getting this wrong means building Unimatrix's self-learning pipeline from first principles when an applicable pattern may already exist in an actively developed Rust library that shares our core data structures (HNSW, ONNX, SQLite-adjacent persistence). Getting it right means faster delivery of the W3-1 successor with a validated technical foundation.

---

## What to Explore

### 0. Unimatrix Architecture Baseline — Query Before Looking Outward

Before reading a line of RuVector code, query Unimatrix itself to retrieve every architectural decision, pattern, convention, and lesson-learned relevant to the four mechanism areas under evaluation. This is not optional — Unimatrix is the authoritative record of why we made the choices we made, and those reasons determine whether a RuVector mechanism is truly new ground or a rejected path we've already walked.

Use `context_search` and `context_briefing` to pull:

- **Vector index decisions**: ADRs covering `hnsw_rs` selection, compaction strategy (ADR-004), deletion handling, VECTOR_MAP persistence. Why did we choose this implementation? What alternatives were evaluated and why were they rejected?
- **Co-access / graph signal decisions**: ADRs and patterns covering the CO_ACCESS table, the 0.03 additive boost cap, why explicit graph primitives were not adopted at design time. What was the reasoning against petgraph or a native graph layer?
- **Scoring and learning pipeline decisions**: ADRs covering the six-factor confidence composite, why the ASS-031 GNN was replaced by Thompson Sampling (ASS-032), EWC++ deferral, the w_phase_explicit=0.0 placeholder. What specifically made supervised GNN learning unsuitable?
- **Embedding boundary decisions**: ADRs covering the ONNX boundary, why Unimatrix does not run its own models. What constraints locked this decision (latency, operational simplicity, model ownership)?
- **Lifecycle and schema decisions**: ADRs covering Active/Deprecated/Quarantined states, correction chains, SHA-256 hash linking. How have these constraints shaped what the vector layer can and cannot do?

**Deliverable from §0**: A baseline table — one row per mechanism area — stating: (a) the decision Unimatrix made, (b) the reason recorded in Unimatrix, (c) any open question or "revisit under condition X" flag left by the original decision. This baseline is the reference against which every RuVector finding is evaluated. A RuVector mechanism that addresses a reason recorded in Unimatrix is a candidate; one that contradicts a constraint recorded in Unimatrix is likely a reject regardless of its technical merit.

---

### 1. RuVector State-of-the-Repository Audit (April 2026)

The ASS-022 comparison is seven weeks old. Before assessing applicability, establish what RuVector actually is now:

- What has changed in the RuVector repository since mid-March 2026? (commit log, changelog, new features, removed features)
- Is the GNN layer (`github.com/ruvnet/ruvector` — the self-learning component described in ASS-022 as "GNN analyzing query patterns, updates in <1ms") implemented, experimental, or aspirational? Read the code, not the README.
- Is the graph database component (Cypher, hyperedges) functional or roadmap? What is the actual API surface?
- What is the HNSW implementation? How does it handle compaction, rebuild, and index mutation? What parameters are exposed?
- What embedding pipeline does it use? Can it coexist with an external ONNX provider (like Unimatrix's current `unimatrix-embed`), or does it require internal model ownership?
- What persistence model does it use? SQLite? Custom? Memory-only? What are the WAL and durability guarantees?
- What is the license? (Was Apache-2.0 in ASS-022; confirm it has not changed to a non-commercial license.)

**Deliverable**: A 2026-04-21 snapshot of what RuVector actually provides, verified from code — not marketing copy. Mark claims that are README-only vs. code-verified.

---

### 2. GNN Query Learning — Deep Evaluation

ASS-022 described RuVector's GNN as "analyzing query patterns and user feedback to continuously reweight results, updates in under 1ms." ASS-031 designed a Unimatrix GNN (5121 params, graph-feature-enriched MLP) that ASS-032 concluded was the wrong approach for Modes 1/2 because of biased training data. The specific question: **does RuVector's GNN formulation avoid the feedback loop failure mode that made the ASS-031 GNN unsuitable for Unimatrix's injection pipeline?**

Evaluate:

- What signals does RuVector's GNN consume? Query text embeddings only? Explicit feedback labels? Implicit click/access signals? All three?
- Does it train on surfaced-entry labels (selection bias problem, same as ASS-031) or on something else (query pattern structure, topology of the HNSW graph, co-access pairs)?
- What is the update mechanism? Online (per-query), batch (periodic), or hybrid?
- What is the cold-start behavior for new entries? Does it exhibit the same category saturation failure that made the Unimatrix ASS-031 GNN unsuitable for unbiased exploration?
- What is the actual latency for the GNN forward pass in RuVector's implementation? Is the "<1ms" claim code-verifiable or a spec claim?

**Assessment framing**: Could RuVector's GNN be dropped into Unimatrix's Mode 1/2 injection pipeline *as a replacement for Thompson Sampling* (if it avoids the feedback loop) or *as a complement to Thompson Sampling* (if it handles a different signal than the bandit)? Or is it solving a different problem entirely (HNSW graph reweighting vs. entry relevance scoring)?

---

### 3. Graph-Augmented Retrieval — Applicability to the Co-Access Graph

ASS-032 §6.3 Direction 3 identified graph-augmented retrieval as a promising direction for Unimatrix, citing GraphRAG and KG-RAG as prior art. Unimatrix has a live co-access graph (368 pairs, CO_ACCESS table) that currently contributes a max 0.03 additive boost at query time — a deliberately capped, simple signal.

RuVector has native graph support (Cypher, hyperedges). Evaluate:

- How does RuVector's graph layer interact with its vector index? Is retrieval a combined operation (graph traversal + ANN search in one pass) or two sequential phases (ANN candidates → graph re-rank)?
- Does RuVector's graph layer support the specific traversal needed for Unimatrix: "given entry A was accessed, what entries tend to follow, and what entries do those lead to (multi-hop co-access)"?
- What would it cost to replace Unimatrix's current CO_ACCESS additive boost with a proper graph traversal? Specifically: does RuVector's graph API support integration with an external SQLite-persisted graph (Unimatrix's CO_ACCESS table), or does it require owning the graph data itself?
- What is the alternative: implementing multi-hop co-access traversal in petgraph (already on the roadmap per MEMORY.md) vs. adopting RuVector's graph primitives? Which is smaller scope? Which has better long-term maintainability?

**Assessment framing**: Would adopting RuVector's graph component for Unimatrix's co-access graph be an improvement over petgraph, or is petgraph + custom traversal the right path given Unimatrix's SQLite-first architecture?

---

### 4. HNSW Implementation Comparison — Compaction and Mutation

Unimatrix uses `hnsw_rs` with a custom `VectorIndex::compact()` + `Store::rewrite_vector_map()` compaction path. The compaction was introduced to handle HNSW graph corruption after deletions and is the known performance bottleneck in large-corpus deployments (ADR-004 in MEMORY.md). RuVector also uses HNSW as its core index.

Evaluate:

- How does RuVector handle deletions from the HNSW index? Lazy tombstoning? Rebuild? Compensation vectors? How does it avoid the graph corruption problem that drove ADR-004 in Unimatrix?
- Does RuVector expose HNSW parameters (ef_construction, M, ef_search) as runtime-configurable, or build-time constants?
- Does RuVector support filtered HNSW search (search only over entries matching a predicate, e.g. `status = Active`)? Unimatrix currently does post-hoc filtering, which degrades recall at low Active/total ratios.
- What quantization support does RuVector provide for HNSW? INT8, binary, PQ? What is the precision/memory tradeoff at Unimatrix's corpus scale (1K-10K entries, 384-dim embeddings)?

**Specific hypothesis to test**: Is RuVector's HNSW deletion handling substantially better than `hnsw_rs`'s? If so, is it worth migrating Unimatrix's vector index to the relevant component, or adopting its approach in the existing `unimatrix-vector` crate?

---

### 5. Embedding Pipeline Coexistence

Unimatrix uses a fixed ONNX boundary for embeddings (`unimatrix-embed`, sentence-transformers model). RuVector runs GGUF models internally and may have its own embedding generation.

Evaluate:

- Can RuVector components (specifically the GNN or graph layer) consume externally generated embeddings (e.g. 384-dim f32 vectors from Unimatrix's existing ONNX pipeline), or do they require embeddings produced by RuVector's internal model?
- Does RuVector's self-supervised contrastive learning (if implemented) work on any embedding space, or only on its own-generated embeddings?
- If RuVector's GNN consumes external embeddings, what is the interface? A Rust crate API, a C FFI, or a network protocol?

This determines whether RuVector components are "plug in" or "replace entire embedding pipeline."

---

### 6. Targeted Learning Assessment — Unimatrix's Specific Use Case

Synthesize findings from §1–5 through the lens of Unimatrix's actual characteristics:

- **Corpus scale**: 1K–10K entries (not 1M+). RuVector's distributed scaling, Raft, and sharding are architectural overkill. The relevant question is: does RuVector have techniques optimized for *small, high-precision* corpora?
- **Knowledge lifecycle**: Unimatrix entries are not immutable blobs — they have state (Active/Deprecated/Quarantined), correction chains, and confidence evolution. Does RuVector have any concept of entry lifecycle? If not, can its graph/GNN components function correctly when entries are removed from the index mid-session (deprecation, quarantine)?
- **Category imbalance**: The live corpus has a 0.43 confidence gap and 6% access penetration in the largest category (lesson-learned). Does RuVector's GNN query learning have a mechanism for exploration in the face of extreme historical access imbalance, or does it exhibit the same feedback loop failure that disqualified the ASS-031 GNN?
- **Correction chains**: Unimatrix's correction chains (SHA-256 hash links, supersedes/superseded_by) are a core differentiator. Do any RuVector patterns interact with or enhance this? (e.g., does its versioning model provide a complementary signal for confidence computation?)
- **Domain pack configurability**: All Unimatrix learning must work for any (categories, phases) configuration — no hardcoded names. Does RuVector's GNN or graph layer make assumptions about entry taxonomy, or is it fully schema-agnostic?

---

### 7. Fit Gate — Triage Before Recommending

This section is the anti-chasing filter. Every mechanism evaluated in §2–5 must be triaged through this gate before appearing in the adoption map. A mechanism that looks interesting but does not pass the gate produces an evaluation protocol, not an adoption recommendation. This prevents the spike from turning into a technology catalogue.

**Gate criteria — a mechanism passes as HIGH_FIT if all of the following are true:**

1. **Content type match**: The mechanism is designed for, or has demonstrated effectiveness on, corpora with the characteristics of Unimatrix's knowledge entries — short structured text (not documents or sensor streams), typed/categorized entries, small-to-medium corpus (1K–10K), high precision required over high recall. Evidence must be code-verified or from benchmarks at comparable scale; analogy from large-corpus benchmarks is insufficient.

2. **Lifecycle compatibility**: The mechanism can function correctly when entries transition state (Active → Deprecated → Quarantined). Specifically: it does not assume index membership is permanent, does not require a full rebuild when entries are removed, and does not break when the active subset is a minority of total indexed entries.

3. **Schema-agnostic**: The mechanism makes no assumptions about category names, phase names, or entry taxonomy. It works for any deployment's domain pack configuration — not just the software development pack.

4. **Architectural fit**: Adoption does not require replacing a decision recorded in the §0 Unimatrix baseline with a reason that remains valid. If the baseline records "ONNX boundary locked for operational simplicity," a mechanism that requires replacing ONNX fails this criterion regardless of its technical merits.

5. **No prior rejection**: The mechanism was not previously evaluated and rejected for a reason that still applies. If the §0 baseline records "rejected because X," and X has not changed, the mechanism is a reject here — not a candidate for re-evaluation.

**Triage outcomes:**

- **HIGH_FIT**: All five criteria satisfied. Proceed to §8 (adoption feasibility). The mechanism belongs in the adoption map.
- **UNCERTAIN**: One or two criteria are partially satisfied but cannot be confirmed from available evidence. Do NOT recommend adoption. Instead, define an **evaluation protocol**: the minimum experiment that would resolve the uncertainty. Specify: what would be measured, what metric threshold constitutes "lift," what the experiment requires (data, compute, time), and what the evaluation investment estimate is before any implementation commitment.
- **REJECT**: One or more criteria definitively not satisfied, or previously rejected for a reason that still applies. Document the reason so the decision is not revisited in future spikes without new evidence.

**Anti-chasing rule**: If a mechanism is rated UNCERTAIN, the evaluation protocol IS the deliverable — not a recommendation to implement. The output tells the next decision-maker what experiment to run before investing, not "this looks promising, let's build it."

---

### 8. Adoption Feasibility and Cost

For each mechanism that passed §7 as HIGH_FIT, produce a concrete adoption assessment:

- **Crate boundary**: Would adoption mean taking a dependency on the `ruvector` crate, copying a specific module, or implementing the same algorithm from scratch using RuVector's code as a reference?
- **Interface friction**: What changes to Unimatrix's data model (schemas, entry structs, SQLite tables) would adoption require?
- **Test coverage**: What test infrastructure would be needed to validate the adopted mechanism?
- **Effort estimate**: T-shirt size (S=<1 day, M=2-5 days, L=1-2 weeks, XL=>2 weeks) per mechanism.
- **License compatibility**: Is the relevant RuVector code under a compatible license for Unimatrix (currently Apache-2.0)?

---

## Output

1. **Unimatrix architecture baseline (§0)** — table of existing decisions per mechanism area (vector index, graph, scoring/learning, embedding, lifecycle). Required reading before the RuVector findings — establishes what we decided and why, not derivable from code alone.

2. **RuVector state snapshot (§1)** — code-verified capability inventory as of 2026-04-21. Distinguish implemented from roadmap.

3. **GNN applicability verdict (§2)** — one of: (A) avoids the feedback loop failure; adopt as Mode 1/2 complement to Thompson Sampling. (B) Same failure mode as ASS-031 GNN; Thompson Sampling remains correct. (C) Solves a different problem; not applicable to injection. Include fit-gate triage result.

4. **Graph-augmented retrieval recommendation (§3)** — one of: (A) worth adopting for co-access traversal. (B) petgraph + custom traversal is correct; RuVector graph is overkill or incompatible. (C) Both viable; recommendation based on scope. Include fit-gate triage result.

5. **HNSW comparison table (§4)** — deletion handling, filtered search, quantization, parameterization. Verdict: meaningful improvement over `hnsw_rs` at Unimatrix's corpus scale? Include fit-gate triage result.

6. **Embedding coexistence verdict (§5)** — yes or no on external embedding consumption. Include fit-gate triage result.

7. **Fit-gate triage summary (§7)** — one row per mechanism: HIGH_FIT (adoption map), UNCERTAIN (evaluation protocol), or REJECT (reason). For each UNCERTAIN mechanism, the evaluation protocol IS the deliverable — not a build recommendation.

8. **Adoption map (§8)** — HIGH_FIT mechanisms only. Prioritized by impact/effort ratio, each with: what it provides, interface friction, effort estimate, license status. This is the primary implementation-facing deliverable.

---

## Constraints

- **Query Unimatrix first.** The §0 baseline is mandatory before any external research. Skipping it risks recommending something we already decided against for a reason not visible in the code.
- **No technology chasing.** A mechanism that is technically interesting but does not pass the §7 fit gate belongs in the UNCERTAIN or REJECT column, not the adoption map. Interesting ≠ applicable. Evaluating a mechanism's potential is not the same as recommending it.
- Read the RuVector repository code, not just the README or marketing pages. The ASS-022 comparison noted that RuVector "undersells and misdirects" — the same caution applies in reverse.
- Do not assume ASS-022 findings are current. Seven weeks is significant in an actively developed repository. Re-verify every claim.
- Unimatrix is SQLite-first, single-node, embedded. Do not recommend distributed components unless there is a clear extraction path for the relevant component in isolation.
- The domain-agnostic invariant is non-negotiable: any adopted mechanism must work for any (categories, phases) configuration. Do not recommend mechanisms that require hardcoded category or phase names.
- Do not recommend replacing the ONNX embedding boundary unless §5 finds RuVector's embedding pipeline is clearly superior AND the coexistence verdict is negative. The ONNX boundary is a stable production dependency.
- If RuVector components are behind a non-commercial or proprietary license (check carefully — some "open source" projects relicense components), flag this immediately and exclude those components from the adoption map.

---

## Confidence Required

`empirical` — all capability claims must be code-verified. README claims that are not backed by code in the repository must be flagged explicitly. The prior ASS-022 comparison relied primarily on documentation; this spike must go deeper.

---

## Breadth

`targeted-deep`

This is not a broad architectural comparison (ASS-022 already did that) and not a multi-library ecosystem survey (ASS-032 §6.2 already did that). The scope is narrow: RuVector only, evaluated against Unimatrix's specific characteristics and open questions. Depth on the four mechanism areas (GNN, graph, HNSW, embeddings) is more valuable than breadth across RuVector's full feature set.

---

## Approach

`unimatrix-first, then code-first`

**Phase 0 — Query Unimatrix before touching external code.** Use `context_search` and `context_briefing` to retrieve all recorded decisions, patterns, and lessons for the four mechanism areas (vector index, graph/co-access, scoring/learning, embedding). Build the §0 baseline table. This takes 20–30 minutes and prevents hours of work on a rejected path.

**Phase 1 — RuVector code audit.** Read the repository — specifically the GNN implementation, graph layer, HNSW deletion/compaction, and embedding interface. Identify what is implemented vs. aspirational.

**Phase 2 — Fit-gate triage.** For each implemented mechanism, apply the §7 gate criteria against the §0 Unimatrix baseline. Assign HIGH_FIT, UNCERTAIN, or REJECT. Produce evaluation protocols for UNCERTAIN mechanisms. Stop here for REJECT mechanisms — do not write adoption details.

**Phase 3 — Deep evaluation of HIGH_FIT candidates only.** For mechanisms that passed the gate, evaluate against Unimatrix's specific characteristics (corpus scale, lifecycle states, category imbalance, correction chains, domain-agnostic schema) and produce the §8 adoption feasibility details.

**Phase 4 — Write output sections.** Lead with the fit-gate triage summary (§7) and adoption map (§8). Findings sections for each mechanism (§2–5) follow.

---

## Inputs

- **Unimatrix itself** — query via `context_search` and `context_briefing` for ADRs, patterns, and lessons on: vector index, co-access/graph, scoring/confidence pipeline, HNSW, embedding boundary, lifecycle states. Must be consulted before external research (§0).
- `github.com/ruvnet/ruvector` — primary external research target (read the code, not just the README)
- `product/research/ass-022/02-ruvector-comparison.md` — prior comparison; treat as baseline, not ground truth
- `product/research/ass-032/SCOPE.md` §6.2–6.3 — ruvector ecosystem survey + novel directions; several open questions directly relevant here
- `product/research/ass-031/` — W3-1 GNN architecture that was superseded by Thompson Sampling; understand exactly why before assessing RuVector's GNN
- `crates/unimatrix-vector/` — current HNSW implementation (`hnsw_rs`-based); the comparison target for §4
- `crates/unimatrix-embed/` — current ONNX embedding pipeline; the integration surface for §5
- `crates/unimatrix-store/` — SQLite schema, CO_ACCESS table, VECTOR_MAP; relevant for §3 graph comparison and §4 compaction
- `crates/unimatrix-server/src/services/search.rs` — `compute_fused_score`, current co-access boost implementation; the baseline for §2 and §3
