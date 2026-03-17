# ASS-022/04: Minimum Viable Expansions — High Value, Integrity Preserved

**Date**: 2026-03-16
**Type**: Strategic design / minimum viable additions
**Question**: What is the smallest set of additions that makes Unimatrix dramatically more valuable
              without compromising the integrity chain?

---

## The Answer Upfront

Four additions. In order of value-to-effort:

1. **Typed relationship graph** — upgrade edges from `()` to `RelationEdge`, persist to SQLite
2. **GNN confidence learning** — learn weight vector from usage signals, solve the hardcoding problem
3. **Embedded NLI model** — real contradiction detection, replaces cosine heuristic
4. **Knowledge synthesis** — distill N related entries into one, provenance-chained

Each builds on the previous. Each preserves the integrity chain. None requires changing the `EntryRecord` schema. Each runs as an ONNX model — no new runtime dependencies.

---

## 1. Typed Relationship Graph

### What Currently Exists

```rust
StableGraph<u64, ()>   // nodes = entry IDs, edges = unweighted supersession
```

The graph is rebuilt at every search query from an in-memory snapshot. Edges have no metadata. Only one relationship type exists (A supersedes B). The graph is never persisted.

### The Minimum Expansion

Upgrade the edge type from `()` to a lightweight `RelationEdge`:

```rust
pub struct RelationEdge {
    pub relation_type: RelationType,
    pub weight: f32,        // learned by GNN; initialized to 1.0
    pub created_at: u64,
    pub created_by: String, // attribution — integrity chain preserved
    pub source: RelationSource,
}

pub enum RelationType {
    Supersedes,     // existing: A was replaced by B
    Contradicts,    // formalized: A conflicts with B (currently only detected, not stored)
    Supports,       // new: A provides evidence for B
    CoAccess,       // promoted: inferred from CO_ACCESS table
    Prerequisite,   // new: A should be understood before B
}

pub enum RelationSource {
    Declared,   // explicitly stored by an agent or human
    Inferred,   // derived from CO_ACCESS or NLI model
    System,     // created by maintenance pipeline
}
```

Add a `GRAPH_EDGES` table to SQLite:

```sql
CREATE TABLE graph_edges (
    from_id   INTEGER NOT NULL REFERENCES entries(id),
    to_id     INTEGER NOT NULL REFERENCES entries(id),
    rel_type  INTEGER NOT NULL,   -- RelationType enum
    weight    REAL    NOT NULL DEFAULT 1.0,
    source    INTEGER NOT NULL,   -- RelationSource enum
    created_at INTEGER NOT NULL,
    created_by TEXT   NOT NULL,
    PRIMARY KEY (from_id, to_id, rel_type)
);
```

**On startup**: populate from existing data automatically:
- `Supersedes` edges from `entries.supersedes` column (already tracked)
- `Contradicts` edges from `shadow_evaluations` (currently never formalized as graph edges)
- `CoAccess` edges from `co_access` table (promote high-count pairs above a threshold)

**What this unlocks immediately**:

| Capability | Before | After |
|-----------|--------|-------|
| Contradiction topology | Detected but not traversable | Graph edges enable cluster detection |
| Co-access as first-class relationships | Additive score at query time | Traversable graph structure |
| Support chains | Not modeled | Evidence trails for decisions/findings |
| Prerequisite ordering | Not modeled | Briefing can order knowledge by dependency |
| GNN (step 2) | Impossible (no edge features) | Enabled — typed edges are node features |
| DOT/GraphViz export | Impossible | Trivial — petgraph supports it |

### Integrity Chain Preservation

Every edge write goes through `context_store` (or an edge-specific tool with equal audit coverage). The `created_by` field on `RelationEdge` ensures every relationship has attribution. The `AUDIT_LOG` records edge creation events with actor and timestamp. Edge creation follows the same write path as entry creation — no bypassing the audit trail.

The `Supersedes` edge is derived from the existing `entries.supersedes` column, which is already part of the hash chain integrity model. The graph is a *view* of the integrity chain, not a separate data store.

---

## 2. GNN Confidence Learning

### The Hardcoding Problem

The current confidence formula:

```
confidence = 0.16*base + 0.16*usage + 0.18*freshness + 0.12*helpfulness
           + 0.14*correction + 0.16*trust
```

These weights were manually calibrated for agentic software development. They cannot adapt to:
- Domains where freshness is near-irrelevant (legal statutes, foundational science)
- Domains where trust is everything (regulatory data, clinical guidance)
- Domains where correction quality is the primary signal (sensor calibration records)
- Knowledge bases where human voting never happens (automated pipelines)

The freshness half-life (168h) is similarly hardcoded. An air quality deployment needs 1-4h for readings and 2 years for regulations. A legal deployment needs decades.

### The Minimum GNN Design

