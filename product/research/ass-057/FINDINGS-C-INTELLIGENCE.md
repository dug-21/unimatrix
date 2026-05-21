# FINDINGS: ASS-057 Track C — Intelligence Pipeline & Value-Add Opportunity Inventory

**Spike**: ASS-057 (Track C)
**Date**: 2026-05-14
**Approach**: investigation
**Confidence**: validated (all claims backed by code at specific line numbers)

---

## 1. NLI Contradiction Detection on Claims

### What the Requirements Assumed

The requirements state: "no built-in NLI / contradiction engine — the model emits contradicts edges; Unimatrix just stores them." The workflow plans to handle contradiction identification entirely in the model layer.

### Current State in Unimatrix — Code-Verified

**Two separate contradiction pipelines exist and serve different purposes:**

**Pipeline A: NLI cross-encoder (background tick, Path B)** — `crates/unimatrix-server/src/services/nli_detection_tick.rs`. The cross-encoder ONNX model (W1-4) is gated at line 564: `if !config.nli_enabled`. Even when `nli_enabled = true`, Path B is restricted to writing **only Supports edges — never Contradicts**. This is a hard architectural constraint (C-13 / AC-10a). Line 716 confirms: "Contradiction is discarded (C-13 / AC-10a)." The `nli_scores.contradiction` field is computed and stored in edge metadata JSON but is never acted upon to write a Contradicts edge.

**Pipeline B: Heuristic contradiction scan (independent tick step)** — `crates/unimatrix-server/src/infra/contradiction.rs`. Multi-signal heuristic: HNSW neighbor search (similarity_threshold = 0.85) + conflict_heuristic combining negation opposition (weight 0.6), incompatible directives (weight 0.3), opposing sentiment (weight 0.1). Runs every 4 ticks (`CONTRADICTION_SCAN_INTERVAL_TICKS = 4`). Critically: **does NOT write Contradicts edges to GRAPH_EDGES**. Writes results to `ContradictionScanCacheHandle` (`Arc<RwLock<Option<ContradictionScanResult>>>`). Cache feeds `context_status` output only.

**Production result: zero Contradicts edges ever written.** The code path from NLI scores to `write_nli_edge("Contradicts", ...)` does not exist. The `Contradicts` RelationType exists in the typed graph (used by `suppress_contradicts`) but no running code writes these edges. ADR entries confirm SNLI task mismatch caused NLI to be removed from the scoring formula (`w_nli = 0.00`) and from Informs-edge guards.

### SDLC Corpus Fit — Why Zero Edges

Three compounding factors:

1. **Task distribution mismatch**: The cross-encoder is fine-tuned on SNLI/MultiNLI — natural language hypothesis-premise pairs. ADRs, patterns, and procedures are imperative/descriptive knowledge entries, not propositional claims in the SNLI sense. The model produces systematically high neutral scores and low contradiction scores.

2. **Semantic non-contradiction**: SDLC entries genuinely do not contradict each other propositionally. Two ADRs addressing different concerns are not contradictions.

3. **Architectural gate (C-13/AC-10a)**: Even if the cross-encoder returned high contradiction scores, Path B is hard-wired to discard them. The constraint was added specifically because SNLI cross-encoder produced false-positive contradiction signals on SDLC text.

### Research Corpus Fit

Research Claims are a far stronger fit:

- **Propositional form**: "Study X found that technique Y reduces latency by 30%" matches the SNLI hypothesis format exactly. Claims extracted from academic sources are first-person propositional statements of findings.
- **Genuine contradictions exist**: "Approach A outperforms Approach B in metric M" vs. "Approach B outperforms Approach A in metric M" is a real factual contradiction common in empirical CS research where results vary by experimental setup.
- **Cross-encoder precision/recall**: The cross-encoder should achieve reasonable precision on Claim-to-Claim pairs. Without domain adaptation, recall will be incomplete for Claims using domain-specific vocabulary not in SNLI. Precision-first posture (high threshold) is appropriate as a first deployment.

### Category-Filtered NLI Feasibility

The current pipeline applies no category filtering. `select_source_candidates` draws from all active entries. To restrict to `claim` category only:

