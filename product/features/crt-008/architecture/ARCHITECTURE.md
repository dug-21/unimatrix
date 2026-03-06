# Architecture: crt-008 Continuous Self-Retraining

## Overview

crt-008 adds three subsystems to the existing `unimatrix-learn` and `unimatrix-server` crates:

1. **Feedback-to-label pipeline** -- types and conversion logic that transform utilization events into typed training samples
2. **Training orchestration** -- `TrainingService` holding per-model reservoirs and EWC state, threshold-triggered fire-and-forget retraining via `spawn_blocking`
3. **Feedback capture hooks** -- integration points in existing MCP handlers that emit feedback signals to the training service

Plus a **Phase 0 trait refactor** that splits `NeuralModel::train_step` into `compute_gradients` + `apply_gradients` for clean EWC integration.

## Architecture Decisions

### ADR-001: NeuralModel Trait Split for Gradient Access

**Context:** crt-007 shipped `NeuralModel::train_step` as a monolithic method that computes gradients AND applies SGD in one call. EWC++ requires injecting `ewc.gradient_contribution(params)` into the gradient vector BETWEEN computation and weight update. The current trait does not expose gradients.

**Decision:** Add two new methods to `NeuralModel`:

```rust
/// Compute loss and gradients without updating weights.
/// Returns (loss, flat_gradient_vector).
fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>);

/// Apply a gradient vector to update weights.
fn apply_gradients(&mut self, gradients: &[f32], lr: f32);
```

`train_step` becomes a default implementation:

```rust
fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
    let (loss, grads) = self.compute_gradients(input, target);
    self.apply_gradients(&grads, lr);
    loss
}
```

**Implementation for each model:**
- `SignalClassifier`: Extract lines 149-183 (gradient computation) from `train_step` into `compute_gradients` returning `(loss, flat_grad_vec)`. Lines 186-191 (SGD update) become `apply_gradients`. Gradient vector uses same ordering as `flat_parameters()`: w1, b1, w2, b2, w3, b3.
- `ConventionScorer`: Same pattern, extracting lines 75-95 and 98-101.

**Rationale:**
- Clean EWC gradient injection: `compute_gradients()` -> add `ewc.gradient_contribution()` -> `apply_gradients(combined)`
- No approximation (post-step correction would be lossy)
- Default `train_step` preserves backward compatibility -- existing callers unchanged
- All future models (crt-009: Duplicate Detector, Pattern Merger, Entry Writer Scorer) get gradient access automatically

