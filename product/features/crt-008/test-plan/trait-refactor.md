# Test Plan: trait-refactor (Wave 0)

## Tests

### T-FR00-01: train_step backward compatibility
- **Location**: `models/classifier.rs::tests`, `models/scorer.rs::tests`
- **Setup**: Create model with `new_with_baseline()`
- **Action**: Call `train_step()` on 10 random samples
- **Assert**: Loss decreases (final < initial)
- **Risk**: Validates FR-00, ensures default impl matches original
- **Note**: This largely duplicates existing gradient_flow tests but explicitly confirms post-refactor behavior

### T-FR00-02: compute_gradients + apply_gradients matches train_step
- **Location**: `models/classifier.rs::tests`, `models/scorer.rs::tests`
- **Setup**: Clone a model. Same input/target.
- **Action**:
  - Copy A: call `train_step(input, target, lr)`
  - Copy B: call `compute_gradients(input, target)` then `apply_gradients(grads, lr)`
- **Assert**: `A.flat_parameters()` == `B.flat_parameters()` within 1e-6
- **Risk**: Validates ADR-001 correctness

### T-R01-01: Parameter ordering identity test (classifier)
- **Location**: `models/classifier.rs::tests`
- **Setup**: `SignalClassifier::new_with_baseline()`
- **Action**: `model.set_parameters(&model.flat_parameters())`
- **Assert**: Forward pass predictions unchanged on 5 test inputs (within 1e-6)
- **Risk**: R-01 mitigation (ADR-002)

### T-R01-02: Parameter ordering identity test (scorer)
- **Location**: `models/scorer.rs::tests`
- **Setup**: `ConventionScorer::new_with_baseline()`
- **Action**: `model.set_parameters(&model.flat_parameters())`
- **Assert**: Forward pass predictions unchanged on 5 test inputs (within 1e-6)
- **Risk**: R-01 mitigation (ADR-002)

### T-R01-03: Gradient vector length matches parameter count
- **Location**: `models/classifier.rs::tests`, `models/scorer.rs::tests`
- **Setup**: Create model, prepare input/target
- **Action**: `(_, grads) = model.compute_gradients(input, target)`
- **Assert**: `grads.len() == model.flat_parameters().len()`
- **Risk**: R-01 mitigation (ADR-002)
