# ASS-015: Self-Learning Neural Architecture — ruv-fann + Continuous Retraining

## The Vision

Unimatrix bundles purpose-built neural models via ruv-fann that continuously retrain from usage signals. No external API dependency. The knowledge base learns to extract better knowledge from its own utilization patterns. Fully self-contained self-learning.

## The Revelation: unimatrix-adapt Already Does This

The `unimatrix-adapt` crate (crt-006, COMPLETE) implements continuous self-retraining for the embedding space:

| Mechanism | What It Does | How It Learns |
|-----------|-------------|---------------|
| MicroLoRA | Adapts ONNX embeddings to project-specific domain | Co-access pairs as contrastive signal |
| EWC++ | Prevents catastrophic forgetting during updates | Online Fisher diagonal accumulation |
| Reservoir sampling | Memory-bounded training buffer | 512-pair capacity with statistical sampling |
| Domain prototypes | Per-category/topic centroids for soft pull | Running mean, LRU eviction |
| Fire-and-forget training | Non-blocking incremental updates | Triggers when reservoir >= batch_size |
| Persistence | Versioned binary state across restarts | Atomic write, generation tracking |

**This infrastructure is directly reusable.** The same patterns — reservoir sampling for memory-bounded training data, EWC++ for forgetting prevention, fire-and-forget training on threshold triggers, versioned persistence — apply to extraction neural models.

---

## Architecture: Five Neural Models, Continuously Retrained

### Model 1: Signal Classifier

**Purpose:** Classify incoming signal digests → `convention | pattern | gap | dead | noise`

**Architecture:**
- ruv-fann MLP: input(N) → hidden(64, sigmoid-symmetric) → hidden(32) → output(5, softmax)
- Input: structured signal features (search miss count, co-access density, consistency score, feature count, etc.)
- Output: category probability distribution
- Size: ~5MB

**Training Signal (from Unimatrix utilization):**
- Positive labels: auto-extracted entries that receive helpful votes → that classification was correct
- Negative labels: auto-extracted entries deprecated within 10 features → misclassification
- Implicit labels: entries never accessed → classification may be wrong (weak negative)
- Correction labels: entry moved to different category via `context_correct` → ground truth for that input

**Retraining trigger:** Every 20 classified signals, or when accuracy drift detected
**Retraining time:** <5 seconds on CPU (small MLP, structured input)

---

### Model 2: Duplicate/Similarity Detector

**Purpose:** Determine if a proposed entry duplicates, extends, or contradicts an existing entry

**Architecture:**
- Siamese network on MiniLM embeddings (reuses unimatrix-embed vectors)
- ruv-fann MLP: input(768 = concat of two 384-dim embeddings) → hidden(128) → hidden(64) → output(3: duplicate/extend/distinct)
- Size: ~10MB

**Training Signal:**
- Positive (duplicate): correction chains where new entry supersedes old → those were duplicates
- Positive (extend): entries in same topic with high co-access → related, possibly extending
- Positive (distinct): entries in different topics, low co-access → distinct
- Negative: false duplicates that human promoted to active → detector was wrong

**Retraining trigger:** Every 10 correction events, or monthly
**Retraining time:** <10 seconds on CPU

---

### Model 3: Convention Scorer

**Purpose:** Score whether a pattern observed across features is a genuine convention

**Architecture:**
- ruv-fann MLP: input(M) → hidden(32) → output(1, sigmoid = convention probability)
- Input features: consistency_ratio, feature_count, deviation_count, category_distribution, age_days
- Output: 0.0-1.0 convention confidence
- Size: ~2MB

**Training Signal:**
- Positive: conventions that accumulate helpful votes and high access counts → confirmed conventions
- Negative: conventions that get deprecated or corrected → false conventions
- Negative: conventions with high deviation rate (agents successfully ignore them) → not real conventions

**Retraining trigger:** Every 5 convention evaluations
**Retraining time:** <2 seconds on CPU

---

### Model 4: Pattern Merger

**Purpose:** Decide whether N similar signal observations should merge into one entry, and which fields to keep