1. Add a category filter in Phase 3: `if source_meta.category != "claim" { continue; }` — ~3 lines.
2. Restore the `write_nli_edge(..., "Contradicts", ...)` call for high-contradiction-score pairs. Currently scores are computed but discarded at line 716. The write path exists in `nli_detection.rs`; `write_nli_edge` accepts "Contradicts" as `relation_type`. It is simply not called.
3. Remove C-13/AC-10a constraint **for claim category only**. The rationale (false positives on SDLC text) does not apply when NLI is restricted to Claims.

This is **3-5 call site changes**, not an architectural change. The `informs_category_pairs` config precedent demonstrates category-gated inference is already an established pattern.

### Human-Confirmed Flag Gap

The requirements spec a `human_confirmed` bool on Contradicts edges. `GRAPH_EDGES.metadata` is a JSON column — storage is trivially achievable. But there is no MCP tool allowing a researcher to confirm or reject a flagged contradiction edge. `context_correct` operates on entries, not edges. This is a **workflow gap**, not a storage gap. A new edge-update tool or convention would be needed.

### Verdict: High value

Research Claims are the use case NLI was built for. The SDLC corpus has failed to produce any Contradicts edges due to fundamental task-distribution mismatch. Claims are propositional, factual, and genuinely contradictory across sources — exactly the SNLI distribution. Enabling category-filtered NLI for Claims requires a small code change (3-5 call sites). The workflow gain is automatic contradiction surfacing that the model cannot reliably provide through in-context reasoning across an accumulating corpus.

---

## 2. Confidence Scoring on Theses

### What the Requirements Assumed

Thesis status is a manually-managed lifecycle: `proposed → supported → refuted → abandoned`. Transitions are human-declared. No automatic confidence model is mentioned.

### Current Confidence Model — Code-Verified

Located in `crates/unimatrix-engine/src/confidence.rs`. Six-factor additive weighted composite:

| Factor | Weight | Computation |
|--------|--------|-------------|
| base_score | 0.16 | Status + trust_source |
| usage_score | 0.16 | Log-transformed access count |
| freshness_score | 0.18 | Exponential decay from last_accessed_at (half-life 8760h) |
| helpfulness_score | 0.12 | Bayesian Beta-Binomial posterior (helpful/unhelpful votes) |
| correction_score | 0.14 | Correction chain quality |
| trust_score | 0.16 | Creator trust level |

The formula signature is `compute_confidence(entry: &EntryRecord, now: u64, params: &ConfidenceParams)`. It takes exactly one EntryRecord and no graph state. There is no edge-count component and no hook for incoming Supports/Refutes edges to modify confidence.

### Extensibility Analysis

`ConfidenceParams` is designed for per-domain weight customization. All weights are tunable via config. However, adding an "evidence count" factor requires:

1. One store query per Thesis to fetch incoming edge counts at confidence-refresh time. The maintenance tick does not currently join GRAPH_EDGES.
2. A new weight field (e.g., `w_evidence`) in `ConfidenceParams`.
3. An evidence score: `supports_count / (supports_count + refutes_count + ε)` — directly mappable to `helpfulness_score(supports_count, refutes_count, alpha0, beta0)`.

The 0.92 weight sum invariant (enforced by tests) means adding a new factor requires redistributing weights or treating it as a query-time boost outside the stored formula. The co-access affinity precedent (the "missing 0.08") shows this pattern already exists.

### Research Domain Fit

For SDLC entries, helpfulness votes are the primary evidence signal. For research Theses, nobody votes. The evidence structure IS the graph: `supports(Claim → Thesis)` and `refutes(Claim → Thesis)` edges are the evidence. The Bayesian helpfulness formula is directly repurposable: `helpfulness_score(supports_count, refutes_count, alpha0, beta0)` gives a well-calibrated evidence ratio with a symmetric neutral prior.

Additionally, a research-domain `ConfidenceParams` preset with `w_usage = 0, w_fresh = 0, w_corr = 0, w_base = 0, w_help = 0.7 (evidence), w_trust = 0.3` would be semantically appropriate for Theses and is achievable via the existing struct without code changes to the formula. Only the store query feeding `helpful_count` (repurposed as `supports_count`) would need to be adapted.

### Steward Signal Infrastructure

No steward signal infrastructure exists. `context_status` returns a lambda coherence metric and contradiction cache results but emits no advisory signals like "this Thesis has reached sufficient supporting evidence." A Thesis-specific section in `context_status` output is the logical vehicle but is new functionality with no existing pattern.

### Verdict: High value

