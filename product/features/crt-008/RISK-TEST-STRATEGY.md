# Risk & Test Strategy: crt-008 Continuous Self-Retraining

## Risk Registry

### R-01: EWC Gradient Ordering Mismatch (from SR-02)
**Severity**: High | **Likelihood**: Low | **Category**: Correctness

If `compute_gradients()` returns gradients in a different order than `flat_parameters()`, EWC penalties push the wrong weights, causing silent quality degradation or training instability.

**Mitigation**: ADR-002 ordering contract. Verification test per model.

**Test**: T-R01-01 through T-R01-03 (see test plan below).

### R-02: Concurrent Training Corruption (from SR-04)
**Severity**: Medium | **Likelihood**: Medium | **Category**: Concurrency

Two `spawn_blocking` training tasks for the same model could race on shadow model file writes or ModelRegistry state updates.

**Mitigation**: ADR-003 per-model AtomicBool lock with Drop guard.

**Test**: T-R02-01 (concurrent training lock test).

### R-03: NaN/Inf Propagation in Training
**Severity**: High | **Likelihood**: Low | **Category**: Stability

Bad input data or numerical instability during training could produce NaN/Inf weights. If not caught, the corrupted model would be saved as shadow and potentially promoted.

**Mitigation**: NaN/Inf check after every training step (FR-05 step 4, FR-11). Immediate discard, no shadow installation.

**Test**: T-R03-01 (NaN injection test).

### R-04: Trust Source Filter Bypass (from SR-05)
**Severity**: Medium | **Likelihood**: Low | **Category**: Data Quality

A feedback capture hook missing the trust_source check would generate training labels from agent-stored entries, polluting model training with unrelated signals.

**Mitigation**: Central `FeedbackCapture` function. Each hook path unit-tested to verify filter.

**Test**: T-R04-01 through T-R04-03 (trust source filtering tests per handler).

### R-05: Training Threshold Never Reached (from SR-03)
**Severity**: Low | **Likelihood**: Medium | **Category**: Feature Value

In low-activity projects, feedback signals may never reach the retraining threshold, making crt-008 effectively inert.

**Mitigation**: Configurable thresholds. Feature outcomes produce bulk labels. Convention scorer has lower threshold (5 vs 20).

**Test**: T-R05-01 (threshold configuration test).

### R-06: Regression in Model Quality After Training
**Severity**: High | **Likelihood**: Low | **Category**: Quality

A retrained model could perform worse than the baseline on certain input distributions (catastrophic forgetting despite EWC, or overfitting to small batch).

**Mitigation**: Shadow mode validation before promotion. Per-class regression check (>10% drop in any class). Auto-rollback on production accuracy drop.

**Test**: T-R06-01 (per-class regression detection test).

## Scope Risk Traceability

| Scope Risk | Architecture Decision | Test Coverage |
|------------|----------------------|---------------|
| SR-01 (Trait split breaks consumers) | ADR-001: default impl preserves behavior | T-FR00-01: existing train_step behavior unchanged |
| SR-02 (EWC gradient ordering) | ADR-002: canonical ordering contract | T-R01-01, T-R01-02, T-R01-03 |
| SR-03 (Sparse feedback) | ADR-005 + configurable thresholds | T-R05-01 |
| SR-04 (Training race conditions) | ADR-003: AtomicBool lock | T-R02-01 |
| SR-05 (Trust source filtering) | Central FeedbackCapture | T-R04-01, T-R04-02, T-R04-03 |
| SR-06 (Ground truth ambiguity) | ADR-006: conservative backfill | T-FR10-01, T-FR10-02 |
| SR-07 (Convention detection complexity) | ADR-005: lightweight matching | T-FR12-01 |

## Test Plan

### Phase 0: Trait Refactor Tests

