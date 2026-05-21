# FINDINGS: ASS-057 Track D — Architecture Fitness & Roadmap Positioning

**Spike**: ass-057 (Track D)
**Date**: 2026-05-14
**Approach**: investigation
**Confidence**: validated (all answers grounded in codebase evidence with line references)
**Questions answered**: Q8 (architecture fitness verdict), Q10 (effort estimate and roadmap positioning), research-domain.toml sketch

---

## 1. Vector-First Architecture Audit

### HNSW as mandatory entry point

The context_search pipeline is unconditionally vector-first. Evidence from `search.rs:551–666`:

- Step 2: embed query via `get_adapter()` — always runs, no bypass
- Step 3: compute raw embedding via rayon pool — always runs
- Step 5: HNSW search (`vector_store.search()` or `search_filtered()`) — always runs, returns `Vec<IndexEntry>` which seeds all downstream phases
- Phase 0 (graph_expand): runs BFS from HNSW seed set; `seed_ids` comes from `results_with_scores`, which is empty without HNSW
- Phase 1 (PPR personalization vector): built from `results_with_scores` — same HNSW-seeded pool
- Phase 2 (PPR): `personalized_pagerank` called with HNSW-derived seed scores

There is no code path in context_search that does pure graph traversal without first running a vector query. The HNSW search is not optional, not gated by a flag, and cannot be bypassed by config.

The write path (`store_ops.rs:121`) is equally mandatory about embeddings:

```rust
// Step 2: Generate embedding if not pre-computed — always runs
let (embedding, adapted_for_prototypes) = match embedding {
    Some(e) => (e, None),
    None => { ... adapter.embed_entry(&title, &content) ... }
};
```

No `embed = false` flag exists. An entry with no embedding (`embedding_dim=0`) is silently skipped at Phase 0 expansion (`search.rs:935`): "On None: silently skip entries with no stored embedding."

### A pure-graph traversal use case does NOT require the vector layer

`build_typed_relation_graph` (`graph.rs:243`) builds the in-memory graph from entries slice + `GraphEdgeRow` slice — no vector dependency. `graph_expand` and `personalized_pagerank` are pure functions consuming only `&TypedRelationGraph`. `GRAPH_EDGES` table schema is self-contained. New MCP tools (context_neighbors, context_subgraph) would read from GRAPH_EDGES or the in-memory cache without touching the embed adapter or HNSW index.

---

## 2. Graph-First Feasibility Assessment

### Can graph traversal tools be added without touching the vector layer?

Yes. The architecture supports it cleanly. New graph traversal MCP tools can follow the same pattern as context_lookup: acquire store read lock, query SQLite or clone the in-memory graph, return results. No embedding adapter path needed.

### The "thin shell" scenario

A research domain that uses only graph traversal tools and never calls context_search is architecturally valid:

**What works fine:**
- context_get, context_lookup — no HNSW
- All new graph traversal tools — pure graph reads

**What degrades gracefully:**
- Confidence scoring with zero helpfulness votes: Bayesian prior `(3.0, 3.0)` returns 0.5 neutral. No crash, no invalid state (`confidence.rs:224`).
- PPR: never fires for research workflow if context_search is never called. No negative side effect.

**What imposes unnecessary overhead:**
- Embedding computation at write time: URL stub entries get meaningless embeddings. Low-quality vectors inserted into HNSW. Degrades HNSW search quality for SDLC use on the same instance.
- Background NLI tick: cosine scans over stub entries (URL content) may produce spurious Supports/Informs edges between semantically trivial stubs. Manageable by configuring `informs_category_pairs` to exclude stub categories.

---

## 3. Config-Only Expressibility Assessment

| Behavior | Config field | Status |
|---|---|---|
| Entry categories (all 8 research) | `[knowledge] categories` | Configurable today |
| Retrieval boost (goal, thesis) | `[knowledge] boosted_categories` | Configurable today |
| Adaptive lifecycle (thesis) | `[knowledge] adaptive_categories` | Configurable today |
| Confidence preset | `[profile] preset = "empirical"` | Configurable today |
| Informs pairs (finding→thesis, claim→thesis) | `[inference] informs_category_pairs` | Configurable today |
| S2 structural vocabulary | `[inference] s2_vocabulary` | Configurable today |
| Freshness half-life | `[knowledge] freshness_half_life_hours` | Configurable today |
| NLI enabled | `[inference] nli_enabled = true` | Configurable today |
| PPR expander | `[inference] ppr_expander_enabled = true` | Configurable today |
| Custom briefing instructions | `[server] instructions` | Configurable today |
| **Embedding mandatory at write time** | Not configurable — `store_ops.rs:121` | **Hardcoded** |
| **NLI contradiction scoped to Claims only** | No `contradicts_category_pairs` field exists | **Hardcoded (requires code)** |
| **HNSW mandatory in context_search** | No skip-HNSW flag | **Hardcoded** |
| **PPR positive edge types** | Hardcoded in `graph_ppr.rs:100–121` (4 types) | **Hardcoded (requires code)** |
| **graph_expand positive edge types** | Hardcoded in `graph_expand.rs:123–136` | **Hardcoded (requires code)** |
| **resolve_supersessions parameter** | No query parameter on any MCP tool | **Hardcoded (requires code)** |
| **context_cycle anchors to feature_cycle** | No `anchor_id` config | **Hardcoded** |
| **Graph traversal tools** | None exist | **Missing (requires new tools)** |

