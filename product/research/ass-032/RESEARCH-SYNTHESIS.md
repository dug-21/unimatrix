# ASS-032: Research Synthesis — Self-Learning Knowledge Engine

**Date**: 2026-03-25
**Inputs**: PIPELINE-AUDIT.md + NOVEL-APPROACHES.md + RUST-ML-ECOSYSTEM.md
**Status**: Research complete. This document answers AC-09 through AC-13.

---

## The Architecture Problem, Precisely

Unimatrix is a **general-purpose, workflow-aware knowledge management engine**. Its schema (categories) and workflow topology (phases) are deployment-configurable. The software development configuration is one domain pack. All architectural proposals in this document are stated in terms of the configurable (categories, phases) abstraction — no hardcoded category names.

The problem with the current pipeline: the corpus has **extreme category access imbalance**. High-access categories (decision, pattern) have avg confidence ~0.53; low-access categories (lesson-learned, outcome) have avg confidence ~0.10. The cause is structural — the confidence composite includes `usage_score` which directly measures access count, creating a feedback loop where accessed categories get higher confidence → surface more often → get accessed more → confidence climbs further. Categories that are never surfaced have no path to confidence improvement.

The goal is a pipeline that **converges toward the distribution of semantically relevant entries, not the distribution of historically popular entries** — and that generalizes to any (categories, phases) configuration.

---

## Critical Finding: The Get Signal Gap

This was identified after the initial synthesis and is the highest-priority signal quality issue in the current architecture.

### The Signal Hierarchy

| Read-side event | access_count | helpful_count | phase captured | Semantic strength |
|---|---|---|---|---|
| Entry appears in briefing | +1 | — | **Never** | Weak (shown, may not be read) |
| Entry appears in search results | +1 | — | **Never** | Weak (shown, may not be read) |
| Agent calls `context_lookup` | +2 | — | **Never** | Medium (explicit by-ID intent) |
| Agent calls `context_get` | +1 | +1 (implicit) | **Never** | **Strong** (agent selected from search results, wanted full content) |
| Explicit `helpful=true` vote | — | +1 | Never | Very strong (explicit) |

**`context_get` is semantically the strongest implicit positive signal, yet it is treated identically to a search appearance for phase-learning purposes.**

### What the Code Actually Does

`tools.rs:677–689`:
```rust
self.services.usage.record_access(&[id], AccessSource::McpTool, UsageContext {
    helpful: params.helpful.or(Some(true)),  // implicit helpful=true — good
    access_weight: 1,                         // same weight as search — wrong
    current_phase: None,                      // ALWAYS None — wrong
    ..
});
```

`usage.rs:69` documents this as **deliberate policy**:
> "`None` for all non-store operations (search, lookup, get, correct, deprecate, etc.)"

Phase is only ever captured on `context_store` writes. Every read-side event is phase-context-free.

### Why This Matters

A `context_get` call means: the agent was shown an entry in search/briefing results, decided the title/snippet was interesting enough, and explicitly fetched the full content. This is a two-step selection act. It is the closest thing the system has to "the agent voted with their feet."

The helpful_count increment on `context_get` does feed into the Beta posterior (Thompson Sampling would use it). But we don't know *in which phase* the agent found it interesting. The phase-conditioned frequency table — "during delivery phase, these entries are consistently chosen" — cannot be built from gets because the phase dimension is always stripped.

### The Briefing Architecture Clarifies the Full Model

`context_briefing` is designed as a wide-net candidate presentation: returns up to 20 entries as a condensed index (snippets only). The agent scans the index and calls `context_get` on anything worth reading in full. This is a deliberate two-step **offer → selection** loop, and the same pattern applies to search results.

The correct signal model for ALL four read-side call sites:

| Event | Role | Signal type |
|---|---|---|
| Entry in briefing results | **Offer** (denominator) | "Available to agent in this phase" |
| Entry in search results | **Offer** (denominator) | "Candidate for agent's query" |
| `context_get` following briefing/search | **Selection** (numerator) | "Agent actively chose this from what was offered" |
| `context_get` without prior context | Pure selection | "Agent already knew they wanted this" |

