# ASS-057: Unimatrix as a Research Domain Substrate — Fit Analysis and Value-Add Assessment

**Date**: 2026-05-14  
**Tier**: 1 — informs roadmap positioning, potential Wave 2+ delivery, domain-agnostic architecture validation  
**Feeds**: Wave 2 deployment model, W3-1 GNN graph architecture, potential `crt-NNN` (new graph traversal tools)  
**Related**: ASS-055 (DependsOn write path), W1-1 (`crt-021`, typed graph + RelationType), WA-4 (proactive delivery), ASS-040 (intelligence roadmap)

---

## Background

A separate repository is evaluating Unimatrix as the knowledge substrate for an **autonomous research workflow** — a standardized, agent-driven methodology for conducting research with persistent memory. That workflow has produced a formal requirements document: a graph behavior specification covering 8 entity categories, 14 first-class edge types, sophisticated supersession traversal semantics, and a 7-API query surface (see Appendix A for the full requirements document).

Unimatrix's vision states: *"Any knowledge-intensive domain — environmental monitoring, SRE operations, scientific research, regulatory compliance — runs on the same engine, configured not rebuilt."* This spike tests that claim against a concrete, well-specified research domain use case.

ASS-055 explored a narrow extension: adding an ADR `DependsOn` write path using the existing `Prerequisite` relation type. This spike is categorically larger — it asks whether Unimatrix's architecture can support a **graph-native domain** whose primary usage pattern is multi-hop traversal (not semantic search), whose entity taxonomy is domain-specific, and whose retrieval patterns include antijoin queries, bidirectional supersession chains, and batch atomic writes.

**Critical framing**: The external requirements document makes assumptions about what Unimatrix will and will not do. Treat these as descriptions of what the workflow currently plans to handle itself — not as binding constraints on Unimatrix's role. Where Unimatrix already has relevant capability that the requirements assumed away, that is a value-add opportunity, not a scope conflict. The NLI infrastructure is the clearest example: the requirements state "no built-in NLI/contradiction engine — the model emits contradicts edges; Unimatrix just stores them." But Unimatrix has NLI infrastructure (W1-4, `crt-023`) that is currently turned off because the SDLC corpus is a poor fit for NLI. A research corpus — where Claims extracted from Sources can genuinely contradict each other — may be the use case NLI was built for.

The spike has two equally weighted outputs: **(1) fit and gap analysis** (what the requirements ask for, what Unimatrix can serve today or with incremental extension) and **(2) value-add assessment** (where Unimatrix's existing intelligence pipeline creates capabilities the research domain didn't know to ask for, and what those are worth).

The evaluation does not commit to implementation. It produces: (1) a gap analysis between current state and stated requirements, (2) an architectural fitness verdict, (3) a value-add opportunity inventory, and (4) a roadmap positioning recommendation.

---

## The Questions

**Primary (fit)**: Can Unimatrix serve as the graph storage and traversal substrate for this research domain workflow — and where it cannot today, is the gap incremental (new tools, new edge types) or architectural (conflicting design assumptions)?

**Primary (value-add)**: Where does Unimatrix's existing intelligence pipeline — NLI, confidence scoring, PPR graph traversal, co-access learning, behavioral signal collection, proactive delivery — create capabilities that would materially improve the research workflow beyond what the requirements document envisioned?

**Secondary**:

1. **Domain mapping**: Can the 8 research categories (Goal, Source, Finding, Claim, Entity, Thesis, POC, Deliverable, Insight) be expressed using Unimatrix's configurable category model (W0-3 `config.toml`)? Are there semantic mismatches that require core changes?

2. **Edge type gap**: Which of the 14 required edge types already exist in `RelationType`, which are new variants, and which would require data model changes (edge properties, symmetric storage)? The `contradicts` edge's logical symmetry — stored once but queried bidirectionally — is a specific question.

3. **Traversal API gap**: The requirements specify 7+ new query APIs (context_neighbors, context_subgraph, context_inverse, context_supersession_chain, context_current, context_path, context_batch_write, context_filter). Which of these are expressible through existing tools (context_search, context_lookup, context_get, context_correct), which require new MCP tools, and which require new storage primitives?

