# Finding 03: Unimatrix Relevance Assessment

**Date:** 2026-02-27
**Status:** Complete — multiple applicable areas identified

---

## Current Unimatrix Numerical Landscape

Full codebase analysis of all numerical subsystems in Unimatrix's 5 crates. None currently use quantized arithmetic — everything is f32/f64 floating point. However, several subsystems exhibit properties that make the concepts relevant either now or at scale.

---

## Directly Relevant Areas

### 1. Stale Confidence Drift (Existing Bug-Adjacent Behavior)

**What happens:** `EntryRecord.confidence` is an f32 computed at mutation time (store/correct/deprecate/quarantine) and stored. It is NOT recomputed at read time. The freshness component uses exponential decay: `(-age_hours / 168.0).exp()`.

**The drift:** An entry stored with freshness=1.0 retains that value indefinitely in storage even though real freshness decays to ~0 after a few weeks. The search re-ranking formula (`0.85 * similarity + 0.15 * confidence`) uses this stale value, systematically over-ranking old entries.

**Connection to the research:** This is conceptually analogous to deterministic drift in always-on systems — not from quantization resonance, but from the gap between stored state and real-time truth. A **coherence gate** (λ) that periodically recomputes confidence for entries above a staleness threshold would address this. The λ concept maps directly.

**Impact:** Medium. The 0.15 confidence weight means a maximally stale entry gets at most +0.15 * 0.18 = +0.027 bonus from a phantom freshness score. Small but systematic.

### 2. Embedding Quantization (Future Scale Concern)

**Current state:** Embeddings are full f32 (384 dimensions = 1,536 bytes per vector). The hnsw_rs index stores them uncompressed in memory and on disk.

**At scale:** At 100K entries, the embedding index alone would be ~150MB. At 1M entries, ~1.5GB. Embedding quantization (binary, scalar int8, or product quantization) becomes necessary.

**Connection to the research:** If Unimatrix ever quantizes embeddings:
- Binary quantization (1-bit) would be most susceptible to resonance — all values snap to {-1, +1}
- Scalar int8 quantization snaps to 256 levels — rounding patterns emerge in clustered embedding spaces
- π-scaled calibration of quantization boundaries could prevent systematic rounding bias in high-density regions of the embedding space
- The 3/5/7-bit adaptive scheme maps to a tiered embedding precision concept: frequently accessed entries keep full f32, stable entries could be compressed to int8, rarely used entries to 4-bit

**Impact:** Low (current), potentially high (at scale). Worth remembering when quantization becomes necessary.

### 3. Search Re-Ranking Score Clustering

**Current formula:**
```
final_score = 0.85 * similarity + 0.15 * confidence + co_access_boost
```

**The concern:** If many entries have similar similarity scores (common in a knowledge base with topically related entries), the ranking depends heavily on the 0.15 * confidence + boost terms. These additive terms use f32 arithmetic with weights that are all multiples of 0.01 (0.85, 0.15, 0.03 max boost). In f32, 0.01 is not exactly representable — it becomes `0.0099999997...`. Repeated operations on these values produce deterministic but non-obvious rounding patterns.

**Connection to the research:** This is not quantization resonance per se, but the f32 representation of decimal scoring constants creates a mild form of the same phenomenon. The rounding errors are small (ULP-level) but deterministic and repeating. If two entries have truly equal relevance, the tie-breaking depends on these rounding artifacts.

**Mitigation idea:** Use π-derived or otherwise irrational scaling constants instead of clean decimal weights. Instead of 0.85/0.15, use e.g., `π/4 ≈ 0.7854` / `(1 - π/4) ≈ 0.2146`. The irrationality of these constants means their f32 rounding errors are maximally spread rather than clustering on binary boundaries.

**Impact:** Very low (ULP-level artifacts). Theoretically interesting but not practically important at current scale.

### 4. HNSW Graph Structure Drift

**Current state:** When entries are re-embedded (via contradiction checking or corrections), a new point is added to the HNSW graph but the old point remains as a "stale" node. The old point is removed from the ID map but continues to participate in graph traversal as a routing node.

**The drift:** Over many re-embed cycles, stale nodes accumulate and degrade graph quality. The graph structure drifts from its optimal configuration. `stale_count()` is tracked but never triggers cleanup.

**Connection to the research:** This is a structural drift problem where the λ coherence gate concept directly applies. A coherence metric measuring graph quality (e.g., ratio of stale to active nodes, or search recall degradation) could gate when graph compaction is triggered.

**Impact:** Medium at scale. Currently negligible with <1K entries.

---

## Indirectly Relevant Areas

### 5. Contradiction Detection Thresholds

The conflict heuristic uses fixed f32 weights: negation (0.6), directive (0.3), sentiment (0.1). These are exact in f32 since the conflict score is discrete-valued (outputs are from {0.0, 0.5, 1.0} scaled by weights). No drift concern exists here, but the general principle of choosing constants that don't resonate with the value space is applicable.

### 6. Confidence Weight Distribution

The 6 stored weights (0.18, 0.14, 0.18, 0.14, 0.14, 0.14) sum to 0.92, leaving 0.08 for co-access affinity at query time. These values were chosen for domain meaning, not numerical properties. If confidence computation ever moves to lower precision (unlikely), the fact that multiple weights share the exact value 0.14 could create aligned rounding.

---

## The λ Coherence Gate: Highest-Value Concept for Unimatrix

Of all the concepts in this research, the **λ coherence gate** has the most direct applicability:

| Unimatrix Subsystem | Coherence Signal | Gated Action |
|---------------------|-----------------|--------------|
| Confidence scoring | Age of stored confidence vs real-time recomputation | Lazy confidence refresh |
| HNSW graph | Stale node ratio, search recall probe | Graph compaction trigger |
| Embedding space | Embedding consistency scan results | Re-embed flagged entries |
| Knowledge base health | Contradiction scan, unused entry ratio | Maintenance cycle trigger |

A unified coherence metric (λ) that combines these signals could gate Unimatrix's self-maintenance operations — only running expensive operations (re-embed, recompute confidence, compact graph) when structural coherence drops below threshold.

This maps to the user's description: "λ measures structural stability and gates updates across multiple crates."

---

## Verdict

**Moderate relevance now, high relevance at scale.** The stale confidence drift and HNSW graph degradation are real current issues that the λ coherence gate concept addresses. Embedding quantization with π-calibration becomes relevant if Unimatrix scales beyond ~50K entries. The mathematical foundations (Weyl equidistribution) are sound and the concepts are worth tracking as Unimatrix evolves.
