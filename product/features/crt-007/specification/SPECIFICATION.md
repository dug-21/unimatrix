# Specification: crt-007 Neural Extraction Pipeline

## Domain Model

### Core Types

#### SignalDigest
Fixed-width feature vector consumed by neural models.

```
SignalDigest {
    features: [f32; 32]     -- normalized input features, [0.0, 1.0] per slot
    schema_version: u32     -- tracks slot assignment version (initial: 1)
    source_rule: String     -- name of extraction rule that produced the digest
    feature_cycle: String   -- feature cycle context
}
```

**Slot assignments (schema_version = 1):**
| Slot | Name | Normalization |
|------|------|---------------|
| 0 | search_miss_count | log(n+1) / log(max+1), max=100 |
| 1 | co_access_density | raw ratio [0, 1] |
| 2 | consistency_score | features_matching / total_features |
| 3 | feature_count | log(n+1) / log(50+1) |
| 4 | observation_count | log(n+1) / log(1000+1) |
| 5 | age_days | 1.0 - exp(-age_days / 90.0) |
| 6 | rule_confidence | raw [0, 1] from extraction rule |
| 7-31 | reserved | 0.0 |

#### ClassificationResult
Output of the Signal Classifier.

```
ClassificationResult {
    probabilities: [f32; 5]   -- [convention, pattern, gap, dead, noise]
    predicted_class: SignalClass
    confidence: f32           -- max probability value
}
```

#### SignalClass (enum)
```
Convention  -- recurring behavioral pattern worth codifying
Pattern     -- structural pattern (file dependency, co-access cluster)
Gap         -- missing knowledge (zero-result searches)
Dead        -- stale knowledge (access cliff)
Noise       -- not actionable (suppress)
```

#### ConventionScore
Output of the Convention Scorer.

```
ConventionScore {
    score: f32    -- convention confidence [0.0, 1.0]
}
```

#### ShadowEvaluation
Persisted record of a shadow mode comparison.

```
ShadowEvaluation {
    id: i64                   -- autoincrement
    model_name: String        -- "signal_classifier" | "convention_scorer"
    model_version: u32        -- version of the shadow model
    ts_millis: i64            -- evaluation timestamp
    signal_digest: [f32; 32]  -- input features
    rule_prediction: String   -- what the rule-only pipeline predicted
    neural_prediction: String -- what the neural model predicted
    neural_confidence: f32    -- model confidence in its prediction
    ground_truth: Option<String> -- filled by feedback (crt-008)
    feature_cycle: Option<String>
}
```

#### ModelSlot
Registry entry for a named model.

```
ModelSlot {
    name: String
    production: Option<LoadedModel>
    shadow: Option<LoadedModel>
    previous_path: Option<PathBuf>
    metrics: RollingMetrics
    schema_version: u32
    features_observed: u64
    state: ModelState
}
```

#### ModelState (enum)
```
Observation    -- features < 5: models observe but produce no output
Shadow         -- features >= 5: models predict, results logged, no influence
Production     -- promoted: models influence extraction decisions
RolledBack     -- production model rolled back to previous
```

#### LoadedModel
```
LoadedModel {
    version: u32
    path: PathBuf
    accuracy: f64          -- accuracy at promotion time (or baseline)
    evaluation_count: u64
    created_at: i64        -- epoch millis
}
```

#### RollingMetrics
Sliding window accuracy tracker.

```
RollingMetrics {
    window: VecDeque<(bool, f64)>  -- (correct, confidence) pairs
    capacity: usize                -- default 100
}
```

Methods:
- `record(correct: bool, confidence: f64)` -- push, evict oldest if at capacity
- `accuracy() -> f64` -- proportion of correct predictions in window
- `mean_confidence() -> f64` -- average confidence in window
- `count() -> usize` -- current window size

### NeuralModel Trait

```
trait NeuralModel: Send + Sync {
    type Input
    type Output

    fn predict(&self, input: &Self::Input) -> Self::Output
    fn save(&self, path: &Path) -> Result<(), String>
    fn load(path: &Path) -> Result<Self, String> where Self: Sized
    fn param_count(&self) -> usize
    fn params_flat(&self) -> Vec<f32>
}
```

### NeuralConfig

```
NeuralConfig {
    models_dir: PathBuf                  -- default: {data_dir}/models/
    classifier_topology: Vec<u32>        -- default: [32, 64, 32, 5]
    scorer_topology: Vec<u32>            -- default: [32, 32, 1]
    classifier_noise_bias: f32           -- default: 2.0
    scorer_output_bias: f32              -- default: -1.0
    shadow_min_evaluations: u64          -- default: 20
    shadow_promotion_threshold: f64      -- default: 0.0 (>= rule accuracy)
    rollback_accuracy_drop: f64          -- default: 0.05
    rolling_window_size: usize           -- default: 100
    observation_feature_threshold: u64   -- default: 5
    neural_override_confidence: f32      -- default: 0.8
}
```

## Functional Requirements

### FR-01: Shared Training Infrastructure Extraction