4. **Supersession semantics gap**: The requirements distinguish `resolve_supersessions=true` (transparent traversal through superseded versions) vs `resolve_supersessions=false` (audit mode, literal edges only). Current Unimatrix models supersession as an entry property (`supersedes`/`superseded_by`) not as a traversal-time parameter. What would it cost to expose this as a query parameter vs. the current implicit behavior?

5. **Edge properties gap**: The requirements expect typed properties on edges (`contribution_kind`, `strength`, `salience`, `revision_reason`, `human_confirmed` bool). Current GRAPH_EDGES schema has a `metadata` JSON column. Is this sufficient for the stated query patterns, or do the query APIs require structured property access that metadata JSON cannot efficiently serve?

6. **Antijoin query feasibility**: Q9 (Sources with no incoming `cites` edge) is antijoin-shaped. What is the query plan? Can this be served by the existing SQLite schema without a materialized index? What is the worst-case scan behavior at the stated scale (a few thousand entities)?

7. **Batch write atomicity**: `context_batch_write` requires writing 20–50 entries + 50–200 edges + supersession operations in a single logical transaction. What would this cost in the current write model (async write queue, bounded 50-event batches, 500ms flush)? Is a batch write API architecturally compatible?

8. **Architecture fitness verdict**: Is this "Unimatrix configured for a research domain" (the stated vision — config, not code) or "a different system that shares some primitives"? The answer determines whether this belongs on the Unimatrix roadmap or should be treated as a graph substrate fork.

9. **Value-add opportunity inventory**: For each Unimatrix intelligence capability that the requirements assumed away, assess whether the research domain is actually a better fit for that capability than the current SDLC use case — and what the research workflow gains if Unimatrix enables it rather than leaving it to the model.

---

## Why This Matters to the Vision

The vision document explicitly lists scientific research as a target domain. The product story states: *"Any knowledge-intensive domain... runs on the same engine, configured not rebuilt."* This spike is the first concrete test of that claim against a fully-specified non-SDLC domain.

