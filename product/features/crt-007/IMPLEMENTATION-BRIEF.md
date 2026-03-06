# Implementation Brief: crt-007 Neural Extraction Pipeline

## Overview

Add neural classification to the col-013 rule-based extraction pipeline. Three implementation waves: (1) extract shared training infrastructure into unimatrix-learn, (2) build two ndarray-based MLP models with hand-rolled forward/backward behind a NeuralModel trait, (3) integrate via shadow mode with model versioning. Zero new external dependencies.

## Resolved Decisions

| ADR | Decision | Unimatrix ID |
|-----|----------|-------------|
| ADR-001 | ndarray-only for neural models (NeuralModel trait) | #404 (deprecated) -> #420 |
| ADR-002 | Flat Vec<f32> parameter interface for EwcState | #405 |
| ADR-003 | Fixed-width 32-slot SignalDigest | #406 |
| ADR-004 | Shadow evaluation persistence in SQLite | #407 |
| ADR-005 | Three-slot model versioning (production/shadow/previous) | #408 |

## Implementation Waves

### Wave 1: Shared Training Infrastructure (~250 lines)

**Crate**: `crates/unimatrix-learn/`

1. Create `crates/unimatrix-learn/Cargo.toml` with dependencies: ndarray 0.16, rand 0.9, serde, bincode
2. Extract `TrainingReservoir<T: Clone>` from `unimatrix-adapt/src/training.rs` -> `unimatrix-learn/src/reservoir.rs`
   - Generalize: remove `TrainingPair` coupling, accept any `T: Clone`
   - `add(&mut self, items: &[T])` instead of `add(&mut self, pairs: &[(u64, u64, u32)])`
   - `sample_batch` returns `Vec<&T>`
3. Extract `EwcState` from `unimatrix-adapt/src/regularization.rs` -> `unimatrix-learn/src/ewc.rs`
   - Replace `update(params, grad_a, grad_b)` with `update_from_flat(params: &[f32], grad_squared: &[f32])`
   - Keep `penalty`, `gradient_contribution`, `to_vecs`, `from_vecs` unchanged
4. Extract atomic save/load from `unimatrix-adapt/src/persistence.rs` -> `unimatrix-learn/src/persistence.rs`
   - `save_atomic(data: &[u8], dir: &Path, filename: &str)`
   - `load_file(dir: &Path, filename: &str) -> Result<Option<Vec<u8>>>`
5. Refactor `unimatrix-adapt` to depend on `unimatrix-learn`:
   - `training.rs`: `type Reservoir = unimatrix_learn::TrainingReservoir<TrainingPair>;`
   - `regularization.rs`: re-export from unimatrix-learn
   - `persistence.rs`: use shared save_atomic/load_file
   - Adjust `execute_training_step` to flatten gradients before `ewc.update_from_flat()`
6. Run `cargo test -p unimatrix-adapt` -- all 174+ tests must pass

**Gate**: All adapt tests pass. No public API changes.

### Wave 2: Neural Models (~350 lines)

**Crate**: `crates/unimatrix-learn/` (no new external dependencies)

1. Implement `NeuralModel` trait in `models/traits.rs`:
   - `forward(&self, input: &[f32]) -> Vec<f32>`
   - `train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32`
   - `flat_parameters(&self) -> Vec<f32>`
   - `set_parameters(&mut self, params: &[f32])`
   - `serialize(&self) -> Vec<u8>`
   - `deserialize(data: &[u8]) -> Result<Self, String>`
2. Implement `SignalDigest` in `models/digest.rs`:
   - `pub struct SignalDigest { pub features: [f32; 32] }`
   - `pub fn from_proposed(entry: &ProposedEntry) -> Self` -- populate slots 0-6, zero rest
   - Category/rule ordinal encoding constants
3. Implement `SignalClassifier` in `models/classifier.rs`:
   - ndarray MLP: Linear(32,64) -> Sigmoid -> Linear(64,32) -> ReLU -> Linear(32,5) -> Softmax
   - Hand-rolled forward pass: `Array2<f32>` weight matrices + `Array1<f32>` bias vectors
   - Hand-rolled backward pass: gradient computation per layer (~30 lines)
   - `pub fn new_with_baseline() -> Self` -- output bias [0, 0, 0, 0, +2.0] for Noise
   - `pub fn classify(&self, digest: &SignalDigest) -> ClassificationResult`
   - Implements `NeuralModel`
4. Implement `ConventionScorer` in `models/scorer.rs`:
   - ndarray MLP: Linear(32,32) -> ReLU -> Linear(32,1) -> Sigmoid
   - Hand-rolled forward/backward passes
   - `pub fn new_with_baseline() -> Self` -- output bias -2.0
   - `pub fn score(&self, digest: &SignalDigest) -> f32`
   - Implements `NeuralModel`
