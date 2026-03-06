# Test Plan: classifier-scorer

## Risks Covered: R-03, R-04, R-07, R-09

### T-CS-01: Classifier baseline output on zero digest (R-04, AC-14)
- Create classifier with new_with_baseline()
- classify(SignalDigest::zeros())
- Assert category == Noise
- Assert probabilities[4] > 0.5 (Noise dominates due to +2.0 bias)

### T-CS-02: Classifier non-degenerate output on typical digest (R-04, R-09)
- Create classifier with new_with_baseline()
- classify(digest with features[0..7] = [0.7, 0.3, 0.5, 0.0, 0.0, 0.25, 0.2])
- Assert probabilities sum to ~1.0 (within 1e-5)
- Assert at least one non-Noise category has probability > 0.05

### T-CS-03: Classifier output shape
- forward() on 32-element input returns 5-element output
- All output values in [0.0, 1.0]
- Sum of output values ~ 1.0 (softmax)

### T-CS-04: Scorer baseline output on zero digest (R-04, AC-14)
- Create scorer with new_with_baseline()
- score(SignalDigest::zeros()) < 0.3 (biased low by -2.0)

### T-CS-05: Scorer output in [0,1] (AC-05)
- score() always returns value in [0.0, 1.0] for various inputs
- Test with zeros, ones, random digests

### T-CS-06: Classifier numerical gradient check (R-03)
- For each layer (w1,b1,w2,b2,w3,b3):
  - Compute analytical gradient via train_step backward pass
  - Compute numerical gradient via finite differences: (f(x+h) - f(x-h)) / 2h
  - Assert relative error < 1e-3 for each parameter
- Use h = 1e-4

### T-CS-07: Scorer numerical gradient check (R-03)
- Same procedure as T-CS-06 but for scorer's 2-layer network

### T-CS-08: Classifier gradient flow test (R-03)
- Run train_step with a simple target
- Verify gradients are non-zero in all layers (no dead gradients)
- Verify loss decreases after 10 train_steps on same input

### T-CS-09: Scorer gradient flow test (R-03)
- Same as T-CS-08 for scorer

### T-CS-10: Classifier inference timing (R-07, AC-07)
- 100 classifier inferences
- p99 < 50ms (release mode only, behind #[cfg(not(debug_assertions))])

### T-CS-11: Scorer inference timing (R-07, AC-08)
- 100 scorer inferences
- p99 < 10ms (release mode only)
