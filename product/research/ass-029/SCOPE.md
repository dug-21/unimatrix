# ASS-029: GNN Architecture for Session-Conditioned Relevance

**Status**: Not started. Required before W3-1 delivery can be scoped.
**Date**: 2026-03-21
**Feeds**: W3-1 design and delivery

---

## Problem Statement

The current W3-1 scope describes a GNN that learns a confidence weight vector — six weights that replace the hardcoded constants in the existing scoring formula. That is a weight calibration problem.

What Wave 1A makes possible is more ambitious: a **session-conditioned relevance function** — a GNN that takes a knowledge graph node (entry) and the current session context as inputs, and outputs a relevance score for that entry in that moment. This function naturally expresses all three delivery surfaces (proactive injection, phase-transition briefing, reactive search re-ranking) as different query modes against the same learned model.

The architecture of this expanded W3-1 is not obvious. The three query modes have different forward pass structures. The training batch must account for three distinct signal sources. Candidate set management for the proactive mode (no query anchor) is a different problem than re-ranking a retrieved candidate set. Tick scheduling must compose with existing compaction, NLI inference, and confidence refresh work.

This spike produces the design that W3-1 delivery executes. It is not a proof-of-concept implementation — it is a written design with enough architectural specificity that the delivery team can estimate and implement without unresolved questions.

---

## Goals

1. Define the GNN forward pass architecture for each of three query modes.
2. Specify the session context feature vector — complete, typed, with sources and freshness.
3. Define the training batch construction from all three signal sources.
4. Resolve the candidate set management strategy for proactive mode.
5. Define tick scheduling and resource envelope relative to existing tick work.
6. Define the cold-start initialization from the Wave 1A manual formulas.
7. Identify what remains as open questions for W3-1 delivery to resolve.

---

## Non-Goals

- This spike does NOT implement any GNN code.
- This spike does NOT select a specific GNN library or crate.
- This spike does NOT design the GGUF integration (W2-4).
- This spike does NOT change the W1-3 eval harness scope.
- This spike does NOT redesign the observation pipeline (W1-5).
- This spike does NOT produce training data or run experiments.

---

## Background

### What Wave 1A Provides

By the time W3-1 begins, the following signals are available and persisted:

| Signal | Source | Freshness |
|--------|--------|-----------|
| `current_phase` | SessionState (WA-1) | Per-phase-transition |
| `category_counts` histogram | SessionState (WA-2) | Per-store call |
| `injection_history` | SessionState | Per-injection |
| `query_count`, `rework_event_count` | SessionState | Per-event |
| `FEATURE_ENTRIES.phase` | analytics.db (WA-1) | Per-store call |
| `MISSED_RETRIEVALS` table | analytics.db (WA-3) | Per-store call |
| `QUERY_LOG` | analytics.db | Per-search |
| `INJECTION_LOG` | analytics.db | Per-injection |
| `GRAPH_EDGES` (NLI-confirmed) | analytics.db | Per-NLI-inference |
| `helpful_count` / `unhelpful_count` | knowledge.db | Per-vote |
| `CO_ACCESS` counts | analytics.db | Per-search |
| W1-5 behavioral outcome signals | SIGNAL_QUEUE | Per-session |

### What the Three Delivery Surfaces Need

**Mode 1 — Proactive (UDS injection)**
No user query. Session context IS the retrieval anchor. Must score all entries not yet served in this session and return the top candidates. Called on every hook event — must be fast.

**Mode 2 — Comprehensive (context_briefing at phase transition)**
No user query. Session context at phase transition moment. Must score all entries for the current topic and return a comprehensive ranked set for the new phase. Called once per phase transition — latency tolerance higher than Mode 1.

**Mode 3 — Reactive (context_search re-ranking)**
Query embedding available. Session context fused with query similarity. Must re-rank a candidate set of ~20 entries from HNSW retrieval. Called on every search — moderate latency tolerance.

---

## Research Questions

This spike must answer each of the following before W3-1 delivery begins.

### Q1: Forward Pass Architecture

The three modes present different computational shapes:

- **Mode 3 (reactive)**: Classic cross-encoder pattern — `(entry, query)` pair scored by attention mechanism. Session context is an additional input that modifies the scoring. Candidate set is pre-selected by HNSW (k=20). This is the most tractable mode and closest to existing NLI re-ranking.

- **Mode 1 and 2 (proactive/comprehensive)**: No query embedding. The session context vector is the sole ranking anchor. This is a **recommendation** problem, not a retrieval problem. Options:
  - **Graph attention over all candidates**: GNN attends to the full candidate graph, with session context injected as a global context node. Expensive if the candidate set is large.
  - **Session context → query proxy embedding**: Transform session context vector into a pseudo-query embedding and fall back to HNSW retrieval followed by Mode 3 re-ranking. Simpler, reuses Mode 3 forward pass, but may lose session-specific signals.
  - **Two-headed architecture**: Shared GNN backbone with separate output heads for query-conditioned and session-conditioned scoring. Single model, two inference paths.

