# Architecture: crt-007 Neural Extraction Pipeline

## System Overview

crt-007 introduces neural classification into the existing rule-based extraction pipeline (col-013). The feature spans three architectural concerns:

1. **Shared training infrastructure** -- Extract generic ML primitives from `unimatrix-adapt` into a new `unimatrix-learn` crate, making them reusable across both MicroLoRA adaptation (crt-006) and neural extraction models (crt-007+).
2. **Neural models** -- Two burn-based MLP models (Signal Classifier, Convention Scorer) that classify and score extraction pipeline outputs.
3. **Shadow mode integration** -- Wire models into the col-013 extraction pipeline with observability, promotion logic, and rollback safety.

```
                         unimatrix-learn (NEW)
                        /                     \
              unimatrix-adapt                  burn models
              (MicroLoRA, crt-006)             (Classifier, Scorer)
                        \                     /
                         unimatrix-server
                         background.rs (extraction_tick)
                              |
                         unimatrix-observe
                         extraction/ (rules, quality gate)
```

## Component Breakdown

### C1: unimatrix-learn (new crate)

**Responsibility**: Shared ML infrastructure + neural model definitions.

| Module | Source | Contents |
|--------|--------|----------|
| `reservoir.rs` | Extracted from `unimatrix-adapt/src/training.rs` | `TrainingReservoir<T>` -- generic over sample type |
| `ewc.rs` | Extracted from `unimatrix-adapt/src/regularization.rs` | `EwcState` -- flat `Vec<f32>` parameter interface |
| `persistence.rs` | Extracted from `unimatrix-adapt/src/persistence.rs` | Atomic save/load helpers (not `AdaptationState`) |
| `registry.rs` | New | `ModelRegistry` -- production/shadow/previous slot management |
| `models/classifier.rs` | New | `SignalClassifier` -- burn MLP for 5-class signal classification |
| `models/scorer.rs` | New | `ConventionScorer` -- burn MLP for convention confidence scoring |
| `models/digest.rs` | New | `SignalDigest` -- fixed-width 32-slot feature vector |
| `models/mod.rs` | New | Model trait, shared inference types |
| `config.rs` | New | `LearnConfig` -- shared ML configuration |

### C2: unimatrix-adapt (refactored)

**Responsibility**: MicroLoRA embedding adaptation (unchanged public API).

Changes:
- `training.rs`: `TrainingReservoir` replaced with `use unimatrix_learn::TrainingReservoir<TrainingPair>`
- `regularization.rs`: `EwcState` replaced with `use unimatrix_learn::EwcState`
- `persistence.rs`: Uses shared atomic save/load helpers; `AdaptationState` struct stays (MicroLoRA-specific)
- New dependency: `unimatrix-learn`
- All 174+ existing tests must pass unchanged

### C3: Shadow Mode (in unimatrix-observe + unimatrix-server)

**Responsibility**: Run neural models alongside rules, log predictions, manage promotion.

| Location | Component | Purpose |
|----------|-----------|---------|
| `unimatrix-observe/src/extraction/neural.rs` | `NeuralEnhancer` | Wraps classifier + scorer, produces `NeuralPrediction` |
| `unimatrix-observe/src/extraction/shadow.rs` | `ShadowEvaluator` | Compares neural vs rule predictions, logs metrics |
| `unimatrix-server/src/background.rs` | Integration point | Calls `NeuralEnhancer` between rules and quality gate |
| SQLite | `shadow_evaluations` table | Persists prediction logs for accuracy tracking |

## Component Interactions

### Extraction Pipeline Flow (with neural enhancement)

```
extraction_tick()
    |
    v
1. Query observations (existing)
    |
    v
2. Run extraction rules -> Vec<ProposedEntry> (existing)
    |
    v
3. [NEW] Build SignalDigest for each ProposedEntry
    |
    v
4. [NEW] NeuralEnhancer::classify(digest) -> NeuralPrediction
    |       |
    |       +-- In SHADOW mode: log prediction, pass original entry unchanged
    |       +-- In ACTIVE mode: reclassify/rescore entry based on neural output
    |
    v
5. Quality gate pipeline (existing checks 1-4)
    |
    v
6. Embedding-based checks 5-6 (existing)
    |
    v
7. Store accepted entries (existing, with trust_source = "neural" if neural-enhanced)
```

### Data Flow: SignalDigest Construction

`ProposedEntry` fields + store queries -> `SignalDigest`:

| Slot | Feature | Source |
|------|---------|--------|
| 0 | `extraction_confidence` | `ProposedEntry.extraction_confidence` |
| 1 | `source_feature_count` | `ProposedEntry.source_features.len()` |
| 2 | `content_length_norm` | `ProposedEntry.content.len() / 1000.0` clamped to [0,1] |
| 3 | `category_idx` | Ordinal encoding of `ProposedEntry.category` |
| 4 | `rule_idx` | Ordinal encoding of `ProposedEntry.source_rule` |
| 5 | `title_length_norm` | `ProposedEntry.title.len() / 200.0` clamped to [0,1] |
| 6 | `tag_count_norm` | `ProposedEntry.tags.len() / 10.0` clamped to [0,1] |
| 7-31 | Reserved | Zero-initialized (future: crt-008/009 signals) |

### Shared Infrastructure Data Flow

```
unimatrix-learn
  TrainingReservoir<T>  <--- unimatrix-adapt (T = TrainingPair)
                        <--- unimatrix-learn models (T = LabeledDigest, future crt-008)
  EwcState              <--- unimatrix-adapt (flat params from MicroLoRA)
                        <--- unimatrix-learn models (flat params from burn, future crt-008)
  ModelRegistry         <--- unimatrix-server (manages model lifecycle)
  save_atomic / load    <--- unimatrix-adapt (AdaptationState)
                        <--- unimatrix-learn models (burn model records)
```

## Technology Decisions

### ADR-001: burn Framework Selection

See `ADR-001-burn-framework.md`.

### ADR-002: Flat Parameter Interface for EwcState

See `ADR-002-flat-parameter-ewc.md`.

### ADR-003: SignalDigest Fixed-Width Layout

See `ADR-003-signal-digest-layout.md`.

### ADR-004: Shadow Mode Persistence in SQLite

See `ADR-004-shadow-persistence.md`.

### ADR-005: Model Slot Architecture

See `ADR-005-model-slot-architecture.md`.

## Integration Points

### Existing Dependencies

| Dependency | Crate | Usage |
|-----------|-------|-------|
| `ndarray 0.16` | unimatrix-adapt | Matrix ops for MicroLoRA |
| `bincode 2` | unimatrix-adapt | State serialization |
| `rusqlite` | unimatrix-store | SQLite access |
| `unimatrix-observe` | unimatrix-server | Extraction rules, quality gate |

### New Dependencies

| Dependency | Crate | Usage |
|-----------|-------|-------|
| `burn 0.16` | unimatrix-learn | Neural model definition and inference |
| `burn-ndarray 0.16` | unimatrix-learn | CPU backend for burn |
| `unimatrix-learn` | unimatrix-adapt | Shared infra (reservoir, EWC) |
| `unimatrix-learn` | unimatrix-server | Model inference, registry |

### ndarray Version Compatibility (SR-02)

burn 0.16 uses its own tensor type, not ndarray directly. The boundary is `Vec<f32>`:
- `unimatrix-adapt` uses `ndarray::Array1/Array2` internally
- `unimatrix-learn` shared infra uses `Vec<f32>` for parameter exchange
- burn models use `Tensor<B, D>` internally, convert to/from `Vec<f32>` at boundaries

