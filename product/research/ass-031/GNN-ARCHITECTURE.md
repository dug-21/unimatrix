# ASS-031: GNN Architecture for Session-Conditioned Relevance

---

## 1. The Naming Question

The product vision calls W3-1 a "GNN" (Graph Neural Network). After examining the codebase, the correct term is **graph-feature-enriched MLP relevance scorer**. This is not a retreat — it is the right architecture for this problem at this scale:

- Graph structure (edge types, degrees, NLI confidence) is captured as **input features** to the scorer, not as message-passing layers
- Unimatrix's knowledge graphs (hundreds to low thousands of entries) are too small to benefit from learned message passing over true structural neighborhoods
- A 2-layer MLP over [entry_features ++ session_context] delivers the session-conditioned relevance function the product vision describes, at far lower training complexity and inference cost
- The `unimatrix-learn` infrastructure already implements this pattern (`ConventionScorer`, `NeuralModel` trait)

If genuine graph message passing becomes warranted as the knowledge base grows (>50K entries), the `unimatrix-adapt` LoRA pattern can be extended. For W3-1, it is overkill.

---

## 2. The Three Query Modes — Unified Model

All three delivery surfaces use the **same model** with different input configurations.

```
Mode 1 — Proactive (UDS injection):
  input = [entry_features ++ session_context ++ [0.0, 0.0, 0.0]]
              (no query: query_sim=0, nli_score=0, query_present=0)

Mode 2 — Comprehensive (context_briefing at phase transition):
  input = [entry_features ++ session_context ++ [0.0, 0.0, 0.0]]
              (same as Mode 1 — no query anchor)

Mode 3 — Reactive (context_search re-ranking):
  input = [entry_features ++ session_context ++ [query_sim, nli_score, 1.0]]
              (query signals present: query_sim from HNSW, nli_score from NLI re-ranker)
```

The three-element query suffix (query_sim, nli_score, query_present) lets the model learn that query-conditioned scoring has different characteristics than session-conditioned scoring, without requiring two separate networks.

The full input dimension is `entry_dim + session_dim + 3`. See `FEATURE-SPEC.md` for exact dimensions. With typical configuration: `18 + 25 + 3 = 46` dims.

---

## 3. Model Architecture: RelevanceScorer

Extends the `NeuralModel` trait from `unimatrix-learn`:

```
RelevanceScorer
  w1: Array2<f32>  [46 × 64]   Xavier init
  b1: Array1<f32>  [64]         zero init
  w2: Array2<f32>  [64 × 32]   Xavier init
  b2: Array1<f32>  [32]         zero init
  w3: Array2<f32>  [32 × 1]    Xavier init, output bias −2.0 (conservative start)
  b3: Array1<f32>  [1]

Forward pass:
  z1 = w1ᵀ · input + b1
  a1 = relu(z1)
  z2 = w2ᵀ · a1 + b2
  a2 = relu(z2)
  z3 = w3ᵀ · a2 + b3
  output = sigmoid(z3)            → scalar in [0.0, 1.0]

Loss: binary cross-entropy (same as ConventionScorer)
Optimizer: SGD (same as existing models; Adam can be added via EWC++ update path)
```

**Parameter count**: (46×64) + 64 + (64×32) + 32 + (32×1) + 1 = 2944 + 64 + 2048 + 32 + 32 + 1 = **5121 params**
**Model size**: 5121 × 4 bytes = ~20KB (well within the 400KB envelope)

**Why three layers instead of two**: The existing `ConventionScorer` uses two layers for a 32-dim input classifying a single signal type. `RelevanceScorer` fuses two distinct modalities (entry state + session context) with an optional query signal. The extra layer allows the model to first learn modality-specific representations before fusing them for the final score.

**Conservative output bias (−2.0)**: Matches `ConventionScorer`. Cold-start produces low relevance scores for all entries. The manual formula remains dominant until training data is sufficient (see Section 6).

---

## 4. Integration Point in the Scoring Pipeline

The current `compute_fused_score` formula (WA-0, crt-026) has a reserved slot:

```
score = w_nli*nli + w_sim*sim + w_conf*conf + w_coac*coac + w_util*util + w_prov*prov
      + w_phase_histogram * phase_histogram_norm   ← WA-2 manual affinity
      + w_phase_explicit * 0.0                     ← W3-1 placeholder (currently dead)
```

W3-1 replaces both WA-2 affinity terms with the GNN affinity score:

```
gnn_affinity = RelevanceScorer.forward(entry_features ++ session_ctx ++ query_signal)

blend_alpha = gnn_state.blend_alpha()   → ramps 0.0→1.0 as training data accumulates

final_score = (1 - blend_alpha) * manual_fused_score
            + blend_alpha * (manual_base + w_gnn * gnn_affinity)

where:
  manual_base     = w_nli*nli + w_sim*sim + w_conf*conf + w_coac*coac + w_util*util + w_prov*prov
  w_gnn           = w_phase_histogram + w_phase_explicit (= 0.02 + 0.0 currently)
  manual_fused    = manual_base + w_phase_histogram * phase_histogram_norm
```