The spike must evaluate these options and select one, with justification. The selection affects the model size, training complexity, and inference cost.

**Question**: Does the proactive mode use a separate forward pass, or can session context be injected into the Mode 3 forward pass as a pseudo-query?

### Q2: Session Context Feature Vector

The session context vector must be specified completely — every dimension, its source, its type, and how it is normalized or encoded for the GNN.

Known dimensions from Wave 1A:
```
current_phase          → one-hot over known phases, or learned phase embedding?
category_histogram[k]  → normalized counts (sum=1) over k categories
injection_count        → scalar, normalized by session length?
query_count            → scalar
topic                  → entry embedding of the topic string, or a separate encoding?
cycle_position         → elapsed time / expected cycle duration (0.0–1.0)?
rework_event_count     → scalar, normalized?
```

Questions:
- How many dimensions total? GNNs are sensitive to feature dimensionality.
- How is `topic` encoded? If it is an embedding, it is the same dimensionality as an entry embedding (~384 dims). Does the session context vector embed the topic, or does the GNN use a separate topic node in the graph?
- How is `current_phase` encoded? One-hot assumes a fixed vocabulary. If phase strings are opaque (as designed in WA-1), the encoding must handle unknown phase strings gracefully.
- How does the GNN handle a session with an empty histogram (cold session, no stores yet)?

### Q3: Entry Feature Vector

The node feature vector for each entry candidate:
```
confidence_6_factors   → 6 floats (already computed)
category               → one-hot or embedding (same vocabulary problem as phase)
access_count           → log-normalized scalar
helpful_ratio          → Wilson score (already computed)
correction_count       → scalar
graph_degree           → in-degree + out-degree from GRAPH_EDGES
days_since_access      → scalar, clipped at some maximum
nli_edge_confidence    → max NLI confidence of edges involving this entry (0 if none)
```

Questions:
- Is NLI edge confidence a useful signal if most entries have no NLI-confirmed edges yet?
- Should graph degree be decomposed by edge type (Supports degree, Contradicts degree, CoAccess degree) rather than total degree?

### Q4: Training Batch Construction

The GNN trains on `(entry, session_context, label)` triples. The three signal sources produce these labels differently:

**Explicit helpfulness (sparse, high quality)**:
```
session_context_at_retrieval × entry → label: +1 (helpful) or -1 (unhelpful)
```
Problem: sparse. A session with zero votes contributes nothing.

**W1-5 behavioral outcomes (automatic, moderate quality)**:
```
session_context at session_close × entries_in_injection_history:
  session outcome = "success" (no rework, no re-search) → label: +1 for all served entries
  session outcome = "rework" → label: -1 for entries served before rework events
  entry served → agent re-searched same topic → label: -1 for that entry
```
Problem: session-level labels applied to all served entries are noisy — some served entries were useful even in a rework session.

**MissedRetrieval (automatic, high quality, targeted)**:
```
session_context_at_store_time × missed_entry_id → label: -1 (should have been served)
```
Problem: only generates negative labels. Positive labels must still come from other sources.

Questions:
- What is the sampling strategy for constructing batches? Random sampling over sessions, or curriculum learning (easier examples first)?
- How are positive and negative labels balanced? MissedRetrieval generates only negatives — does this create class imbalance?
- How is `session_context_at_retrieval_time` reconstructed for historical sessions? The session context vector is partially in-memory (SessionState) and partially in the DB (INJECTION_LOG, QUERY_LOG). Can it be faithfully reconstructed from DB records?
- What is the minimum viable training set size before the first training run?

### Q5: Candidate Set Management for Proactive Mode

For Mode 1 (proactive injection), the system must score entries without a query anchor. Two strategies:

**Strategy A: Phase-transition rebuild**
At each `current_phase` change, rebuild a candidate list:
```
candidates = Active entries where:
  topic = current feature_cycle
  AND category ∈ expected_categories(current_phase)
  AND entry_id NOT IN injection_history

scored by: GNN(entry_features, session_ctx_at_phase_transition, interaction_features)
stored in: in-memory phase-transition cache (Arc<RwLock<Vec<ScoredCandidate>>>)
```
On each hook event: pop top candidate from cache (not yet served). Cache refreshed on next phase transition or when injection_history grows significantly.

**Strategy B: On-demand GNN scoring at hook time**
On each hook event, run a forward pass over the candidate set. No cache — always fresh. More expensive, simpler state management.

**Strategy C: Pseudo-query proxy**
Transform session context into a pseudo-query embedding via a learned projection. Use HNSW retrieval with this pseudo-embedding as the anchor, then apply Mode 3 re-ranking. Reuses existing retrieval infrastructure, avoids full-graph scoring.