The **selection rate** per `(entry, phase)` — how often an entry is chosen when offered — is the implicit relevance signal we are not collecting. It requires phase context on all four call sites.

**Secondary finding: briefing `access_weight: 1` is overcounting.** Every briefing call currently gives `access_count += 1` to all 20 returned entries, inflating `usage_score` for entries the agent may never have noticed in the index table. Briefing appearances should be `access_weight: 0` — tracked in `injection_history` for the offer log but not credited toward confidence. The `access_count` increment belongs at selection time (get), not offer time (briefing).

### The Fix (Four Call Sites)

All four `record_access` calls in `tools.rs` that set `current_phase: None` need the phase snapshot:

| Tool | Line | Additional change |
|---|---|---|
| `context_search` | 367 | Phase capture only |
| `context_lookup` | 483 | Phase capture; `confirmed_entries` when single result |
| `context_get` | 687 | Phase capture; `access_weight` 1→2; always add to `confirmed_entries` |
| `context_briefing` | 1012 | Phase capture; `access_weight` 1→0 (offer log only) |

**`SessionState` new field**: `confirmed_entries: HashSet<u64>` — "what was chosen" as distinct from `injection_history` "what was shown."

**Helper method**: `current_phase_for_session(session_id: Option<&str>) -> Option<String>` on the server, shared across all four call sites rather than duplicating the `get_state()` snapshot.

### Impact on the Three Learning Loops

| Loop | Current limitation | With phase on all four call sites |
|---|---|---|
| Phase-conditioned freq table | Only search query patterns; no confirmation | Gets weighted ~5x offers; briefing provides denominator for selection rate per (entry, phase) |
| Thompson Sampling | Beta posterior updated but phase-free | Per-(phase, entry) arms; briefing-then-no-get = weak negative; briefing-then-get = strong positive |
| Gap detection | Low search similarity only | High get concentration on narrow briefing subset = surrounding knowledge missing |

---

## Current State Summary

From PIPELINE-AUDIT.md:

```
Fused score = 0.25·similarity + 0.35·NLI + 0.15·confidence + 0.10·co-access
            + 0.05·utility + 0.05·provenance + 0.02·phase-histogram + 0.0·phase-explicit

Status penalty: multiplicative, applied after fused score
Category diversity: NOT enforced anywhere — all modes are pure score-sorted top-k
Phase signal: 0.02 from session histogram; 0.0 from phase-explicit (placeholder)
```

**Critical observations:**
1. `w_phase_explicit = 0.0` — the phase-routing signal is **entirely absent** from scoring today
2. The GRAPH_EDGES table (Supports, Prerequisite, CoAccess, Contradicts, Supersedes) is used **only for penalties** — never for retrieval boosting
3. ort is already running an ONNX model; adding a cross-encoder re-ranker requires **zero new infrastructure**
4. No category diversity enforcement exists in any of the three serving modes

---

## Recommended Architecture: Three Composable Learning Loops

The architecture that emerges from all three research tracks is a set of **three composable loops**, each operating at a different timescale, each independent of the domain pack's category/phase configuration:

### Loop 1 — Query-Time (< 50ms) — Hybrid Retrieval

Replace the current single-signal retrieval with a **three-signal hybrid**:

```
Candidates = HNSW(query, top-200)                    [dense semantic, existing]
           ∪ BM25_match(query, top-50)               [sparse keyword, new — `bm25` crate]
           ∪ graph_neighbors(top-5_dense)            [graph expansion, new — petgraph]

Re-rank candidates with cross-encoder NLI model     [existing ort session, extend model]
Apply phase-affinity gate                            [new — phase × category affinity matrix]
Apply Thompson Sampling blend                        [new — per-entry Beta posterior]
Apply category coverage floor                        [new — guarantee ≥1 entry per configured category]
```

