# Test Plan: learn-crate (Wave 1)

## Risk Coverage: R-02 (adapt refactoring regression)

### T-R02-1: All existing adapt tests pass after refactoring
- **Type**: Regression
- **Method**: `cargo test -p unimatrix-adapt` -- all 174+ tests green
- **Pass criteria**: Zero failures, zero new warnings
- **Location**: No new test code; existing adapt tests serve as regression gate

### T-R02-2: AdaptationState serde compatibility
- **Type**: Unit
- **Method**: Create AdaptationState with old code path, serialize with bincode,
  deserialize with new code. Verify all fields match.
- **Pass criteria**: Round-trip produces identical state
- **Location**: `crates/unimatrix-adapt/src/persistence.rs` (existing T-PER-01 covers this)
- **Note**: Existing test already validates this. Just verify it still passes.

### T-R02-3: TrainingReservoir<TrainingPair> behavioral equivalence
- **Type**: Unit
- **Method**: Create reservoir via new wrapper, add same tuples as old code,
  verify len(), total_seen(), sample_batch() produce same results for same seed.
- **Pass criteria**: Deterministic sampling matches pre-refactoring behavior
- **Location**: `crates/unimatrix-learn/src/reservoir.rs` tests

### T-R02-4: EwcState flat gradient update equivalence
- **Type**: Unit
- **Method**: Create EwcState, update with known flat gradients.
  Compare fisher and reference_params against expected values
  (verified by hand-computing the EWC++ formula).
- **Pass criteria**: Fisher and reference params match expected values
- **Location**: `crates/unimatrix-learn/src/ewc.rs` tests

### T-R02-5: Full workspace test suite
- **Type**: Regression
- **Method**: `cargo test --workspace`
- **Pass criteria**: All tests pass (store + vector + embed + core + engine + adapt + learn + observe + server)
- **Location**: CI / command line

## Unit Tests for New Code

### T-LRN-01: TrainingReservoir<T> generic construction
- **Location**: `crates/unimatrix-learn/src/reservoir.rs`
- Test with T=u32, T=String, T=custom struct
- Verify add/sample/len/clear work for each

### T-LRN-02: Persistence helpers
- **Location**: `crates/unimatrix-learn/src/persistence.rs`
- save_atomic: writes file atomically (no .tmp left behind)
- load_bytes: missing file -> None, empty file -> None, valid file -> Some(bytes)
- save_atomic + load_bytes round-trip

### T-LRN-03: NeuralConfig defaults
- **Location**: `crates/unimatrix-learn/src/config.rs`
- Default values match specification
- classifier_topology = [32, 64, 32, 5]
- scorer_topology = [32, 32, 1]