**The key insight**: do not replace the formula. Learn the weight vector that parameterizes it.

The formula structure is valuable — it's interpretable, auditable, and sum-constrained. What the GNN learns is: for *this* knowledge base, with *this* usage pattern, what weight distribution over these six factors best predicts "this entry will be marked helpful"?

```
GNN input (per node):
  - 6 raw factor scores (base_score, usage_score, freshness_score, etc.)
  - category encoding (learned embedding, ~8 dims)
  - trust_level (ordinal: 0.0 to 1.0)
  - graph structural: in_degree, out_degree, chain_depth, contradiction_neighbor_count
  - co_access_count, co_access_recency

GNN message passing (2 layers, Graph Attention):
  - Layer 1: 32-dim hidden, typed edge attention weights
  - Layer 2: 16-dim hidden

GNN output:
  - learned_weights: [w_base, w_usage, w_fresh, w_help, w_corr, w_trust]  (sum = 0.92)
  - learned_half_life_hours: f64  (the freshness decay parameter)
  - per_entry_quality_score: f64  (direct quality estimate, independent of formula)
```

The learned weights are **deployment-level** — one weight vector for the whole knowledge base, updated periodically by the maintenance tick, not per-query. This means:
- The formula remains deterministic at query time (no stochastic inference on the hot path)
- The learning happens in the background, asynchronously
- The current cold-start weights `[0.16, 0.16, 0.18, 0.12, 0.14, 0.16]` become the GNN's initialization

**Training signal**: the GNN is trained on `(entry_features, helpful_count / total_votes)` pairs. Entries with no votes are excluded from training (Bayesian priors handle cold-start). The supervision signal already exists in the database — no new data collection needed.

**Model size**: a 2-layer GAT with 32-dim hidden state on a 384-dim input is ~100K parameters. As an ONNX model, ~400KB. Inference time: <1ms. Training time: seconds on the maintenance tick, only when sufficient new votes accumulate.

### What This Solves

1. **The hardcoding problem** — weights adapt to each deployment's domain and usage patterns
2. **The freshness half-life problem** — becomes a learned parameter, not a constant
3. **Domain agnosticism** — different deployments naturally converge to different weight distributions
4. **The "lesson-learned" hardcode** — the GNN learns to boost whichever categories actual users find valuable, without category names being baked in
5. **Cold start in new domains** — initialize from dev-domain weights, converge to domain-specific weights as users provide feedback

### Integrity Chain Preservation

The learned weight vector is stored in a new `model_state` row (or small table) with:
- `updated_at` timestamp
- `trained_on_n_votes` count
- `previous_weights` (the weights before this training run)
- `created_by: "system"` attribution

Every time the weights update, the change is logged in `AUDIT_LOG` with a diff of the previous and new values. The GNN model itself is an ONNX file with a pinned hash — immutable, versioned. Model updates are treated like schema migrations: explicit, logged, reversible to previous version.

**Critically**: the GNN does not modify `EntryRecord` confidence values directly. It updates the weight vector that the *confidence recomputation* uses. The next maintenance tick then recomputes confidence for all entries using the new weights. The recomputation is logged. The `content_hash` chain on individual entries is never touched by the GNN.

---

## 3. Embedded NLI Model

### The Current Problem

Contradiction detection uses cosine similarity on embeddings:

```
if similarity > 0.92 and entries_have_conflicting_signals:
    flag_contradiction()
```

This is a heuristic. Two entries saying opposite things about the same topic might have 0.70 similarity (similar topic, opposite content) and never get flagged. Two entries that look textually similar but say compatible things at different confidence levels might get false-flagged.

### The Minimum Addition

One ONNX model: a fine-tuned Natural Language Inference (NLI) classifier.

```
Input: (premise: &str, hypothesis: &str)
Output: {entailment: f32, neutral: f32, contradiction: f32}
```

Models available: `cross-encoder/nli-deberta-v3-small` (~180MB ONNX), or `facebook/bart-large-mnli` (larger, more accurate). For Unimatrix's use case the small DeBERTa model is sufficient — pairs are short knowledge entries, not documents.

**Usage in Unimatrix**:

1. **Contradiction detection** (replaces shadow_evaluations heuristic): When a new entry is stored, NLI-check it against the top-K nearest neighbors by embedding similarity. If `contradiction > 0.80`, create a `Contradicts` edge in the graph (step 1) and flag both entries. This is a major accuracy improvement over the current cosine heuristic.

2. **Supports relationship detection**: When `entailment > 0.85`, create a `Supports` edge automatically. This is the "known relationships" formalization the user is asking about.

3. **GNN edge quality signal** (step 2): NLI scores become edge features in the GNN. A `Contradicts` edge with 0.95 contradiction probability has different semantic weight than one with 0.82 probability.

