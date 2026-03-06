# crt-008: Continuous Self-Retraining

## Problem Statement

crt-007 (Neural Extraction Pipeline) ships two neural models -- Signal Classifier and Convention Scorer -- with hand-tuned baseline weights. These models classify signal digests and score convention confidence but cannot improve from experience. The models observe utilization patterns during shadow mode but the observations never feed back into training. The self-learning loop remains open: models are static while the domain they classify evolves with every feature delivered.

Unimatrix already captures the signals needed to close this loop. Helpful/unhelpful votes (crt-001), correction chains (crt-003), deprecation events, co-access patterns (crt-004), feature outcomes (col-001), and shadow evaluation logs (crt-007) all encode ground truth about model performance. These signals are scattered across existing subsystems with no pipeline to convert them into training labels, feed them into reservoirs, trigger retraining, or validate improvements before promotion.

crt-008 bridges this gap: it builds the feedback-to-label pipeline that converts utilization events into typed training samples, routes them to per-model `TrainingReservoir<T>` instances, triggers fire-and-forget incremental training via EWC++ regularization when thresholds are met, and validates retrained models through shadow mode before promotion. The shared training infrastructure (`TrainingReservoir`, `EwcState`, `ModelRegistry`) from `unimatrix-learn` (crt-007) provides the foundation; crt-008 wires the feedback capture points and training orchestration on top.

## Goals

