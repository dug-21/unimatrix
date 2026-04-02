# ASS-040: Self-Learning Knowledge Engine — Roadmap v2

**Date**: 2026-04-02  
**Research basis**: ASS-035, ASS-036, ASS-037, ASS-038, ASS-039  
**Eval foundation**: 1,585 behavioral scenarios, behavioral ground truth MRR=0.2913  
**Prior roadmap**: ASS-032 complete.

---

## Architecture Summary

The target flywheel — current state annotated with what is confirmed, in-flight, and planned:

```
Query + current_phase + goal_embedding (†)
    │
    ├── phase_affinity_score          ← frequency table [DELIVERED #414]
    ├── goal_cluster_lookup (†)       ← behavioral signal [PLANNED]
    ▼
HNSW candidates (semantic, k=20)
    │
    ▼
graph_expand(seeds=k20, depth=2, max=200) (†)  ← PPR expander [PLANNED — the unlock]
    → expanded candidate pool (cross-category entries reachable)
    │
    ▼
PPR over TypedRelationGraph
(CoAccess + Informs + Supports + Prerequisite)
    │
    ▼
Fused scoring [FORMULA CONFIRMED — conf-boost-c]
  w_sim  = 0.50   ← cosine, load-bearing floor [CONFIRMED ASS-037/039]
  w_conf = 0.35   ← Wilson-score confidence, +9% MRR [CONFIRMED ASS-039]
  w_nli  = 0.00   ← removed: task mismatch [CONFIRMED ASS-035/037]
  w_coac = 0.00   ← moved to PPR topology [DELIVERED crt-032]
  w_util = 0.00   ← redundant with confidence [CONFIRMED ASS-039]
  w_prov = 0.00   ← redundant with confidence [CONFIRMED ASS-039]
  w_phase_histogram  ← real-time session signal [+1% MRR, CONFIRMED ASS-039]
  w_phase_explicit   ← frequency table [+1% MRR, CONFIRMED ASS-039]
    │
    ▼
Entries surface → agent selects via context_get
    │
    ├── CoAccess recorded → GRAPH_EDGES.CoAccess
    ├── S8: search co-retrieval → GRAPH_EDGES.CoAccess (†) [PLANNED]
    ├── cosine Supports (≥0.65) → GRAPH_EDGES.Supports (†) [PLANNED]
    ├── structural Informs Phase 4b (cosine ≥0.5) → GRAPH_EDGES.Informs (†) [PLANNED]
    ├── S1: tag co-occurrence → GRAPH_EDGES.Informs (†) [PLANNED]
    ├── S2: structural vocabulary → GRAPH_EDGES.Informs (†) [PLANNED]
    ├── query_log with phase → frequency table rebuilt next tick [DELIVERED #414]
    ├── context_cycle_review → behavioral Informs edges (†) [PLANNED]
    └── context_cycle_review → goal-cluster update (†) [PLANNED]

(†) = not yet delivered
```

Signal sources → graph → PPR expander → scoring → access → signal.  
The graph builds itself from multiple weak sources. PPR expander makes cross-category
entries reachable. Behavioral signals close the self-sustaining loop.

---

## Research Findings That Ground This Roadmap

| Spike | Central Finding |
|-------|----------------|
| ASS-035 | NLI task mismatch confirmed (SNLI-trained model, structured knowledge entries). Cosine ≥ 0.65 validated for Supports detection. |
| ASS-036 | GGUF FAIL at deployable model sizes. No near-term LLM replacement for NLI. |
| ASS-037 | Formula: conf-boost-c confirmed. NLI dead (zero contribution). PPR zero-effect at current graph density. Phase signals load-bearing (+1% MRR). |
| ASS-038 | PPR bottleneck is architecture, not density. 6,738 edges → zero delta. Cross-category ground truth entries are outside k=20. PPR must be an expander, not a re-ranker. S1/S2/S8 edge sources viable. Labeled edge set GNN-ready. |
| ASS-039 | 1,585 behavioral scenarios built (proper ground truth). All ASS-037 findings confirmed on valid ground truth. Goal clustering real (2.7–9.5× effect). Phase stratification real (82% of entries are phase-specific within a cycle). Behavioral signal infrastructure mostly in place; needs feature_cycle on audit_log, goal embedding, agent_role population. |

