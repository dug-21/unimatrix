# Acceptance Map: crt-008 Continuous Self-Retraining

## AC to Wave Mapping

| AC | Description | Wave | Test(s) | FR |
|----|-------------|------|---------|-----|
| AC-01 | TrainingSample and FeedbackSignal types defined | 1 | T-FR01-01 | FR-01 |
| AC-02 | LabelGenerator converts all 9 signal types | 1 | T-FR02-01 through T-FR02-09 | FR-02 |
| AC-03 | Helpful vote -> positive classifier sample | 1 | T-FR02-01 | FR-06 |
| AC-04 | Unhelpful vote -> negative classifier sample | 1 | T-FR02-02 | FR-06 |
| AC-05 | Category correction -> ground truth re-label (weight 1.0) | 1 | T-FR02-03 | FR-07 |
| AC-06 | Deprecation -> negative for classifier + scorer | 1 | T-FR02-04 | FR-08 |
| AC-07 | Feature outcome success -> weak positive (0.3) | 1 | T-FR02-05 | FR-09 |
| AC-08 | Convention followed/deviated -> scorer labels | 1,4 | T-FR02-06, T-FR02-07, T-FR12-01 | FR-12 |
| AC-09 | Samples route to per-model reservoirs | 2 | T-FR04-01 | FR-04 |
| AC-10 | Classifier retrains at 20 signals | 2 | T-FR04-02 | FR-05 |
| AC-11 | Scorer retrains at 5 evaluations | 2 | T-FR04-03 | FR-05 |
| AC-12 | Training executes as spawn_blocking | 2 | T-FR05-01 | FR-05 |
| AC-13 | EWC++ gradient added before weight update | 0,2 | T-FR05-02, T-FR00-02 | FR-00, FR-05 |
| AC-14 | Retrained model saved as shadow | 2 | T-FR05-03 | FR-05 |
| AC-15 | NaN/Inf detected and model discarded | 3 | T-R03-01 | FR-11 |
| AC-16 | Per-class >10% regression prevents promotion | 3 | T-R06-01 | FR-11 |
| AC-17 | Ground truth backfilled in shadow_evaluations | 5 | T-FR10-01 | FR-10 |
| AC-18 | Training <5s classifier, <2s scorer | 2 | T-FR05-01 (timing) | FR-05 |
| AC-19 | Hooks only fire for auto/neural trust_source | 4 | T-R04-01, T-R04-02, T-R04-03 | FR-06,07,08 |
| AC-20 | All thresholds configurable via LearnConfig | 2 | T-FR-CONFIG-01, T-R05-01 | FR-03 |
| AC-21 | Unit tests for label generation (9 signal types) | 1 | T-FR02-01 through T-FR02-09 | FR-02 |
| AC-22 | Unit tests for training step | 2 | T-FR05-01, T-FR05-02, T-FR05-03 | FR-05 |
| AC-23 | Integration test: end-to-end | 6 | T-INT-01 | FR-04,05,06 |

## Wave Completion Gates

### Wave 0 Gate
- [ ] `compute_gradients` and `apply_gradients` added to NeuralModel trait
- [ ] `train_step` is default impl calling both
- [ ] SignalClassifier implements both new methods
- [ ] ConventionScorer implements both new methods
- [ ] Parameter ordering identity tests pass (T-R01-01, T-R01-02)
- [ ] Gradient length matches parameter count (T-R01-03)
- [ ] All existing crt-007 tests pass unchanged

### Wave 1 Gate
- [ ] TrainingSample, TrainingTarget, FeedbackSignal types defined
- [ ] LabelGenerator converts all 9 signal types correctly
- [ ] All 9 label generation tests pass (T-FR02-01 through T-FR02-09)

### Wave 2 Gate
- [ ] TrainingService constructs with per-model reservoirs and EWC states
- [ ] record_feedback routes to correct reservoirs
- [ ] Threshold crossing triggers try_train_step
- [ ] AtomicBool lock prevents concurrent training
- [ ] Training step produces shadow model
- [ ] EWC penalty is active during training

### Wave 3 Gate
- [ ] NaN/Inf in trained weights detected and model discarded
- [ ] Per-class regression >10% blocks promotion

### Wave 4 Gate
- [ ] Server state includes Arc<TrainingService>
- [ ] Helpful/unhelpful vote on auto/neural entries generates signals
- [ ] Correction of auto/neural entries generates signals
- [ ] Deprecation of auto/neural entries generates signals
- [ ] Agent-stored entries do NOT generate signals

### Wave 5 Gate
- [ ] Category correction backfills ground_truth
- [ ] Single votes do NOT backfill ground_truth

### Wave 6 Gate
- [ ] End-to-end integration test passes
- [ ] Shadow model exists with different weights from baseline after training
