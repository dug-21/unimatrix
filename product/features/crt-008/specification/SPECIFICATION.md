# Specification: crt-008 Continuous Self-Retraining

## Domain Model

### Core Types

#### TrainingSample
A labeled training example for a neural model.

```
TrainingSample {
    digest: SignalDigest        -- [f32; 32] input features from the extraction pipeline
    target: TrainingTarget      -- model-specific label
    weight: f32                 -- label confidence: 1.0 for strong signals, 0.3 for weak
    source: FeedbackSignal      -- which event produced this sample
    entry_id: u64               -- source entry ID (for ground truth backfill)
    timestamp: u64              -- epoch millis when label was generated
}
```

#### TrainingTarget (enum)
```
Classification([f32; 5])      -- one-hot: [convention, pattern, gap, dead, noise]
ConventionScore(f32)          -- target score in [0.0, 1.0]
```

#### FeedbackSignal (enum)
The 9 utilization event types that produce training labels.

```
HelpfulVote { entry_id: u64, category: String, digest: SignalDigest }
UnhelpfulVote { entry_id: u64, category: String, digest: SignalDigest }
CategoryCorrection { entry_id: u64, old_category: String, new_category: String, digest: SignalDigest }
ContentCorrection { entry_id: u64, category: String, digest: SignalDigest }
Deprecation { entry_id: u64, category: String, digest: SignalDigest }
StaleEntry { entry_id: u64, category: String, digest: SignalDigest }
ConventionFollowed { entry_id: u64, digest: SignalDigest }
ConventionDeviated { entry_id: u64, digest: SignalDigest }
FeatureOutcome { feature_cycle: String, result: OutcomeResult, entry_ids: Vec<u64>, digests: Vec<SignalDigest> }
```

#### OutcomeResult (enum)
```
Success    -- feature completed without rework
Rework     -- feature required rework iterations
```

#### LabelGenerator
Stateless converter from `FeedbackSignal` to `Vec<(String, TrainingSample)>` where the String is the target model name ("signal_classifier" or "convention_scorer").

**Label generation rules:**

| Signal | Model | Target | Weight |
|--------|-------|--------|--------|
| HelpfulVote | signal_classifier | one-hot for `category` | 1.0 |
| UnhelpfulVote | signal_classifier | one-hot for `noise` | 1.0 |
| CategoryCorrection | signal_classifier | one-hot for `new_category` | 1.0 |
| ContentCorrection | signal_classifier | one-hot for `noise` (original misclassified) | 0.7 |
| Deprecation | signal_classifier | one-hot for `noise` | 1.0 |
| Deprecation | convention_scorer | target 0.0 (not a convention) | 1.0 |
| StaleEntry | signal_classifier | one-hot for `dead` | 0.3 (weak) |
| ConventionFollowed | convention_scorer | target 1.0 | 1.0 |
| ConventionDeviated | convention_scorer | target 0.0 | 1.0 |
| FeatureOutcome(Success) | signal_classifier | one-hot for entry's `category` | 0.3 (weak) |
| FeatureOutcome(Rework) | signal_classifier | one-hot for `noise` | 0.3 (weak) |

#### TrainingService
Orchestrator holding per-model state and coordinating the training lifecycle.

```
TrainingService {
    reservoirs: HashMap<String, TrainingReservoir<TrainingSample>>
    ewc_states: HashMap<String, EwcState>
    training_locks: HashMap<String, Arc<AtomicBool>>
    registry: Arc<Mutex<ModelRegistry>>
    config: LearnConfig
    label_generator: LabelGenerator
}
```

### Extended LearnConfig

```
LearnConfig {
    // Existing fields (from crt-007)
    models_dir: PathBuf
    shadow_min_evaluations: u32
    rollback_threshold: f64
    rollback_window: usize

    // New fields (crt-008)
    classifier_retrain_threshold: u64   -- default 20
    classifier_batch_size: usize        -- default 16
    scorer_retrain_threshold: u64       -- default 5
    scorer_batch_size: usize            -- default 4
    ewc_alpha: f32                      -- default 0.95
    ewc_lambda: f32                     -- default 0.5
    per_class_regression_threshold: f64 -- default 0.10
    weak_label_weight: f32              -- default 0.3
    training_lr: f32                    -- default 0.01
    reservoir_capacity: usize           -- default 500
    reservoir_seed: u64                 -- default 42
}
```

