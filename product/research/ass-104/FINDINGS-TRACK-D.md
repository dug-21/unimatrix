# FINDINGS: ass-104 Track D — External research (world-class bar)

**Spike**: ass-104 (Track D of 4) · **Agent**: `researcher-ass104-track-d` (`uni-external-researcher`) · **Date**: 2026-07-20
**Approach**: literature / ecosystem · **Confidence**: **DIRECTIONAL** — envisioning only. No build, no PoC, no benchmark. A feature/pattern map and a bar to aim at, not validated mechanism choices.

## How to read this

Five families: **F1** lifecycle (aging, archival, tombstoning, purge, compaction) · **F2** integrity (contradiction, provenance, resolution, repair) · **F3** relevance decay & reinforcement · **F4** background health architecture · **F5** telemetry & observability.

Each pattern carries what it is / who does it (named source + URL) / why it matters / **maturity** (`PRODUCTION` = shipping in a named product, `EMERGING` = multiple independent implementations no settled form, `RESEARCH` = papers only) / **verdict** (`NET-NEW`, `LIKELY-COVERED`, `PARTIAL`).

**Caveat on the verdict column, and it is important.** I am barred from Unimatrix and ass-103. Verdicts are inferred *only* from the four facts in my brief — in-memory hot path rebuilt by a tick; typed edges; embeddings/HNSW; lifecycle states incl. DEPRECATED/QUARANTINED; per-domain config packs. **Every `LIKELY-COVERED` and `PARTIAL` is a hypothesis for Tracks A/C to confirm or overturn.** Per SCOPE.md this does not re-derive ass-032 or ass-052; plausible touches are flagged `[possible overlap — Track A to dedupe]`.

## The single most important external finding

Every mature system surveyed converges on the same shape, and it is **not** "a periodic job that recomputes state":

> An append-only, provenance-bearing write log; **incremental** maintenance operators driven by change deltas; a separate **asynchronous quality-improving** pass that may use expensive computation; and a **periodic full reconciliation** that exists only to correct the drift the incremental path accumulates.

Four tiers, not one loop. Zep/Graphiti, Letta, FreshDiskANN, RDFox and Materialize are instances of this shape despite sharing nothing else. Unimatrix as described has tier 4. Tiers 2 and 3 are where the bar sits.

Second load-bearing finding, corroborating SCOPE.md's core lens: **the literature independently arrived at "age ≠ staleness."** 2025–26 temporal-RAG work explicitly separates **time-sensitive** from **timeless** facts and decays only the former ([arXiv:2509.19376](https://arxiv.org/abs/2509.19376); [temporal-rag](https://github.com/Emmimal/temporal-rag) ships "document kind classification" as a pipeline stage). Uniform time decay over a corpus containing timeless facts is a **named error mode** externally, not merely a Unimatrix preference. The domain-pack framing is ecosystem-correct; the refinement the outside view adds is that classification can be **per-entry**, not only per-domain (F3-2).

---

## F1 — Knowledge lifecycle

### F1-1. Bi-temporal validity: supersede, don't delete ★ TOP-RANKED
Every fact carries **valid time** (true in the world) and **transaction time** (when the system learned it). Superseding sets `t_invalid`/`t_expired` rather than removing.