A Thesis is the one entity type in the research domain where confidence scoring has clear semantics and genuine utility — confidence should track evidence, not access patterns. The helpfulness formula is directly repurposable for supports/refutes ratios. The gap is feeding edge counts into the computation: one store query per Thesis at refresh time. The workflow gain: automatic confidence tracking replacing manual status management, and a defensible quantitative signal for Thesis promotion/abandonment decisions.

---

## 3. Automatic Relationship Detection (S1/S2/S8 Sources)

### Current State — Code-Verified

All three sources are fully implemented and running in `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`.

**S1 (tag co-occurrence → Informs edges)**: SQL joins `entry_tags` twice on the same tag with `HAVING COUNT(*) >= 3`. Weight = min(shared_tag_count × 0.1, 1.0). Writes bidirectional Informs edges. The `≥ 3` threshold is hardcoded in the SQL, not configurable.

**S2 (structural vocabulary → Informs edges)**: Pattern-matches entries against operator-configured `s2_vocabulary` using `instr(lower(...))` SQL. Writes bidirectional Informs edges. No-op when `config.s2_vocabulary` is empty (operator opt-in required). Domain-neutral by design.

**S8 (search co-retrieval → CoAccess edges)**: Reads `audit_log` for `context_search` success rows, expands pairs from returned `target_ids` JSON arrays. Writes bidirectional CoAccess edges with fixed weight 0.25. Uses watermark counter for incremental processing — compatible with any access frequency.

### S1 for Research Domain

S1 fires when two entries share at least 3 tags. If Claims A and B both carry tags `["transformer", "attention", "fine-tuning"]`, S1 writes `Informs(A, B)` automatically. This is real value: Claims sharing entity-tags are co-evidence about the same topic. However, the hardcoded `≥ 3` threshold may be too high for research-specific tagging, where two Claims about the same entity might share only 1-2 tags. Lowering the threshold requires a code change (the `HAVING >= 3` literal) or a new config parameter.

S1 produces only Informs edges. It cannot produce `mentions` or `cites` edges regardless of category.

### S2 for Research Domain

S2 is a **zero-code-change** capability for the research domain. Operator configures `s2_vocabulary` with research terms ("LoRA", "RLHF", "constitutional AI", "MMLU", "HumanEval", specific model names), and Informs edges appear automatically between entries mentioning the same terms. This is the cleanest expression of the "configured not rebuilt" vision. No deployment or code changes required.

Limitation: S2 matches full entries (title + content) and produces only Informs edges. It cannot distinguish "Claim mentions Entity" from "Finding discusses technique" — semantic edge typing requires the model.

### S8 for Research Domain

S8 captures search co-retrieval via audit_log. In a research workflow, if a researcher repeatedly retrieves Finding F and Thesis T together, S8 writes CoAccess(F, T) and CoAccess(T, F). These edges are weak (weight 0.25) but feed PPR — contributing to serendipitous discovery. CoAccess edges encode co-relevance without semantic typing.

Watermark-based processing makes S8 compatible with daily-cadence research sessions. Each day's context_search calls are processed on the next tick. No compatibility issues.

### Convergent-Citation S9 — New Source Feasibility

Two Findings both citing the same Source via Cites edges is a strong structural signal of relevance — two independent interpretations of the same material. This is analogous to S1 (shared tags → Informs) but at the edge level.

SQL: join `graph_edges` twice on `target_id` with `relation_type = 'Cites'`, filtering both sources by `category = 'finding'`. Implementation follows the S1 pattern in `graph_enrichment_tick.rs`: ~50 lines. Complexity: low. **Dependency**: Cites must be a stored RelationType in GRAPH_EDGES (Track B gap territory). Deferred until Cites edges confirmed to be written.

### Verdict by Source

- **S1**: Medium value. Works if research entities are consistently tagged, but `≥ 3` threshold may need lowering via code change.
- **S2**: **High value, zero code change**. Configure `s2_vocabulary` with research terms; Informs edges appear immediately.
- **S8**: Medium value. Supplementary PPR input at no cost; compatible with daily cadence.
- **Convergent-citation S9**: High value if Cites edges are stored; feasibility is high but depends on Track B. Flag as carry-forward.

---

## 4. PPR Graph Traversal for Serendipitous Discovery

### PPR Mechanics — Code-Verified