## Functional Requirements

### FR-00: NeuralModel Trait Split (Phase 0)

Add to `NeuralModel` trait:

- `compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>)` -- forward + backward pass, returns (loss, flat gradient vector)
- `apply_gradients(&mut self, gradients: &[f32], lr: f32)` -- applies gradient vector as SGD weight update

`train_step` becomes a default implementation:

```rust
fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
    let (loss, grads) = self.compute_gradients(input, target);
    self.apply_gradients(&grads, lr);
    loss
}
```

**For SignalClassifier**: extract lines 149-183 (gradient computation) into `compute_gradients`, returning a flat Vec with ordering: dw1, db1, dw2, db2, dw3, db3 (matching `flat_parameters()`). Extract lines 186-191 (SGD update) into `apply_gradients`.

**For ConventionScorer**: extract lines 75-95 into `compute_gradients` (dw1, db1, dw2, db2). Extract lines 98-101 into `apply_gradients`.

Verification test: `model.set_parameters(&model.flat_parameters())` is an identity (predictions unchanged).

### FR-01: TrainingSample and FeedbackSignal Types

- `TrainingSample` struct with fields as defined in Domain Model
- `TrainingTarget` enum with `Classification([f32; 5])` and `ConventionScore(f32)` variants
- `FeedbackSignal` enum with all 9 variants
- All types derive `Clone`, `Debug`
- `TrainingSample` implements `Clone` (required by `TrainingReservoir<T: Clone>`)

### FR-02: LabelGenerator

- `LabelGenerator::generate(signal: &FeedbackSignal) -> Vec<(String, TrainingSample)>`
- Stateless: no internal state, pure function
- Returns 0-2 samples per signal (some signals target multiple models)
- Maps each signal type to the correct model and target as specified in the label generation rules table
- Weak labels use `config.weak_label_weight` (passed at construction or as parameter)
- Each returned tuple: `(model_name, sample)` where model_name is "signal_classifier" or "convention_scorer"

### FR-03: TrainingService Construction

- `TrainingService::new(config: LearnConfig, registry: Arc<Mutex<ModelRegistry>>) -> Self`
- Creates `TrainingReservoir<TrainingSample>` for each model:
  - "signal_classifier": capacity = `config.reservoir_capacity`, seed = `config.reservoir_seed`
  - "convention_scorer": capacity = `config.reservoir_capacity`, seed = `config.reservoir_seed + 1`
- Creates `EwcState` for each model:
  - "signal_classifier": param_count from `SignalClassifier::new_with_baseline().flat_parameters().len()`, alpha = `config.ewc_alpha`, lambda = `config.ewc_lambda`
  - "convention_scorer": param_count from `ConventionScorer::new_with_baseline().flat_parameters().len()`, alpha = `config.ewc_alpha`, lambda = `config.ewc_lambda`
- Creates `AtomicBool` training lock per model (initially `false`)

### FR-04: Feedback Recording

- `TrainingService::record_feedback(&self, signal: FeedbackSignal)`
- Generates labels via `LabelGenerator::generate()`
- Routes each sample to the corresponding model's reservoir via `reservoir.add(&[sample])`
- After adding: check if `reservoir.len() >= threshold` for each affected model
- If threshold met AND training lock is not held: call `try_train_step(model_name)`

### FR-05: Training Step Execution