Questions:
- Is Strategy A's phase-transition cache stale enough to matter? If a phase lasts 30 minutes and injection_history grows by 5-10 entries, does re-using cached scores degrade quality meaningfully?
- Does Strategy C lose important session-specific signals that don't project cleanly into the entry embedding space?
- What is the maximum candidate set size before any strategy becomes impractical?

### Q6: Tick Scheduling and Resource Envelope

The maintenance tick currently runs:
1. Compaction (HNSW rebuild + graph compaction)
2. Confidence refresh (batch 100 entries)
3. Co-access cleanup (staleness)
4. NLI post-store inference (queued pairs)

W3-1 adds:
5. GNN training run

Questions:
- Can GNN training run on the same maintenance tick as compaction, or does it need a separate lower-frequency tick (e.g., daily rather than per-compaction)?
- What is the expected training run duration at typical knowledge base sizes (100 entries, 1000 entries, 10,000 entries)?
- Does GNN training need the full GRAPH_EDGES table, or only a subgraph? How does this interact with the in-memory graph cache rebuilt by compaction?
- What is the rayon pool thread budget for GNN training? It must not starve NLI inference (50-200ms, high frequency) during a training run (potentially seconds to minutes).
- Is incremental weight update (online learning on recent sessions) viable, or does batch retraining from the full historical set produce better results at these knowledge base sizes?

### Q7: Cold-Start and Fallback

Before the GNN has enough training data, the system must fall back gracefully to the Wave 1A manual formulas. The transition must be smooth:

```
if gnn_scores.is_none() || training_data_insufficient():
    use manual formula:
        score = 0.85·similarity + 0.15·confidence
              + co_access_boost
              + phase_category_boost * 0.015 (if current_phase set)
              + histogram_boost * 0.005 (fallback)
else:
    score = gnn_scores[entry_id] fused with similarity (Mode 3)
            or gnn_scores[entry_id] directly (Mode 1, 2)
```

Questions:
- What is "training data insufficient"? Minimum entry count? Minimum vote count? Minimum MissedRetrieval events?
- How does the in-memory GNN score cache get populated? Is it a full forward pass over all Active entries at the end of each training run, or computed lazily at query time?
- How does the GNN score cache interact with the effectiveness state cache (crt-018b, `EffectivenessStateHandle`)? Do they merge, or does the GNN score replace the effectiveness classification?

---

## Outputs Required

This spike must produce:

1. **`SCOPE.md`** (this file) — updated with answers to all research questions above
2. **`GNN-ARCHITECTURE.md`** — forward pass design for each query mode, with diagrams or pseudocode; model size estimate; library/crate recommendation
3. **`FEATURE-SPEC.md`** — complete typed specification of the session context feature vector and entry feature vector, with normalization strategy for each dimension
4. **`TRAINING-DESIGN.md`** — batch construction pseudocode, sampling strategy, label balancing, minimum training set thresholds, session context reconstruction from DB records
5. **`TICK-DESIGN.md`** — tick scheduling, resource envelope, rayon pool budget, interaction with existing tick work, incremental vs. batch training recommendation
6. **`OPEN-QUESTIONS.md`** — anything not resolved in this spike that W3-1 delivery must decide, with explicit decision gates

---

## Relationship to Planned Work

| Work item | Relationship |
|---|---|
| W1-5 (col-023) | Provides domain-neutral behavioral outcome signals; required for W3-1 training signal completeness |
| WA-1 (#330) | Provides `current_phase` and `FEATURE_ENTRIES.phase`; required for session context vector and training labels |
| WA-2 (ASS-028 R1) | Provides `category_histogram`; required for session context vector |
| WA-3 (MissedRetrieval) | Provides targeted negative training labels; required for feedback loop closure |
| WA-4 (Proactive delivery) | Implements Q5 candidate set management as manual formula; GNN replaces this |
| W3-1 | This spike is the design prerequisite |
| W3-2 (synthesis) | Depends on W3-1 learned weights; not in scope here |

---

## Open Questions (Pre-Spike)

These are known unknowns at the time of writing. The spike resolves them.

**OQ-01**: Does the proactive query mode require a separate model head or can session context be injected into the reactive forward pass as a pseudo-query?

**OQ-02**: Is a two-headed architecture (shared backbone, separate scoring heads) meaningfully better than two separate lightweight models?

**OQ-03**: How is `topic` encoded — as an entry embedding, as a separate lookup in the graph, or as a free-text embedding computed at query time?

**OQ-04**: Can session context be faithfully reconstructed for historical sessions from INJECTION_LOG + QUERY_LOG + FEATURE_ENTRIES records, or is the in-memory SessionState the only complete source?

**OQ-05**: What GNN depth and width is appropriate at Unimatrix's expected knowledge base sizes (hundreds to low thousands of entries)? A 2-layer graph attention network (~400KB) was proposed; is this too small, too large, or about right?

**OQ-06**: Should the GNN score replace or be fused with the NLI re-ranking signal in Mode 3? They are measuring related but distinct things (session-conditioned relevance vs. query-conditioned entailment).