**Who**: Zep/Graphiti is the reference implementation for agent memory ([arXiv:2501.13956](https://arxiv.org/html/2501.13956v1)). Primary-source quote: *"t′created and t′expired ∈ T′ monitor when facts are created or invalidated in the system, while tvalid and tinvalid ∈ T track the temporal range during which facts held true."* The technique is decades old in databases (SQL:2011 system-versioned tables); its transfer to *agent memory* is the recent move.

**Why**: The only mechanism that makes contradiction handling **non-destructive**. Without it, resolving "X is now false" costs the ability to answer "why did we act on X in March?" — for an SDLC corpus, exactly the audit question with value. It also converts deletion from a data-loss event into a state transition, which is what makes aggressive lifecycle policy safe to automate.

**Maturity**: `PRODUCTION` (Graphiti ~20k stars, powers Zep commercially). **Verdict: NET-NEW (high confidence).** DEPRECATED/QUARANTINED is a single-axis status field. It can say "deprecated *now*"; it cannot reconstruct "what the corpus asserted on 2026-03-14."

### F1-2. Hard purge is a distinct stage, and tombstone-only is not deletion ★ TOP-RANKED
**Who**: **Weaviate** runs tombstone cleanup on `cleanupIntervalSeconds` and documents that cost *grows with index size* ([docs](https://docs.weaviate.io/weaviate/config-refs/indexing/vector-index)). **FreshDiskANN** processes inserts eagerly but deletes **lazily**, then runs explicit **`consolidation`** that frees deleted vectors and *re-links the graph around removed nodes* ([Pinecone](https://www.pinecone.io/blog/hnsw-not-enough/)). LSM compaction generalizes it.

**The sharp finding — tombstones leak.** [Ghost Vectors (Chakraborttii et al., arXiv:2606.18497)](https://arxiv.org/pdf/2606.18497) shows HNSW neighbour topology retains enough structure to **reconstruct vectors marked deleted**, across Chroma, Pinecone, Weaviate, FAISS; tombstone-only deletion gives *"minimal security guarantees"*, remediation requires secure index rebuild. **Recency check: 2026 preprint, not peer-reviewed — treat exploitability as unconfirmed, but the structural claim (deleted geometry survives in the graph) as sound, since it follows from how HNSW works.** Machine-unlearning work makes the parallel point at the model layer ([ScienceDirect](https://www.sciencedirect.com/science/article/pii/S026736492300095X)).

**Verdict: NET-NEW.** Track C names this a gap; the external view confirms it *and* adds the non-obvious requirement that purge must **rebuild or repair index structure**. A purge that leaves HNSW untouched is not a purge.

### F1-3. Tiered hot/warm/cold with differential retention
**Who**: Letta memory blocks vs archival ([docs](https://docs.letta.com/letta-agent/memory)); FreshDiskANN buffer/disk split; [FadeMem (arXiv:2601.18642)](https://arxiv.org/abs/2601.18642) — *"differential decay rates across a dual-layer memory hierarchy."*

**Why**: Gives lifecycle a *reversible* middle step, and is the safe answer to "should aged DEPRECATED auto-quarantine?" The ecosystem's answer: **demote first, quarantine later, purge last, each step reversible.** `PRODUCTION`. **Verdict: PARTIAL** — an in-memory hot path over a durable store *is* two tiers; what appears absent is tiering as a **policy surface** (per-tier decay rates, explicit demotion, re-promotion on access).

### F1-4. Consolidation / dedup as a scheduled background op
**Who**: Mem0's updater runs `ADD/UPDATE/DELETE/NOOP` against semantically similar memories via LLM function-calling ([arXiv:2504.19413](https://arxiv.org/html/2504.19413v1), [repo](https://github.com/mem0ai/mem0)). Guru ships **duplicate detection before search** ([Guru](https://www.getguru.com/features/verification)). Cognee: add-only writes + **periodic re-consolidation**.

**Why**: Redundancy is a *retrieval-quality* bug — near-duplicates crowd top-k and starve diversity. Write-time dedup is cheap but misses cross-session duplicates. `PRODUCTION`. **Verdict: PARTIAL / possible overlap — Track A to dedupe against ass-032.**

---

## F2 — Integrity maintenance

### F2-1. Contradiction as an *ingest-time* gate plus a *background* sweep ★ TOP-RANKED
**Who**: Zep — *"When new information contradicts existing facts, the system employs an LLM to identify contradictions... it invalidates the affected edges by setting their t_invalid to the t_valid of the invalidating edge."* Mem0's `DELETE` is the same idea without the graph. [arXiv:2504.00180](https://arxiv.org/pdf/2504.00180); survey: [Knowledge Conflicts for LLMs, EMNLP 2024](https://github.com/pillowsofwind/Knowledge-Conflicts-Survey).

**Methodological update, bearing directly on Track C's "restore NLI?"**: the classical answer is a trained NLI classifier (SNLI/MNLI, [arXiv:1508.05326](https://arxiv.org/pdf/1508.05326)). **The 2024–26 ecosystem has largely moved off standalone NLI for this task.** Sentence-pair classifiers were trained on short, self-contained, single-domain pairs and degrade badly on long, context-dependent, domain-specific text — which is what a knowledge corpus is. Production systems use **LLM comparison against a *retrieved candidate set***. The retrieval step is what makes it tractable: never all-pairs, only top-k neighbours of the incoming fact.

**Why**: Contradiction is the only integrity defect that *silently poisons* downstream answers rather than degrading them. A corpus storing both "we chose X" and "we rejected X" with no relation hands an agent whichever it retrieves first.

**Verdict: NET-NEW as a mechanism recommendation.** The capability is on the radar (ass-092/035/036). Net-new: (a) **do it at write time against retrieved candidates, not as an all-pairs background scan** — this is what collapses the cost that presumably killed it before; (b) **resolution must be supersession (F1-1), not deletion**; (c) if previously scoped as "restore the NLI model," re-scope as "retrieval-gated conflict adjudication."

### F2-2. Statement-level provenance with attribution and confidence ★ TOP-RANKED
**Who**: [W3C PROV-O](https://www.w3.org/TR/prov-o/) (Recommendation, 2013-04-30) — entity/activity/agent triad. **RDF-star**, now in **RDF 1.2**, is the annotation mechanism: *"RDF-star allows properties like confidence scores to be attached to quoted triples"* ([metaphacts](https://blog.metaphacts.com/citation-needed-provenance-with-rdf-star), [W3C RDF-DEV](https://www.w3.org/community/rdf-dev/2022/01/26/provenance-in-rdf-star/)). Atomic-triple granularity: [Dibowski, FOIS 2024](https://www.utwente.nl/en/eemcs/fois2024/resources/papers/dibowski-full-traceability-and-provenance-for-knowledge-graphs.pdf). Zep carries **episode-level provenance** — every fact points back to its raw episode.

**Why**: Provenance is the **precondition for every automated integrity action.** You cannot safely auto-quarantine, auto-supersede or auto-purge without answering "on what basis?" and "what else did that basis produce?" It also makes failures recoverable: one bad ingest producing 200 wrong assertions is a targeted retraction, not a corpus-wide audit.

`PRODUCTION` (W3C Rec; RDF 1.2; shipping in metaphactory, PoolParty, Zep). **Verdict: NET-NEW / PARTIAL** — typed edges are a strong substrate; what's likely absent is the *discipline* (mandatory per-assertion attribution, confidence field, the retraction operator provenance exists to enable).

### F2-3. Declarative constraints + a violation *report*, not a violation *block* ★ HIGH-VALUE, UNDER-APPRECIATED
**Who**: **SHACL** (W3C Rec). [Trav-SHACL, WWW '21](https://dl.acm.org/doi/fullHtml/10.1145/3442381.3449877) shows incremental streaming validation. Critical limitation, primary source: [SHACL Validation under Graph Updates (arXiv:2508.00137)](https://arxiv.org/pdf/2508.00137) — *"if a validated graph is updated, it has to be re-validated from scratch, which can be expensive."* **Honest counter-evidence**: [Is SHACL Suitable for Data Quality Assessment? (arXiv:2507.22305)](https://arxiv.org/html/2507.22305) argues SHACL fits *conformance checking*, not *quality assessment* — cited so this isn't one-sided. **Wikidata is the scale proof**: constraints are explicitly *"hints to guide editors rather than firm restrictions"*; a bot regenerates [constraint-violation reports](https://www.wikidata.org/wiki/Wikidata:Database_reports/Constraint_violations) **daily** with a [summary page](https://www.wikidata.org/wiki/Wikidata:Database_reports/Constraint_violations/Summary). The largest open collaborative KG maintains quality by **reporting and triaging, not blocking**. Stardog ships it commercially as [Data Quality Constraints](https://docs.stardog.com/data-quality-constraints).

**Why**: Highest value-per-effort in the entire map. No ML, no LLM, no new storage — deterministic graph queries on a schedule. Converts "the corpus is probably fine" into **a number that trends**. The advisory-not-blocking posture is the key insight: hard-blocking writes makes ingestion brittle and pushes agents to work around the engine.

**Verdict: NET-NEW (highest confidence in the map).** Also the most direct answer to LAMBDA-HONEST / the missing tick-health nfr.

### F2-4. Entity & edge resolution as continuous reconciliation
**Who**: Zep shortlists by hybrid search, adjudicates by LLM, then updates via *"predefined Cypher queries to maintain schema consistency."* Canonical: [Paulheim, KG Refinement Survey, SWJ 2016](https://www.semantic-web-journal.net/system/files/swj1167.pdf) (completion + error detection as the two axes; still standard a decade on); updated by [ACM CSUR 2024](https://dl.acm.org/doi/10.1145/3640313).

**Note the retrieval-gating pattern recurring a third time**: Zep resolution, Zep contradiction, Mem0 update all use *embed → retrieve top-k → LLM adjudicate*. This is the ecosystem's standard answer to avoiding O(n²) on a growing corpus. **Design implication: these are not three background jobs — they are one retrieval-gated adjudication pass with three verdict types.** That consolidation is itself a net-new architectural suggestion. **Verdict: PARTIAL / possible overlap — Track A to dedupe against ass-032.**

### F2-5. Truth maintenance under retraction — the deletion problem
**Who**: **RDFox** improves classic **DRed** (overdelete downstream, then rederive) with **FBF** for exact deletion ([docs](https://docs.oxfordsemantic.tech/reasoning.html), [paper](https://pdfs.semanticscholar.org/88f2/ac0f0a15ad2340985cfdba14c589e53c51fc.pdf)). **Stardog explicitly declines to materialize**, using query rewriting, on the stated grounds that *"truth maintenance... is always computationally expensive, especially after deletions"* ([Stardog](https://docs.stardog.com/inference-engine/)).

**Why — a direct warning to Track B**: two mature commercial engines examined the same tradeoff and split, with **deletion cost** the deciding factor. **Any move to event-driven incremental maintenance must be costed on the retraction path, not the insertion path.** Incremental insert is easy and misleadingly cheap. Notably, a full tick-rebuild is **immune to this entire problem class** — a real, under-credited virtue of the current architecture that a rewrite would forfeit. **Verdict: NET-NEW as a constraint on Track B — a caution, not a feature.**

---

## F3 — Relevance decay & reinforcement

### F3-1. Multi-factor scoring: recency × importance × relevance ★ TOP-RANKED
**Who**: [Park et al., Generative Agents (arXiv:2304.03442, UIST '23)](https://ar5iv.labs.arxiv.org/html/2304.03442) — decay **γ = 0.995/hour**, all weights α = 1, three min-max-normalized terms summed. One of the most-replicated designs in the agent literature. Extended by [arXiv:2606.12945](https://arxiv.org/pdf/2606.12945).

**Two details load-bearing and usually lost in summaries:**
1. **Recency decays over time since *last access*, not creation.** This is the whole ballgame. Under last-access decay a 2023 entry retrieved yesterday is *fresh*; an entry created yesterday and never used is *stale*. This satisfies SCOPE.md's "#597 surfacing months later is fully valuable" **without needing a domain-pack exemption at all** — the usage signal does the work. The mode SCOPE.md correctly calls wrong for SDLC is *creation-time* decay, a strictly worse and more common variant.
2. **Importance is assessed once at write time and never decays** — a permanent property, and what stops a high-value-but-rarely-needed entry decaying to invisibility.

**Why**: Cheapest available correction to pure-vector retrieval; makes the corpus **self-curating**. That is the "learning" in self-learning. `PRODUCTION`; source is peer-reviewed (UIST '23) — strongest evidence tier here. **Verdict: NET-NEW / possible overlap — Track A to dedupe against ass-032.** The *decay* and *reinforcement* halves are less likely covered by a surfacing spike than the ranking half.

### F3-2. Per-entry temporal-sensitivity classification — the refinement to the domain-pack lens ★ HIGH-VALUE
**Who**: [arXiv:2509.19376](https://arxiv.org/abs/2509.19376) separates timeless (semantic proximity suffices) from recency-critical, finding a simple half-life prior reaches Latest@10 ≈ 0.60 where the freshest item is *not* the most similar. [temporal-rag](https://github.com/Emmimal/temporal-rag) ships document-kind classification + validity filtering. [arXiv:2606.26511](https://arxiv.org/html/2606.26511v1) and [Don't Ask the LLM to Track Freshness (arXiv:2606.01435)](https://arxiv.org/pdf/2606.01435) both argue **deterministic supersession over LLM-judged freshness** — notable, since it cuts against the LLM-adjudication trend in F2-1 and is worth Track C weighing.

**Why — where the outside view most usefully sharpens SCOPE.md.** The domain-pack framing is directionally right but **coarser than the literature's answer, and coarser than reality**: an SDLC corpus contains both timeless entries (architectural rationale, a hard-won gotcha) *and* genuinely time-sensitive ones (a dependency version pin, a workaround for a now-fixed bug, a current-owner note). A per-domain switch cannot distinguish them; it must pick one and be wrong about the other half. **The stronger design is two-level**: the pack sets default + half-life, a per-entry attribute overrides. With F3-1's last-access decay, that is safe-by-default in every domain — which is what `goal:domain-agnostic` actually requires. `EMERGING` (preprints; weight accordingly). **Verdict: NET-NEW.**

### F3-3. Access-frequency reinforcement (usage as a first-class signal)
**Who**: Taxonomy worth adopting wholesale — [arXiv:2602.06052](https://arxiv.org/pdf/2602.06052) and [arXiv:2603.07670](https://arxiv.org/html/2603.07670v1) classify forgetting as **passive decay-based, active deletion-based, safety-triggered, adaptive reinforcement-based**. KARMA uses counting Bloom filters for cheap access frequency. [FadeMem](https://arxiv.org/abs/2601.18642) modulates decay by relevance *and* frequency. Peer-reviewed and best-grounded: [Human-Like Remembering and Forgetting in LLM Agents, HAI '25](https://dl.acm.org/doi/10.1145/3765766.3765803) — ACT-R base-level activation is a 30-year-validated model of exactly this.

**Why**: Usage is the only *ground-truth* relevance signal a knowledge engine ever gets; everything else is proxy. It is also what makes decay **safe** — a genuinely valuable entry keeps being retrieved and never decays.

**Honest caveat**: reinforcement creates **rich-get-richer**. Entries surfacing early accumulate strength and crowd out equally-good entries that happened not to surface. Needs an exploration allowance or normalization. **I found no settled solution in the surveyed work** — a real open problem, not a footnote. **Verdict: PARTIAL, leaning NET-NEW** — access counters are common; decay-and-reinforcement *as a lifecycle driver* is the pattern.

### F3-4. Safety-triggered forgetting and memory governance
**Who**: [SSGM framework (arXiv:2603.11768)](https://arxiv.org/pdf/2603.11768); threat model: [ER-MIA (arXiv:2602.15344)](https://arxiv.org/pdf/2602.15344).

**Why**: An engine that writes what agents tell it has an **injection surface**; a poisoned entry is retrieved and acted on like any other. Containment *requires* F2-2 provenance, because containment means "retract everything from source S." `RESEARCH` — preprints only, directional signal not a build spec. **Verdict: PARTIAL** — QUARANTINED plausibly *is* the safety-triggered state; likely missing is the **trigger set** and the provenance-driven bulk-retraction operator.

---

## F4 — Background health architecture

### F4-1. Asynchronous quality-improvement passes ("sleep-time compute") ★ TOP-RANKED
**Who**: [Letta, Sleep-time Compute (2025-04-21)](https://www.letta.com/blog/sleep-time-compute/), [docs](https://docs.letta.com/guides/agents/architectures/sleeptime/). Architecture detail worth noting: **the primary agent is deliberately denied tools to edit its own core memory — those belong exclusively to the sleep-time agent.** Separation of duties as an architectural constraint, not a convention. They pair a *fast* model interactively with a *stronger, slower* one in background, claiming a Pareto improvement on AIME/GSM. A-MEM's link-evolution pass is the graph analogue ([arXiv:2502.12110, NeurIPS 2025](https://arxiv.org/abs/2502.12110)) — new memories trigger `strengthen`/`update_neighbor` on existing ones.

**Why — the largest conceptual gap in the map.** A tick that rebuilds derived state is a **consistency** mechanism: it makes the hot path match the store. Sleep-time compute is an **improvement** mechanism: it makes the store *better than it was*. Different jobs, different failure modes, different budgets, different correctness criteria. Conflating them means the expensive-but-valuable work either never gets budget or blocks the cheap-and-mandatory work. Given the self-learning framing this is pointed: **a corpus that is only ever reconciled does not learn. Learning happens in the improvement pass.**

`PRODUCTION` (Letta shipping since 2025); A-MEM peer-reviewed at NeurIPS 2025. **Verdict: NET-NEW (high confidence).** A distinct, budgeted, separately-observable improvement pass — allowed expensive computation, allowed to be *skipped* under load — is not implied by a rebuild tick.

### F4-2. Incremental maintenance driven by change deltas
**Who**: [Timely Dataflow, CACM](https://cacm.acm.org/research/incremental-iterative-data-processing-with-timely-dataflow/) (peer-reviewed, canonical); **Materialize** productizes it; [RisingWave on IVM](https://risingwave.com/blog/what-is-incremental-view-maintenance/); [Graphsurge (arXiv:2004.05297)](https://arxiv.org/pdf/2004.05297) applies differential computation to graph analytics over changing graphs; [pg-trickle](https://github.com/trickle-labs/pg-trickle) is Rust differential-dataflow IVM for Postgres — an ecosystem signal for a Rust workspace.

**Why for Track B**: IVM is the mature, formally-grounded name for "event-driven graph currency." Key contribution: **it is not a spectrum choice** — differential dataflow gives *exact* equivalence to full recompute, so an incremental path can be validated against the tick-rebuild **as an oracle**. Run both, assert equality, then retire the rebuild.

**Countervailing evidence, stated plainly**: F2-5 (retraction cost; Stardog's refusal). And **Zep itself does not fully incrementalize** — community maintenance uses *dynamic extension* (assign a new node to the plurality community of its neighbours, update that summary), and the paper is explicit this **"delays full refreshes but causes gradual divergence, necessitating periodic complete recalculations."** A state-of-the-art system chose **hybrid**: incremental for currency, periodic full for correctness. Direct primary-source support for Track B's hybrid option over a pure event-driven rewrite. **Verdict: NET-NEW as vocabulary and as an oracle-based migration strategy.**

### F4-3. Index repair and consolidation as an explicit scheduled operation
**Who**: FreshDiskANN's `consolidation` re-organizes the graph around deleted nodes *"to improve search quality"* — stated purpose is **recall, not space** ([Pinecone](https://www.pinecone.io/blog/hnsw-not-enough/)). Weaviate's timed cleanup. Peer-reviewed: [Wolverine, PVLDB 18](https://www.vldb.org/pvldb/vol18/p2268-zheng.pdf) (HNSW search-path repair), [HAKES, PVLDB 18](https://www.vldb.org/pvldb/vol18/p3049-ooi.pdf). Evaluation methodology: [How Should We Evaluate Data Deletion in Graph-Based ANN Indexes?](https://openreview.net/pdf?id=lnaC19Pd30)

**Why**: HNSW recall degradation after heavy mutation is **silent** — no error, no exception, no log line; queries just quietly return worse neighbours. Undetectable without deliberate measurement (F5-2). **The most dangerous failure mode in the map, because nothing surfaces it.** **Verdict: NET-NEW.** Track C names the gap; the external view confirms it as table-stakes and adds that **repair is recall-preservation, not space-reclamation** — changing both its priority and its trigger condition.

### F4-4. Embedding-model versioning and dual-index migration
**Who**: Practitioner consensus ([Index Drift Problem, 2026-04](https://tianpan.co/blog/2026-04-09-embedding-models-production-versioning-index-drift)); research on avoiding full rebuild: [Drift-Adapter (arXiv:2509.23471)](https://arxiv.org/pdf/2509.23471). Consensus pattern: stamp every vector with model version + preprocessing hash + chunking config; build the new index in parallel, dual-query, compare MRR/recall on a golden set, shift traffic gradually.

**Sourcing caveat, honestly**: documented mainly in practitioner blogs, not peer-reviewed — weaker evidence. Included because the *mechanism* (vectors from different models are not comparable, so mixing corrupts neighbour rankings) is a mathematical fact, not an empirical claim. A mixed-model index produces **plausible-looking wrong answers**, and models are upgraded on a 12–24 month cadence — a *when*, not an *if*. **Verdict: NET-NEW / possible overlap — Track A to dedupe against ass-052 (RuVector).** Not pursued further per scope.

---

## F5 — Health telemetry & observability

### F5-1. A published corpus-health scorecard ★ TOP-RANKED, LOWEST COST
**Who**: **Guru's "Internal Trust Score"** — *"tells leadership exactly how much of their knowledge base is actually up-to-date"*, with verification rates, search trends and **knowledge gaps** ([Guru](https://www.getguru.com/features/verification), [verification docs](https://help.getguru.com/docs/verifying-and-unverifying-cards)). Wikidata's [violation summary](https://www.wikidata.org/wiki/Wikidata:Database_reports/Constraint_violations/Summary) is the same at open-collaborative scale, regenerated daily.

**Why**: **A single scalar that trends is worth more than a hundred metrics that don't.** Converts corpus health from anecdote ("search feels worse lately") into an observable, and makes every background operation's *value* measurable. Direct answer to "the engine watching the corpus" and the missing tick-health nfr.

Candidate signals by cost-to-compute: constraint-violation count (F2-3, free); orphan/dangling-edge count (free); quarantine backlog and *age distribution* (free); duplicate-cluster count; **unretrieved-entry fraction** (the "dead knowledge" ratio — cheap and highly diagnostic); contradiction count; recall@k on a golden set (F5-2, the only expensive one). `PRODUCTION`. **Verdict: NET-NEW — highest value-to-effort in the map**; most signals are counting queries over structure that already exists.

### F5-2. Golden-set retrieval regression testing
**Who**: Standard RAG-eval practice ([Redis](https://redis.io/blog/rag-system-evaluation/), [Weaviate](https://weaviate.io/blog/retrieval-evaluation-metrics)). Recurring theme across sources: **"silent failures"** — degradation no conventional monitoring surfaces because nothing errors.

**Why**: The **only** detector for F4-3 and F4-4, both invisible by construction. Without it a knowledge engine cannot answer "is retrieval as good as it was last month?" — for a self-learning system, the defining question. **Verdict: NET-NEW.**

### F5-3. Human-in-the-loop verification with expiry
**Who**: Guru — expiry alerts the assigned SME, who audits and re-verifies; freshness date shown on every card; **default verification frequency 1 month**. The Glean contrast is instructive: Glean relies on real-time source freshness and permission-aware access **instead of** human-review loops. Two credible products, opposite answers.

**Why, with a caveat**: Guru's model works because it has SMEs with an org mandate. An agent-written corpus has no such staffing; importing it wholesale creates a review queue nobody drains. **The transferable part is the *state machine*, not the human** — verified/unverified/expired as a first-class attribute, expiry as a *ranking demotion* rather than a notification, verified fraction as the headline metric.

**Recorded conflict in the evidence**: Guru says freshness needs periodic human re-attestation; Glean says derive it from the source; the agent-memory literature effectively sides with Glean-plus-usage (F3-1/F3-3 — derive strength from behaviour). **I am not resolving this.** For an agent-written, agent-consumed corpus the usage-signal path looks more tractable, but that is a judgement, not a finding. **Verdict: PARTIAL** — DEPRECATED is an *authored* state; a *verification* axis orthogonal to it is not implied.

---

## Ranked: value to a knowledge engine

Ranked by (impact on corpus quality) × (breadth of downstream enablement) ÷ (cost). Cost is a directional judgement, not an estimate.

| # | Pattern | Family | Verdict | Cost | Why this rank |
|---|---|---|---|---|---|
| 1 | **Declarative constraints + violation report** | F2-3 | NET-NEW | Low | Deterministic queries, no ML/LLM. Turns "probably fine" into a trending number. Wikidata-proven at scale. |
| 2 | **Corpus-health scorecard** | F5-1 | NET-NEW | Low | Makes everything else measurable. Mostly counting queries. Prerequisite for judging any change. |
| 3 | **Bi-temporal validity / supersession** | F1-1 | NET-NEW | Med | Substrate that makes contradiction handling and aggressive lifecycle *safe*. Enables 4, 6, 9. |
| 4 | **Retrieval-gated conflict adjudication** | F2-1 | NET-NEW (mech.) | Med | The only defect that actively poisons answers. Retrieval-gating makes it affordable. |
| 5 | **Last-access decay + importance + reinforcement** | F3-1/3-3 | PARTIAL | Med | Self-curation. Peer-reviewed (UIST '23). Last-*access* framing resolves age≠staleness directly. |
| 6 | **Statement-level provenance + confidence** | F2-2 | NET-NEW/PARTIAL | Med | Precondition for every automated integrity action; nothing above #6 can safely auto-act without it. |
| 7 | **Sleep-time / async improvement pass** | F4-1 | NET-NEW | Med-High | Separates *improving* the corpus from *reconciling* it. Largest conceptual gap found. |
| 8 | **Golden-set retrieval regression** | F5-2 | NET-NEW | Med | Only detector for silent index/embedding degradation. |
| 9 | **Purge lifecycle with index repair** | F1-2 | NET-NEW | Med-High | Unbounded quarantine is unbounded liability. Must repair HNSW, not just drop rows. |
| 10 | **Per-entry temporal-sensitivity class** | F3-2 | NET-NEW | Low-Med | Sharpens the domain-pack lens where per-domain is provably too coarse. |
| 11 | **Index repair / consolidation** | F4-3 | NET-NEW | Med | Silent recall decay. Rank driven by *detectability*, not severity — pair with #8. |
| 12 | **Tiered retention with demotion** | F1-3 | PARTIAL | Med | Makes lifecycle reversible; safe answer to auto-quarantine-on-age. |
| 13 | **Verification state + expiry** | F5-3 | PARTIAL | Low-Med | Transferable as a state machine; the human-review loop isn't staffable here. |
| 14 | **Incremental maintenance (IVM/differential)** | F4-2 | NET-NEW (vocab) | High | Track B owns it. Value is the formal framing + oracle migration, not a rewrite mandate. |
| 15 | **Safety-triggered forgetting / governance** | F3-4 | PARTIAL | Med | Real injection surface, `RESEARCH` maturity only. Directional. |
| 16 | **Truth maintenance under retraction** | F2-5 | NET-NEW (caution) | — | A constraint on #14, not a feature. Deletion is where incremental gets expensive. |

**If only three things are taken from this track: #1, #2, #3.**

---

## Cross-track notes (offered, not asserted — those tracks own their calls)

- **→ Track B**: Zep, state-of-the-art, chose **hybrid** — dynamic extension for currency, periodic full recalculation for correctness, with explicit acknowledgement of gradual divergence. RDFox-vs-Stardog shows two mature engines splitting with **deletion cost** deciding. Differential dataflow's exactness enables validating an incremental path against the existing rebuild as an oracle. The external view does *not* support "replace the tick"; it supports "add an incremental path and keep the rebuild as the reconciler."
- **→ Track C (NLI)**: the ecosystem has moved off standalone NLI classifiers. Re-scope as retrieval-gated LLM adjudication — with the counter-current (arXiv:2606.01435) that deterministic supersession may beat LLM-judged freshness for the *temporal* subset specifically.
- **→ Track C (auto-quarantine of aged DEPRECATED)**: the external answer is **no — not on age.** Decay on *last access*, demote before quarantining, let usage decide. Age-triggered quarantine is the exact anti-pattern the temporal-RAG literature names.
- **→ Track C (purge)**: purge must repair the index, not just drop the row. A tombstone is not a deletion.
- **→ Track C (monitoring)**: F5-1 + F2-3 together are a complete, low-cost first version.
- **→ Track C (GNN keep/retire)**: **no external signal either way.** I found no evidence that production agent-memory or KG platforms run GNNs in background maintenance loops; the 2025–26 production stack is LLM-adjudication + graph algorithms (label propagation, Personalized PageRank in HippoRAG) + vector search. Absence of evidence is weak evidence and I did not search this exhaustively — but nothing in the surveyed landscape argues *for* retaining GNN machinery.

---

## Unanswered Questions

1. **Which patterns are genuinely net-new to Unimatrix?** Structurally unanswerable from this track — barred from Unimatrix and ass-103. Every `LIKELY-COVERED`/`PARTIAL` is a hypothesis. **Tracks A and C must adjudicate the verdict column.** By design, not a shortfall.
2. **What does any of this cost at Unimatrix's corpus size?** No sizing data reaches this track (the "2504 quarantined entries" figure is the only quantity visible, and it is not a corpus size). Cost rankings are relative and directional only.
3. **Does retrieval-gated LLM adjudication stay affordable at scale?** Zep and Mem0 both do it; neither publishes per-write cost. The Zep paper reports ~90% latency reduction vs baseline but **provides no explicit operational cost figures.** Unresolved in the literature.
4. **What half-life is right for a decay policy?** γ=0.995/hour is tuned for a simulation with sandbox hours, not a durable engineering corpus. No source gives a defensible default for an SDLC-like domain. Empirical; needs the golden set (F5-2).
5. **Guru-style attestation vs Glean-style derived freshness?** Two credible products, opposite answers, with the agent-memory literature implicitly on a third path (usage signal). Surfaced as a live conflict, deliberately not resolved.
6. **Is rich-get-richer in reinforcement (F3-3) actually harmful in practice?** The failure mode is real; I found no settled mitigation. Open problem in the field, not just here.
7. **Is the Ghost Vectors attack practical?** 2026 preprint, not peer-reviewed, no independent replication found. Structural claim sound; exploitability unconfirmed.
8. **Incremental SHACL validation.** arXiv:2508.00137 confirms re-validation from scratch is the current state and incremental validation is an open research problem. Do not assume continuous constraint checking is free at scale.

---

## Out-of-Scope Discoveries

1. **Memory-injection attacks on long-term agent memory** — [ER-MIA (arXiv:2602.15344)](https://arxiv.org/pdf/2602.15344) demonstrates black-box adversarial injection into agent memory stores. An engine accepting agent-authored writes has this surface by construction. **Likely warrants its own security spike** — a threat-model question, not a background-processing one.
2. **Ghost-vector reconstruction as a privacy issue** — surfaced here for its lifecycle implication; the privacy dimension is separate. **Possible spike** if the corpus ever holds sensitive material.
3. **LazyGraphRAG's defer-to-query-time inversion** — [Microsoft Research](https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/) reports comparable quality at >700× lower query cost by deferring heavy analysis to query time and avoiding regular full re-indexing. Out of scope here, but the strongest single argument found for **"do less in the background."** **Worth flagging to Track A for ass-032 overlap.**
4. **DBSP / differential dataflow in Rust** — [pg-trickle](https://github.com/trickle-labs/pg-trickle) shows production-grade differential dataflow in the Rust ecosystem. Supply-side signal if Track B goes incremental; not evaluated for fitness.
5. **Evaluation benchmarks for memory systems** — surveys note most systems *"fail conspicuously on selective forgetting."* An emerging benchmark literature could supply a ready-made golden set (F5-2). **Small spike, high leverage.**
6. **ACT-R base-level activation as a decay model** — [HAI '25](https://dl.acm.org/doi/10.1145/3765766.3765803). A 30-year-validated cognitive model of exactly the F3-1/F3-3 dynamic, with published parameters. Better-grounded than the ad-hoc exponentials most agent systems use. Candidate if a decay model is ever designed for real.

---

## Recommendations Summary

*Directional only. Mechanism choices belong to delivery; ratification belongs to uni-zero + human.*

- **Overall bar**: world-class background knowledge processing is **four tiers** — provenance-bearing write log → delta-driven incremental maintenance → asynchronous *improvement* pass → periodic full reconciliation. Unimatrix as described has tier 4. Tiers 2 and 3 are the gap.
- **Lifecycle**: adopt **bi-temporal supersession** (F1-1) as substrate; make **purge a real, index-repairing stage** (F1-2); **demote before quarantining** (F1-3). Never trigger lifecycle transitions on **creation age**.
- **Integrity**: highest-value/lowest-cost is a **declarative constraint layer with a violations report** (F2-3, Wikidata/SHACL/Stardog-proven). Then **statement-level provenance** (F2-2) as the precondition for automated action. Re-scope NLI as **retrieval-gated LLM adjudication** (F2-1). Note dedup, resolution and contradiction are **one retrieval-gated pass with three verdicts**, not three jobs.
- **Relevance decay**: decay on **last access, not creation** (F3-1) — this alone resolves age≠staleness without a domain exemption. Pair with never-decaying **importance** and **access reinforcement** (F3-3). Refine the domain-pack lens with a **per-entry time-sensitive/timeless attribute** (F3-2), because an SDLC corpus provably contains both.
- **Background health**: add a **distinct asynchronous improvement pass** separate from the consistency tick (F4-1) — the largest conceptual gap. Schedule **index repair** as recall-preservation (F4-3). For event-vs-tick the external evidence supports **hybrid, not replacement** (F4-2 + F2-5); cost it on the **retraction** path.
- **Observability**: publish a **corpus-health scorecard** (F5-1) and run **golden-set retrieval regression** (F5-2). Without F5-2, index and embedding degradation are undetectable by construction.
- **Start here**: F2-3 and F5-1 — low cost, no ML, and together they make every subsequent change measurable.
- **Do not**: apply uniform creation-time decay; treat tombstoning as deletion; assume incremental graph maintenance is cheap because insertion is; or import a human verification-queue model with no staffing behind it.