Located in `crates/unimatrix-engine/src/graph_ppr.rs`. The algorithm is **reverse-walk personalized PageRank (transpose PPR)**:

The traversal uses `Direction::Outgoing`. For an edge A→B (Supports: A supports B), node A accumulates mass from B's score. Effect: when B is a seed (highly scored HNSW result), mass flows backward to A (the Claim that supports B). This surfaces entries that *point to* seeds — not entries that seeds point to.

**Personalization vector construction** (`search.rs` lines 963-985): Seeds are HNSW top-k results. Each seed score = HNSW cosine similarity × phase affinity. Phase affinity is a per-category frequency weight from PhaseFreqTable. If no phase is provided, affinity = 1.0 for all seeds (neutral cold-start).

**PPR expander (crt-042)**: `graph_expand` BFS from HNSW seeds widens the candidate pool to up to 200 entries before PPR. Entries graph-connected to HNSW seeds but semantically distant can enter the result set — this is the serendipity mechanism.

**Edges consumed by PPR**: Supports, CoAccess, Prerequisite, Informs. Contradicts and Supersedes excluded.

### Research Domain PPR Adaptation

**Goal as PPR anchor**: No code changes to PPR algorithm required. Pass the Goal entry's embedding as the query to `context_search`. HNSW returns semantically similar entries; PPR expands through `advances` and `supports` edges from those entries. Claims supporting Theses advancing the Goal gain PPR mass. This works if `advances` is added as a PPR-traversed RelationType (currently not in the four-type list).

**Concrete serendipity scenario**: Researcher queries "attention mechanism efficiency." HNSW returns Thesis T about attention efficiency. PPR traverses outgoing edges from T: `supports(Claim C1 → Thesis T)` causes C1 to gain mass. C1's content is "Yang et al. found 2x speedup with sparse attention in encoder layers" — too semantically specific to match "attention mechanism efficiency" at high HNSW cosine. PPR brings C1 into the result set via the Supports edge. This is "related work you didn't know to look for."

**Adaptation required**: Add `advances` as a RelationType variant recognized in `positive_out_degree_weight` and the four `edges_of_type` calls in `personalized_pagerank`. ~10 lines across the PPR module and RelationType enum. No algorithm changes.

### Goal-Conditioned Briefing (crt-046) as PPR Complement

The `goal_clusters` table and `blend_cluster_entries` function provide a non-PPR Goal-anchoring mechanism that is already in production. When a cycle starts with a `goal` text, past cycles with similar goal embeddings inject their entry IDs into the briefing. This is distinct from PPR (similarity-based retrieval, not graph propagation) but complementary: it injects entries from past similar research contexts while PPR propagates relevance through the current session's graph.

### WA-4 Proactive Delivery Gap

WA-4 is implemented as briefing injection — entries from goal-similar cycles injected at `context_briefing` time. There is no unsolicited "push" delivery mechanism. A "here is a Claim that contradicts your working Thesis before you ask" signal would require extending `context_briefing` output to include Contradicts-edge neighbors of recently accessed Theses. This is new briefing logic, not a configuration change.

### Cold-Start Limitation

PPR adds no value when the graph is empty. Serendipity requires existing `supports`, `advances`, and Informs edges. For a new research project:
- Session 1: PPR = HNSW (graph empty)
- Sessions 5-10 (50+ Claims, 10+ Theses, multiple advances edges): PPR provides meaningful discovery

This is inherent to any graph-based system. Cold-start is a constraint, not a fatal limitation.

### Verdict: High value (cold-start limited)

PPR is the correct mechanism for serendipitous discovery in a research graph — it propagates relevance through typed edges in a way semantic search cannot replicate. The algorithm requires zero changes; only `advances` needs to be added as a PPR-traversed RelationType (~10 lines). The Goal-conditioned briefing infrastructure (crt-046) provides Goal-anchoring independently of graph density. Cold-start limitation: no serendipity value in sessions 1-4; increasing value as the graph populates.

---

## 5. Behavioral Signal Learning from Researcher Activity

### Current Pipeline — Code-Verified

**Signal collection**: `crates/unimatrix-server/src/services/behavioral_signals.rs`. Pipeline: collect `context_get` observations from ObservationRow records, group by session, build canonical co-access pairs (entries retrieved together in a session), emit bidirectional `Informs` edges with weight = `outcome_to_weight(outcome)` (1.0 success, 0.5 other). Runs in `run_step_8b`, called from `context_cycle_review` on every invocation.