---

## 4. RelationType Extension Cascade Analysis

### Current: 6 types in graph.rs:81–90

String storage (`relation_type TEXT NOT NULL`) means no schema migration for new variants. However, `build_typed_relation_graph` (`graph.rs:300`) skips unrecognized strings with `warn!` — new variants must be added to the enum AND `from_str()` to avoid silent drop.

### 10 new variants needed

Advances, Cites, Asserts, Mentions, Refutes, Tests, DerivedFrom, Motivates, About, RelatedTo.

(Supports, Prerequisite, Contradicts, Supersedes already exist and map directly to supports/depends_on/contradicts/supersedes per Tracks A and B.)

### Cascade impact

**Files that MUST change for enum extension (~40 lines total):**
- `graph.rs:81–121`: add 10 variants + `as_str()` + `from_str()` match arms

**Files that benefit for free (zero changes):**
- `build_typed_relation_graph` Pass 2b: loads any recognized `RelationType` string from GRAPH_EDGES automatically
- GRAPH_EDGES UNIQUE constraint `(source_id, target_id, relation_type)`: applies to new types without change
- `write_graph_edge` (`nli_detection.rs:78–118`): accepts `relation_type: &str` — new type strings work immediately

**Files that require explicit decisions:**
- `graph_ppr.rs:100–121`: hardcoded 4-type loop (Supports, CoAccess, Prerequisite, Informs). New types do NOT automatically participate in PPR. For thin-shell scenario: irrelevant. For full PPR integration: add `Advances`, `Motivates` explicitly (~8 lines each).
- `graph_expand.rs:123–136`: same issue, same decision.

**Symmetric `contradicts` edges:**

GRAPH_EDGES UNIQUE constraint is asymmetric — `(A, B, Contradicts)` and `(B, A, Contradicts)` are distinct slots. Resolution: bidirectional insert at write time (Track A Option a), exactly as CoAccess edges are handled (migration.rs v19→v20 precedent). One extra GRAPH_EDGES row per contradicts edge. Simplifies all traversal queries.

---

## 5. Fundamental Tension Inventory

| Tension | Severity | Notes |
|---|---|---|
| Every entry requires an embedding (stubs get noise vectors) | Manageable | System doesn't break; HNSW quality degrades for stub-heavy corpora. Mitigated by putting meaningful text in `content`. Long-term fix: configurable `no_embed` flag. |
| HNSW overhead for graph-only workloads | Non-issue | At 3,000 entries HNSW overhead is negligible. No architectural impact. |
| One graph, one domain | Non-issue | Per-project SQLite isolation already built in; research and SDLC use different repo hashes. |
| Zero helpfulness votes → confidence 0.5 | Non-issue | Bayesian prior `(3,3)` returns 0.5 neutral. Graceful degradation, no crash, no invalid state. |
| PPR positive edge set is hardcoded | Manageable for thin shell; code for full PPR | Not used in thin-shell scenario. ~8-line change per new positive type. |
| NLI scans all entries, not Claims only | Manageable with code | Config workaround: set `informs_category_pairs` to include only Claim-bearing categories. Proper fix: `contradicts_category_pairs` config field (~20-line addition). |
| context_cycle anchors to feature_cycle not Goal ID | Architectural tension for intelligence features | Irrelevant for thin-shell graph storage use. Requires cycle anchor redesign for Goal-conditioned proactive delivery. |

---

## 6. Architecture Fitness Verdict

### ALIGNED WITH EXTENSIONS

The research domain can be served with:
- 10 new RelationType variants (trivial enum extension, ~40 lines, 1 day)
- 6–8 new MCP tools (context_neighbors, context_subgraph, context_inverse, context_supersession_chain, context_current, context_path, context_batch_write, context_filter)
- Zero storage schema changes (GRAPH_EDGES metadata JSON column is sufficient for all edge properties)
- Intelligence pipeline adapts with config (categories, boosted_categories, informs_category_pairs, nli_enabled=true, preset="empirical")