If the gap is incremental (new edge types, new MCP tools, extended query parameters), the spike validates the architecture and defines a Wave 2+ delivery scope. If the gap is architectural (the research domain is graph-first in a way that conflicts with Unimatrix's vector-first design), the spike surfaces that conflict before any implementation investment.

The research domain's stated traversal patterns — especially multi-hop subgraph retrieval (Q2) and antijoin queries (Q9) — stress-test assumptions baked into the current architecture: that the HNSW vector index is the primary retrieval anchor and that graph traversal is a secondary enrichment layer. In the research domain, the graph IS the primary access pattern. This inversion may or may not require architectural changes; the spike determines which.

This also has strategic implications for Wave 2's multi-project routing (W2-3) and the domain-agnostic deployment model: if a research domain can be supported via `config.toml` alone, every Wave 2 deployment story gets stronger.

---

## Prior Art to Build On

### Unimatrix current graph state

**Typed graph** (`crt-021`, `crates/unimatrix-engine/src/graph.rs`):
- `StableGraph<u64, RelationEdge>` with in-memory cache (`Arc<RwLock<_>>`, tick-rebuilt)
- `RelationType` enum: `CoAccess`, `Supports`, `Informs`, `Prerequisite`, `Contradicts`, `Supersedes` — 6 types today
- `GRAPH_EDGES` table: `(source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)`
- UNIQUE constraint: `(source_id, target_id, relation_type)` — one edge per type per pair

**PPR + graph_expand** (`graph_ppr.rs`, `graph_expand.rs`):
- PPR: reverse-walk personalized PageRank (seeding a node causes mass to flow to its incoming neighbors)
- `graph_expand`: BFS from HNSW seed set, outgoing edges, bounded by `max_candidates`
- Both already consume `Prerequisite`, `CoAccess`, `Supports`, `Informs`

**Supersession model**:
- Stored on entries: `supersedes` and `superseded_by` fields (entry-level, not edge-level)
- `build_typed_relation_graph` Pass 2a: Supersedes edges derived from `entries.supersedes` field (GRAPH_EDGES Supersedes rows skipped)
- Current traversal: no `resolve_supersessions` parameter; superseded entries are filtered by status at query time
- Multi-hop supersession chain: not directly queryable via MCP today

**Existing MCP tools relevant to traversal**:
- `context_search`: semantic HNSW → PPR expander → fused scoring → top-k
- `context_lookup`: filter by topic, category, tags, status
- `context_get`: single entry by ID
- `context_correct`: deprecates old, creates new with correction chain link (supersession write path)

**Config-driven categories** (W0-3, `dsn-001`):
- Categories fully configurable via `config.toml`
- No hardcoded category names in scoring pipeline (boosted_categories is config-driven)
- Category allowlist enforced at ingest — new domains define their own taxonomy

### ASS-055 findings (prior art, directly relevant)

ASS-055 resolved the direction contract for `Prerequisite`: edge stored as A→B means "A is a prerequisite of B" (B depends on A). PPR reverse-walk from B correctly surfaces A. `graph_expand` outgoing from B does NOT reach A under current BFS direction — this was flagged as a known gap requiring either direction reversal or a second inbound pass.

The research domain's `depends_on` edge (Thesis→Thesis) and the `Prerequisite` edge from ASS-055 are semantically equivalent. The direction resolution from ASS-055 applies directly. ASS-057 should not re-litigate this — reference the findings.

### Research domain requirements document

The full requirements document (provided by the human, 2026-05-14) is preserved as Appendix A. Researcher: treat it as the authoritative statement of external requirements. Do not re-scope it; evaluate it as written.

---

## What to Investigate

### Q1 — Entity category mapping

For each of the 8 research categories (Goal, Source, Finding, Claim, Entity, Thesis, POC, Deliverable, Insight), assess:
- Can it be expressed as a Unimatrix entry category without semantic distortion?
- Does the "stub pattern" (Source, POC, Deliverable carry pointers; substance lives elsewhere) align with how Unimatrix entries work? Unimatrix entries carry `content` as their primary payload. If a Source is a stub-with-URL, what goes in `content`? Is this just a convention or does it require schema support?
- Are there properties these categories need that Unimatrix entries don't have? (e.g., Thesis has a `status` lifecycle — `proposed`, `supported`, `refuted`, `abandoned`. Unimatrix entries have `active`/`deprecated`/`quarantined`. Is this a mismatch or can Thesis status be stored in `metadata`?)

Produce a mapping table: category → `config.toml` category name → what goes in `content` → any schema gaps.

### Q2 — Edge type gap analysis

Map each of the 14 required edge types to current `RelationType`:

| Required edge | Current RelationType | Gap |
|---------------|---------------------|-----|
| advances | ? | ? |
| cites | ? | ? |
| asserts | ? | ? |
| mentions | ? | ? |
| supports | Supports | check direction + property fit |
| refutes | ? | ? |
| contradicts | Contradicts | check symmetry storage + human_confirmed bool |
| depends_on | Prerequisite | ASS-055 direction resolution applies |
| tests | ? | ? |
| derived_from | ? | ? |
| motivates | ? | ? |
| about | ? | ? |
| supersedes | Supersedes | check property fit (revision_reason) |
| related_to | ? | ? |

For each new type:
- What is the edge direction? What does PPR reverse-walk behavior mean for this type? Desirable or not?
- Does `graph_expand` outgoing traversal give useful results? Or does this edge type require inbound BFS expansion?
- What edge-specific properties does it carry (from the requirements table), and can `metadata` JSON serve those properties for the query patterns in §5?

Focus: the `contradicts` edge is logically symmetric but stored once. Current GRAPH_EDGES UNIQUE constraint is `(source_id, target_id, relation_type)` — asymmetric. Storing `(A, B, Contradicts)` and querying for `(B, A)` would find nothing. How should symmetric edges be handled? Options: (a) store both directions, (b) query layer deduplicates, (c) use `OR (source_id=B AND target_id=A)` in all queries. Evaluate each.

### Q3 — Traversal API gap analysis

For each of the 7+ required query APIs:

**context_neighbors(id, edge_types, direction, depth, resolve_supersessions)**
- Closest current analog: none (context_search is semantic, not graph-hop)
- What storage query would serve this? Single-hop: `SELECT * FROM GRAPH_EDGES WHERE source_id=? AND relation_type IN (?)` (outgoing) or `target_id=?` (incoming). Multi-hop requires BFS iteration.
- How does `resolve_supersessions` change the query? At each hop, if `target_id` points to a deprecated entry, follow its `superseded_by` chain to the current version.
- Is this a new MCP tool or can it be served by extending `context_lookup` with an edge_filter parameter?

**context_subgraph(seed_ids, edge_types, direction, max_depth, max_nodes, resolve_supersessions)**
- Multi-hop BFS returning node + edge set. No current equivalent.
- How does this interact with the in-memory graph cache (`Arc<RwLock<TypedRelationGraph>>`)? Can the BFS run against the in-memory graph or does it require GRAPH_EDGES reads?
- Memory + result size: max_nodes=200 cap as stated. Is this LLM-context-friendly to return as JSON?

**context_inverse(entity_type, missing_edge_types, limit)**
- Antijoin: entities of type X with no incoming edges of type Y. 
- SQL: `SELECT e.id FROM entries e LEFT JOIN graph_edges g ON e.id = g.target_id AND g.relation_type = ? WHERE e.category = ? AND g.target_id IS NULL LIMIT ?`
- Does the existing GRAPH_EDGES index support this antijoin efficiently? Check: `idx_graph_edges_target` — does it exist? What is the query plan?
- Worst case at 3,000 entities: antijoin over GRAPH_EDGES is bounded by index reads, not a full scan. Verify.

**context_supersession_chain(id, direction)**
- Walk `supersedes`/`superseded_by` fields iteratively (or recursively via CTE).
- SQLite recursive CTE: `WITH RECURSIVE chain(id) AS (SELECT id FROM entries WHERE id=? UNION ALL SELECT e.id FROM entries e JOIN chain c ON e.supersedes=c.id) SELECT * FROM chain`
- Is this correct given that `supersedes` is an entry field (not a GRAPH_EDGES row)? Check direction: `supersedes` points to the entry being replaced; `superseded_by` points to the replacement.
- Performance: chains are short (< 20 versions in practice). O(chain length) SQL reads.

**context_current(id)**
- Cheap lookup: given any ID in a chain, return the active end.
- Iterative: follow `superseded_by` until null. O(chain length).
- Can this be a thin wrapper around a single SQL query with recursive CTE? Or is the in-memory graph sufficient?

**context_path(from_id, to_id, edge_types, max_depth)**
- Shortest path between two nodes. Not currently implemented.
- BFS from `from_id` following edges until `to_id` found or max_depth exceeded.
- Against in-memory graph: fast. Against GRAPH_EDGES: requires iterative joins or recursive CTE (SQLite has no native graph path query).
- For typical depth (max_depth=5) and graph size (a few thousand nodes, 10k+ edges): is BFS over the in-memory cache acceptable? Or does path query require separate indexing?

**context_batch_write(entries, edges, supersessions)**
- Atomic transaction: write N entries + M edges + K supersessions.
- Current write model: async write queue, bounded 50 events, 500ms flush, not a single-caller-controlled transaction boundary.
- This would require a synchronous write path that bypasses the async queue. Is that compatible with the current `write_pool` design? What is the latency implication for 50 entries + 200 edges?
- Integrity requirement: all entries must have `content_hash` computed and `previous_hash` chained before batch commits. Batch write must maintain the integrity chain.
- Security: batch write capability — same as `Write` capability, or a new `BatchWrite` capability?

**context_filter(entity_type, where, edge_filters, limit)**
- Property filters + edge count filters in one call.
- SQL: `SELECT e.* FROM entries e WHERE e.category=? AND <where_clauses> AND (SELECT COUNT(*) FROM graph_edges WHERE source_id=e.id AND relation_type=?)=0 LIMIT ?`
- Edge count filters require subqueries or JOINs. Assess query plan at scale.

**as_of timestamp parameter** (on all read APIs, Phase 3+ per requirements):
- Would require entries and edges to be queryable by `created_at ≤ T AND (deprecated_at IS NULL OR deprecated_at > T)`.
- Current schema: `entries.created_at` exists; `deprecated_at` does not (deprecation is a status change, not a timestamp column). Would need schema migration for full as-of support.
- Verdict: defer as stated. Flag as a schema migration if pursued.

### Q4 — Architecture fitness verdict

This is the most important output. Answer:

**Is the research domain use case "Unimatrix configured for a domain" or "a graph database with semantic search bolted on"?**

The research domain's primary access pattern is graph traversal (the document explicitly says "the workbench reads the graph far more than it writes it"). Unimatrix's primary access pattern is semantic vector search with graph enrichment as secondary. This inversion is the central tension.

Assess:
- If graph traversal tools (context_neighbors, context_subgraph, context_path) are added as new MCP tools backed by the existing `GRAPH_EDGES` table and in-memory graph cache, does the research domain get what it needs without changing the vector-first retrieval architecture?
- Or does the research domain's requirement for 14 edge types with typed properties, symmetric edges, antijoin queries, and batch writes push Unimatrix toward a general-purpose graph database — a direction that conflicts with the intelligence pipeline's vector-first design?
- Is there a "thin shell" answer: Unimatrix handles storage and graph APIs, and the research workflow uses only the graph tools (not semantic search)? Is that a valid partial use case?
- What does the config-driven domain model look like for this use case? Write a sketch of `research-domain.toml` that configures the 8 categories, describes how boosted categories would work, and identifies what cannot be configured.

Produce a verdict: **Aligned / Aligned with extensions / Architectural tension / Out of scope** — with explicit rationale.

### Q5 — Value-add opportunity inventory

This is the forward-looking half of the spike. For each Unimatrix intelligence capability below, assess: (a) what the requirements document assumed about who handles this, (b) whether the research corpus is a better fit than the current SDLC corpus, (c) what the workflow gains, and (d) what Unimatrix would need to expose or extend.

**NLI contradiction detection on Claims**

The requirements document states: "no built-in NLI/contradiction engine — the model emits contradicts edges; Unimatrix just stores them." Current state: NLI infrastructure exists (W1-4, cross-encoder ONNX model) but is effectively off because SDLC corpus entries (ADRs, patterns, procedures) are not semantically contradictory in the way NLI is trained to detect.

A research corpus is different. Claims extracted from Sources can genuinely contradict each other at the propositional level — that is exactly the SNLI/NLI training domain. Assess:
- Would NLI contradiction detection on Claim-to-Claim pairs produce meaningful `Contradicts` edges, where the current SDLC corpus has produced zero?
- What would the research workflow gain? Today: agents must manually identify contradictions or hope the model notices. With Unimatrix NLI: contradictions are detected automatically as Claims are stored, flagged as edges, and surfaced via Q5 traversal.
- Is the cross-encoder model (85MB, NLI fine-tuned) appropriate for Claim-level text, or does it need a domain-adapted model?
- Effort: NLI is already wired; the gap is enabling it for the research domain's Claim category specifically (category-filtered NLI, not all entries).

**Confidence scoring on Theses**

The requirements define Thesis status as a manually-managed lifecycle (proposed → supported → refuted → abandoned). Unimatrix has a confidence scoring system (Wilson score helpfulness + 6-factor composite) that learns from usage.

Assess whether a Thesis-specific confidence model makes sense: `supports` edges from Claims → Thesis and `refutes` edges from Claims → Thesis provide structured evidence that could drive confidence directly, without requiring explicit helpfulness votes. A Thesis with 5 supporting Claims and 0 refuting Claims should score differently from one with 3 supporting and 3 refuting.
- Is the current confidence model extensible to evidence-driven scoring (edge-count-weighted rather than vote-weighted)?
- What would the workflow gain: thesis status transitions could be confidence-gated rather than manual, with Unimatrix surfacing "this Thesis now has sufficient supporting evidence to promote" as a steward signal.

**Automatic relationship detection (S1/S2/S8 edge sources)**

The requirements assume the model (or human) declares all edges explicitly. Unimatrix has three automatic edge inference sources already in production: S1 (tag co-occurrence → Informs), S2 (structural vocabulary → Informs), S8 (search co-retrieval → CoAccess).

Assess whether these sources produce useful edges for the research domain:
- S1: if Claims share tags (entities mentioned), does that produce meaningful `mentions` or co-evidence edges automatically?
- S8: if researchers consistently retrieve Finding A and Thesis B together, does co-retrieval produce a weak `advances`-or-`supports` signal worth capturing?
- What new edge inference sources might be valuable for research domain specifically? Example: if two Findings cite the same Source (`cites` edges converge), that is a structural signal of relevance worth an Informs edge.

**PPR graph traversal for serendipitous discovery**

The requirements treat graph traversal as explicit and query-driven. Unimatrix's PPR expander surfaces entries that are graph-connected but semantically distant from the query. For research workflows, this is "related work you didn't know to look for."

Assess:
- When a researcher queries on a Thesis, does PPR expansion through `supports`/`refutes`/`depends_on` chains surface relevant Claims or Findings that semantic search alone would miss?
- Is the current PPR personalization vector construction (co-access affinity + phase category boost) adaptable to a research domain session context (current Goal as the anchor instead of current feature cycle)?
- What is the research workflow analogue of WA-4 proactive delivery? Could Unimatrix surface "here is a Claim you haven't seen that contradicts your current working Thesis" before the researcher asks?

**Behavioral signal learning from researcher activity**

The requirements do not mention any learning or adaptation. Unimatrix's behavioral signal pipeline (W1-5, col-023; crt-049/crt-050) learns from agent access patterns to adapt retrieval.

Assess:
- In a research workflow, researcher access patterns (which Findings they retrieve after setting a Goal, which Theses they promote after reading which Claims) are high-quality behavioral signals.
- Could `context_cycle` with a Goal as the cycle topic enable Goal-conditioned proactive delivery using the existing goal-cluster infrastructure (crt-046)?
- Is the research domain's daily cadence (one run per day, not continuous) compatible with the current tick-based learning infrastructure?

**For each capability**: produce a brief verdict — **High value / Low value / Needs investigation** — with the key reason. The goal is not to design implementation but to identify which capabilities create genuine new value for the research domain and belong in the roadmap positioning recommendation.

---

### Q6 — Effort estimate and roadmap positioning

If the verdict is Aligned or Aligned with extensions:
- Categorize each gap as: (a) configuration only, (b) new MCP tool over existing storage, (c) storage schema change required, (d) core architecture change required
- Estimate rough effort per category (days, not weeks per item)
- Propose where this fits on the roadmap: Wave 2 addition? Wave 3? Post-roadmap? Does it gate on any existing items?

If the verdict is Architectural tension or Out of scope:
- Identify the minimum subset of requirements that IS aligned (likely: configurable categories + new typed edge types + supersession chain query)
- Propose what Unimatrix could offer as a research-domain-compatible configuration without stretching the architecture
- Assess whether a thin "graph traversal layer" crate could be factored out that satisfies the research domain without conflating it with the intelligence pipeline

---

## Out of Scope

- Implementing any of the traversal APIs or new edge types — this is a feasibility and gap analysis
- Designing the research workflow tooling itself (skills, agents) — that belongs to the external repository
- NLI contradiction detection (`contradicts` edges are model-emitted, per the requirements; Unimatrix just stores them)
- Graph visualization — excluded by the requirements document itself
- Full `as_of` timestamp support — deferred to Phase 3+ per requirements; note what it would cost but do not design it
- Multi-project or cross-repository graph federation

---

## Expected Output

A `FINDINGS.md` in this directory with:

1. **Entity category mapping table** — each of 8 research categories mapped to Unimatrix config-layer, with stub-pattern assessment and property gap identification.

2. **Edge type gap table** — 14 required edge types mapped to existing RelationType (or "new"), with direction contract, PPR behavior assessment, and edge property fit verdict per type. Flag `contradicts` symmetry resolution explicitly.

3. **Traversal API feasibility table** — for each of the 7+ required APIs: can it be served by existing tools, requires new MCP tool, or requires storage/schema change. Include the Q9 antijoin query plan assessment and the batch write compatibility analysis.

4. **Supersession semantics gap** — concrete assessment of what `resolve_supersessions=true/false` would cost to implement as a query parameter given the current entry-field supersession model.

5. **Architecture fitness verdict** — the Aligned / Aligned with extensions / Tension / Out of scope verdict, with explicit rationale. Address the vector-first vs. graph-first inversion directly.

6. **Value-add opportunity table** — for each Unimatrix intelligence capability assessed in Q5: High / Low / Needs investigation verdict, the key reason, and what the research domain gains. This is the strategic output — it should surface where Unimatrix creates value the workflow didn't design for.

7. **Roadmap positioning recommendation** — if aligned: which wave, what gate conditions, rough effort — covering both the gap-closing items and the highest-value value-add opportunities. If not aligned: minimum viable Unimatrix offering for the research domain.

8. **`research-domain.toml` sketch** — what a domain config file for this use case would look like; what it can configure, what it cannot.

---

## Constraints

- Researcher reads code only — no code changes, no Unimatrix writes
- The requirements document (Appendix A) describes the research domain and the workflow's current intentions — it is **not** a binding constraint on what Unimatrix will or won't do. Where Unimatrix has existing capabilities the requirements assumed away, assess those as value-add opportunities, not as scope conflicts
- ASS-055 direction resolution for `Prerequisite`/`DependsOn` is established — do not re-litigate; reference its findings
- The research domain is an external use case. Unimatrix should not be restructured around it — but it should be assessed both for fitness as-is and for what it could become with targeted evolution

---

## Appendix A — Research Domain Requirements Document

The full graph requirements document follows. This is the external specification this spike evaluates.

**Source**: arch-research repository, `docs/graph-requirements.md`, Draft v0.1 (2026-05-14)

---

### Why this exists

The research workflow is graph-native, not document-native. Goals anchor everything; Findings, Theses, POCs, Insights, and Deliverables `advances` Goals; Claims connect Findings to Theses through `supports`/`refutes`; Entities normalize references. The workbench reads the graph far more than it writes it — every digest, every `/goal review` session, every `/topic-state` query is a multi-hop traversal.

It is **not** an implementation spec. It says only: *given a seed and an intent, here is what we need to be able to ask, and here is what we expect back*.

---

### Entity categories (8 required + 1 optional)

- **Goal** — anchor; the work-defining entity.
- **Source** — captured external item (stub-with-URL, substance lives in Reader/cache).
- **Finding** — our triaged interpretation of one Source.
- **Claim** — atomic proposition extracted from a Finding.
- **Entity** — normalized named-thing (tool, paper, technique, person, company, concept).
- **Thesis** — our own proposition, with status lifecycle.
- **POC** — runnable experiment stub (substance lives on a git branch).
- **Deliverable** — final packaged output stub (substance lives on disk).
- *(optional)* **Insight** — cross-item synthesis output.

---

### Edge types (14 required, first-class graph records)

Each edge: `from_id`, `to_id`, `edge_type`, `created_at`, `created_by`, `confidence` (nullable), `properties` (JSON). Indices on `(from_id, edge_type)` and `(to_id, edge_type)`.

| Edge | Direction | Cardinality | Properties | Why first-class |
|------|-----------|-------------|------------|-----------------|
| `advances` | Finding\|Thesis\|POC\|Insight\|Deliverable → Goal | many-to-many | `contribution_kind` | Most-walked edge |
| `cites` | Finding → Source | 1:1 | — | Reverse-direction audit |
| `asserts` | Finding → Claim | 1:N | — | Evidence bridge |
| `mentions` | Finding\|Thesis\|Claim\|Insight\|Goal → Entity | many-to-many | `count`, `salience` | Entity-centric retrieval |
| `supports` | Claim → Thesis | many-to-many | `strength` | Thesis confidence |
| `refutes` | Claim → Thesis | many-to-many | `strength` | Thesis confidence |
| `contradicts` | Claim ↔ Claim | many-to-many (symmetric) | `confidence`, `human_confirmed` | Cross-item synthesis |
| `depends_on` | Thesis → Thesis | many-to-many | — | Dependency context |
| `tests` | POC → Thesis | many-to-1 | — | POC lifecycle |
| `derived_from` | Insight → Finding\|Claim\|Thesis | 1:N | — | Insight lineage |
| `motivates` | Insight → Thesis | many-to-many | — | Synthesis to proposition |
| `about` | Thesis → Entity | many-to-many | — | Thesis entity tagging |
| `supersedes` | * → * (same category) | many-to-1 | `revision_reason` | Audit chain |
| `related_to` | * → * | many-to-many | `note` | Soft fallback |

`contradicts` is logically symmetric — stored once, queried bidirectionally.

---

### Supersession semantics

- **`resolve_supersessions=true`** (default): traversal transparently follows supersession chains; edges to superseded entities include the superseder
- **`resolve_supersessions=false`** (audit mode): edges returned as stored
- Chain walking: both directions, unbounded depth, O(chain length) `current(X)` lookup
- Invariants: intra-category only; single current member (forks = two new nodes each superseding parent, return both children)

---

### 12 traversal patterns (Q1–Q12)

Q1 Goal evidence (1 hop, advances, incoming, resolve=true)  
Q2 Goal full subgraph (3–4 hops, mixed edges, max 200 nodes)  
Q3 Thesis evidence (3 hops, supports+refutes → asserts inverse → cites)  
Q4 Entity-centric retrieval (2 hops: mentions incoming, advances outgoing)  
Q5 Contradiction surface (3 hops: advances → asserts → contradicts)  
Q6 Supersession chain (unbounded, resolve=false)  
Q7 POC test linkage (1 hop, tests incoming)  
Q8 Insight lineage (1 hop, derived_from + motivates outgoing)  
Q9 Sources with no incoming cites (antijoin)  
Q10 Stale Goal detection (property filter + edge count)  
Q11 Multi-Goal advancement detection (outgoing advances count > 1)  
Q12 As-of-time-T reconstruction (Phase 3+, optional)

---

### Required query API shapes

```
context_neighbors(id, edge_types=[], direction='both', depth=1, resolve_supersessions=True)
context_subgraph(seed_ids, edge_types=[], direction='both', max_depth=3, max_nodes=200, resolve_supersessions=True)
context_inverse(entity_type, missing_edge_types, limit=100)
context_supersession_chain(id, direction='both')
context_current(id)
context_path(from_id, to_id, edge_types=[], max_depth=5)
context_batch_write({entries, edges, supersessions})
context_filter(entity_type, where, edge_filters=[], limit)
```

All read APIs: optional `as_of: timestamp` (Phase 3+).

---

### What is explicitly NOT required

- Query language (Cypher, GraphQL)
- Reasoning/inference (model does that)
- Real-time streaming / change feeds
- Sub-millisecond p99 (sub-second is sufficient)
- Distributed graph / sharding (single host, few thousand entities)
- Built-in NLI / contradiction engine (model emits edges; Unimatrix stores them)
- Full-text indexing of substance (stub entities carry pointers)
- Graph visualization

---

### Open items from requirements document

1. Edge property storage: on edge record vs. side table?
2. `resolve_supersessions` default — true (transparent) or false (explicit opt-in)?
3. `related_to` retention — may deprecate if noisy
4. Edge confidence: `NULL` = human-asserted, float = model-inferred — document this convention
5. Q9 antijoin performance — needs real query plan, not a scan
6. `as_of` query support — Phase 3+, defer if not free