**Consequences:**
- ~10 lines refactored per model (extract, don't rewrite)
- All existing tests pass unchanged (they call `train_step` which delegates to the new methods)
- `flat_parameters()` ordering contract is now load-bearing for EWC correctness (previously only used for serialization)

### ADR-002: Flat Gradient Ordering Contract

**Context:** `EwcState::gradient_contribution(params)` returns a `Vec<f32>` that must be added element-wise to the gradient vector from `compute_gradients()`. Both vectors must use the same parameter ordering: layer-by-layer, weights then biases, row-major. crt-007 established this ordering in `flat_parameters()`/`set_parameters()` for serialization, but it was not explicitly a training contract.

**Decision:** The parameter ordering from `flat_parameters()` is the canonical ordering for ALL parameter-level operations: serialization, EWC penalty, EWC gradient contribution, gradient vectors from `compute_gradients()`, and gradient vectors consumed by `apply_gradients()`. This is documented as a trait-level invariant.

Additionally, a verification test asserts the identity: for any model `m`, calling `m.set_parameters(&m.flat_parameters())` produces identical predictions. This test runs for both `SignalClassifier` and `ConventionScorer`.

**Rationale:**
- EWC correctness depends on this alignment (SR-02 mitigation)
- Making it explicit prevents future models from accidentally using different orderings
- The verification test catches misalignment at test time, not at training time (when the symptom would be silent quality degradation)

**Consequences:**
- All `NeuralModel` implementations must maintain consistent ordering across `flat_parameters()`, `set_parameters()`, `compute_gradients()`, and `apply_gradients()`
- New models (crt-009) must include the ordering verification test

### ADR-003: Per-Model Training Lock

**Context:** `try_train_step` fires training as `spawn_blocking`. If two feedback signals arrive in quick succession and both exceed the reservoir threshold, two concurrent training tasks could clone the same model and both try to save as shadow (SR-04).

**Decision:** Each model in `TrainingService` has an `Arc<AtomicBool>` training lock. `try_train_step` checks the lock with `compare_exchange(false, true, SeqCst, SeqCst)`. If the lock is already held, the training attempt is skipped (samples remain in the reservoir). The `spawn_blocking` closure sets the lock back to `false` on completion (in a `Drop` guard to handle panics).

**Rationale:**
- Simple, zero-contention in the common case (training is rare: every 20+ signals)
- No mutex overhead -- atomic bool is the lightest synchronization primitive
- Skipped training is safe: samples persist in the reservoir and are included in the next training step
- Drop guard prevents lock leaks on panic

**Consequences:**
- At most one training task per model at any time
- If training takes longer than the interval between threshold crossings, samples accumulate and the next training step processes a larger effective batch
- No need for per-model mutex or channel

### ADR-004: Weighted Loss for Weak Labels

**Context:** Training samples have different confidence levels. Direct feedback (helpful vote, category correction) is high-confidence. Feature outcomes and stale entry detection are weak signals (weight 0.3). If all samples are treated equally, weak signals could dominate the training signal when they outnumber strong signals.

**Decision:** Each `TrainingSample` carries a `weight: f32` field. During training, the loss for each sample is multiplied by its weight before gradient computation. Strong signals (votes, corrections) have weight 1.0. Weak signals (feature outcomes, stale detection) have weight configurable via `LearnConfig.weak_label_weight` (default 0.3).

The weight is applied by scaling the target gradient: for the classifier, `da3 = weight * (a3 - target)`. For the scorer, `da2 = weight * (y - t)`. This is equivalent to scaling the loss but avoids an extra multiply per parameter in the gradient vector.

**Rationale:**
- Matches the confidence of the labeling signal to its influence on training
- Configurable weight allows tuning without code changes
- Single `weak_label_weight` value is sufficient initially -- per-signal-type weights can be added later if empirical data shows the need
- Gradient scaling is mathematically equivalent to loss scaling but more efficient

**Consequences:**
- `compute_gradients` signature may need a `weight` parameter, or the weight can be applied in `TrainingService` by scaling the gradient vector post-computation
- The effective batch contribution of weak samples is 30% of a strong sample

### ADR-005: Convention Follow/Deviate Detection via Observation Pattern Matching

**Context:** The convention scorer needs positive labels (convention followed) and negative labels (convention deviated from) to train. These signals come from matching observation data against convention entries. The maintenance tick needs to detect these patterns without semantic search (which would be expensive in a background tick).

**Decision:** Convention follow/deviate detection uses a lightweight approach:

1. Query all active entries with `trust_source` "auto" or "neural" and `category` "convention"
2. For each convention entry, extract its topic and tags
3. Query observations from the last N features for matching patterns:
   - **Followed**: observation data shows consistent behavior matching the convention description across all recent sessions (simple topic+tag match against observation tool calls)
   - **Deviated**: observation data shows a session where the convention topic was relevant (entry was injected) but behavior diverged (entry was accessed but followed by different actions)
4. If all sessions follow: positive label. If any session deviates without negative outcome: negative label.

This is approximate -- it uses metadata matching (topic, tags, injection history) rather than semantic content comparison. The approximation is acceptable because:
- Convention entries have structured metadata (topic, tags) from the extraction pipeline
- The injection log records which entries were delivered to which sessions
- Deviation detection only requires checking if injected convention entries correlate with session outcomes

**Rationale:**
- No semantic search in the maintenance tick (lightweight)
- Uses existing infrastructure: injection_log, observations table, convention entry metadata
- Approximation is acceptable for training labels (not for user-facing decisions)
- Falls back gracefully: if no follow/deviate signal is detected, the scorer relies on direct feedback only

**Consequences:**
- Convention scorer training may be slower to start than classifier training (fewer signals)
- Topic+tag matching may miss subtle deviations that only semantic analysis would catch
- The detection logic lives in the `TrainingService` (not in the maintenance tick itself), called by the maintenance tick

### ADR-006: Ground Truth Backfill Scope

**Context:** The `shadow_evaluations` table has a `ground_truth` column that is `NULL` when the evaluation is logged. When feedback arrives later, we can retroactively fill it. But not all feedback signals are appropriate as ground truth -- some are ambiguous (SR-06).

**Decision:** Only two signal types backfill `ground_truth`:

1. **Category correction** (`context_correct` changing an entry's category): The new category IS the ground truth for that entry's classification. Backfill: `ground_truth = new_category`.
2. **Consistent multi-vote**: When an entry accumulates 3+ unhelpful votes with 0 helpful votes, backfill `ground_truth = "noise"` (the entry should not have been extracted).

Single helpful/unhelpful votes, deprecations, and feature outcomes do NOT backfill ground truth. They contribute to training via the reservoir but do not retroactively change shadow evaluation accuracy metrics.

**Rationale:**
- Category correction is unambiguous: a human explicitly said "this should have been classified as X"
- Consistent multi-vote (3+ unhelpful, 0 helpful) is a strong signal that extraction was wrong
- Single votes are noisy (user may have voted on content quality, not classification correctness)
- Deprecation is ambiguous (could be staleness, not misclassification)
- Conservative backfill avoids corrupting accuracy metrics that drive promotion decisions

**Consequences:**
- Shadow evaluation accuracy metrics are based on a subset of evaluations (those with ground truth)
- Promotion decisions use ground-truth-based accuracy when available, rule-prediction-agreement accuracy otherwise
- The backfill UPDATE query matches on `entry_id` (joining shadow_evaluations to entries via the signal_digest or feature_cycle)

## Component Diagram

```
crates/unimatrix-learn/  (EXTENDED)
  |-- models/
  |   |-- traits.rs              (REFACTORED: add compute_gradients + apply_gradients)
  |   |-- classifier.rs          (REFACTORED: extract gradient computation from train_step)
  |   |-- scorer.rs              (REFACTORED: extract gradient computation from train_step)
  |   |-- digest.rs              (unchanged)
  |-- training.rs                (NEW: TrainingSample, TrainingTarget, FeedbackSignal, LabelGenerator)
  |-- service.rs                 (NEW: TrainingService with per-model reservoirs + EWC + orchestration)
  |-- config.rs                  (EXTENDED: training thresholds, EWC params, weak label weight)
  |-- ewc.rs                     (unchanged -- already provides gradient_contribution + update_from_flat)
  |-- reservoir.rs               (unchanged -- generic TrainingReservoir<T> already works)
  |-- registry.rs                (unchanged -- promote/rollback already implemented)
  |-- persistence.rs             (unchanged)

crates/unimatrix-server/  (EXTENDED)
  |-- services/usage.rs          (HOOKED: emit FeedbackSignal on helpful/unhelpful for auto entries)
  |-- mcp/correct.rs             (HOOKED: emit FeedbackSignal on category correction for auto entries)
  |-- mcp/deprecate.rs           (HOOKED: emit FeedbackSignal on deprecation for auto entries)
  |-- background.rs              (HOOKED: emit FeedbackSignal from maintenance tick for stale/convention signals)
  |-- mcp/store.rs               (HOOKED: emit FeedbackSignal on outcome recording for feature entries)
```

## Data Flow

### Feedback Capture (per MCP handler invocation)

```
1. Handler executes normal operation (record vote, correct entry, deprecate, etc.)
2. After success: check entry trust_source
   - If trust_source NOT in ["auto", "neural"]: return (no training signal)
   - If trust_source in ["auto", "neural"]: continue
3. Build FeedbackSignal from event context:
   - HelpfulVote { entry_id, category, signal_digest }
   - UnhelpfulVote { entry_id, category, signal_digest }
   - CategoryCorrection { entry_id, old_category, new_category, signal_digest }
   - Deprecation { entry_id, category, signal_digest }
   - FeatureOutcome { feature_cycle, result, entry_ids }
4. Send signal to TrainingService via Arc<TrainingService> on server state
   - TrainingService.record_feedback(signal) is non-blocking
   - Internally: generate labels, add to reservoirs, check thresholds
```

### Training Step (fire-and-forget, per threshold crossing)

```
1. TrainingService detects reservoir.len() >= threshold for model X
2. Check training lock (AtomicBool): if locked, skip
3. Acquire lock, clone model state + EWC state + reservoir batch
4. spawn_blocking:
   a. For each sample in batch:
      - params = model.flat_parameters()
      - (loss, grads) = model.compute_gradients(input, target)
      - ewc_grads = ewc.gradient_contribution(params)
      - combined = grads.zip(ewc_grads).map(|(g, e)| g + e)
      - Scale combined by sample.weight (if not 1.0)
      - model.apply_gradients(combined, lr)
   b. Final params = model.flat_parameters()
   c. Check for NaN/Inf -> if found, discard, release lock, return
   d. ewc.update_from_flat(params, accumulated_grad_squared)
   e. Save model via serialize() + save_atomic() as shadow version
   f. Update ModelRegistry: install shadow
   g. Release training lock
5. On next extraction tick: shadow model runs alongside production
6. After shadow_min_evaluations: check_promotion()
   - Overall accuracy >= production
   - Per-class accuracy: no class drops > 10%
   - If pass: promote(). If fail: discard shadow.
```

### Ground Truth Backfill (on category correction)

```
1. context_correct handler processes correction for auto/neural entry
2. FeedbackSignal::CategoryCorrection emitted
3. TrainingService generates training label AND triggers backfill:
   UPDATE shadow_evaluations
   SET ground_truth = ?1
   WHERE model_name = 'signal_classifier'
     AND neural_prediction != ?1
     AND ground_truth IS NULL
     AND feature_cycle = ?2
```

## Integration Points

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `unimatrix-learn/models/traits.rs` | Trait extension | Add `compute_gradients`, `apply_gradients`; `train_step` becomes default impl |
| `unimatrix-learn/models/classifier.rs` | Refactor | Extract gradient code from `train_step` into new methods (~10 lines moved) |
| `unimatrix-learn/models/scorer.rs` | Refactor | Extract gradient code from `train_step` into new methods (~10 lines moved) |
| `unimatrix-learn/training.rs` | New file | `TrainingSample`, `TrainingTarget`, `FeedbackSignal`, `LabelGenerator` |
| `unimatrix-learn/service.rs` | New file | `TrainingService` orchestration |
| `unimatrix-learn/config.rs` | Extension | Training thresholds, EWC params, weak label weight, learning rate |
| `unimatrix-server/services/usage.rs` | Hook | Emit feedback signal on helpful/unhelpful vote for auto/neural entries |
| `unimatrix-server/mcp/correct.rs` | Hook | Emit feedback signal + ground truth backfill on category correction |
| `unimatrix-server/mcp/deprecate.rs` | Hook | Emit feedback signal on deprecation of auto/neural entries |
| `unimatrix-server/background.rs` | Hook | Emit feedback signals from maintenance tick (stale entries, convention patterns) |
| `unimatrix-server/mcp/store.rs` | Hook | Emit feedback signal on outcome recording |
| `unimatrix-server/lib.rs` | Wire | Add `Arc<TrainingService>` to server state |

## Risk Mitigations (from Scope Risk Assessment)

| Risk | Architectural Mitigation |
|------|-------------------------|
| SR-01 (Trait split breaks consumers) | ADR-001: `train_step` becomes default impl calling new methods. Existing callers unchanged. |
| SR-02 (EWC gradient ordering) | ADR-002: Canonical ordering contract documented as trait invariant. Verification test asserts identity round-trip. |
| SR-03 (Sparse feedback) | ADR-005: Convention detection adds bulk labels. Config thresholds are tunable. Feature outcomes generate multi-entry labels. |
| SR-04 (Training race conditions) | ADR-003: Per-model AtomicBool lock with Drop guard. At most one training task per model. |
| SR-05 (Trust source filtering) | Central `FeedbackCapture` function with trust_source check. Unit tests verify filtering. |
| SR-06 (Ground truth ambiguity) | ADR-006: Only category corrections and consistent multi-votes backfill ground truth. |
| SR-07 (Convention detection complexity) | ADR-005: Lightweight topic+tag matching. Falls back to direct feedback only if detection not feasible. |