**T-FR00-01: train_step backward compatibility**
- Create `SignalClassifier::new_with_baseline()`
- Call `train_step(input, target, lr)` on 10 random samples
- Verify loss decreases monotonically (or at least final loss < initial loss)
- Repeat for `ConventionScorer`
- **Validates**: AC-13, FR-00

**T-FR00-02: compute_gradients + apply_gradients matches train_step**
- Clone a model. Train copy A with `train_step`. Train copy B with `compute_gradients` + `apply_gradients`.
- Verify identical predictions after training.
- **Validates**: FR-00

**T-R01-01: Parameter ordering identity test (classifier)**
- Create `SignalClassifier::new_with_baseline()`
- `params = model.flat_parameters()`
- `model.set_parameters(&params)`
- Verify predictions unchanged on 5 test inputs
- **Validates**: ADR-002

**T-R01-02: Parameter ordering identity test (scorer)**
- Same as T-R01-01 for `ConventionScorer`
- **Validates**: ADR-002

**T-R01-03: Gradient vector length matches parameter count**
- `(_, grads) = model.compute_gradients(input, target)`
- `assert_eq!(grads.len(), model.flat_parameters().len())`
- For both classifier and scorer
- **Validates**: ADR-002

### Label Generation Tests

**T-FR01-01: TrainingSample type construction**
- Construct `TrainingSample` with all field types
- Verify `Clone` works
- **Validates**: AC-01

**T-FR02-01: HelpfulVote generates positive classifier label**
- Generate label from `FeedbackSignal::HelpfulVote` with category "convention"
- Verify: target model = "signal_classifier", target = one-hot [1,0,0,0,0], weight = 1.0
- **Validates**: AC-02, AC-03

**T-FR02-02: UnhelpfulVote generates noise classifier label**
- Generate label from `FeedbackSignal::UnhelpfulVote`
- Verify: target = one-hot [0,0,0,0,1] (noise), weight = 1.0
- **Validates**: AC-02, AC-04

**T-FR02-03: CategoryCorrection generates ground truth re-label**
- Generate label from `FeedbackSignal::CategoryCorrection { old: "noise", new: "convention" }`
- Verify: target = one-hot [1,0,0,0,0], weight = 1.0
- **Validates**: AC-02, AC-05

**T-FR02-04: Deprecation generates dual model labels**
- Generate labels from `FeedbackSignal::Deprecation` with category "convention"
- Verify: two labels returned -- one for "signal_classifier" (noise), one for "convention_scorer" (0.0)
- **Validates**: AC-02, AC-06

**T-FR02-05: FeatureOutcome success generates weak labels**
- Generate labels from `FeedbackSignal::FeatureOutcome { result: Success, entry_ids: [1,2], ... }`
- Verify: two classifier labels, weight = 0.3, targets match entry categories
- **Validates**: AC-02, AC-07

**T-FR02-06: ConventionFollowed generates positive scorer label**
- Generate label from `FeedbackSignal::ConventionFollowed`
- Verify: model = "convention_scorer", target = 1.0, weight = 1.0
- **Validates**: AC-02, AC-08

**T-FR02-07: ConventionDeviated generates negative scorer label**
- Generate label from `FeedbackSignal::ConventionDeviated`
- Verify: model = "convention_scorer", target = 0.0, weight = 1.0
- **Validates**: AC-02, AC-08

**T-FR02-08: StaleEntry generates weak dead label**
- Generate label from `FeedbackSignal::StaleEntry`
- Verify: model = "signal_classifier", target = one-hot for Dead, weight = 0.3
- **Validates**: AC-02

**T-FR02-09: ContentCorrection generates noise label with 0.7 weight**
- Generate label from `FeedbackSignal::ContentCorrection`
- Verify: model = "signal_classifier", target = noise, weight = 0.7
- **Validates**: AC-02

### Training Orchestration Tests

**T-FR04-01: Reservoir routing by model name**
- Record 5 HelpfulVote signals (classifier-targeted)
- Record 3 ConventionFollowed signals (scorer-targeted)
- Verify classifier reservoir has 5 items, scorer reservoir has 3
- **Validates**: AC-09

