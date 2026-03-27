# ASS-032: Self-Learning Knowledge Engine — Implementation Roadmap

**Date**: 2026-03-26
**Research**: RESEARCH-SYNTHESIS.md, PIPELINE-AUDIT.md, NOVEL-APPROACHES.md, RUST-ML-ECOSYSTEM.md

---

## Architecture Summary

The system evolves through a self-reinforcing flywheel:

```
Query + current_phase
    │
    ├──× phase_affinity_score          ← frequency table (#414)
    ▼
HNSW candidates (semantic)
    │
    ▼
PPR over TypedRelationGraph            ← graph built by #412 + #395
(CoAccess + Supports + Prerequisite)
    │
    ▼
Fused scoring
  + w_phase_histogram  (real-time, in-session)
  + w_phase_explicit   (frequency table, activated by #414)
  + w_coac             (transitions → 0.0 as PPR proves itself, #415)
    │
    ▼
Entries surface → agent selects via context_get
    │
    ├── co_access recorded WITH phase  (#394 ✓)
    │       → GRAPH_EDGES.CoAccess
    ├── NLI entailment → GRAPH_EDGES.Supports    (#412)
    ├── query_log with phase (#397 ✓)
    │       → frequency table rebuilt next tick   (#414)
    └── back to top
```

Signal (co_access + NLI) → graph → PPR → scoring → access with phase → signal.
The graph builds itself. Phase affinity self-improves with every query.

---

## Completed

| Issue | Title |
|---|---|
| ✅ #399 | CC@k and ICD distribution metrics in eval harness |
| ✅ #394 | Phase signal capture on all four read-side call sites |
| ✅ #397 / col-028 | `query_log.phase` column (schema v16→v17) |

---

## Roadmap

### Hotfix — ship before `maintain=true` is re-enabled

| Issue | Title | Notes |
|---|---|---|
| ✅ #408 | `CO_ACCESS_STALENESS_SECONDS` 30d → 365d | One-line fix. System is ~30 days old. |

---

### Eval infrastructure — unblocked, no learning deps

| Issue | Title | Notes |
|---|---|---|
| ✅ #400 | Phase in eval scenarios | Unblocked after col-028. Phase-stratified P@K/MRR in report. |
| ✅ #402 | Distribution gate for distribution-changing features | Unblocked after #399. Replaces zero-regression check for PPR/graph features. Must be in place before #398 ships. |

---

### Graph foundation

| Issue | Title | Notes |
|---|---|---|
| #413 | Graph cohesion metrics in `context_status` | No deps. `isolated_entry_count`, `cross_category_edge_count`, `supports_coverage`, `mean_entry_degree`. Primary health view for graph inference. |
| #395 | Contradicts collision suppression | Independent. Validates TypedRelationGraph retrieval path before PPR. Post-scoring filter using `edges_of_type(Contradicts)`. |
| #412 | Automated graph inference pass | NLI entailment → `Supports` edges. Asymmetric entailment → `Prerequisite` candidates. HNSW pre-filter (similarity > 0.5). Same `max_*_per_tick` throttle as contradiction detection. Cross-category pairs and isolated entries prioritised. Zero new ML — same ort session, same model. |

---

### Phase learning

| Issue | Title | Notes |
|---|---|---|
| #414 | Phase-conditioned frequency table | Rebuilt each tick from `query_log` with phase. `HashMap<(phase, category), Vec<(entry_id, f32)>>`. Two wire-ups: (1) activates `w_phase_explicit` (currently 0.0 placeholder) in fused score; (2) weights PPR personalization vector so graph traversal is phase-informed. Cold start degrades gracefully to neutral weights. |

---

### PPR — the main event

| Issue | Title | Notes |
|---|---|---|
| #398 | Personalized PageRank with phase-weighted personalization | Full PPR over positive edges (Supports + CoAccess + Prerequisite). Personalization vector = HNSW score × phase_affinity_score (from #414). 20 power iterations. petgraph. Gates: #402 distribution gate in place + #412 edges exist + #414 frequency table active. Replaces #396 (depth-1 Supports expansion — skip). |

---

### Transition + validation

| Issue | Title | Notes |
|---|---|---|
| #415 | co_access direct boost → PPR deprecation plan | ADR + three-phase transition. Phase 1: both active, measure. Phase 2: reduce w_coac after CC@k ≥ baseline+0.10 and no MRR regression. Phase 3: w_coac → 0.0. No code change until measurement gate passes. |

---

### Retention (end — no deps blocking other work)

| Issue | Title | Notes |
|---|---|---|
| #409 | Intelligence-driven retention framework | co_access: count + feature-cycle-activity. `query_log`: last K completed cycles (K also governs frequency table + GNN lookback). `audit_log`: 180-day time-based. K is the single configurable parameter governing all learning-signal retention. |

---

## Dependency graph

```
#408  (hotfix, independent)

#400 ──── depends on col-028 ✓
#402 ──── depends on #399 ✓
#413 ──── no deps
#395 ──── no deps
#412 ──── no deps (but #413 surfaces its output)
#414 ──── depends on col-028 ✓

#398 ──── depends on #402 + #412 + #414

#415 ──── depends on #398 (PPR must ship and prove itself)

#409 ──── no blocking deps (design work, can progress in parallel)
```

---

## Eval harness gates

| Feature | Gate |
|---|---|
| #395 (Contradicts) | Zero-regression check (preserves ranking, doesn't change distribution) |
| #412 (Graph inference) | Graph cohesion metrics (#413): `isolated_entry_count` trending down, `cross_category_edge_count` trending up. `SUPPORTS_EDGE_THRESHOLD` tuned if growth stalls. |
| #398 (PPR) | Distribution gate (#402): CC@5 ≥ baseline + 0.10, ICD improvement, MRR floor ≥ 0.35 |
| #415 (w_coac reduction) | Eval profiles: PPR-only vs PPR+direct, measurement gate before any weight reduction |

---

## Phase vocabulary note

All phase-conditioned signals use string keys resolved at runtime from `query_log.phase`. Adding a new phase = cold start for that phase, graceful degradation to neutral weights. Renaming a phase requires a one-query `UPDATE query_log SET phase = 'new-name' WHERE phase = 'old-name'` migration; the frequency table rebuilds correctly on the next tick. No compile-time phase enum anywhere in the learning path.

---

## Deferred

| Item | When to revisit |
|---|---|
| Thompson Sampling (per-entry Beta posterior exploration) | After PPR baseline ICD measured; add if ICD < 1.5 nats |
| SimCSE embedding fine-tuning | Corpus ≥ 2K entries, offline Python build step acceptable |
| W3-1 GNN (replaces frequency table w_phase_explicit) | After frequency table CC@k ≥ 0.7 and training data is sufficient |
| Epinet uncertainty heads | After query diversity across phases/agents generates meaningful estimates |
| Leiden community detection | Active entries > 500 |
| DER++ (replace EWC++ in unimatrix-learn) | Before W3-1 activates — correctness improvement for scorer training |
| NEER (Novel Entry Exposure Rate) | After session-level eval is designed; requires cross-query session context |
| Briefing relationship annotations in response | After PPR + phase signals are contributing (provenance annotations meaningful) |
| Category Coverage Floor | Measure CC@k after PPR lands; add hard floor only if organic signals insufficient |