1. Build a feedback-to-label pipeline that converts 9 utilization event types into typed training samples for Signal Classifier and Convention Scorer
2. Hook into existing MCP handlers (`context_store` helpful/unhelpful, `context_correct`, `context_deprecate`) and background maintenance to capture training signals for auto-extracted entries
3. Route training labels to per-model `TrainingReservoir<TrainingSample>` instances in `unimatrix-learn`
4. Implement threshold-triggered retraining: Classifier retrains every 20 labeled signals (batch 16), Convention Scorer every 5 labeled evaluations (batch 4)
5. Apply EWC++ regularization during training to prevent catastrophic forgetting of earlier patterns
6. Execute retraining as fire-and-forget `spawn_blocking` tasks, non-blocking to the MCP server
7. After retraining, install the new model as shadow; validate via existing shadow mode infrastructure (crt-007) before promotion
8. Backfill `ground_truth` column in `shadow_evaluations` table when feedback arrives for previously-evaluated entries
9. Add NaN/Inf weight detection as an auto-rollback trigger (complementing crt-007's accuracy-based rollback)
10. Add per-category accuracy regression detection (>10% drop in any single class triggers rollback)

## Non-Goals

- **Duplicate Detector, Pattern Merger, Entry Writer Scorer training** -- deferred to crt-009 (those models do not yet exist)
- **New MCP tools** (e.g., `context_review` for human review of proposals) -- deferred to crt-009
- **Drift detection** (KS test, PSI, Page-Hinkley on input distribution) -- deferred; threshold-triggered retraining is sufficient for current signal volumes
- **Self-training / pseudo-labeling** -- crt-008 uses only confirmed labels from utilization events, not model self-predictions
- **Curriculum learning / difficulty ordering** -- simple random sampling from reservoir is sufficient at current data volumes
- **Shrink-and-perturb warm restart** -- pure EWC++ is the first approach; shrink-and-perturb adds complexity better evaluated after empirical data
- **GPU acceleration** -- CPU-only; training budgets are <5s (classifier) and <2s (scorer)
- **Multi-repository model sharing** -- per-repo scope; training state lives in `~/.unimatrix/{project_hash}/models/`
- **Online calibration / temperature scaling** -- useful but adds complexity; raw softmax confidence is acceptable for threshold decisions
- **Embedding-dependent model training** (Duplicate Detector needs adapted embeddings) -- crt-009 concern
- **Daemon-mode training** -- training runs within session-scoped server process lifetime
- **AdamW or other adaptive optimizers** -- SGD is sufficient for small models with infrequent retraining; adaptive optimizers add state management complexity

## Background Research

### Existing Infrastructure (crt-007, unimatrix-learn)

The `unimatrix-learn` crate provides all training primitives:

| Component | File | What crt-008 Uses |
|-----------|------|-------------------|
| `TrainingReservoir<T>` | `reservoir.rs` | Generic reservoir with `add()`, `sample_batch()`, `len()` -- route training samples here |
| `EwcState` | `ewc.rs` | `penalty()`, `gradient_contribution()`, `update_from_flat()` -- add EWC term to gradients during training |
| `ModelRegistry` | `registry.rs` | `promote()`, `rollback()`, three-slot versioning (ADR-005) -- manages model lifecycle |
| `NeuralModel` trait | `models/traits.rs` | `forward()`, `train_step()`, `flat_parameters()`, `set_parameters()`, `serialize()`, `deserialize()` |
| `ShadowEvaluator` | `unimatrix-observe` | Shadow vs rule prediction logging, accuracy tracking |
| `SignalClassifier` | `models/classifier.rs` | ndarray MLP (32->64->32->5), hand-rolled forward/backward, SGD |
| `ConventionScorer` | `models/scorer.rs` | ndarray MLP (32->32->1), hand-rolled forward/backward, SGD |
| `LearnConfig` | `config.rs` | Thresholds, topology, rollback parameters |
| `SignalDigest` | `models/digest.rs` | Fixed-width `[f32; 32]` input vector |

### NeuralModel Trait (crt-007 ADR-001)

crt-007 uses ndarray with hand-rolled forward/backward passes instead of a framework (burn/ruv-fann). The `NeuralModel` trait provides the abstraction:

```rust
pub trait NeuralModel: Send + Sync {
    fn forward(&self, input: &[f32]) -> Vec<f32>;
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32;
    fn flat_parameters(&self) -> Vec<f32>;
    fn set_parameters(&mut self, params: &[f32]);
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> Result<Self, String> where Self: Sized;
}
```

Key properties for crt-008:
- `train_step` performs forward pass, backward pass, AND SGD weight update in a single call
- `flat_parameters()` / `set_parameters()` provide the parameter interface needed by `EwcState`
- Both models use deterministic layer-by-layer, row-major parameter ordering (ADR-002)

### EWC Integration with train_step

The current `train_step` bakes gradient computation and SGD weight update together. For EWC, we need to add `ewc.gradient_contribution(params)` to the task gradients BEFORE the weight update. Since the backward pass is hand-rolled ndarray code (not a framework black box), we have two clean options:

**Option A: Extend NeuralModel trait** -- Add a `train_step_ewc` method or split into `compute_gradients()` + `apply_gradients()`. This makes EWC a first-class concept in the trait.

**Option B: Post-step parameter correction** -- Call `train_step` (SGD updates), then pull weights back via `flat_parameters()` + EWC correction + `set_parameters()`. Simpler but approximate (same limitation as the original ruv-fann approach).

**Option C: Training function outside the trait** -- Don't extend the trait. Instead, crt-008's `TrainingService` reimplements the training loop using `forward()` for the forward pass, then computes loss + EWC gradients externally, and applies combined updates via `set_parameters()`. This keeps the trait unchanged but duplicates the backward pass logic.

**Recommended: Option A** -- The backward pass is ~20 lines per model. Adding a `compute_gradients` / `apply_gradients` split is a ~10-line refactor to each model. This gives crt-008 clean gradient access without approximation and without duplicating backprop code. The trait extension is additive (default impl of `train_step` calls `compute_gradients` then `apply_gradients`).

### Utilization Signal Sources

| Signal | Source Module | Current Handler | Label Type |
|--------|-------------|----------------|------------|
| Helpful vote on auto-extracted entry | `unimatrix-server` | `UsageService::record_mcp_usage()` | Positive for classifier category |
| Unhelpful vote on auto-extracted entry | `unimatrix-server` | `UsageService::record_mcp_usage()` | Negative for classifier category |
| Entry deprecated (auto-extracted) | `unimatrix-server` | `context_deprecate` handler | Negative for classifier + scorer |
| Entry corrected (category changed) | `unimatrix-server` | `context_correct` handler | Ground truth re-label for classifier |
| Entry corrected (content replaced) | `unimatrix-server` | `context_correct` handler | Negative for original classification |
| Entry never accessed (10+ features) | background tick | `maintenance_tick()` | Weak negative for classifier |
| Convention followed by all agents | background tick | `maintenance_tick()` | Positive for convention scorer |
| Convention deviated from successfully | background tick | `maintenance_tick()` | Negative for convention scorer |
| Feature outcome (success/rework) | `unimatrix-server` | `context_store` (outcome type) | Weak +/- for entries injected in that feature |

### Training Flow

```
Utilization event occurs (vote, correct, deprecate, outcome)
       |
       v
  FeedbackCapture: filter for auto/neural trust_source entries
       |
       v
  LabelGenerator: convert event -> TrainingSample with (input, target, weight)
       |
       v
  TrainingReservoir<TrainingSample>.add() per model
       |
       v
  Check threshold: reservoir.len() >= batch_size?
       |  no         |  yes
       v             v
    (wait)    spawn_blocking {
                1. sample_batch(batch_size)
                2. For each sample in batch:
                   a. params = model.flat_parameters()
                   b. (loss, grads) = model.compute_gradients(input, target)
                   c. ewc_grads = ewc.gradient_contribution(params)
                   d. combined = grads + ewc_grads  (element-wise)
                   e. model.apply_gradients(combined, lr)
                3. After batch: ewc.update_from_flat(params, grad_squared)
                4. Check NaN/Inf in flat_parameters()
                5. Save as shadow via serialize() + save_atomic()
                6. Increment registry version
              }
                      |
                      v
              Shadow mode validation (existing crt-007 infra)
                      |
                      v
              Promotion check (accuracy, per-class, min 20 evals)
                      |  pass         |  fail
                      v               v
              promote()          discard shadow
```

### ndarray Training API (crt-007 ADR-001)

All neural models use ndarray 0.16 (already in workspace) with hand-rolled backpropagation:

- **Forward pass**: Matrix multiply (`w.t().dot(&input) + &bias`) + activation (sigmoid, ReLU, softmax)
- **Backward pass**: Explicit gradient computation layer-by-layer (~20 lines per model)
- **Weight update**: SGD (`w = w - lr * grad`)
- **Parameter access**: `flat_parameters()` returns `Vec<f32>` in deterministic layer-by-layer order; `set_parameters()` reverses the flattening
- **Serialization**: `bincode::serde::encode_to_vec` on the flat parameter vector
- **No framework dependency**: Zero new crates beyond what's already in the workspace

EWC integration is clean because we own the backward pass code (~20 lines per model). crt-007 shipped `train_step` monolithic, but crt-008 Phase 0 splits it into `compute_gradients` + `apply_gradients` (~10 lines per model refactor). This exposes the flat gradient vector between computation and weight update, allowing crt-008's `TrainingService` to inject `ewc.gradient_contribution(params)` as an element-wise addition before `apply_gradients`. No approximation needed.

### Constraints Discovered

- `NeuralModel::train_step` bakes gradient computation and SGD update together (confirmed in shipped crt-007 code) -- crt-008 Phase 0 splits this into `compute_gradients` + `apply_gradients`, with `train_step` as default impl
- Training samples need the original `SignalDigest` (input) plus the correct label (target) -- the shadow_evaluations table already stores digests as BLOBs
- `trust_source` field on entries distinguishes auto-extracted ("auto", "neural") from agent-stored entries -- feedback capture must filter by trust_source
- Feature outcome attribution requires joining `outcome` records with entries injected during that feature cycle -- the `FEATURE_ENTRIES` table (if present) or `feature_cycle` field enables this
- `EwcState` uses flat `Vec<f32>` interface (ADR-002) -- both models' `flat_parameters()` produce deterministic ordering compatible with EWC's element-wise operations
- Parameter ordering between `flat_parameters()` and `set_parameters()` must remain stable across training steps -- enforced by deterministic layer iteration order

## Proposed Approach

### Phase 1: Training Types and Label Pipeline (~150 lines)

New types in `unimatrix-learn`:
- `TrainingSample` struct: `{ digest: SignalDigest, target: TrainingTarget, weight: f32 }`
- `TrainingTarget` enum: classifier targets (5-class one-hot), scorer targets (f32 score)
- `FeedbackSignal` enum: the 9 utilization event types above
- `LabelGenerator` that converts `FeedbackSignal` -> `Vec<TrainingSample>` for the appropriate models

### Phase 2: Feedback Capture Hooks (~100 lines)

Wire into existing server handlers:
- In `UsageService::record_mcp_usage()`: after recording helpful/unhelpful, check if entry has `trust_source` "auto" or "neural"; if so, generate training signal
- In `context_correct` handler: if corrected entry has auto/neural trust_source, generate ground truth re-label
- In `context_deprecate` handler: if deprecated entry has auto/neural trust_source, generate negative label
- In `maintenance_tick()`: scan for stale auto entries, convention follow/deviate patterns
- In outcome recording path: when feature outcome stored, generate weak labels for feature's injected entries

### Phase 3: Training Orchestration (~200 lines)

New `TrainingService` in `unimatrix-learn`:
- Holds per-model `TrainingReservoir<TrainingSample>` and `EwcState`
- `record_feedback(signal: FeedbackSignal)`: generates labels, routes to reservoirs, checks thresholds
- `try_train_step(model_name: &str)`: fire-and-forget training via `spawn_blocking`
- Training step using ndarray models:
  1. Clone model for the training closure (small models, acceptable cost)
  2. Sample batch from reservoir
  3. For each sample: `compute_gradients()` -> add `ewc.gradient_contribution()` -> `apply_gradients()`
  4. After batch: `ewc.update_from_flat(params, grad_squared)`
  5. Check NaN/Inf in `flat_parameters()`
  6. Save as shadow via `serialize()` + `save_atomic()`
- Post-training: trigger shadow evaluation check via `ModelRegistry`

### Phase 4: Rollback Enhancements (~50 lines)

Extend `ModelRegistry`:
- NaN/Inf detection on newly trained weights (immediate discard, no promotion to shadow)
- Per-category accuracy regression: query `ShadowEvaluator::per_class_accuracy()`, reject if any class drops >10%
- Backfill `ground_truth` in `shadow_evaluations` when feedback arrives for previously-evaluated entries

### Phase 5: Ground Truth Backfill (~50 lines)

- When feedback arrives for an entry that was previously shadow-evaluated, update the `ground_truth` column
- Enables retrospective accuracy computation on shadow evaluations
- Simple UPDATE query: `UPDATE shadow_evaluations SET ground_truth = ?1 WHERE signal_digest = ?2 AND ground_truth IS NULL`

### Phase 6: Config Extensions (~50 lines)

Extend `LearnConfig`:
- `classifier_retrain_threshold: u64` (default 20)
- `classifier_batch_size: usize` (default 16)
- `scorer_retrain_threshold: u64` (default 5)
- `scorer_batch_size: usize` (default 4)
- `ewc_alpha: f32` (default 0.95)
- `ewc_lambda: f32` (default 0.5)
- `per_class_regression_threshold: f64` (default 0.10)
- `weak_label_weight: f32` (default 0.3)
- `training_lr: f32` (default 0.01, SGD learning rate)

## Acceptance Criteria

- AC-01: `TrainingSample` and `FeedbackSignal` types defined in `unimatrix-learn`
- AC-02: `LabelGenerator` converts all 9 feedback signal types into correctly typed training samples
- AC-03: Helpful vote on auto-extracted entry generates positive training sample for classifier
- AC-04: Unhelpful vote on auto-extracted entry generates negative training sample for classifier
- AC-05: Category correction generates ground truth re-label with weight 1.0
- AC-06: Deprecation generates negative sample for both classifier and convention scorer
- AC-07: Feature outcome success generates weak positive (weight 0.3) for injected entries
- AC-08: Convention scorer receives positive labels when conventions followed, negative when deviated
- AC-09: Training samples route to per-model `TrainingReservoir<TrainingSample>` instances
- AC-10: Classifier retraining triggers when reservoir reaches 20 labeled signals
- AC-11: Convention Scorer retraining triggers when reservoir reaches 5 labeled evaluations
- AC-12: Training executes as `spawn_blocking` (non-blocking to server event loop)
- AC-13: EWC++ gradient contribution added to task gradients before weight update via `compute_gradients` + `apply_gradients` trait split (no post-step approximation)
- AC-14: Retrained model saved as shadow version (not immediately promoted to production)
- AC-15: NaN/Inf in trained weights detected and model discarded (no shadow promotion)
- AC-16: Per-category accuracy regression >10% prevents shadow promotion
- AC-17: `ground_truth` column backfilled in `shadow_evaluations` when feedback arrives
- AC-18: Training completes within budget: Classifier <5s, Scorer <2s on CPU
- AC-19: Feedback capture hooks only fire for entries with trust_source "auto" or "neural"
- AC-20: All training thresholds configurable via `LearnConfig`
- AC-21: Unit tests for label generation (all 9 signal types)
- AC-22: Unit tests for training step (reservoir fill -> threshold -> train -> shadow model exists)
- AC-23: Integration test: end-to-end feedback -> label -> reservoir -> retrain -> shadow model saved

## Constraints

- **crt-007 dependency**: Neural models (ndarray-based), `TrainingReservoir`, `EwcState`, `ModelRegistry`, `ShadowEvaluator`, and `shadow_evaluations` table must exist
- **No new MCP tools**: All feedback capture piggybacks on existing handler paths
- **No new SQLite tables**: Uses existing `shadow_evaluations` (ground_truth backfill) and `ENTRIES` table (trust_source check)
- **CPU only**: All training runs on CPU via ndarray; <5s classifier, <2s scorer per training step
- **Per-repo isolation**: Training state is project-scoped via `{project_hash}`
- **~600 lines total**: ~150 types/labels, ~100 hooks, ~200 orchestration, ~50 rollback, ~50 backfill, ~50 config
- **No breaking changes**: Existing `NeuralModel` trait consumers unaffected; trait extension is additive
- **Zero new dependencies**: All training uses ndarray 0.16 (already in workspace via unimatrix-adapt and unimatrix-learn)
- **ndarray training path**: Full gradient access via hand-rolled backward passes; EWC gradient injection between gradient computation and weight update; SGD optimizer

## Resolved Questions

1. **NeuralModel trait extension for gradient access**: crt-007 shipped with `train_step` monolithic -- gradient computation and SGD update are baked together in one method (confirmed: `classifier.rs:137-193`, `scorer.rs:65-104`). There is no `compute_gradients` or `apply_gradients` split. crt-008 includes the trait split as Phase 0.

2. **EWC gradient injection mechanism**: **Option A (trait split) selected.** Add `compute_gradients(&self, input, target) -> (f32, Vec<f32>)` + `apply_gradients(&mut self, gradients, lr)` to `NeuralModel`. `train_step` becomes a default impl calling both. ~10 lines per model refactor. This is the cleanest approach and applies uniformly across crt-008 (Signal Classifier, Convention Scorer) and crt-009 (Siamese MLP, Pattern Merger, Entry Writer Scorer). All models that implement `NeuralModel` get gradient-level access for EWC integration. Options B (post-step approximation) and C (sub-trait) rejected -- B is approximate and would need replacing later anyway; C fragments the trait hierarchy unnecessarily when all models will eventually need retraining.

## Open Questions

1. **Weak label weight calibration** (architect decision): The 0.3 weight for feature outcome signals (success/rework) and stale entry signals is a starting estimate. Should this be configurable per signal type, or is a single `weak_label_weight` sufficient?

2. **Convention follow/deviate detection** (architect decision): How does the maintenance tick detect that "a convention was followed by all agents" or "deviated from successfully"? This likely requires querying observation data for patterns matching convention entries. The detection logic may already exist in col-013 extraction rules or may need a small addition.

## crt-009 Note

With ndarray and the `NeuralModel` trait, crt-009's more complex models (Siamese MLP, Pattern Merger, Entry Writer Scorer) are implementable as hand-rolled ndarray MLPs following the same pattern. However, the Siamese architecture (shared weights, distance layer) and N-way merging may push the hand-rolled approach to its practical limit. The trait is designed (ADR-001) so that future burn/candle implementations can be added behind cargo feature gates without changing the interface. crt-009 scoping should evaluate whether ndarray remains sufficient or if a framework dependency is warranted for those specific architectures.

## Tracking

GitHub Issue to be created during Session 1 synthesis phase.
