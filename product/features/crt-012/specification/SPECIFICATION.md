# crt-012: Specification — Neural Pipeline Cleanup

## Domain Model

### Entities Modified

**TrainingReservoir<T: Clone>** (canonical: `unimatrix-learn/src/reservoir.rs`)
- No structural changes. Already generic. Becomes the single source of truth.
- Adapt-side usage: `TrainingReservoir<TrainingPair>`.

**EwcState** (canonical: `unimatrix-learn/src/ewc.rs`)
- No structural changes. Becomes the single source of truth via re-export.
- All existing methods retained: `new()`, `update()`, `update_from_flat()`, `penalty()`, `gradient_contribution()`, `to_vecs()`, `from_vecs()`, `is_initialized()`.

**LearnConfig** (`unimatrix-learn/src/config.rs`)
- New field: `classifier_init_seed: u64` (default: 42)
- New field: `scorer_init_seed: u64` (default: 123)

**AdaptConfig** (`unimatrix-adapt/src/config.rs`)
- New field: `reservoir_seed: u64` (default: 42, `#[serde(default)]`)
- New field: `init_seed: u64` (default: 42, `#[serde(default)]`)

**SignalClassifier** (`unimatrix-learn/src/models/classifier.rs`)
- New constructor: `new_with_baseline_seed(seed: u64)`
- Existing: `new_with_baseline()` delegates to `new_with_baseline_seed(42)`

**ConventionScorer** (`unimatrix-learn/src/models/scorer.rs`)
- New constructor: `new_with_baseline_seed(seed: u64)`
- Existing: `new_with_baseline()` delegates to `new_with_baseline_seed(123)`

### Entities Removed

**TrainingReservoir** (from `unimatrix-adapt/src/training.rs`)
- Concrete type removed. Replaced by `unimatrix_learn::reservoir::TrainingReservoir<TrainingPair>`.
- `TrainingPair` struct remains in `training.rs` (it is adapt-specific).

**EwcState** (from `unimatrix-adapt/src/regularization.rs`)
- Duplicate implementation removed. Module becomes a re-export.

## Acceptance Criteria

### AC-01: EwcState Deduplication
- `unimatrix-adapt/src/regularization.rs` contains only `pub use unimatrix_learn::ewc::EwcState;` (plus optional module doc).
- No EwcState struct definition exists in `unimatrix-adapt`.
- All adapt-side code that uses `EwcState` compiles without changes to import paths.

### AC-02: TrainingReservoir Deduplication
- No `TrainingReservoir` struct definition exists in `unimatrix-adapt/src/training.rs`.
- `unimatrix-adapt` imports `TrainingReservoir` from `unimatrix-learn::reservoir`.
- The adapt-side reservoir type is `TrainingReservoir<TrainingPair>`.
- `TrainingPair` struct remains in `unimatrix-adapt/src/training.rs`.

### AC-03: AdaptConfig Reservoir Seed
- `AdaptConfig` has a `reservoir_seed: u64` field with default value `42`.
- `AdaptationService::new()` passes `config.reservoir_seed` to `TrainingReservoir::new()`.
- The field has `#[serde(default)]` for backward-compatible deserialization.

### AC-04: AdaptConfig Init Seed
- `AdaptConfig` has an `init_seed: u64` field with default value `42`.
- `AdaptationService::new()` passes `config.init_seed` to `MicroLoRA::with_seed()`.
- The field has `#[serde(default)]` for backward-compatible deserialization.

### AC-05: LearnConfig Model Init Seeds
- `LearnConfig` has `classifier_init_seed: u64` (default 42) and `scorer_init_seed: u64` (default 123).
- `TrainingService::new()` and `try_train_step()` use these seeds when constructing models.

### AC-06: Classifier Seed Constructor
- `SignalClassifier::new_with_baseline_seed(seed: u64)` exists and produces deterministic weights from the given seed.
- `SignalClassifier::new_with_baseline()` calls `new_with_baseline_seed(42)` and produces identical output to the current implementation.

### AC-07: Scorer Seed Constructor
- `ConventionScorer::new_with_baseline_seed(seed: u64)` exists and produces deterministic weights from the given seed.
- `ConventionScorer::new_with_baseline()` calls `new_with_baseline_seed(123)` and produces identical output to the current implementation.

### AC-08: Persistence Backward Compatibility
- Existing `adaptation.state` files (version 1) deserialize successfully with new `AdaptConfig` fields receiving default values.
- The `AdaptationState` version remains 1.
- A round-trip test (save with new fields, load without them in config) passes.

### AC-09: Test Suite Passes
- All existing unit tests in `unimatrix-learn` pass without modification.
- All existing unit tests in `unimatrix-adapt` pass (with import path adjustments only where needed due to type unification).
- All integration tests pass.
- No new test failures introduced.

### AC-10: Duplicate Tests Removed
- Tests T-REG-01 through T-REG-09 in `unimatrix-adapt/src/regularization.rs` are removed (covered by identical tests in `unimatrix-learn/src/ewc.rs`).
- Tests T-TRN-01 through T-TRN-03 and T-TRN-10 for `TrainingReservoir` in `unimatrix-adapt/src/training.rs` are removed (covered by T-LC-01 through T-LC-03 and `reservoir_overflow_no_growth` in `unimatrix-learn/src/reservoir.rs`).

### AC-11: Different Seeds Produce Different Results
- `SignalClassifier::new_with_baseline_seed(42)` and `new_with_baseline_seed(99)` produce different weight matrices.
- `ConventionScorer::new_with_baseline_seed(123)` and `new_with_baseline_seed(99)` produce different weight matrices.
- `TrainingReservoir::new(100, 42)` and `TrainingReservoir::new(100, 99)` produce different sampling sequences.

### AC-12: No Public API Changes
- No changes to MCP tool interfaces.
- No changes to `AdaptationService` public method signatures.
- No changes to `TrainingService` public method signatures.
- `unimatrix-server` requires zero changes.

## Constraints

### C-01: No New Crates
No `unimatrix-ml-core` or similar. Shared types live in `unimatrix-learn`.

### C-02: Default Values Preserve Behavior
All new config field defaults must match the previously hardcoded values. Tests relying on deterministic initialization (e.g., T-CS-01 baseline zero-digest noise classification) must continue to pass.

### C-03: Serde Compatibility
New fields in `AdaptConfig` must use `#[serde(default)]` with explicit default functions returning the correct values. Bincode v2 serde path must handle missing fields gracefully.

### C-04: No Downstream Crate Changes
`unimatrix-server`, `unimatrix-observe`, `unimatrix-engine`, `unimatrix-core` must not require any source changes.

### C-05: Anti-Stub
No TODO comments, no `unimplemented!()`, no placeholder functions. Complete implementation required.
