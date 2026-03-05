# Test Plan: model-trait (Wave 2)

## Risk Coverage: R-01 (ruv-fann validation)

### T-R01-1: ruv-fann MLP construction with crt-007 topologies
- **Type**: Unit (P0 gate)
- **Method**: Create Fann with [32, 64, 32, 5] and [32, 32, 1]
- **Pass criteria**: No panic, network object created successfully
- **Location**: `crates/unimatrix-learn/src/classifier.rs` or `scorer.rs` tests

### T-R01-2: Forward pass on zero input produces finite output
- **Type**: Unit (P0 gate)
- **Method**: Run predict with all-zero SignalDigest
- **Pass criteria**: All output values are finite (not NaN, not Inf)
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R01-3: Forward pass produces valid distribution
- **Type**: Unit (P0 gate)
- **Method**: Run classifier predict with random input
- **Pass criteria**: 5 probabilities sum to ~1.0; scorer output in [0, 1]
- **Location**: `crates/unimatrix-learn/src/classifier.rs` and `scorer.rs` tests

### T-R01-4: Model save/load round-trip
- **Type**: Unit
- **Method**: Create model, save to tempdir, load, predict with same input
- **Pass criteria**: Predictions match (within f32 epsilon)
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R01-5: NaN/Inf detection in params_flat
- **Type**: Unit
- **Method**: Call params_flat() on freshly created model
- **Pass criteria**: No NaN or Inf values in parameter vector
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

## Risk Coverage: R-03 (SignalDigest stability)

### T-R03-1: SignalDigest normalization with known values
- **Type**: Unit
- **Method**: Construct digest with known raw values, verify each slot
  matches expected normalized output
- **Pass criteria**: Each slot within f32 epsilon of expected value
- **Location**: `crates/unimatrix-learn/src/digest.rs` tests
- **Values**: search_miss=10 -> log(11)/log(101) ~= 0.519
              feature_count=50 -> log(51)/log(51) = 1.0
              age_days=90 -> 1-exp(-1) ~= 0.632

### T-R03-2: Reserved slots are zero
- **Type**: Unit
- **Method**: Construct digest with any inputs, verify features[7..32] are all 0.0
- **Pass criteria**: All reserved slots exactly 0.0
- **Location**: `crates/unimatrix-learn/src/digest.rs` tests

### T-R03-3: Schema version mismatch detection
- **Type**: Unit
- **Method**: Create digest with version 1, compare against model expecting version 2
- **Pass criteria**: ModelRegistry detects mismatch (tested in registry)
- **Location**: `crates/unimatrix-learn/src/registry.rs` tests

### T-R03-4: Normalization determinism
- **Type**: Unit
- **Method**: Call normalize_log and normalize_age 1000 times with same input
- **Pass criteria**: All results identical (bitwise f32 equality)
- **Location**: `crates/unimatrix-learn/src/digest.rs` tests

### Additional digest tests

#### T-DIG-01: Boundary values
- normalize_log(0, 100) = 0.0
- normalize_log(100, 100) = 1.0
- normalize_log(1000, 100) clamped to 1.0
- normalize_age(0.0) = 0.0
- normalize_age(f32::MAX) clamped to 1.0

#### T-DIG-02: to_bytes / from_bytes round-trip
- Construct digest, to_bytes, from_bytes, compare features
- Verify byte length = 128 (32 * 4)