**Eval baseline (behavioral ground truth, behavioral scenarios)**:

| Metric | Value | Notes |
|--------|-------|-------|
| Scenarios | 1,443 | 1,585 records; 135 duplicate IDs collapse to 1,443 unique eval scenarios |
| MRR (conf-boost-c) | 0.2875 | Live DB, 2026-04-02, `run_eval.py` (GH #487). Replaces 0.2913 snapshot baseline. |
| P@5 | 0.1115 | Formula-invariant — determined by HNSW recall, not scoring |
| Prior MRR (crt-038 snapshot) | 0.2913 | Was measured against ASS-037 snapshot at crt-038; superseded by live-DB run |

All future eval runs use `product/research/ass-039/harness/scenarios.jsonl`.  
The old ASS-037 qlog scenarios are deleted — do not reference them.

---

## Roadmap

### Group 1 — Formula and NLI Cleanup ✅ COMPLETE (`crt-038`, PR #484, GH #483)

Config defaults changed to conf-boost-c. Three dead NLI code paths removed. Eval gate
passed: MRR = 0.2913 (commit 6a6d864b, behavioral ground truth, 1,585 scenarios).
Notable: `impl Default for InferenceConfig` had hardcoded old weights separate from
`default_w_*()` backing fns — caught by Gate 3b, pattern stored (#4011).

| Issue | Title | Notes |
|-------|-------|-------|
| ✅ crt-038 | Apply conf-boost-c formula | `w_sim=0.50, w_conf=0.35, w_nli=0.00, w_util=0.00, w_prov=0.00`. `FusionWeights::effective()` short-circuit added (w_nli==0.0 → no re-normalization). MRR +0.0031 confirmed. |
| ✅ crt-038 | Remove NLI from post-store detection | `run_post_store_nli` deleted. `NliStoreConfig` struct deleted entirely. `parse_nli_contradiction_from_metadata` + 5 cascaded dead-code tests also removed (found during delivery). |
| ✅ crt-038 | Remove auto-quarantine NLI guard | `nli_auto_quarantine_allowed`, `NliQuarantineCheck` deleted. `process_auto_quarantine` signature cleaned (2 params dropped — threaded through 5 signatures + main.rs). |
| ✅ crt-038 | Remove bootstrap promotion | `maybe_run_bootstrap_promotion`, `run_bootstrap_promotion` deleted. |

---

### Group 2 — Tick Decomposition ✅ COMPLETE (`crt-039`, PR #486, GH #485)

Option Z internal split: Phase 4b (structural Informs) runs unconditionally in Path A;
Phase 8 (NLI Supports) gated by `get_provider()` in Path B. `NliCandidatePair::Informs`
and `PairOrigin::Informs` variants removed. `apply_informs_composite_guard` simplified to
2 guards (temporal + cross-feature). `nli_informs_cosine_floor` raised 0.45 → 0.5.
Follow-up filed: #487 (run_eval.py eval harness runner, needed before Group 3 ships).

| Issue | Title | Notes |
|-------|-------|-------|
| ✅ crt-039 | Structural graph tick — decouple from NLI gate | `if nli_enabled` outer gate removed from `background.rs`. Phase 4b now runs every tick regardless of NLI state. `get_provider()` moved to Path B entry — gates Phase 6/7/8 only. |
| ✅ crt-039 | Remove Phase 8b NLI guard from Informs path | `nli_scores.neutral > 0.5` removed from `apply_informs_composite_guard`. Guards 4+5 (NLI mutual exclusion) also removed — candidate set separation enforces mutual exclusion structurally (AC-13 explicit subtraction). Cosine floor raised 0.45 → 0.50. |
| ✅ crt-039 | Separate contradiction scan as own periodic tick | Named comment block added in `background.rs`. Zero behavioral change — condition and ordering preserved. |

**Ordering invariant**: compaction → co_access_promotion → graph-rebuild → PhaseFreqTable::rebuild → contradiction_scan (if embed adapter ready && tick_multiple) → extraction_tick → structural_graph_tick (always). Confirmed unchanged.

---

### Group 3 — Graph Enrichment

Populate the graph from multiple signal sources. Each source is independent and can
ship separately. All generate `Informs` or `CoAccess` edges to the production graph.
Target combined density: ≥3,000 active→active edges (currently 1,086).

| Issue | Title | Notes |
|-------|-------|-------|
| — | Cosine Supports detection | Replace NLI post-store path. Threshold ≥ 0.65, category pair filter from `informs_category_pairs`. Validated in ASS-035: 6/8 true pairs, 0/10 false positives. Runs in structural_graph_tick (Group 2 prerequisite). |
| — | S1: Tag co-occurrence Informs edges | Background tick: pairs sharing ≥3 tags → `Informs` edge. SQL-only, no model. Yield: ~1,052 new edges at current corpus. Tag `signal_origin='S1'` on edge for GNN feature construction. |
| — | S2: Structural vocabulary Informs edges | Background tick: configurable domain term list; pairs sharing ≥2 terms → `Informs` edge. Vocabulary in config (domain-agnostic). Yield: ~1,830 new edges. Tag `signal_origin='S2'`. |
| — | S8: Search co-retrieval CoAccess edges | Periodic batch from `audit_log`: pairs co-appearing in search results across sessions → `CoAccess` edge. Yield: ~2,770 new edges. Not real-time — batch every N ticks reading from audit_log. Tag `signal_origin='S8'`. |

**Note on edge labeling**: All new edges must carry `signal_origin` field for GNN feature
construction. The labeled edge set from ASS-038 is the W3-1 training data specification —
do not inject unlabeled edges.

---

### Group 4 — PPR Expander (The Unlock)

**Critical path.** Without this, all graph enrichment (Group 3) produces zero retrieval
improvement. The current PPR implementation is a re-ranker within k=20. Cross-category
ground truth entries are outside k=20 by construction — semantically distant entries
are exactly what the graph is designed to surface. PPR cannot reach them until
the expander ships.

Confirmed by ASS-038: 6,738 edges, zero delta. Confirmed again by ASS-039 re-run on
behavioral ground truth: identical zero delta.

| Issue | Title | Notes |
|-------|-------|-------|
| — | PPR expander: HNSW seeds → graph expand → expanded candidate pool | Change: HNSW(k=20) produces seeds. `graph_expand(seeds, depth=2, max_candidates=200)` traverses GRAPH_EDGES to add entries outside k=20 to the candidate pool. Score and rank from the expanded pool. Gate behind feature flag. |

**Architecture**:
```
Current:  HNSW(k=20) → 20 candidates → PPR re-ranks within 20
Target:   HNSW(k=20) → seeds → graph_expand → ≤220 candidates → PPR scores all → top-k
```

**Scale**: With 6,738 edges and max_candidates=200, graph traversal is bounded and fast
(SQLite graph walk). Latency addition is meaningful — feature flag and latency measurement
required before enabling by default.

**Eval gate**: After shipping, run Profile A (expander enabled) vs Profile B (expander
disabled) on behavioral scenarios. Measure P@5 and MRR delta. For the first time, P@5
should respond to formula changes — cross-category entries previously outside k=20 will
enter the candidate pool.

---

### Group 5 — Behavioral Signal Infrastructure

Enables Goal × Phase × Entries × Outcome signals validated directionally in ASS-039.
Three infrastructure items required before behavioral edge emission and goal-conditioned
briefing can ship.

| Issue | Title | Notes |
|-------|-------|-------|
| — | audit_log: add feature_cycle_id at write time | Single field addition. At `log_audit_event()`, include the currently-active context_cycle feature_id (or null if no active cycle). Unblocks S6 (outcome co-retrieval) and S7 (briefing selection) signal sources. |
| — | Goal embedding at context_cycle start | At `context_cycle` start: fetch GH issue title + body for the feature_cycle_id. Embed via existing pipeline. Store goal_embedding + goal_text on cycle record. Enables H1 (goal clustering) with proper cosine similarity instead of keyword Jaccard proxy. |
| — | agent_role: mandatory population on sessions | Currently 4/182 sessions have agent_role populated. Make it mandatory at session open. Enables H3 (phase stratification cluster test) and role-conditioned briefing. |

---

### Group 6 — Behavioral Signal Delivery

**Conditional on Group 5 infrastructure shipping.** Closes the self-sustaining loop:
each cycle completion becomes a learning event that enriches the graph.

| Issue | Title | Notes |
|-------|-------|-------|
| — | context_cycle_review: behavioral Informs edge emission | At cycle close: write `Informs` edges for context_get co-access pairs within the cycle. Weight by outcome: success=1.0, rework=0.5. `signal_origin='behavioral'`. Additive only — never remove existing edges. |
| — | Goal-cluster store: schema + population | New table: `goal_clusters (feature_cycle, goal_embedding, phase, entry_ids[], outcome)`. Populated by context_cycle_review at cycle close. Enables goal-conditioned retrieval. |
| — | context_briefing: goal-conditioned entry retrieval | At briefing: retrieve goal-similar past cycles (cosine on goal_embedding, same phase). Blend goal-cluster entries with semantic retrieval results. Cold-start: zero history → pure semantic retrieval (no behavior change for new deployments). |

---

### Group 7 — Data Hygiene

| Issue | Title | Notes |
|-------|-------|-------|
| — | Purge phantom entries + HNSW vector removal mechanism | ~2,491 quarantined entries are phantom records written by a tick bug (numeric findings stored as outcome entries). They can be deleted but no atomic delete + HNSW vector removal exists. Requires new capability: `context_purge` or equivalent that removes the DB row, VECTOR_MAP entry, and HNSW index point atomically. |
| ✅ #477 | Quarantine guard at co_access write time | Pre-existing issue. Prevents stale-pair accumulation at write time. |
| ✅ #476 | co_access promotion cycle bug | Fixed. Promotion SELECT now filters quarantined entries on both sides. |
| ✅ #471 | Orphaned-edge compaction for deprecated entries | Fixed. |

---

### Group 8 — Open Carry-forwards

Two items from the prior roadmap remain open:

| Issue | Title | Notes |
|-------|-------|-------|
| #415 | co_access direct boost → PPR deprecation plan | Phase 1 measurement complete. Phase 2 gate (CC@k ≥ baseline+0.10) requires re-measurement against behavioral ground truth — prior soft-GT metrics invalid for this gate. Re-baseline after conf-boost-c ships (Group 1). |
| #409 | Intelligence-driven retention for analytic tables | Unblocked. Entry auto-deprecation for adaptive categories. `K` configurable parameter governing learning-signal retention across co_access, query_log, audit_log. |

---

## Dependency Graph

```
Group 1 (Formula + NLI cleanup) ─── no deps ─── ship first

Group 2 (Tick decomposition)    ─── no deps ─── ship concurrently with Group 1
  └── prerequisite for: Group 3 (structural Informs inference)

Group 3 (Graph enrichment)      ─── depends on: Group 2 (tick decomposition)
  ├── S1/S2/S8 independent of each other
  ├── Cosine Supports: depends on Group 2 tick decomposition
  └── prerequisite for: Group 4 (PPR expander — needs dense graph to traverse)

Group 4 (PPR expander)          ─── depends on: Group 3 (graph density)
  └── the unlock for all cross-category retrieval

Group 5 (Behavioral infrastructure) ─── no deps ─── ship concurrently with Groups 2/3
  └── prerequisite for: Group 6

Group 6 (Behavioral signal delivery) ─── depends on: Group 5 + Group 4
  └── goal-conditioned briefing benefits from PPR expander being live

Group 7 (Data hygiene)          ─── no deps ─── ship independently
  └── phantom entry purge improves eval quality and snapshot cleanliness

Group 8 (Carry-forward)
  └── #415: re-baseline after Group 1 ships (conf-boost-c changes the measurement context)
  └── #409: no blocking deps
```

---

## Eval Harness Gates

The behavioral scenario set is the canonical measurement instrument for all future
features. Reference: `product/research/ass-039/harness/scenarios.jsonl`.

| Feature | Gate |
|---------|------|
| conf-boost-c formula | ✅ PASSED — MRR = 0.2875 (live DB, 2026-04-02, run_eval.py). Prior snapshot baseline 0.2913 superseded. |
| Cosine Supports detection | Graph cohesion metrics: supports_coverage increase. No MRR regression vs conf-boost-c. |
| S1/S2/S8 edge generation | Graph cohesion: `cross_category_edge_count` increase, `isolated_entry_count` decrease. |
| PPR expander | **First gate where P@5 should respond**: expect P@5 increase as cross-category entries enter candidate pool. MRR ≥ 0.2913. If P@5 unchanged after expander, diagnose why ground truth entries are still outside expanded pool. |
| Goal-conditioned briefing | Measure MRR on briefing-sourced scenarios specifically (149 scenarios). Compare briefing profile vs. semantic-only profile. |

---

## Deferred

| Item | Trigger condition |
|------|------------------|
| W3-1 GNN | After PPR expander ships + behavioral edges accumulate. Labeled edge set (ASS-038) is the training data spec. GNN replaces hand-tuned signal weights. |
| H2 re-test (outcome correlation) | When corpus has ≥10 rework cycles with entry access data. Consider gate-failure tagging at context_cycle stop. |
| H3 cluster test (phase stratification) | When agent_role is populated on ≥50 sessions AND ≥20 cycles cover similar goal domains. |
| H1 embedding validation | When goal embedding (Group 5) is live — re-run H1 with proper cosine on embeddings vs. keyword Jaccard proxy. |
| S6 (outcome co-retrieval edges) | After audit_log feature_cycle correlation ships (Group 5). |
| S7 (briefing selection edges) | After audit_log feature_cycle correlation ships (Group 5). |
| S3/S4/S5 edge sources | Re-evaluate at corpus ≥ 3,000 entries. S3 (keyword overlap) and S4 (citation detection) yielded <20 pairs at current scale. |
| NLI replacement model | Blocked until domain-adapted model available. GGUF failed (ASS-036). SNLI cross-encoder explicitly not for Unimatrix corpus. |
| Contradiction detection | NLI-gated, blocked on NLI replacement. 0 Contradicts edges ever written in production. |
| Thompson Sampling | After PPR expander ICD measured — add if ICD < 1.5 nats. |
| SimCSE fine-tuning | Corpus ≥ 2,000 active entries, offline Python build acceptable. |
| ASS-034 parameterized relationship taxonomy | After Group 3/4 stabilizes. Targets remaining isolation with text_reference, feature_field detection, InformedBy/ImplementsDecision/Mentions edge types, per-type PPR weights. |
| Briefing relationship annotations | After PPR expander contributing — provenance annotations become meaningful. |
| NEER (Novel Entry Exposure Rate) | After session-level eval designed. |
| Category Coverage Floor | Measure CC@k after PPR expander lands; add hard floor if organic signals insufficient. |