**Rationale:**

1. **The vector-first vs. graph-first inversion is NOT an architectural conflict.** HNSW-first search and graph-only traversal tools are independent code paths that share storage but not the retrieval pipeline. New graph traversal MCP tools read from GRAPH_EDGES / in-memory TypedRelationGraph without touching the vector layer. The thin-shell scenario (research domain uses only graph tools, ignores context_search) is architecturally valid and fully supported.

2. **The graph infrastructure already handles the core requirements.** GRAPH_EDGES schema is string-typed for relation_type — no migration for new edge types. In-memory TypedRelationGraph cache is available for synchronous BFS. `write_graph_edge` already provides the write path. The infrastructure is more capable than the research requirements assumed.

3. **Confidence scoring degrades gracefully with zero votes.** Bayesian prior returns 0.5 neutral. No crash, no invalid state. Research entries are fully usable at retrieval time regardless of vote history.

4. **Category taxonomy is fully config-driven.** All 8–9 research categories expressible in `config.toml` without code. `boosted_categories`, `adaptive_categories`, and `informs_category_pairs` are all config-driven today.

5. **The critical gap is tools, not architecture.** The research domain's 12 traversal patterns (Q1–Q12) cannot be served by context_search or context_lookup today. They require 6–8 new MCP tools. These are pure graph operations backed by existing storage — extensions of the MCP surface, not restructuring of the engine.

**Explicit address of vector-first vs. graph-first:** The concern is legitimate as a product positioning question (does adding 8 graph-only tools "bend Unimatrix toward graph-database semantics?") but not an architectural one. The two retrieval paradigms are orthogonal in the current codebase. The risk is product scope creep, which can be managed by framing graph traversal tools as a coherent "deep traversal" feature set with a single delivery item.

---

## 7. Effort Categorization Table

| Gap | Category | Estimate | Notes |
|---|---|---|---|
| Research categories + boosted_categories | (a) Config only | 0 days | `[knowledge]` section |
| informs_category_pairs for research | (a) Config only | 0 days | finding→thesis, claim→thesis |
| NLI enabled, empirical preset | (a) Config + ops | 0.5 days | Model download + config |
| Add 10 RelationType variants | (b) Code, existing storage | 1 day | ~40 lines, graph.rs |
| context_neighbors | (b) New MCP tool | 3–4 days | BFS 1+ hops; resolve_supersessions +1 day |
| context_subgraph | (b) New MCP tool | 5–7 days | Multi-hop BFS + edge collection + max_nodes cap |
| context_inverse | (b) New MCP tool | 2–3 days | SQL LEFT JOIN antijoin; existing idx sufficient |
| context_supersession_chain + context_current | (b) New MCP tools | 3 days | CTE or iterative walk; `find_terminal_active` already exists |
| context_path | (b) New MCP tool | 3–4 days | BFS shortest path on in-memory graph; petgraph algo available |
| context_filter | (b) New MCP tool | 3–4 days | SQL subqueries for property + edge count filters |
| context_batch_write | (b+) New MCP tool + synchronous write path | 7–10 days | Most complex; integrity chain + new capability; HNSW atomicity blocker |
| Bidirectional contradicts insert | (b) Code, existing storage | 0.5 days | CoAccess precedent; 10 lines |
| Add Advances/Motivates to PPR positive set | (b) Code, graph_ppr.rs | 1 day | ~8 lines per type |
| NLI contradicts_category_pairs scoping | (b) Code, config + filter | 1–2 days | 20-line config + filter in nli_detection_tick |
| Optional no_embed flag at write time | (b) Code, store_ops | 2–3 days | Reduces HNSW noise from stub entries; optional |
| **MVI total (5 tools + enum + config)** | | **20–25 days** | Without context_batch_write, NLI scoping, PPR extension |
| **Full implementation** | | **45–55 days** | All tools, intelligence integration, bidirectional contradicts |

**Category (c) — storage schema change required:** None. GRAPH_EDGES metadata JSON column is sufficient for all edge properties.

**Category (d) — core architecture change required:** None. All gaps are MCP tool layer or minor engine extensions.

---

## 8. Roadmap Positioning Recommendation

### Wave placement: Wave 3

Wave 2 is fully committed (W2-0 through W2-5). Research domain support is Wave 3 scope.

**Gate conditions:**
1. Wave 2 shipped
2. ASS-055 `context_relate` / `depends_on` write path shipped (Wave 2 — already planned)
3. External repository confirms requirements from this spike's findings
4. W1-1 crt-021 typed graph stable (already complete)

