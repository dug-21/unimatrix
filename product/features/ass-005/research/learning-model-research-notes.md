# Learning Model Research Notes

**Date**: 2026-02-20
**Purpose**: Raw findings and notable details from Track 1C research

---

## Sona Crate Identity

- **Published as**: `ruvector-sona` on crates.io (NOT standalone `sona`)
- **Version**: 0.1.5 (published 2026-02-08)
- **Code size**: 7,404 lines of Rust across 27 files (v0.1.5), up from ~2,942 at v0.1.0
- **Total downloads**: 939
- **Dependencies**: parking_lot, crossbeam, rand, optional serde/serde_json/wasm-bindgen/napi
- **Source**: github.com/ruvnet/ruvector, crates/sona/

---

## ReasoningBank: What's Actually Implemented vs. What's Planned

### Critical Finding: Gap Between Plan and Code

The ruvector-plan.md describes features that do NOT exist in the actual sona source code:

| Claimed Feature | Actual Status |
|----------------|---------------|
| Short-term to long-term promotion | NOT IMPLEMENTED — no state field, no promotion logic |
| Minimum 3 uses before promotion | NOT IMPLEMENTED — no use-count threshold |
| Quality threshold 0.7 for promotion | NOT IMPLEMENTED — only 0.3 threshold for cluster filtering |
| Bug fix auto-promote on 2nd success | NOT IMPLEMENTED |
| Architecture decisions manual-only | NOT IMPLEMENTED |
| Confidence decay -0.05/week | NOT IMPLEMENTED automatically; `decay(factor)` exists but never called |
| Dedup similarity 0.92 | NOT IMPLEMENTED as default; `consolidate()` takes threshold as param |

These are **design aspirations**, not implemented features.

### Actual ReasoningBank API

There is **NO `store_pattern()` method**. Instead:
```rust
pub fn add_trajectory(&mut self, trajectory: &QueryTrajectory)
// Trajectories accumulate, then:
pub fn extract_patterns(&mut self) -> Vec<LearnedPattern>  // K-means++ clustering
pub fn find_similar(&self, query: &[f32], k: usize) -> Vec<&LearnedPattern>  // linear scan
```

Pattern search is a **brute-force linear scan** over cosine similarity — no ANN index. Works for 100-200 patterns but would need replacement at scale.

### LearnedPattern Struct

```rust
pub struct LearnedPattern {
    pub id: u64,
    pub centroid: Vec<f32>,        // cluster centroid embedding
    pub cluster_size: usize,
    pub total_weight: f32,         // sum of member qualities
    pub avg_quality: f32,          // mean quality of members
    pub created_at: u64,           // unix timestamp
    pub last_accessed: u64,
    pub access_count: u32,
    pub pattern_type: PatternType, // General|Reasoning|Factual|Creative|CodeGen|Conversational
}
```

No `status` field. No `supersedes`/`superseded_by`. No correction tracking.

---

## Trajectory Model: Deeper Than Expected, Less Useful Than Claimed

### What TrajectoryStep Actually Contains

```rust
pub struct TrajectoryStep {
    pub activations: Vec<f32>,        // arbitrary float vectors (NOT from a neural network)
    pub attention_weights: Vec<f32>,  // arbitrary float vectors
    pub reward: f32,                  // caller-provided reward signal
    pub step_idx: usize,
    pub layer_name: Option<String>,
}
```

Despite naming ("activations", "attention_weights"), these are **generic float containers**. No neural network produces these values. The caller must provide them externally. This is a critical design mismatch — the API was designed for LLM inference routing, not development knowledge management.

### The Reward Signal Problem

The entire system depends on `quality_score` and per-step `reward` values provided by the caller. There is **no internal quality evaluation**. If the caller provides random or constant rewards, the entire learning pipeline produces noise. For a development knowledge system, the question becomes: who provides the reward? The human? Claude? There's no natural reward signal for "was this pattern useful?"

---

## LoRA: Real Math, Wrong Application

### Two-Tier System

| Tier | Rank | Update Frequency | Purpose |
|------|------|-----------------|---------|
| MicroLoRA | 1-2 | Per-request (<1ms) | Instant adaptation |
| BaseLoRA | 4-16 | Hourly background | Long-term adaptation |

### What's Being Adapted (Nothing)

In standard LoRA, the adapter modifies existing weight matrices in a pretrained model: `output = W*x + scale * B*A*x`. Here, there is **no base model weight W**. The LoRA forward pass computes `output += scale * B * A * input` and adds it to whatever buffer you pass. The "adaptation" is a free-standing learned linear transform on embedding vectors with no base model to adapt.

The `find_similar()` pattern lookup in ReasoningBank uses raw cosine similarity on centroids — **the LoRA transform is not involved in pattern lookup at all**.

### MicroLoRA Update Mechanics