4. **Relevance re-ranking** (optional): After HNSW retrieval, NLI can assess whether each candidate actually entails/supports the query, providing a re-ranking signal more precise than cosine similarity. This replaces the current `0.85*similarity + 0.15*confidence` blend with something semantically grounded.

**Run context**: async, on `context_store` (post-commit), in a `spawn_blocking` task. Not on the search hot path.

### Integrity Chain Preservation

NLI model outputs result in `Contradicts`/`Supports` edges with `source: RelationSource::Inferred` and `created_by: "system"`. Every inferred relationship is:
- Flagged as system-inferred (not human-declared)
- Audited in `AUDIT_LOG`
- Reversible (edges can be deleted by a privileged agent)
- Lower-trust than declared relationships (the `weight` field on the edge reflects NLI confidence)

Human agents can override: a `context_correct` call with explicit relationship declaration creates a `source: RelationSource::Declared` edge that supersedes the inferred one. The correction is part of the correction chain.

---

## 4. Knowledge Synthesis

### The Problem

When a knowledge base matures, related entries accumulate on a topic. A query for "PM2.5 calibration" might return 7 entries spanning 3 years of calibration notes, corrections, and findings — each individually correct, but collectively verbose. The agent has to synthesize them mentally.

### The Minimum Design

A small summarization/synthesis model (ONNX, ~300MB) that runs during the maintenance tick, not at query time.

**Trigger condition**: 3+ Active entries with:
- Same `topic` and `category`
- Mutual `Supports` or `CoAccess` edges
- Combined content > 800 tokens
- No existing synthesis entry for this cluster

**Synthesis process**:
1. Fetch the N entries in the cluster
2. Run the summarization model: condense into a single synthesis entry
3. Store the synthesis with:
   - `category` = same as source entries
   - `topic` = same as source entries
   - `trust_source = "neural"`
   - `confidence` = weighted average of source confidences (GNN-weighted)
   - `supersedes` = the lowest-confidence source entry (the synthesis takes its place)
   - Source entry IDs stored in `tags` as `synthesized-from:{id}` markers

**What agents get**: instead of 7 calibration entries, one synthesis entry that says "As of 2026-03, the consensus on Sensor 7 PM2.5 calibration is: [condensed]. This synthesizes findings from March 2025, August 2025, and January 2026. Source entries: [ids]."

### Integrity Chain Preservation

The synthesis entry:
- Has a `correction_count` reflecting the number of source corrections already in the chain
- Has `previous_hash` linking to the most recently created source entry
- Does not delete source entries — they are deprecated in-place with `reason: "synthesized into entry {id}"`
- Full traversal of the synthesis's supersession chain reaches all original sources
- A human can always restore a deprecated source entry via `context_quarantine(action: restore)`

The synthesis is audited. The ONNX model is pinned to a specific hash. The synthesis itself is a correction-chain-compatible entry that can be corrected or deprecated like any other.

---

## How They Interact

```
                    ┌─────────────────────────────────────┐
                    │         EntryRecord + Hash Chain     │  ← unchanged
                    │  (content_hash, previous_hash,       │
                    │   correction chains, audit log)      │
                    └──────────────┬──────────────────────┘
                                   │ foundation
                    ┌──────────────▼──────────────────────┐
                    │     Typed Relationship Graph         │  ← step 1
                    │  Supersedes | Contradicts | Supports  │
                    │  CoAccess | Prerequisite              │
                    │  + RelationEdge attribution           │
                    └───┬──────────────────┬──────────────┘
                        │ graph features   │ typed edges
          ┌─────────────▼───┐        ┌─────▼──────────────┐
          │  GNN Confidence  │        │    NLI Model        │  ← step 2 + 3
          │  Learning        │        │  Contradiction +    │
          │                  │        │  Support Detection  │
          │  learns:         │        │                     │
          │  - weight vector │◄───────│  NLI scores become  │
          │  - half-life     │        │  edge features for  │
          │  - per-entry     │        │  GNN attention      │
          │    quality       │        └────────────────────┘
          └──────────┬──────┘
                     │ learned weights
          ┌──────────▼──────────────────────────────────────┐
          │           Knowledge Synthesis                    │  ← step 4
          │  Triggered when cluster density > threshold      │
          │  Output: provenance-chained summary entry        │
          │  Input quality: GNN-weighted source confidences  │
          └─────────────────────────────────────────────────┘
```

The graph (step 1) provides structure for the GNN (step 2) to learn from. The NLI model (step 3) provides semantic edge quality signals that improve the GNN's attention weights. The synthesis (step 4) uses GNN-computed quality scores to weight source entries during summarization.

Each step adds value independently. Steps 2-4 each require step 1. Steps 3 and 4 do not require step 2 (but are better with it).

---

## What NOT to Add

Preserving value means knowing what not to build.