No ndarray version conflict because burn does not re-export ndarray.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `TrainingReservoir<T>` | `pub struct TrainingReservoir<T: Clone> { ... }` | `unimatrix-learn/src/reservoir.rs` |
| `TrainingReservoir::new` | `pub fn new(capacity: usize, seed: u64) -> Self` | `unimatrix-learn/src/reservoir.rs` |
| `TrainingReservoir::add` | `pub fn add(&mut self, items: &[T])` | `unimatrix-learn/src/reservoir.rs` |
| `TrainingReservoir::sample_batch` | `pub fn sample_batch(&mut self, batch_size: usize) -> Vec<&T>` | `unimatrix-learn/src/reservoir.rs` |
| `EwcState::new` | `pub fn new(param_count: usize, alpha: f32, lambda: f32) -> Self` | `unimatrix-learn/src/ewc.rs` |
| `EwcState::penalty` | `pub fn penalty(&self, current_params: &[f32]) -> f32` | `unimatrix-learn/src/ewc.rs` |
| `EwcState::gradient_contribution` | `pub fn gradient_contribution(&self, current_params: &[f32]) -> Vec<f32>` | `unimatrix-learn/src/ewc.rs` |
| `EwcState::update_from_flat` | `pub fn update_from_flat(&mut self, params: &[f32], grad_squared: &[f32])` | `unimatrix-learn/src/ewc.rs` |
| `EwcState::to_vecs` / `from_vecs` | `pub fn to_vecs(&self) -> (Vec<f32>, Vec<f32>)` | `unimatrix-learn/src/ewc.rs` |
| `SignalDigest` | `pub struct SignalDigest { pub features: [f32; 32] }` | `unimatrix-learn/src/models/digest.rs` |
| `SignalDigest::from_proposed` | `pub fn from_proposed(entry: &ProposedEntry) -> Self` | `unimatrix-learn/src/models/digest.rs` |
| `SignalClassifier::classify` | `pub fn classify(&self, digest: &SignalDigest) -> ClassificationResult` | `unimatrix-learn/src/models/classifier.rs` |
| `ClassificationResult` | `pub struct ClassificationResult { pub category: SignalCategory, pub probabilities: [f32; 5], pub confidence: f32 }` | `unimatrix-learn/src/models/classifier.rs` |
| `SignalCategory` | `pub enum SignalCategory { Convention, Pattern, Gap, Dead, Noise }` | `unimatrix-learn/src/models/classifier.rs` |
| `ConventionScorer::score` | `pub fn score(&self, digest: &SignalDigest) -> f32` | `unimatrix-learn/src/models/scorer.rs` |
| `ModelRegistry::new` | `pub fn new(models_dir: PathBuf) -> Self` | `unimatrix-learn/src/registry.rs` |
| `ModelRegistry::get_production` | `pub fn get_production(&self, model_name: &str) -> Option<&ModelVersion>` | `unimatrix-learn/src/registry.rs` |
| `ModelRegistry::promote` | `pub fn promote(&mut self, model_name: &str) -> Result<(), RegistryError>` | `unimatrix-learn/src/registry.rs` |
| `ModelRegistry::rollback` | `pub fn rollback(&mut self, model_name: &str) -> Result<(), RegistryError>` | `unimatrix-learn/src/registry.rs` |
| `ModelVersion` | `pub struct ModelVersion { pub generation: u64, pub timestamp: u64, pub accuracy: Option<f64>, pub burn_version: String, pub slot: ModelSlot }` | `unimatrix-learn/src/registry.rs` |
| `ModelSlot` | `pub enum ModelSlot { Production, Shadow, Previous }` | `unimatrix-learn/src/registry.rs` |
| `NeuralEnhancer::new` | `pub fn new(classifier: SignalClassifier, scorer: ConventionScorer, mode: EnhancerMode) -> Self` | `unimatrix-observe/src/extraction/neural.rs` |
| `NeuralEnhancer::enhance` | `pub fn enhance(&self, entry: &ProposedEntry) -> NeuralPrediction` | `unimatrix-observe/src/extraction/neural.rs` |
| `NeuralPrediction` | `pub struct NeuralPrediction { pub classification: ClassificationResult, pub convention_score: f32, pub digest: SignalDigest }` | `unimatrix-observe/src/extraction/neural.rs` |
| `EnhancerMode` | `pub enum EnhancerMode { Shadow, Active }` | `unimatrix-observe/src/extraction/neural.rs` |
| `ShadowEvaluator::log_prediction` | `pub fn log_prediction(&mut self, entry: &ProposedEntry, prediction: &NeuralPrediction, rule_accepted: bool)` | `unimatrix-observe/src/extraction/shadow.rs` |
| `ShadowEvaluator::accuracy` | `pub fn accuracy(&self) -> ShadowAccuracy` | `unimatrix-observe/src/extraction/shadow.rs` |
| `save_atomic` | `pub fn save_atomic(data: &[u8], dir: &Path, filename: &str) -> Result<(), String>` | `unimatrix-learn/src/persistence.rs` |
| `load_file` | `pub fn load_file(dir: &Path, filename: &str) -> Result<Option<Vec<u8>>, String>` | `unimatrix-learn/src/persistence.rs` |

### Error Boundaries

| Boundary | Error Type | Handling |
|----------|-----------|----------|
| burn model inference failure | `ModelError` (new) | Log, fall back to rule-only (no neural enhancement) |
| Model file missing/corrupt | `RegistryError` (new) | Cold-start with baseline weights; log warning |
| Shadow evaluation DB write failure | `rusqlite::Error` | Log, skip shadow log (non-fatal) |
| unimatrix-learn shared infra | Same error types as current adapt | Transparent -- adapt consumers see same errors |

### crt-002 Integration

Add `"neural"` to the `trust_source_weight` map in confidence scoring:

```rust
// In unimatrix-server confidence computation
"auto" => 0.35,   // existing (col-013)
"neural" => 0.40, // new (crt-007)
```

This is ~5 lines in the confidence service.

## Workspace Dependency Graph

```
unimatrix-learn (new)
  deps: ndarray 0.16, rand 0.9, serde, bincode, burn 0.16, burn-ndarray 0.16

unimatrix-adapt (modified)
  deps: ndarray 0.16, rand 0.9, serde, bincode, unimatrix-learn (new dep)

unimatrix-observe (modified)
  deps: ..., unimatrix-learn (new dep), unimatrix-store

unimatrix-server (modified)
  deps: ..., unimatrix-learn (new dep)
```