**Goal cluster storage**: `crates/unimatrix-store/src/goal_clusters.rs`. At cycle close, if a goal embedding exists, a row is inserted into `goal_clusters(feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at)`. At briefing time, `query_goal_clusters_by_embedding` finds past cycles with similar goals (cosine similarity threshold, recency cap 100) and injects their entry_ids into the briefing result at score = `(EntryRecord.confidence × w_goal_cluster_conf) + (goal_cosine × w_goal_boost)`.

**crt-046 status**: Confirmed COMPLETE (PR #511, schema v22). Goal-conditioned context_briefing blending is in production, not planned.

### Goal as cycle_topic — Compatibility Confirmed

`context_cycle` accepts `topic` (required) and `goal` (optional string, validated in tools.rs). For a research workflow: `context_cycle(type="start", topic="goal-001", goal="Evaluate transformer efficiency techniques for long-context tasks")`:
1. Stores cycle under topic `goal-001`
2. Embeds goal text and saves to `cycle_events.goal_embedding`
3. At cycle close via `context_cycle_review`, populates `goal_clusters` with all accessed entries

This works **today**. No code changes required. The `topic` field can be any string identifying the research goal.

### ASS-039 Validation of Behavioral Signal

Confirmed: goal-similar cycles share entries 9.5x more than dissimilar cycles. The signal is real. For a research domain where Goals are persistent and specific, this signal should be stronger than in the SDLC domain (where cycles vary more within similar goals).

### Daily Cadence Compatibility

All components are compatible with daily-cadence research sessions:

- **Behavioral Informs edges**: Written at cycle close. One batch per session day. Persist in GRAPH_EDGES immediately.
- **Goal cluster population**: One row per session day. Accumulates meaningful data over weeks.
- **S8 co-retrieval**: Watermark-based, processes all context_search calls since last batch. Daily batches handled correctly.
- **Tick-based confidence refresh**: Runs every 15 minutes but needs only one run post-session. Daily sessions are within normal parameters.

No compatibility issues. The SDLC workflow is itself session-bounded and non-continuous — the infrastructure was designed for this pattern.

### Cold Start

A new research project has zero behavioral history. After session 1: 1 goal cluster row, behavioral Informs edges for co-accessed entries, access counts > 0. Cold start breaks immediately. By session 3-5, the goal cluster has enough history for meaningful similarity matching. Each session contributes immediately with no minimum threshold.

### Verdict: Highest asymmetric value, zero code change

crt-046 is fully implemented and running. A research workflow using `context_cycle` with a `goal` parameter gets Goal-conditioned briefing blending from session 2 onward. Behavioral Informs edge emission from co-access patterns feeds PPR directly. The infrastructure is compatible with daily-cadence research sessions. Most critically: **this capability is invisible to the requirements document** but delivers meaningful personalization from session 2 onward. It is the highest asymmetric value opportunity in this inventory.

---

## 6. Value-Add Opportunity Table

| Capability | Requirement Assumption | Current State | Research Fit | Workflow Gain | Extension Needed | Verdict |
|---|---|---|---|---|---|---|
| NLI Contradiction Detection | Model emits Contradicts edges; Unimatrix stores | Infrastructure exists; C-13/AC-10a gate discards contradiction scores; scan writes cache only, not GRAPH_EDGES; zero Contradicts edges in production | Strong — Claims are propositional SNLI-format with genuine factual contradictions | Automatic Contradicts edges as Claims are stored; Q5 traversal works without model re-querying | ~3-5 call site changes: category filter, restore write_nli_edge("Contradicts"), remove C-13 for claim category | **High** |
| Confidence Scoring on Theses | Manual status lifecycle | Six-factor composite; no edge-count inputs; formula signature takes only EntryRecord | Strong — supports/refutes edge ratio is well-defined evidence; Bayesian formula directly repurposable | Automatic thesis confidence tracking; replaces manual status management | 1 store query per Thesis at refresh; 1 new ConfidenceParams weight field; weight-sum invariant update | **High** |
| S1 Tag Co-occurrence | All edges declared explicitly | Fully implemented; bidirectional Informs on ≥3 shared tags | Moderate — works if consistently tagged; `≥3` threshold may be too high | Free Informs edges between co-tagged Claims | Possibly lower HAVING >= 3 threshold via new config param | **Medium** |
| S2 Vocabulary Informs | All edges declared explicitly | Fully implemented; domain-neutral; no-op if vocabulary empty | Strong — research vocabulary directly configurable | Zero-code Informs edges between entries sharing research terminology | Config only: populate `s2_vocabulary` | **High** |
| S8 Search Co-retrieval | All edges declared explicitly | Fully implemented; daily cadence compatible via watermark | Moderate — CoAccess edges weak but feed PPR | Supplementary co-relevance signal for PPR at no cost | None | **Medium** |
| PPR Graph Traversal | Traversal is explicit and query-driven | Fully implemented (crt-042/044/045 COMPLETE) | Strong — research graph edges form correct propagation structure for serendipitous Claim/Finding discovery | Claims supporting queried Theses surface via PPR even when semantically distant from query | Add `advances` as PPR-traversed RelationType (~10 lines); no algorithm changes | **High** (cold-start limited) |
| Goal-Conditioned Learning (crt-046) | No learning or adaptation mentioned | COMPLETE in production (PR #511, schema v22); goal_clusters table; behavioral Informs edges; briefing blending | Direct fit — research Goals are exactly the session anchor crt-046 was designed for | Goal-conditioned briefing from session 2 onward; compounding value | None — use context_cycle with goal parameter as-is | **High** |

---

## 7. Strategic Observation: Two Asymmetric Value Opportunities

**The two capabilities with the highest gain relative to effort:**

### 1. S2 Vocabulary Informs (zero effort, immediate high gain)

S2 is operator-configurable with no code changes. A research deployment populates `s2_vocabulary` with domain terms ("LoRA", "RLHF", "MMLU", "HumanEval", specific model/benchmark names), and Informs edges appear automatically between entries mentioning the same terms. The inference graph is populated from existing entry content without any researcher action. This is the cleanest expression of the "configured not rebuilt" vision.

### 2. Goal-Conditioned Behavioral Learning (zero effort, compounding gain)

crt-046 is fully implemented. A research workflow calling `context_cycle(type="start", topic="goal-NNN", goal="<goal text>")` and `context_cycle_review` at session close gets Goal-conditioned briefing blending from session 2 onward, with behavioral Informs edges feeding PPR from session 1 close. The requirements document does not mention this capability at all — it is invisible to the external specification but represents genuine differential value that a raw graph storage layer cannot provide. The value compounds with each session.

**For roadmap positioning**: lead with these two in the first research domain deployment — they demonstrate Unimatrix intelligence with zero code changes. NLI category-filtered contradiction detection and evidence-driven Thesis confidence are high value but require code changes; position them as Wave 2+ enhancements once the research domain is established as a live deployment context.

---

## Out-of-Scope Discoveries

**OSC-1: Convergent-citation S9 edge source** — Two Findings citing the same Source via Cites edges is a strong structural signal of relevance (analogous to S1 shared tags). Implementation follows S1 pattern in `graph_enrichment_tick.rs` (~50 lines). Dependency: Cites must be a stored RelationType. Flag for a targeted spike after Cites edges confirmed to be stored in GRAPH_EDGES.

**OSC-2: Thesis-specific ConfidenceParams preset** — The dsn-001 configurable confidence system supports per-domain weight presets. A `research` preset with `w_usage = 0, w_fresh = 0, w_corr = 0, w_base = 0, w_help = 0.7, w_trust = 0.3` would give Theses evidence-weighted confidence without code changes — only config. Should appear in the `research-domain.toml` sketch.

**OSC-3: `informs_category_pairs` as research domain configurator** — The existing `informs_category_pairs` config controls which category pairs generate structural Informs edges via Path A cosine scan. Pairs like `["claim", "thesis"]`, `["finding", "claim"]`, `["finding", "thesis"]` would enable Path A to generate Informs edges across research categories automatically. Pure config-layer capability, no code required.

**OSC-4: NLI suppression interaction with research Claims** — `suppress_contradicts` (called from the search pipeline) suppresses entries that have Contradicts edges with high-scoring results. If category-filtered NLI writes Contradicts edges between Claims, `suppress_contradicts` will run on Claim results. This is desirable (contradictory Claims suppressed in direct search, surfaced via PPR), but the interaction should be documented to prevent confusion.