- `TrainingService::try_train_step(&self, model_name: &str)`
- Check `AtomicBool` lock via `compare_exchange(false, true, SeqCst, SeqCst)`. If already locked, return.
- Clone: current model parameters (via `flat_parameters()`), EWC state, batch from reservoir
- `spawn_blocking` with cloned state:
  1. Reconstruct model from parameters (deserialize or set_parameters on a fresh model)
  2. For each sample in batch:
     a. `(loss, grads) = model.compute_gradients(sample.digest.as_slice(), target_slice)`
     b. `ewc_grads = ewc.gradient_contribution(model.flat_parameters())`
     c. `combined[i] = sample.weight * grads[i] + ewc_grads[i]` for each parameter
     d. `model.apply_gradients(&combined, config.training_lr)`
  3. `final_params = model.flat_parameters()`
  4. NaN/Inf check: if any `param.is_nan() || param.is_infinite()`, discard model, release lock, return
  5. Compute accumulated gradient squared for EWC update
  6. `ewc.update_from_flat(&final_params, &grad_squared_accum)`
  7. `model.serialize()` -> `save_atomic()` as shadow version file
  8. Update `ModelRegistry`: install shadow version
  9. Release training lock (via Drop guard)

### FR-06: Feedback Capture -- Helpful/Unhelpful Votes

In `UsageService::record_mcp_usage()` (or equivalent), after recording the vote:

1. Load entry by ID
2. Check `trust_source` field: if NOT "auto" or "neural", return
3. Reconstruct `SignalDigest` from entry metadata (if stored as provenance) or build from entry fields
4. Emit `FeedbackSignal::HelpfulVote` or `FeedbackSignal::UnhelpfulVote`
5. Call `training_service.record_feedback(signal)`

### FR-07: Feedback Capture -- Category Correction

In `context_correct` handler, after successful correction:

1. Check corrected entry's `trust_source`: if NOT "auto" or "neural", return
2. Emit `FeedbackSignal::CategoryCorrection` with old and new category
3. Call `training_service.record_feedback(signal)`
4. Trigger ground truth backfill (FR-10)

### FR-08: Feedback Capture -- Deprecation

In `context_deprecate` handler, after successful deprecation:

1. Check deprecated entry's `trust_source`: if NOT "auto" or "neural", return
2. Emit `FeedbackSignal::Deprecation`
3. Call `training_service.record_feedback(signal)`

### FR-09: Feedback Capture -- Feature Outcome

In the outcome recording path (when `category: "outcome"` is stored):

1. Extract `feature_cycle` and `result` from outcome entry tags
2. Query entries with matching `feature_cycle` and `trust_source` in ["auto", "neural"]
3. If matching entries found: emit `FeedbackSignal::FeatureOutcome` with all entry IDs and digests
4. Call `training_service.record_feedback(signal)`

### FR-10: Ground Truth Backfill

When a `CategoryCorrection` feedback signal is processed:

1. Execute SQL: `UPDATE shadow_evaluations SET ground_truth = ?1 WHERE ground_truth IS NULL AND feature_cycle = ?2`
2. The ground_truth value is the new (corrected) category

When an entry accumulates 3+ unhelpful votes with 0 helpful votes:

1. Execute SQL: `UPDATE shadow_evaluations SET ground_truth = 'noise' WHERE ground_truth IS NULL AND model_name = 'signal_classifier'` (filtered by entry matching)

### FR-11: Enhanced Rollback Checks

Extend `ModelRegistry` or `TrainingService` promotion checks:

1. **NaN/Inf check**: Before installing a shadow model, verify `flat_parameters()` contains no NaN or Inf values. If detected, discard the model and do not install as shadow.
2. **Per-class regression check**: When evaluating promotion criteria, query `ShadowEvaluator::per_class_accuracy()`. If any class accuracy drops more than `config.per_class_regression_threshold` (10%) below production's per-class accuracy, reject promotion.

### FR-12: Feedback Capture -- Maintenance Tick

In `maintenance_tick()`, after standard maintenance operations:

1. **Stale entry detection**: Query entries with `trust_source` "auto" or "neural" that have not been accessed in 10+ features. Emit `FeedbackSignal::StaleEntry` for each.
2. **Convention follow/deviate**: For active convention entries with `trust_source` "auto" or "neural":
   a. Query injection_log for recent sessions where the convention was injected
   b. Query observation data for sessions where behavior matched (followed) or diverged (deviated)
   c. Emit `FeedbackSignal::ConventionFollowed` or `FeedbackSignal::ConventionDeviated` as appropriate