**T-FR04-02: Classifier threshold triggers training**
- Record 20 feedback signals targeting the classifier
- Verify `try_train_step` is called (or would be called)
- **Validates**: AC-10

**T-FR04-03: Scorer threshold triggers training**
- Record 5 feedback signals targeting the convention scorer
- Verify training triggered
- **Validates**: AC-11

**T-FR05-01: Training executes in spawn_blocking**
- Record enough signals to trigger training
- Verify the training task runs without blocking the test's async runtime
- **Validates**: AC-12

**T-FR05-02: EWC penalty decreases over training**
- Initialize EWC with reference params
- Train for 5 steps with EWC
- Verify EWC penalty at step 5 > EWC penalty at step 1 (weights diverge from reference -> penalty increases, confirming EWC is active)
- **Validates**: AC-13

**T-FR05-03: Retrained model saved as shadow**
- Trigger a training step
- Verify ModelRegistry has a shadow model for the trained model name
- Verify shadow model file exists on disk
- **Validates**: AC-14

**T-R02-01: Concurrent training lock prevents double execution**
- Acquire training lock manually
- Call `try_train_step` -- verify it returns immediately without training
- Release lock, call again -- verify training proceeds
- **Validates**: ADR-003

**T-R03-01: NaN/Inf detection discards model**
- Manually set model parameters to include NaN
- Run the NaN/Inf check
- Verify model is discarded, no shadow installed
- **Validates**: AC-15

**T-R06-01: Per-class regression prevents promotion**
- Set up shadow evaluations where one class has >10% accuracy drop
- Call promotion check
- Verify promotion is rejected
- **Validates**: AC-16

### Feedback Capture Tests

**T-R04-01: Helpful vote on agent entry does NOT generate signal**
- Create entry with trust_source = "agent"
- Simulate helpful vote
- Verify no TrainingSample generated
- **Validates**: AC-19

**T-R04-02: Helpful vote on auto entry DOES generate signal**
- Create entry with trust_source = "auto"
- Simulate helpful vote
- Verify TrainingSample generated
- **Validates**: AC-19

**T-R04-03: Deprecation on neural entry DOES generate signal**
- Create entry with trust_source = "neural"
- Simulate deprecation
- Verify TrainingSample generated
- **Validates**: AC-19

### Ground Truth Backfill Tests

**T-FR10-01: Category correction backfills ground truth**
- Insert shadow_evaluation with ground_truth = NULL
- Process CategoryCorrection feedback
- Verify ground_truth column updated
- **Validates**: AC-17

**T-FR10-02: Single vote does NOT backfill ground truth**
- Insert shadow_evaluation with ground_truth = NULL
- Process single HelpfulVote
- Verify ground_truth remains NULL
- **Validates**: ADR-006

### Config Tests

**T-FR-CONFIG-01: Default config values**
- Verify `LearnConfig::default()` returns expected defaults for all new fields
- **Validates**: AC-20

**T-R05-01: Custom threshold triggers training at configured value**
- Set `classifier_retrain_threshold = 5` (non-default)
- Record 5 signals
- Verify training triggers
- **Validates**: AC-20

### Integration Tests

**T-INT-01: End-to-end feedback -> label -> reservoir -> retrain -> shadow**
- Create a SignalClassifier with baseline weights
- Initialize TrainingService
- Record 20 HelpfulVote feedback signals
- Wait for spawn_blocking to complete
- Verify: shadow model exists, shadow model produces different predictions than baseline
- **Validates**: AC-23

## Test Summary

| Category | Count |
|----------|-------|
| Phase 0 trait refactor | 5 |
| Label generation | 9 |
| Training orchestration | 7 |
| Feedback capture | 3 |
| Ground truth backfill | 2 |
| Config | 2 |
| Integration | 1 |
| **Total** | **29** |