**Architecture:**
- Encoder: ruv-fann MLP that produces a fixed-size representation per observation
- Merger: MLP that takes concatenated representations → merge decision + field selection weights
- Size: ~50MB

**Training Signal:**
- Positive merges: entries that were manually merged via `context_correct` (supersedes chains)
- Negative merges: proposed merges where human created separate entries instead
- Quality signal: merged entries that subsequently get high access → good merge

**Retraining trigger:** Every 10 merge decisions
**Retraining time:** ~30 seconds on CPU

---

### Model 5: Entry Writer

**Purpose:** Generate coherent knowledge entry text from structured signal digest

**Architecture:**
- This is the hardest model. Two options:

**Option A — Template + Neural Polish (Recommended for v1):**
- Rule-based template fills structured fields (title, topic, category, core content)
- ruv-fann MLP scores template quality (coherence, completeness, specificity)
- If score < threshold, regenerate with different template
- Size: ~20MB (scorer only)
- Retraining: scorer learns from helpful/unhelpful votes on generated entries

**Option B — Full Sequence Generator (v2, if needed):**
- Small transformer or RNN decoder conditioned on structured input
- Requires more training data (all existing entries as training corpus)
- Size: ~100-200MB
- Retraining: fine-tune on entries with highest helpfulness scores

**Training Signal:**
- Helpful votes on generated entries → good text quality
- Unhelpful votes → bad text quality
- Access count → relevance of generated content
- Correction events → human showed what the text should have been

**Retraining trigger:** Every 10 generated entries with feedback
**Retraining time:** ~30 seconds (option A), ~5-15 minutes (option B)

---

## Continuous Retraining Architecture

### The Self-Learning Loop

```
Agent uses Unimatrix (search, briefing, get)
       |
       v
  Passive signal capture (JSONL append)
       |
       v
  Signal digest (rules compress raw signals)
       |
       v
  Neural extraction pipeline (5 models)
       |
       +──→ Proposed entries stored in KB
       |
       v
  Agents interact with proposed entries
       |
       +──→ Helpful votes, access patterns, corrections, deprecations
       |
       v
  TRAINING LABELS (derived from utilization)
       |
       v
  Continuous retraining (fire-and-forget, background)
       |
       v
  Models improve → better extraction → better entries → more useful votes
       |
       v
  (loop continues)
```

### Label Generation from Utilization

The critical insight: **Unimatrix's existing quality signals ARE training labels.**

| Utilization Event | Training Label | For Which Model(s) |
|-------------------|---------------|---------------------|
| Entry gets helpful vote | Positive for classification + writer quality | Classifier, Writer |
| Entry gets unhelpful vote | Negative for classification + writer quality | Classifier, Writer |
| Entry never accessed (10+ features) | Weak negative for classification | Classifier, Convention Scorer |
| Entry corrected (category changed) | Ground truth re-labeling | Classifier |
| Entry corrected (content replaced) | Positive merge example | Merger, Duplicate Detector |
| Entry deprecated | Negative for whatever produced it | Classifier, Convention Scorer |
| Entry in correction chain | Duplicate/extend training pair | Duplicate Detector |
| High co-access pair | Related entries signal | Duplicate Detector, Merger |
| Convention followed by all agents | Positive convention label | Convention Scorer |
| Convention deviated from successfully | Negative convention label | Convention Scorer |
| Feature outcome: success | Positive for entries injected in that feature | Classifier |
| Feature outcome: rework | Weak negative for entries injected | Classifier |

### Retraining Infrastructure (Extending unimatrix-adapt Patterns)