**Why three signals?**
- Dense (HNSW): semantic similarity — already best-in-class
- Sparse (BM25): exact keyword match — catches domain-specific terminology that dense misses
- Graph: relevance propagation through Supports/CoAccess/Prerequisite edges — HippoRAG showed 20% multi-hop improvement using exactly this pattern; Unimatrix already has the graph

**Rust cost**: 3–6 days total (ort cross-encoder 1–2d, petgraph graph expansion 1–2d, BM25 1–2d). No new crate decisions — all three use existing or minimal new dependencies.

### Loop 2 — Session-Time (seconds to minutes) — Phase-Conditioned Learning

A background tick (already the maintenance pattern) rebuilds a **phase-conditioned frequency table**:

```
phaseFreqTable: HashMap<(phase_key: String, category_key: String), Vec<(entry_id, score)>>
```

Built from QUERY_LOG + injection_log over a rolling 30-day window. This table is the non-parametric implementation of RA-DIT's "retrain the retriever from feedback" loop. Every query updates the signal; no training step required. At serving time, the phase-conditioned prior blends with fused score to activate `w_phase_explicit` (currently 0.0).

This is fully domain-agnostic: the (phase, category) keys are strings resolved at runtime from whatever domain pack is configured.

### Loop 3 — Knowledge-Time (hours to days) — Gap Detection

A second background analysis rebuilds **knowledge gap candidates**:

```
gapCandidates: Vec<GapCandidate { phase, category, mean_top1_similarity, query_count }>
```

Flag clusters with `mean_similarity < 0.55` and `query_count >= 5` as active gaps. Surface via `context_status` response field. Self-reinforcing: a gap that triggers a new entry write will show improved mean_similarity in the next tick. This is the only loop that addresses *missing* knowledge rather than improving surfacing of *existing* knowledge.

---

## Component-Level Recommendations

### Keep, Extend

| Component | Verdict | Change |
|---|---|---|
| HNSW (hnsw_rs) | Keep | Extend candidate pool via BM25 and graph expansion |
| NLI cross-encoder (ort) | Keep + extend | Add ms-marco-MiniLM-L6 re-ranker in same ort session pattern |
| Confidence composite | Keep but decouple | Separate `usage_score` from exploration-critical path; expose as soft signal, not hard bias |
| Co-access (CO_ACCESS table) | Keep + extend | Wire into petgraph for Personalized PageRank; currently only used for a capped 0.03 additive boost |
| EWC++ (unimatrix-learn) | Replace | Switch to DER++ + focal loss before W3-1 activates: extend `TrainingReservoir<T>` to store `(input, label, stored_logit)` |
| Thompson Sampling | Add | Per-entry Beta posterior sampling at serving time; operates on existing `helpful_count/unhelpful_count` fields |

### Add

| Component | Verdict | Evidence |
|---|---|---|
| Phase-affinity matrix (w_phase_explicit) | Add immediately | Activates the 0.0 placeholder; zero training; contextual bandit literature confirms phase is the highest-signal discrete feature |
| Category coverage floor | Add immediately | Guarantees ≥1 entry per configured category in top-k; configurable similarity floor (cosine > 0.2) prevents irrelevant force-inclusions |
| BM25 hybrid (`bm25` crate) | Add | Catches keyword-exact queries that dense retrieval misses; no new model downloads |
| Graph expansion (petgraph) | Add | Personalized PageRank over Supports/CoAccess/Prerequisite edges; HippoRAG: 20% multi-hop improvement |
| Gap detection (new tick analysis) | Add | Only mechanism for identifying missing knowledge; all raw data already in QUERY_LOG |
| Phase-conditioned frequency table | Add | Non-parametric RA-DIT loop; self-improves with every query |

### Defer