- Only `up_proj` (B matrix) receives gradient updates
- `down_proj` (A matrix) is never updated after initialization
- Gradients come from REINFORCE estimation: `gradient = (reward - baseline) * activations`
- With arbitrary inputs and single-sample estimates, this gradient is almost pure noise

### AVX2 SIMD

Real and correctly implemented for the matrix-vector multiply. But at rank 1-2 with dim 256, this is a 256-element dot product — trivial computation. The SIMD optimization is technically correct but irrelevant at this scale.

---

## EWC++: Solving a Non-Problem

### What It Does

Prevents "catastrophic forgetting" by attenuating gradient updates to parameters that were important for past tasks. Uses:
- Online Fisher diagonal estimate via EMA: `F_t = decay * F_{t-1} + (1-decay) * g^2`
- Task boundary detection via gradient distribution z-score (threshold: 2.0)
- Constraint application: `constrained_gradient[i] *= 1.0 / (1.0 + lambda * fisher[i])`
- Lambda default: 2000 (very aggressive regularization)

### Why It's Irrelevant for Knowledge Stores

Catastrophic forgetting is a concern for neural networks with millions of parameters. Here:
- The "parameters" are a rank-2 LoRA adapter with `256 * 2 * 2 = 1024` floats
- The update signal is extremely weak (noisy REINFORCE gradients)
- Adding EWC++ with lambda=2000 on already-noisy, barely-moving 1024 parameters is over-engineering
- Metadata databases DO NOT have catastrophic forgetting — old entries persist until explicitly removed

---

## K-means++ Clustering Details

### Initialization

First centroid: deterministically index 0 (not random). Subsequent centroids: always picks farthest point (simplified D-squared, deterministic, not probabilistic).

### Trigger Conditions

- Timer: hourly (3600 seconds default)
- Minimum: 100 buffered trajectories
- OR: `force_learn()` called manually

### Pattern Filtering

- Clusters below `min_cluster_size` (5) discarded
- Clusters below `quality_threshold` (0.3 avg quality) discarded

### Consolidation (NOT Automatic)

`consolidate(similarity_threshold)` is never called during the background cycle. Must be invoked manually. No default threshold — caller provides it.

---

## Production Memory Tools Landscape

### What Mem0 Actually Does (No ML)

- GPT-4o-mini function calls at write time to extract facts and choose operations (ADD/UPDATE/DELETE/NOOP)
- Zero custom neural networks, zero fine-tuning
- No temporal decay, no lifecycle management
- $24M raised, 97K GitHub stars
- The "AI memory" market leader uses zero ML for memory management

### What Zep/Graphiti Does (Metadata + LLM at Write Time)

Most sophisticated production memory system:
- Bi-temporal metadata: `t_valid`/`t_invalid` (truth time) + `t'_created`/`t'_expired` (system time)
- LLM-based conflict detection at write time
- 18.5% accuracy improvement over baselines
- Power comes from temporal metadata and graph structure, not trained models

### MCP Memory Servers (Primitive)

All use simple data structures (JSON, JSONL, knowledge graphs). None use ML. None have lifecycle management. None handle corrections. The market gap is metadata lifecycle + correction tracking, not neural networks.

---

## Key Comparative Metrics

### Complexity Cost

| Approach | Lines of Code | New Dependencies | Maintenance |
|----------|--------------|-----------------|-------------|
| Sona ML | ~7,500 | Heavy ML framework | Ongoing retraining, hyperparameter tuning, model versioning |
| Metadata lifecycle | ~930 | None (already in stack) | Configuration tuning only |

### Confidence Score Computation

| ML approach | Metadata approach |
|-------------|------------------|
| LoRA + EWC++ + trajectory rewards | Wilson score + exponential decay + correction penalty |
| Seconds-to-minutes GPU time | Microseconds CPU |
| Black box, requires model introspection | Fully explainable |
| Undertrained at <10K entries | Works at any scale |

---

## Interface Forward-Compatibility

The critical insight: **the MCP tool interface is identical for both approaches**. The `confidence` field is a float [0.0, 1.0]. The client does not know or care whether it came from a formula or a neural network. If ML proves necessary at scale (100K+ entries, 5+ projects), it can be added behind the same interface with zero breaking changes.

---

## Gaps Requiring Future Investigation

| Gap | How to Resolve | Priority |
|-----|---------------|----------|
| Optimal Wilson score parameters for dev knowledge | A/B test after shipping with initial defaults | Low |
| Freshness half-life tuning per knowledge category | Monitor usage patterns post-launch | Low |
| Whether LLM-at-write-time generalization (mem0 pattern) is worth the API cost | Phase 2 evaluation after correction patterns emerge | Medium |
| Cross-project transfer accuracy with metadata-only approach | Evaluate once 3+ projects exist with overlapping knowledge | Low |
| Dedup similarity threshold optimization (0.90 vs 0.92 vs 0.95) | Benchmark with real development knowledge entries | Low |

These are implementation-level details, not blockers for interface design.