```rust
/// Mirrors AdaptationService pattern from unimatrix-adapt
pub struct ExtractionService {
    /// Neural models
    classifier: Arc<RwLock<ClassifierModel>>,
    dedup_detector: Arc<RwLock<DedupModel>>,
    convention_scorer: Arc<RwLock<ConventionModel>>,
    merger: Arc<RwLock<MergerModel>>,
    writer_scorer: Arc<RwLock<WriterModel>>,

    /// Training infrastructure (mirrors adapt crate)
    training_reservoirs: HashMap<ModelId, TrainingReservoir>,
    ewc_states: HashMap<ModelId, EwcState>,

    /// Model versioning
    registry: ModelRegistry,

    /// Config
    config: ExtractionConfig,
}

impl ExtractionService {
    /// Record a training signal (non-blocking)
    pub fn record_feedback(&self, entry_id: u64, signal: FeedbackSignal) {
        // Convert feedback to training pairs for relevant models
        // Add to appropriate reservoir(s)
        // Check if any reservoir >= batch_size → trigger training
    }

    /// Fire-and-forget training step (mirrors adapt's try_train_step)
    pub fn try_train_step(&self, model_id: ModelId) {
        if self.training_reservoirs[&model_id].len() >= self.config.batch_size {
            tokio::task::spawn_blocking(move || {
                // Sample batch from reservoir
                // Forward pass through current model
                // Compute loss + EWC penalty
                // Backward pass + weight update
                // Update EWC state
                // Increment generation
                // Debounced save (every N steps)
            });
        }
    }

    /// Shadow mode: run new model version in parallel, compare
    pub fn shadow_evaluate(&self, model_id: ModelId, input: &SignalDigest) -> ShadowResult {
        let production = self.registry.production(model_id).predict(input);
        let shadow = self.registry.shadow(model_id).predict(input);
        ShadowResult { production, shadow, divergence: distance(production, shadow) }
    }
}
```

### Retraining Schedule

| Model | Trigger | Batch Size | CPU Time | Frequency (est.) |
|-------|---------|-----------|----------|-------------------|
| Classifier | 20 classified signals with feedback | 16 | <5s | Every 2-3 features |
| Duplicate Detector | 10 correction events | 8 | <10s | Every 5-10 features |
| Convention Scorer | 5 convention evaluations with outcome | 4 | <2s | Every 3-5 features |
| Merger | 10 merge decisions with feedback | 8 | ~30s | Every 10 features |
| Writer Scorer | 10 generated entries with votes | 8 | ~30s | Every 5 features |

**All retraining is incremental** (warm-start from current weights, not from scratch). The EWC++ regularization from unimatrix-adapt prevents catastrophic forgetting.

### Cold-Start Strategy

When Unimatrix has zero training data (new project):