At `blend_alpha = 0.0`: identical to current behavior (no regression).
At `blend_alpha = 1.0`: GNN affinity replaces manual histogram boost.
`w_gnn` starts at the existing WA-2 weight (0.02) and may be tuned via config.

**Mode 1 and Mode 2 integration** (no search pipeline involvement):

For proactive injection and briefing, `compute_fused_score` is not called — scoring is done directly:

```
candidate_score = RelevanceScorer.forward(entry_features ++ session_ctx ++ [0.0, 0.0, 0.0])
```

This replaces the manual `confidence + co_access_affinity + phase_category_boost` formula in the WA-4 phase-transition cache.

---

## 5. Candidate Set Management — Proactive Mode

The WA-4 phase-transition cache (already implemented) rebuilds candidate scores at each phase transition. W3-1 replaces the scoring function in that cache:

```
On phase transition:
  session_ctx = build_session_context_vector(session_state, config)
  for entry_id in active_entries_for_topic_not_in_injection_history:
      entry_feat = build_entry_feature_vector(entry, graph_cache)
      score = RelevanceScorer.forward(entry_feat ++ session_ctx ++ [0.0, 0.0, 0.0])
      cache.insert(entry_id, score)

On hook event (draw from cache):
  top_candidate = cache.top_not_in_injection_history()
```

**Cache staleness**: Phase transitions are infrequent (typically 3-6 per feature cycle). Within a phase, the session_context vector changes only via new stores (category_counts) and hook events (injection_count, query_count). These changes are small. Rebuilding the full cache on each category_count increment is too expensive; rebuilding at phase transition is sufficient. Cache is also invalidated when injection_history grows by more than 5 entries.

**Candidate set size bound**: Active entries for a topic rarely exceed 50-100. A forward pass over 100 entries × 46 dim input × 5121 params is O(100 × 2 × 46 × 64 + 100 × 64 × 32 + 100 × 32) ≈ O(1M flops) — sub-millisecond on one CPU core. No rayon pool needed for Mode 1/2 scoring.

Mode 3 (search re-ranking) scores only 20 HNSW candidates — trivially fast.

---

## 6. Cold-Start and Fallback

```rust
impl GnnState {
    /// Blend alpha ramps from 0.0 to 1.0 over MIN..FULL training samples.
    pub fn blend_alpha(&self) -> f32 {
        let n = self.reservoir.len() as f32;
        if n < MIN_TRAIN_SIZE as f32 {
            0.0
        } else if n >= FULL_TRAIN_SIZE as f32 {
            1.0
        } else {
            (n - MIN_TRAIN_SIZE as f32) / (FULL_TRAIN_SIZE - MIN_TRAIN_SIZE) as f32
        }
    }
}
```

Constants (config-driven, not hardcoded):
- `MIN_TRAIN_SIZE = 50`: first training run threshold; below this, GNN is not trained and alpha = 0.0
- `FULL_TRAIN_SIZE = 150`: at this point alpha = 1.0 and GNN is fully trusted

At `alpha = 0.0` the codebase is bit-for-bit identical to pre-W3-1. No path changes required for the cold-start case — it is handled by the blend formula.

**Fallback on model load failure**: If the model file is missing, corrupt, or has a schema version mismatch, `GnnState::load()` returns `None` and `blend_alpha` returns 0.0. The system starts in manual-formula mode and accumulates training data. Once the model is trained, it is promoted to shadow, validated, then promoted to production via the existing `ModelRegistry` slot pattern.

---

## 7. Library and Infrastructure

**No new ML library needed.** The existing `unimatrix-learn` crate provides all required primitives:

| Need | Existing asset |
|---|---|
| Forward pass + backprop | `NeuralModel` trait (scorer.rs pattern) |
| Training data management | `TrainingReservoir<RelevanceSample>` |
| Continual learning | `EwcState` (prevents catastrophic forgetting as new sessions arrive) |
| Model versioning | `ModelRegistry` (shadow → production → previous) |
| Serialization | `bincode` + `save_atomic` / `load_file` |
| Reservoir sampling | `TrainingReservoir<T>` |

**New additions to `unimatrix-learn`**:
- `models/relevance_scorer.rs`: `RelevanceScorer` implementing `NeuralModel`
- `relevance_digest.rs`: `RelevanceDigest` — the input feature vector (replaces `SignalDigest` for this model)

**No changes to `unimatrix-adapt`**: That crate handles embedding adaptation; W3-1 is a scoring head.

---

## 8. Thread Safety and Hot Path

The `RelevanceScorer` (once loaded) is read-only during inference. It is stored behind `Arc<RwLock<Option<RelevanceScorer>>>` following the same pattern as `EmbedServiceHandle` and `NliServiceHandle`. Multiple concurrent sessions read the same model without contention.

Training modifies a separate `TrainingReservoir` and produces a new model file on the rayon pool. Model promotion uses `save_atomic` (temp file + rename) — the same atomic swap pattern already in `unimatrix-learn`.

During a training run (rayon pool, non-blocking), the serving model continues unchanged. Only after the training run completes and the new model is promoted to production does the `RwLock` acquire a write lock for the swap. Write lock hold time: model deserialize latency (~1ms for 20KB) — negligible.