**NLI status:** NLI cross-encoder is COMPLETE (Wave 1). No additional NLI development required — only `nli_enabled = true` in config plus optional `contradicts_category_pairs` scoping (1–2 days).

**ASS-055 dependency:** The `Prerequisite` / `DependsOn` write path (ASS-055) is the reference implementation for research domain's `depends_on` edges and the `write_graph_edge` pattern all new edge types will use. Its Wave 2 delivery is a prerequisite for MVI.

### Minimum Viable Research Domain (MVI)

5 tools + 10 enum variants + config:

1. Config changes (0 days)
2. 10 RelationType variants (1 day)
3. context_neighbors (3–4 days): serves Q1 (Goal evidence), Q7 (POC test linkage), Q8 (Insight lineage)
4. context_subgraph (5–7 days): serves Q2 (Goal full subgraph), Q4 (Entity-centric retrieval)
5. context_supersession_chain + context_current (3 days): serves Q6 (supersession chain)
6. context_inverse (2–3 days): serves Q9 (Sources with no incoming cites)
7. Bidirectional contradicts insert (0.5 days): enables Q5 (contradiction surface)

**MVI total: ~20–25 days.** Covers 6 of 12 traversal patterns. Defers context_batch_write (7–10 days) and context_filter/context_path (6–8 days) to Phase 2 within Wave 3.

### What the research domain gets today without any changes

- context_store for all 9 categories (fully configurable)
- context_get and context_lookup for direct retrieval and category-filtered browsing
- Semantic discovery via context_search (useful even if graph traversal is primary)
- S2 vocabulary Informs edges (immediate, zero code, just config)
- Goal-conditioned briefing via context_cycle + crt-046 (in production, zero code)

---

## 9. Research-Domain.toml Sketch

```toml
# research-domain.toml — Unimatrix configuration for autonomous research workflow
# ASS-057 Track D design artifact
#
# Legend:
#   [CONFIGURABLE TODAY]   — works with current codebase
#   [NEEDS CODE]           — requires code change before effective
#   [NOT YET CONFIGURABLE] — hardcoded; no config mechanism exists

# =============================================================================
# [profile]
# =============================================================================

[profile]
# "empirical" preset: freshness and helpfulness signals elevated.
# Research corpora are time-sensitive; freshness-heavy scoring appropriate.
# [CONFIGURABLE TODAY]
preset = "empirical"

# =============================================================================
# [knowledge]
# =============================================================================

[knowledge]
# All 8 required research categories (+ optional Insight).
# Remove INITIAL_CATEGORIES (lesson-learned etc.) for pure-research instance.
# For mixed SDLC + research: append to default list — NLI cross-domain noise warning.
# [CONFIGURABLE TODAY]
categories = [
  "goal", "source", "finding", "claim",
  "entity", "thesis", "poc", "deliverable", "insight",
]

# Goal and Thesis are primary retrieval targets. Boost in context_search re-ranking.
# [CONFIGURABLE TODAY]
boosted_categories = ["goal", "thesis"]

# Thesis has a lifecycle; confidence decay adjustment enabled.
# Note: this is a weak proxy for lifecycle management, not a full state machine.
# [CONFIGURABLE TODAY]
adaptive_categories = ["thesis"]

# Research corpora evolve quickly. One-week half-life for freshness scoring.
# [CONFIGURABLE TODAY]
freshness_half_life_hours = 168.0

# =============================================================================
# [server]
# =============================================================================

[server]
# Research-domain briefing prompt override.
# [CONFIGURABLE TODAY]
instructions = """
You are the knowledge engine for an autonomous research workflow.
Entity taxonomy: Goal (anchor), Source (external item), Finding (our interpretation),
Claim (atomic proposition), Entity (named thing), Thesis (our proposition),
POC (experiment stub), Deliverable (output stub), Insight (synthesis).

Prefer context_neighbors / context_subgraph for structured graph queries.
Use context_search for serendipitous discovery.
"""

# =============================================================================
# [inference]
# =============================================================================

[inference]
# Enable NLI contradiction detection for Claim-level text.
# This is the use case NLI was built for.
# [CONFIGURABLE TODAY — requires model download ~85MB]
nli_enabled                 = true
nli_model_name              = "minilm2-q8"
nli_contradiction_threshold = 0.70
nli_entailment_threshold    = 0.65

# Informs edge detection for research-domain category pairs.
# Finding → Thesis: empirical → normative bridge
# Claim → Thesis: direct support bridge
# [CONFIGURABLE TODAY]
informs_category_pairs = [
  ["finding", "thesis"],
  ["claim",   "thesis"],
  ["finding", "claim" ],
]

# S2 structural vocabulary: terms triggering implicit Informs edges.
# [CONFIGURABLE TODAY]
s2_vocabulary = [
  "hypothesis", "evidence", "methodology", "replication",
  "benchmark", "baseline", "ablation", "dataset",
  "experiment", "evaluation", "citation",
]

# PPR expansion in context_search for serendipitous discovery.
# [CONFIGURABLE TODAY]
ppr_expander_enabled     = true
expansion_depth          = 2
max_expansion_candidates = 150

# =============================================================================
# [graph] — NOT A CURRENT CONFIG SECTION; design-artifact only
# =============================================================================

# [NEEDS CODE] — No [graph] section exists in Unimatrix config today.
# The following illustrates what would be needed:

# Edge type registration: 10 new RelationType variants
# [NEEDS CODE: add to RelationType enum in graph.rs; ~40 lines]
# edge_types = [
#   "Advances", "Cites", "Asserts", "Mentions", "Refutes",
#   "Tests", "DerivedFrom", "Motivates", "About", "RelatedTo",
# ]

# PPR positive edges for research domain
# [NEEDS CODE: modify graph_ppr.rs:100-121 and positive_out_degree_weight]
# ppr_positive_edge_types = [
#   "Supports", "CoAccess", "Prerequisite", "Informs",  # existing
#   "Advances", "Motivates",                             # new for research domain
# ]

# Symmetric edges: store both directions at write time
# [NEEDS CODE: bidirectional insert in context_store handler; CoAccess precedent]
# symmetric_edge_types = ["Contradicts"]

# NLI contradiction detection scoped to Claim-to-Claim pairs only
# [NEEDS CODE: add contradicts_category_pairs to InferenceConfig; ~20 lines]
# contradicts_category_pairs = [["claim", "claim"]]

# Optional: suppress embedding for stub entries (URL, git-path)
# [NEEDS CODE: no_embed flag on context_store; ~2-3 days]
# no_embed_categories = ["source", "poc", "deliverable"]

# =============================================================================
# [cycle] — NOT A CURRENT CONFIG SECTION; design-artifact only
# =============================================================================

# context_cycle is hardcoded to feature_cycle (SDLC concept).
# Research domain analogue: Goal entry ID as cycle anchor.
# Workaround: treat Goal ID as a pseudo-feature-cycle string ("goal-{id}").
# Proper solution requires extending context_cycle with anchor_id parameter.
# [NOT YET CONFIGURABLE]
# cycle_anchor_category = "goal"
```

