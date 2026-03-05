# Test Plan: classifier-scorer (Wave 3)

## Risk Coverage: R-07 (conservative bias calibration)

### T-R07-1: Strong convention signal classified correctly
- **Type**: Unit
- **Method**: Create classifier with baseline weights. Build digest with
  consistency=1.0, feature_count=10, rule_confidence=0.9 (strong signal).
  Predict and verify predicted_class is NOT Noise.
- **Pass criteria**: predicted_class != Noise
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R07-2: Scorer produces score > 0.5 for strong signal
- **Type**: Unit
- **Method**: Same strong signal digest as T-R07-1 input to scorer.
- **Pass criteria**: score > 0.5
- **Location**: `crates/unimatrix-learn/src/scorer.rs` tests

### T-R07-3: Weak signal biased to noise
- **Type**: Unit
- **Method**: Create digest with near-zero features (all slots ~0.0).
  Classify with baseline weights.
- **Pass criteria**: Noise probability > any other individual class probability
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R07-4: Bias values configurable via NeuralConfig
- **Type**: Unit
- **Method**: Create NeuralConfig with different bias values, create classifier,
  verify predictions differ from default.
- **Pass criteria**: Different config produces different predictions for same input
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

## Additional Unit Tests

### T-CLS-01: ClassificationResult probabilities sum to 1.0
- For 10 different random digests, verify sum within 0.01

### T-CLS-02: Deterministic inference
- Same input produces exact same ClassificationResult across 100 calls

### T-CLS-03: Classifier latency < 50ms
- Time 1000 predictions, verify average < 50ms (T-R09-1)

### T-SCR-01: ConventionScore in [0, 1]
- For 10 different random digests, verify score in [0.0, 1.0]

### T-SCR-02: Scorer latency < 10ms
- Time 1000 predictions, verify average < 10ms (T-R09-2)

### T-SCR-03: Scorer baseline output ~0.27 for zero input
- Verify sigmoid(-1.0) ~= 0.27 for all-zero digest