| Component | When to revisit |
|---|---|
| W3-1 MLP (Mode 3 search re-ranking) | After Thompson Sampling + phase matrix are in place and training data CC@k ≥ 0.7 |
| SimCSE embedding fine-tuning | When corpus reaches 2K+ entries and an offline Python build step is acceptable |
| Epinet uncertainty heads | When query diversity across phases/agents generates meaningful joint uncertainty estimates |
| SPLADE sparse expansion | Corpus > 100K entries |
| Leiden community detection | Active entries > 500 |
| burn (neural framework) | When models grow past 1M params or GPU path needed |
| Neural Thompson Sampling (NTS) | After Thompson Sampling baseline ICD is measured; add uncertainty head if ICD < 1.5 nats |

---

## Domain-Agnostic Invariants

All proposed changes maintain domain-agnosticism:

1. **Phase-affinity matrix**: keys are string `(phase, category)` pairs resolved at runtime from `KnowledgeConfig.categories` and cycle phase. No hardcoded phase/category names.
2. **Category coverage floor**: iterates over `KnowledgeConfig.categories` (configurable) — works for any domain pack.
3. **Phase-conditioned frequency table**: keys are string pairs from whatever (phase, category) combinations appear in QUERY_LOG.
4. **Gap detection**: checks configured categories against QUERY_LOG clusters — domain-agnostic by construction.
5. **Graph expansion**: traverses typed edges (Supports, Prerequisite, CoAccess) — edge types are schema-level, not domain-specific.

---

## Evaluation Framework (Domain-Agnostic)

The existing eval harness (P@K, MRR) must be augmented. Three new primary metrics, each defined over configured categories (not hardcoded names):

**CC@k (Category Coverage at k)**
```
CC@k = |{cat : ∃ entry ∈ top-k with entry.category = cat}| / |configured_categories|
```
Target: CC@5 ≥ 0.7 before W3-1 promotion.

**ICD (Intra-Session Category Diversity)**
```
ICD = -Σ_cat p(cat) * log(p(cat))    [Shannon entropy over category distribution within a session]
Maximum = log(|configured_categories|)
```
Target: ICD ≥ 1.5 nats for a 6-category deployment.

**NEER (Novel Entry Exposure Rate)**
```
NEER = |surfaced_entries - previously_surfaced| / |surfaced_entries|
```
Target: NEER starts high (> 0.8 in early session), converges to < 0.3 by end of session.

---

## The Minimum Viable Self-Learning Architecture

Three changes, each independent, each deployable in 1–2 days:

1. **Phase-affinity matrix + Category Coverage Floor** — activates the `w_phase_explicit` placeholder, adds category diversity guarantee to briefing/injection. Zero training, auditable, immediate. Changes: `InferenceConfig` + `IndexBriefingService`.

2. **Cross-encoder re-ranker via ort** — adds a ms-marco-MiniLM-L6 (quantized int8) session alongside the existing embedding session. Changes: `unimatrix-embed` or a new ort session in the search pipeline. This directly improves Mode 3 (search) precision.

3. **petgraph Personalized PageRank for graph expansion** — wires the existing `GRAPH_EDGES` Supports/CoAccess/Prerequisite edges into a PageRank pass initialized from HNSW scores. Converts dead-weight graph structure into live retrieval signal. Changes: post-HNSW scoring step in `search.rs`.

These three together constitute a hybrid retrieval system (dense + cross-encoder + graph) that self-improves through two mechanisms: the phase-affinity matrix is updated by query feedback via the phase-conditioned frequency table, and the graph expands via co-access pairs recorded each session.

The feedback loop bias (high-confidence categories crowd out low-confidence ones) is addressed immediately by the category coverage floor (hard diversity guarantee) and over time by the Thompson Sampling blend (uncertainty-driven exploration of underexposed categories).

---

## What This Is Not

- This is not a proposal to replace the current architecture. It is layered on top.
- This does not require a dedicated ML team or training infrastructure.
- This does not require external service dependencies.
- This does not require the W3-1 MLP to be complete.
- This is not software-development-specific — all components generalize to any domain pack.
