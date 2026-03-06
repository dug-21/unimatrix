# Implementation Brief: crt-008 Continuous Self-Retraining

## Summary

Close the self-learning loop by building a feedback-to-label pipeline that converts utilization signals into training data, orchestrates threshold-triggered incremental retraining with EWC++ regularization, and validates retrained models through shadow mode before promotion. ~600 lines across unimatrix-learn (new types, training service, trait refactor) and unimatrix-server (feedback capture hooks).

## Implementation Waves

### Wave 0: NeuralModel Trait Split (~20 lines)

**Files**: `crates/unimatrix-learn/src/models/traits.rs`, `classifier.rs`, `scorer.rs`

1. Add `compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>)` to `NeuralModel` trait
2. Add `apply_gradients(&mut self, gradients: &[f32], lr: f32)` to `NeuralModel` trait
3. Make `train_step` a default implementation calling both
4. For `SignalClassifier`: extract lines 149-183 (gradient computation) into `compute_gradients`, lines 186-191 into `apply_gradients`
5. For `ConventionScorer`: extract lines 75-95 into `compute_gradients`, lines 98-101 into `apply_gradients`
6. Add ordering verification test for each model

**Gate**: All existing crt-007 tests pass. New tests T-FR00-01, T-FR00-02, T-R01-01 through T-R01-03 pass.

### Wave 1: Training Types and Label Generator (~150 lines)

**Files**: `crates/unimatrix-learn/src/training.rs` (new)

1. Define `TrainingSample`, `TrainingTarget`, `FeedbackSignal`, `OutcomeResult` types
2. Implement `LabelGenerator::generate(signal) -> Vec<(String, TrainingSample)>`
3. Implement all 9 signal-to-label conversion rules per specification table
4. Add unit tests for all 9 label types (T-FR02-01 through T-FR02-09)

**Gate**: All label generation tests pass. Each signal type maps to correct model(s) with correct target and weight.

### Wave 2: Training Service (~200 lines)

**Files**: `crates/unimatrix-learn/src/service.rs` (new), `config.rs` (extend)

1. Define `TrainingService` struct with per-model reservoirs, EWC states, training locks
2. Implement `TrainingService::new(config, registry)` -- initializes reservoirs and EWC per model
3. Implement `record_feedback(signal)` -- generates labels, routes to reservoirs, checks thresholds
4. Implement `try_train_step(model_name)` -- AtomicBool lock, spawn_blocking, EWC-augmented training loop
5. Add training config fields to `LearnConfig` with defaults
6. Add unit tests: T-FR04-01 through T-FR04-03, T-FR05-01 through T-FR05-03, T-R02-01

**Gate**: Training orchestration tests pass. Training lock prevents concurrent execution. Shadow model produced after threshold crossing.

### Wave 3: Rollback Enhancements (~50 lines)

**Files**: `crates/unimatrix-learn/src/service.rs` (extend)

1. Add NaN/Inf check after training step -- discard model if detected
2. Add per-class regression check to promotion criteria
3. Add config field `per_class_regression_threshold`
4. Tests: T-R03-01, T-R06-01

**Gate**: NaN models discarded. Per-class regression blocks promotion.

### Wave 4: Feedback Capture Hooks (~100 lines)

**Files**: `crates/unimatrix-server/src/services/usage.rs`, `mcp/correct.rs`, `mcp/deprecate.rs`, `mcp/store.rs`, `background.rs`, `lib.rs`

1. Add `Arc<TrainingService>` to server state (wired in `lib.rs`)
2. In usage service: after helpful/unhelpful, check trust_source, emit signal
3. In correct handler: after correction of auto/neural entry, emit signal
4. In deprecate handler: after deprecation of auto/neural entry, emit signal
5. In store handler: when outcome stored, query affected auto/neural entries, emit signal
6. In background tick: stale entry detection, convention follow/deviate (ADR-005)
7. Tests: T-R04-01 through T-R04-03

**Gate**: All feedback capture tests pass. Agent-stored entries do NOT generate signals. Auto/neural entries DO generate signals.

### Wave 5: Ground Truth Backfill (~50 lines)

**Files**: `crates/unimatrix-learn/src/service.rs` (extend)

1. On CategoryCorrection: UPDATE shadow_evaluations SET ground_truth
2. On consistent multi-vote (3+ unhelpful, 0 helpful): UPDATE shadow_evaluations SET ground_truth = 'noise'
3. Tests: T-FR10-01, T-FR10-02

**Gate**: Backfill tests pass. Single votes do NOT backfill.

### Wave 6: Integration Test (~30 lines)

**Files**: `crates/unimatrix-learn/tests/` (new integration test)

1. End-to-end: create baseline model -> TrainingService -> 20 feedback signals -> wait for training -> verify shadow model exists with different weights
2. Test: T-INT-01

**Gate**: Integration test passes. Shadow model predictions differ from baseline.

## Dependencies

| Dependency | Status | Required By |
|------------|--------|-------------|
| crt-007 unimatrix-learn crate | In progress (same branch) | Wave 0 |
| crt-007 NeuralModel trait | In progress | Wave 0 |
| crt-007 ModelRegistry | In progress | Wave 2 |
| crt-007 ShadowEvaluator | In progress | Wave 3 |
| crt-007 shadow_evaluations table | In progress | Wave 5 |
| col-013 extraction pipeline | Complete | Wave 4 (background tick hook) |
| crt-001 usage tracking | Complete | Wave 4 (helpful/unhelpful hook) |
| col-001 outcome tracking | Complete | Wave 4 (feature outcome hook) |

## Key Decisions

- **ADR-001**: NeuralModel trait split: `compute_gradients` + `apply_gradients`. `train_step` becomes default impl.
- **ADR-002**: Flat gradient ordering contract. Canonical ordering enforced by verification tests.
- **ADR-003**: Per-model AtomicBool training lock. At most one spawn_blocking per model.
- **ADR-004**: Weighted loss for weak labels. Weight 0.3 for feature outcomes and stale entries.
- **ADR-005**: Convention follow/deviate detection via topic+tag matching against observation data.
- **ADR-006**: Ground truth backfill limited to category corrections and consistent multi-votes.

## Risk Watch Items

1. **EWC gradient ordering** (R-01, High severity): Verification tests are the primary defense. If tests pass, the contract holds.
2. **Signal volume in early features** (R-05): Monitor reservoir sizes after first few features. Lower thresholds if needed.
3. **Phase 0 trait refactor scope**: If crt-007 has not merged, include the trait split in crt-007 to avoid a separate migration.

## Line Count Estimate

| Component | Lines |
|-----------|-------|
| Trait refactor (Wave 0) | ~20 |
| Training types + label generator (Wave 1) | ~150 |
| Training service (Wave 2) | ~200 |
| Rollback enhancements (Wave 3) | ~50 |
| Feedback capture hooks (Wave 4) | ~100 |
| Ground truth backfill (Wave 5) | ~50 |
| Config extensions | ~30 |
| **Total** | **~600** |