### FR-13: SignalDigest Reconstruction

When feedback arrives for an entry, we need the original `SignalDigest` that was used to classify it. Two approaches:

1. **From shadow_evaluations**: If the entry was shadow-evaluated, the `signal_digest` BLOB column contains the 128-byte digest. Query: `SELECT signal_digest FROM shadow_evaluations WHERE feature_cycle = ?1 LIMIT 1`.
2. **Reconstruct from entry**: If no shadow evaluation exists (entry was rule-classified before neural models activated), reconstruct the digest from entry metadata using `SignalDigest::from_fields()`.

Approach 1 is preferred (exact digest). Approach 2 is the fallback.

## Non-Functional Requirements

### NFR-01: Training Latency
- Signal Classifier training step (batch of 16): < 5 seconds on CPU
- Convention Scorer training step (batch of 4): < 2 seconds on CPU
- `record_feedback()` call: < 1ms (non-blocking, only generates labels and adds to reservoir)

### NFR-02: Memory Footprint
- Per-model `TrainingReservoir`: ~500 samples * ~160 bytes/sample = ~80KB per model
- Per-model `EwcState`: flat parameter count * 2 * 4 bytes (fisher + reference) = ~40KB for classifier, ~9KB for scorer
- Training lock: negligible (AtomicBool)
- Total additional RAM: < 200KB

### NFR-03: Concurrency Safety
- `record_feedback()` is safe to call from any tokio task
- At most one `spawn_blocking` training task per model at any time (AtomicBool lock)
- `ModelRegistry` mutations (install shadow, promote, rollback) go through `Arc<Mutex<ModelRegistry>>`
- No deadlock risk: training closure holds only the AtomicBool lock, never the registry mutex during training computation

### NFR-04: Backward Compatibility
- `NeuralModel` trait extension is additive: `train_step` becomes default impl
- All existing crt-007 tests pass unchanged
- `LearnConfig::default()` includes all new fields with defaults
- No new SQLite tables or schema migrations

### NFR-05: Failure Isolation
- Training failure (NaN/Inf, panic in spawn_blocking) does not affect inference
- Failed training discards the model and releases the lock; production model continues serving
- Ground truth backfill failure is logged but does not block feedback processing
- Reservoir overflow is handled by reservoir sampling (bounded memory)

## Acceptance Criteria Traceability

| AC | Functional Requirement | Non-Functional Requirement |
|----|----------------------|---------------------------|
| AC-01 | FR-01 | |
| AC-02 | FR-02 | |
| AC-03 | FR-06 | |
| AC-04 | FR-06 | |
| AC-05 | FR-07 | |
| AC-06 | FR-08 | |
| AC-07 | FR-09 | NFR-01 |
| AC-08 | FR-12 | |
| AC-09 | FR-04 | |
| AC-10 | FR-05 | |
| AC-11 | FR-05 | |
| AC-12 | FR-05 | NFR-01, NFR-03 |
| AC-13 | FR-00, FR-05 | |
| AC-14 | FR-05 | |
| AC-15 | FR-11 | NFR-05 |
| AC-16 | FR-11 | |
| AC-17 | FR-10 | |
| AC-18 | FR-05 | NFR-01 |
| AC-19 | FR-06, FR-07, FR-08 | |
| AC-20 | FR-03 | |
| AC-21 | FR-02 | |
| AC-22 | FR-05 | |
| AC-23 | FR-04, FR-05, FR-06 | |

## Constraints

- All training uses ndarray 0.16 (already in workspace) -- zero new dependencies
- Models are per-project (isolated by `{project_hash}`)
- No training occurs until feedback signals accumulate past threshold
- `compute_gradients` gradient vector MUST use the same ordering as `flat_parameters()` (ADR-002)
- Training step must not block the tokio event loop (`spawn_blocking` mandatory)
- Total new code: ~600 lines (~20 trait refactor, ~150 types/labels, ~200 orchestration, ~100 hooks, ~50 rollback, ~50 backfill, ~30 config)