**Full LLM inference inside Unimatrix** — that's RuVector's territory. Unimatrix's inference should be narrow: knowledge *quality assessment* (GNN, NLI, synthesis). Generation belongs in the calling LLM. The moment Unimatrix tries to run 7B parameter models, it loses its zero-infrastructure advantage.

**Graph database features** (Cypher, hyperedges) — petgraph with typed edges is sufficient for everything Unimatrix needs. Full graph DB capabilities would require replacing SQLite with a graph-native store, destroying the embedded single-binary model. The typed relationship graph covers the use cases without this cost.

**Real-time GNN inference on search hot path** — the GNN runs on the maintenance tick, not per-query. Learned weights are cached; confidence scores are precomputed. Hot path remains deterministic and fast.

**Training from scratch** — all three models (GNN, NLI, synthesis) are fine-tuned from existing public models, not trained from scratch. Unimatrix ships with pre-trained ONNX weights that adapt to each deployment via transfer learning (the GNN) or are used inference-only (NLI, synthesis). No GPU required.

**Cross-entry mutation via GNN** — the GNN does not modify EntryRecord fields directly. It updates the weight vector; the maintenance tick recomputes confidence. The GNN never touches `content_hash` or `previous_hash`. These are inviolable.

---

## Scalability: The Minimum Architecture

The user has deployment ideas; this is the minimum to validate them against.

### The Core Constraint

The integrity chain is per-instance. A `content_hash` on entry 1234 is only meaningful within the store that contains entry 1234. Cross-instance integrity requires a root-of-trust — either a shared hash registry or federated hash exchange.

### Three Viable Approaches (in order of complexity)

**Tier 1: Read Replicas (zero code changes)**
- SQLite WAL mode (already enabled) supports concurrent readers
- Deploy multiple MCP server instances pointing to the same SQLite file (on shared NFS/EFS)
- All reads distributed; all writes go to single writer
- Integrity: unchanged (single authoritative chain)
- Suitable for: multi-agent concurrent access, read-heavy workloads

**Tier 2: Topic-Sharded Instances**
- Multiple Unimatrix instances, each responsible for a topic namespace
- A thin router (< 100 lines) inspects the `topic` param on MCP calls and routes to the appropriate shard
- Cross-shard search: fan-out query, merge results by score
- Integrity: each shard maintains its own chain; cross-shard references use `{shard_id}:{entry_id}` format
- Suitable for: large knowledge bases, domain-separated teams

**Tier 3: Federated Instances with Trust Exchange**
- Independent Unimatrix deployments (different organizations, different domains)
- Federated search: query propagates to trusted peers, results include source attribution
- Integrity: each instance signs its responses; the receiving instance stores federated entries with `trust_source = "federated:{peer_id}"` at reduced trust
- This is where the four-tier trust model becomes a cross-instance protocol
- Suitable for: multi-organization environmental monitoring networks, research consortia, distributed SRE teams

The graph, GNN, and NLI additions are all **shard-local** in tiers 1 and 2. In tier 3, the GNN would eventually need to learn from cross-instance signals — but that's a future problem.

---

## Implementation Order

| Step | What | Effort | Unlocks |
|------|------|--------|---------|
| 0 | Config externalization (vnc-004) | 1 day | Domain agnosticism, prerequisite for domain packs |
| 1a | `RelationEdge` type + `GRAPH_EDGES` table | 2 days | Graph persistence, DOT export, typed traversal |
| 1b | Promote CO_ACCESS + shadow_evaluations to graph edges | 1 day | Contradiction graph, co-access formalized |
| 2 | NLI model integration (ONNX, post-store inference) | 3 days | Real contradiction detection, Supports edges |
| 3 | GNN weight learner (maintenance tick, ONNX output) | 1 week | Adaptive weights, freshness learning, domain agnosticism |
| 4 | Knowledge synthesis (maintenance tick, ONNX) | 1 week | Cluster distillation, signal-to-noise improvement |
| 5 | Scalability tier selection + implementation | TBD | (user-led) |

Total for steps 0-4: ~3-4 weeks. Each step is independently valuable and independently shippable. The integrity chain is preserved throughout — no step modifies the hash chain model.

---

## The Structural Insight

Everything above is essentially one idea: **move from static, manually calibrated parameters to learned, domain-adaptive parameters, while keeping the interpretable formula structure and the integrity chain inviolable**.

The hash chain, correction chains, audit log, and trust attribution are not implementation details to work around — they are the product's defensible moat. Every addition above is designed to enhance the intelligence layer *on top of* that foundation, not through it.

The result: a knowledge engine that is genuinely domain-agnostic (learned weights, configurable categories, passthrough embeddings), semantically aware (NLI-based relationships), self-improving (GNN adaptation), and progressively more signal-rich (synthesis reduces noise as knowledge matures) — while remaining a single binary, zero-infrastructure, fully auditable system.

That combination does not exist anywhere else.