**TrainingReservoir<T>** must be genericized from the current `TrainingPair`-specific implementation:
- Generic type parameter `T: Clone`
- `new(capacity: usize, seed: u64) -> Self`
- `add(&mut self, items: &[T])` -- reservoir sampling
- `sample_batch(&mut self, batch_size: usize) -> Vec<&T>`
- `len() -> usize`
- `is_empty() -> bool`
- `clear(&mut self)`

**EwcState** must be generalized for flat parameter vectors:
- `new(param_count: usize, alpha: f32, lambda: f32) -> Self`
- `penalty(&self, current_params: &[f32]) -> f32`
- `gradient_contribution(&self, current_params: &[f32]) -> Vec<f32>`
- `update(&mut self, current_params: &[f32], gradients: &[f32])` -- flat gradient vector (previously took `Array2<f32>`)
- Must preserve existing behavior for MicroLoRA (when called via `unimatrix-adapt`)

**Persistence helpers**:
- `save_atomic(data: &[u8], path: &Path) -> Result<(), String>` -- write to tmp, rename
- `load_bytes(path: &Path) -> Result<Option<Vec<u8>>, String>` -- read with graceful fallback

### FR-02: Signal Classifier

MLP with topology `input(32) -> hidden(64, sigmoid-symmetric) -> hidden(32, sigmoid-symmetric) -> output(5, softmax)`.

- `SignalClassifier::new(config: &NeuralConfig) -> Self` -- creates with baseline weights
- `SignalClassifier::predict(&self, digest: &SignalDigest) -> ClassificationResult`
- `SignalClassifier::save(&self, path: &Path) -> Result<(), String>`
- `SignalClassifier::load(path: &Path) -> Result<Self, String>`

Baseline weight initialization:
- Hidden layers: Xavier/Glorot uniform
- Output layer weights: Xavier/Glorot uniform
- Output layer biases: [convention=-0.5, pattern=-0.5, gap=0.0, dead=0.0, noise=+2.0]

Inference constraints:
- Input: `SignalDigest.features` (32 f32 values)
- Output: 5-class probability distribution summing to 1.0
- Latency: < 50ms on modern CPU
- Deterministic: same input produces same output (no dropout at inference)

### FR-03: Convention Scorer

MLP with topology `input(32) -> hidden(32, sigmoid-symmetric) -> output(1, sigmoid)`.

- `ConventionScorer::new(config: &NeuralConfig) -> Self` -- creates with baseline weights
- `ConventionScorer::predict(&self, digest: &SignalDigest) -> ConventionScore`
- `ConventionScorer::save(&self, path: &Path) -> Result<(), String>`
- `ConventionScorer::load(path: &Path) -> Result<Self, String>`

Baseline weight initialization:
- Hidden layer: Xavier/Glorot uniform
- Output layer weight: Xavier/Glorot uniform
- Output layer bias: -1.0 (sigmoid(-1.0) ~= 0.27)

Inference constraints:
- Input: `SignalDigest.features` (32 f32 values)
- Output: single f32 in [0.0, 1.0]
- Latency: < 10ms on modern CPU

### FR-04: ModelRegistry

- `ModelRegistry::new(models_dir: PathBuf) -> Self`
- `ModelRegistry::register(name: &str, model: impl NeuralModel, config: &NeuralConfig) -> Result<(), String>`
- `ModelRegistry::get_production(name: &str) -> Option<&LoadedModel>`
- `ModelRegistry::get_shadow(name: &str) -> Option<&LoadedModel>`
- `ModelRegistry::promote_shadow(name: &str) -> Result<(), String>` -- shadow -> production, production -> previous
- `ModelRegistry::rollback(name: &str) -> Result<(), String>` -- previous -> production
- `ModelRegistry::save_registry(&self) -> Result<(), String>` -- persist registry.json
- `ModelRegistry::load_registry(models_dir: &Path) -> Result<Self, String>`
- `ModelRegistry::state(name: &str) -> ModelState`
- `ModelRegistry::check_promotion(name: &str) -> bool` -- evaluates promotion criteria
- `ModelRegistry::check_rollback(name: &str) -> bool` -- evaluates rollback criteria

Promotion criteria (all must be true):
- Shadow evaluation count >= `shadow_min_evaluations` (default 20)
- Shadow accuracy >= production accuracy (or rule-only baseline for first promotion)
- No per-category regression (query shadow_evaluations for per-class accuracy)

Rollback criteria (any triggers rollback):
- Rolling accuracy drops > `rollback_accuracy_drop` (default 5%) below pre-promotion accuracy
- NaN or Inf detected in model parameters

### FR-05: ShadowEvaluator

- `ShadowEvaluator::new(store: Arc<Store>) -> Self`
- `ShadowEvaluator::evaluate(model_name: &str, model_version: u32, digest: &SignalDigest, rule_prediction: &str, neural_prediction: &str, neural_confidence: f32, feature_cycle: Option<&str>) -> Result<(), String>`
- `ShadowEvaluator::accuracy(model_name: &str, model_version: u32) -> Result<f64, String>`
- `ShadowEvaluator::evaluation_count(model_name: &str, model_version: u32) -> Result<u64, String>`
- `ShadowEvaluator::per_class_accuracy(model_name: &str, model_version: u32) -> Result<HashMap<String, f64>, String>`