5. Implement `ModelRegistry` in `registry.rs`:
   - Three slots: Production, Shadow, Previous
   - `promote()`, `rollback()`, `get_production()`, `get_shadow()`
   - Registry state persisted to JSON
   - ModelVersion metadata: generation, timestamp, accuracy, schema_version
6. Unit tests: model inference, gradient correctness (numerical checks), registry state transitions

**Gate**: Classifier produces non-degenerate output on test digests. Scorer output in [0,1]. Registry state transitions correct.

### Wave 3: Shadow Mode + Pipeline Integration (~200 lines)

**Crate**: `crates/unimatrix-observe/` + `crates/unimatrix-server/`

1. Add `shadow_evaluations` table to SQLite schema (schema version bump in unimatrix-store)
2. Implement `NeuralEnhancer` in `unimatrix-observe/src/extraction/neural.rs`:
   - Wraps classifier + scorer + mode (Shadow/Active)
   - `enhance(entry: &ProposedEntry) -> NeuralPrediction`
3. Implement `ShadowEvaluator` in `unimatrix-observe/src/extraction/shadow.rs`:
   - `log_prediction(entry, prediction, rule_accepted)`
   - `accuracy() -> ShadowAccuracy` (per-category and overall)
   - Writes to shadow_evaluations table
4. Integrate into `extraction_tick()` in `unimatrix-server/src/background.rs`:
   - After rules produce proposals, before quality gate
   - Initialize NeuralEnhancer during server startup (with graceful fallback)
   - In shadow mode: log predictions, pass entries through unchanged
5. Add `"neural" -> 0.40` trust_source weight in confidence service (~5 lines)
6. Integration test: end-to-end shadow mode

**Gate**: Shadow mode logs predictions without affecting extraction output. Graceful degradation when models unavailable.

## Key Files to Modify

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add unimatrix-learn to members (already in crates/*) |
| `crates/unimatrix-learn/Cargo.toml` | New crate |
| `crates/unimatrix-learn/src/lib.rs` | New: module declarations |
| `crates/unimatrix-learn/src/reservoir.rs` | Extracted: TrainingReservoir<T> |
| `crates/unimatrix-learn/src/ewc.rs` | Extracted: EwcState |
| `crates/unimatrix-learn/src/persistence.rs` | Extracted: save_atomic, load_file |
| `crates/unimatrix-learn/src/registry.rs` | New: ModelRegistry |
| `crates/unimatrix-learn/src/config.rs` | New: LearnConfig |
| `crates/unimatrix-learn/src/models/mod.rs` | New: module re-exports, shared types |
| `crates/unimatrix-learn/src/models/traits.rs` | New: NeuralModel trait |
| `crates/unimatrix-learn/src/models/digest.rs` | New: SignalDigest |
| `crates/unimatrix-learn/src/models/classifier.rs` | New: SignalClassifier |
| `crates/unimatrix-learn/src/models/scorer.rs` | New: ConventionScorer |
| `crates/unimatrix-adapt/Cargo.toml` | Add unimatrix-learn dependency |
| `crates/unimatrix-adapt/src/training.rs` | Refactor: use shared TrainingReservoir |
| `crates/unimatrix-adapt/src/regularization.rs` | Refactor: use shared EwcState |
| `crates/unimatrix-adapt/src/persistence.rs` | Refactor: use shared save_atomic/load_file |
| `crates/unimatrix-observe/Cargo.toml` | Add unimatrix-learn dependency |
| `crates/unimatrix-observe/src/extraction/mod.rs` | Add neural, shadow modules |
| `crates/unimatrix-observe/src/extraction/neural.rs` | New: NeuralEnhancer |
| `crates/unimatrix-observe/src/extraction/shadow.rs` | New: ShadowEvaluator |
| `crates/unimatrix-server/Cargo.toml` | Add unimatrix-learn dependency |
| `crates/unimatrix-server/src/background.rs` | Integrate neural enhancement into extraction_tick |
| `crates/unimatrix-server/src/services/confidence.rs` | Add "neural" trust_source weight |
| `crates/unimatrix-store/src/schema.rs` | Add shadow_evaluations table (schema bump) |

## Risk Mitigations

| Risk | Mitigation in Implementation |
|------|------------------------------|
| R-01 (adapt breakage) | Wave 1 gate: all adapt tests pass before Wave 2 starts |
| R-03 (gradient correctness) | Numerical gradient checks in Wave 2 gate |
| R-04 (degenerate weights) | Smoke tests in Wave 2 gate |
| R-05 (bad promotion) | Unit tests for all promotion criteria in Wave 2 |
| R-06 (model compat) | schema_version in ModelVersion, corruption fallback |

## Estimated Scope

~800 lines total:
- Wave 1: ~250 lines (extraction + refactoring)
- Wave 2: ~350 lines (models + registry)
- Wave 3: ~200 lines (shadow + integration)

GitHub Issue: #109