1. **Models start with hand-tuned baseline weights** (not random)
   - Classifier: bias toward `noise` class (conservative — don't extract until confident)
   - Convention Scorer: low scores (require strong evidence before classifying as convention)
   - Duplicate Detector: bias toward `distinct` (conservative — don't merge prematurely)
   - Writer Scorer: permissive (accept most templates initially, refine from feedback)

2. **First 5 features: observation-only mode**
   - Rules (Tier 1) extract high-confidence patterns
   - Neural models observe but don't extract (shadow mode)
   - Training data accumulates from rule-based extractions + agent feedback

3. **After 5 features: neural models activate**
   - Enough training data for initial incremental update
   - Shadow mode continues for 2 more features (model runs but results not stored)
   - After shadow validation: promote to production

4. **Ongoing: continuous improvement**
   - Every feature adds ~5-20 training signals per model
   - After 20+ features: models are well-calibrated for the project's domain

### Model Versioning and Rollback

Following the shadow mode pattern from self-retraining research:

```
                                    Quality
Model Version     State             Metric     Notes
─────────────     ──────            ──────     ─────
v0 (baseline)     archived          0.65       Hand-tuned initial weights
v1                archived          0.71       After 5 features of training
v2                previous          0.78       After 10 features
v3                production  ←──   0.82       Current
v4                shadow      ←──   ?          Training from recent data

v4 promotion criteria:
  - shadow accuracy >= production accuracy (on held-out validation)
  - no regression on any category
  - minimum 20 shadow evaluations

v4 auto-rollback if:
  - accuracy drops > 5% after promotion
  - any category accuracy drops > 10%
  - NaN/Inf in weights detected
```

---

## Total Footprint

| Component | Size | RAM (inference) | RAM (training) |
|-----------|------|-----------------|----------------|
| Signal Classifier | ~5MB | ~10MB | ~20MB |
| Duplicate Detector | ~10MB | ~15MB | ~30MB |
| Convention Scorer | ~2MB | ~5MB | ~10MB |
| Pattern Merger | ~50MB | ~60MB | ~120MB |
| Writer Scorer | ~20MB | ~25MB | ~50MB |
| **Total** | **~87MB** | **~115MB** | **~230MB** |

Compare to:
- Current unimatrix-adapt MicroLoRA: ~50KB state + negligible RAM
- Bundled small LLM (3B Q4): ~2GB disk, ~3GB RAM
- MiniLM embedding model: ~90MB disk, ~200MB RAM

The neural models add **~87MB disk and ~115MB inference RAM** — comparable to the embedding model already bundled. Training peaks at ~230MB during incremental updates, well within any development machine.

---

## Integration with ruv-fann

ruv-fann (v0.2.0) provides:
- Pure Rust neural network primitives (no unsafe, no C FFI)
- RPROP training (well-suited for small batch incremental updates)
- Sigmoid-symmetric activation (good for hidden layers)
- Model save/load with metadata
- Parallel computation via rayon (optional)
- MIT/Apache-2.0 licensed

**What we use from ruv-fann:**
- MLP construction (layer sizes, activations)
- Forward pass (inference)
- RPROP training step (incremental weight updates)
- Model serialization (save/load with version metadata)

**What we build on top (following unimatrix-adapt patterns):**
- Training reservoir (reservoir sampling, memory-bounded)
- EWC++ regularization (from unimatrix-adapt, directly reusable)
- Model registry (production/shadow/previous versioning)
- Feedback-to-label pipeline (utilization events → training pairs)
- Fire-and-forget training trigger (threshold-based, non-blocking)

---

## The Lesson Extraction Boundary

Explicitly excluded from neural models: **extracting lessons from failures.**

Why:
- Requires multi-hop causal reasoning across long traces (10-30K tokens)
- "The gate failed because the agent didn't read the schema migration before editing the store"
- No small model or specialized neural net handles this well
- Agents already do this during retrospectives (uni-retro-scrum-master + human)

This boundary is clean and permanent. The neural pipeline handles the structural fabric of knowledge (conventions, patterns, gaps, dependencies). The wisdom layer (lessons, decisions, nuanced understanding) remains agent-driven.

---

## Self-Learning Timeline

| Project Maturity | Neural Pipeline State | Extraction Quality |
|-----------------|----------------------|-------------------|
| Features 1-3 | Observation only, rules extract | Rules only — conservative |
| Features 4-5 | Shadow mode, models training but not extracting | Rules + validation data accumulating |
| Features 6-10 | Models activated, low confidence threshold | Rules + neural — improving |
| Features 11-20 | Models well-calibrated, confidence rising | Full pipeline — good |
| Features 21-50 | Models refined through continuous retraining | Full pipeline — great |
| Features 50+ | Models deeply domain-adapted, high precision | Self-sustaining knowledge fabric |

**The system gets better the more you use it.** Not through external intervention, but through its own utilization patterns. This is the self-learning vision completed.

---

## Open Questions

1. **ruv-fann maturity:** v0.2.0 with ~4K downloads. Need to validate RPROP implementation and model persistence against our test suite. If insufficient, fall back to ndarray + hand-rolled training (following unimatrix-adapt's approach).

2. **Writer model architecture:** Template + scorer (v1) vs sequence generator (v2). The template approach is simpler, more predictable, and retrainable from fewer examples. Recommend starting there.

3. **Minimum viable training data:** How many features before the classifier outperforms random baseline? Hypothesis: 5 features (based on ~100 signal digests with ~20% feedback rate = 20 labeled examples). Needs empirical validation.

4. **Interaction with unimatrix-adapt:** The MicroLoRA adaptation changes the embedding space. Neural models that use embeddings (Duplicate Detector, Merger) must use adapted embeddings, not raw ones. This creates a dependency: when adaptation weights update, similarity-based models may need recalibration.

5. **EWC++ sharing:** Should extraction models share EWC state with the MicroLoRA adaptation, or maintain independent EWC state? Independent is safer (no cross-contamination) but duplicates infrastructure.