---

## Unanswered Questions

**as-of timestamp support (Phase 3+):** Full as-of support requires adding `deprecated_at` column to entries (missing today — deprecation is a status field change, not a timestamp event) and equivalent for GRAPH_EDGES. Estimated 5–7 days schema migration. Correctly deferred per requirements document.

**Mixed SDLC + research instance isolation:** If SDLC and research use the same Unimatrix instance (same repo hash), NLI scans SDLC entries for contradictions with research entries. Cross-domain noise is mitigated by configuring `informs_category_pairs` to exclude SDLC categories, but not eliminated.

---

## Out-of-Scope Discoveries

1. **Evidence-weighted Thesis confidence**: `supports` and `refutes` edge counts could drive Thesis confidence directly (evidence-weighted scoring), replacing manual lifecycle management. The confidence pipeline in `confidence.rs` is extensible with a new component. High-value Wave 3 feature — Thesis status becomes confidence-gated rather than agent-managed. See Track C for full analysis.

2. **Goal cluster as context_briefing anchor**: The existing `goal_cluster` infrastructure (`goal_clusters.rs`) can seed PPR from the active Goal's embedding. Extension point: `context_briefing` Level 2 guard. Requires redesigning the cycle anchor concept.

3. **Antijoin index sufficiency confirmed**: context_inverse (Q9) antijoin query plan: `SELECT e.id FROM entries e LEFT JOIN graph_edges g ON e.id = g.target_id AND g.relation_type = 'Cites' WHERE e.category = 'source' AND g.target_id IS NULL LIMIT ?`. The existing `idx_graph_edges_target_id` covers the JOIN condition. At 3,000 entries and 10,000 edges, this is a fast bounded-scan. No additional index needed for MVI.

4. **S1/S8 automatic edge inference for research**: Claims mentioning the same Entity (via `mentions` edges converging on the same Entity entry) produce a structural co-mention signal analogous to S1 shared tags. Worth a dedicated follow-up spike to design a "co-citation" automatic edge inference source.