Persists `ShadowEvaluation` records to `shadow_evaluations` SQLite table.

### FR-06: Pipeline Integration

The neural enhancement step runs within the extraction tick (col-013's `extraction_tick()` function in `background.rs`):

1. After extraction rules produce `Vec<ProposedEntry>`:
2. For each entry, build `SignalDigest` from entry metadata
3. Query `ModelRegistry` for model state:
   - `Observation`: skip neural step
   - `Shadow`: predict, log to shadow_evaluations, pass entry unchanged
   - `Production`: predict, apply neural override if confidence > threshold
4. Neural override logic:
   - If classifier confidence > `neural_override_confidence` (0.8) AND classifier disagrees with rule classification: use neural classification
   - Convention score supplements rule confidence when scorer output > rule confidence
5. Enhanced entries use `trust_source: "neural"` instead of `"auto"`

### FR-07: trust_source "neural" in Confidence Scoring

Add to `trust_score()` in `unimatrix-engine/src/confidence.rs`:

```
"neural" => 0.40
```

Position: between "auto" (0.35) and "agent" (0.50). Neural-enhanced extraction has higher trust than purely automatic extraction because the model has been validated through shadow mode.

### FR-08: Schema Migration v7 -> v8

Add `shadow_evaluations` table:

```sql
CREATE TABLE IF NOT EXISTS shadow_evaluations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    model_name TEXT NOT NULL,
    model_version INTEGER NOT NULL,
    ts_millis INTEGER NOT NULL,
    signal_digest BLOB NOT NULL,
    rule_prediction TEXT NOT NULL,
    neural_prediction TEXT NOT NULL,
    neural_confidence REAL NOT NULL,
    ground_truth TEXT,
    feature_cycle TEXT
);
CREATE INDEX IF NOT EXISTS idx_shadow_model
    ON shadow_evaluations(model_name, model_version);
CREATE INDEX IF NOT EXISTS idx_shadow_ts
    ON shadow_evaluations(ts_millis);
```

Follows existing migration pattern: check schema_version, execute DDL, update version.

## Non-Functional Requirements

### NFR-01: Inference Latency
- Signal Classifier: < 50ms per prediction
- Convention Scorer: < 10ms per prediction
- Total neural enhancement overhead per extraction tick: < 1 second for 10 entries

### NFR-02: Memory Footprint
- Signal Classifier loaded model: < 10MB RAM
- Convention Scorer loaded model: < 5MB RAM
- ModelRegistry overhead: < 1MB RAM
- Shadow evaluations: bounded by SQLite (no in-memory cache)

### NFR-03: Disk Footprint
- Model files: < 21MB total (3 files x 2 models x ~3.5MB average)
- shadow_evaluations table: grows ~1KB per evaluation, bounded by observation retention

### NFR-04: Determinism
- Same SignalDigest input produces same ClassificationResult / ConventionScore
- No random dropout or stochastic elements at inference time
- Model loading is deterministic (same file -> same predictions)

### NFR-05: Backward Compatibility
- `unimatrix-adapt` public API unchanged after refactoring
- Existing `adaptation.state` files load successfully after refactoring
- `trust_source: "auto"` entries unaffected by new "neural" value
- Schema v7 databases auto-migrate to v8 on open

### NFR-06: Failure Isolation
- ruv-fann failure (load/predict) does not crash server -- returns error, extraction continues without neural enhancement
- Model file corruption detected on load -- falls back to baseline weights
- SQLite shadow_evaluations write failure logged, does not block extraction

## Acceptance Criteria Traceability

| AC | Functional Requirement | Non-Functional Requirement |
|----|----------------------|---------------------------|
| AC-01 | FR-01 | |
| AC-02 | FR-01 | NFR-05 |
| AC-03 | FR-01 | NFR-05 |
| AC-04 | FR-02 | |
| AC-05 | FR-03 | |
| AC-06 | FR-01, FR-02, FR-03 | |
| AC-07 | FR-02 | NFR-01 |
| AC-08 | FR-03 | NFR-01 |
| AC-09 | FR-06 | |
| AC-10 | FR-05 | |
| AC-11 | FR-04 | |
| AC-12 | FR-04 | |
| AC-13 | FR-04 | NFR-03 |
| AC-14 | FR-02, FR-03 | |
| AC-15 | FR-07 | |
| AC-16 | FR-06 | |
| AC-17 | FR-02, FR-03, FR-04, FR-05 | |
| AC-18 | FR-05, FR-06 | |

## Constraints

- All model inference is CPU-only (no GPU dependencies)
- ruv-fann is the primary ML framework; ndarray is the fallback (ADR-002 trait abstraction)
- Models are per-project (isolated by `{project_hash}`)
- No training occurs in crt-007 -- only inference with baseline weights and shadow evaluation
- Schema migration v7->v8 must be backward-compatible (new table, no column changes to existing tables)
- Total new code: ~800 lines (~250 shared infra, ~350 models, ~200 shadow mode)
