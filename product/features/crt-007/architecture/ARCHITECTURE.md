# Architecture: crt-007 Neural Extraction Pipeline

## Overview

crt-007 introduces three architectural subsystems:

1. **Shared training infrastructure** -- `unimatrix-learn` crate extracting generic ML primitives from `unimatrix-adapt`
2. **Neural models** -- Signal Classifier and Convention Scorer MLPs via ruv-fann, living in `unimatrix-learn`
3. **Shadow mode + model versioning** -- `ModelRegistry` with production/shadow/previous slots, evaluation logging to SQLite

Plus a **pipeline integration** -- neural models wire into col-013's extraction pipeline as an enhancement layer.

## Architecture Decisions

### ADR-001: New unimatrix-learn Crate

**Context:** Shared training primitives (`TrainingReservoir`, `EwcState`, persistence helpers) need to be consumed by both `unimatrix-adapt` (MicroLoRA) and the new neural extraction models. These primitives are currently coupled to MicroLoRA in `unimatrix-adapt`.

**Decision:** Create a new `crates/unimatrix-learn/` crate. Extract generic training infrastructure from `unimatrix-adapt`. `unimatrix-adapt` depends on `unimatrix-learn`. Neural models also live in `unimatrix-learn`.

**Rationale:**
- Clean separation: `unimatrix-learn` = ML infrastructure + models; `unimatrix-adapt` = MicroLoRA-specific embedding adaptation
- Avoids circular dependencies: `unimatrix-learn` has no dependency on `unimatrix-adapt`
- Through crt-008/009, `unimatrix-learn` becomes "the ML crate" -- all 5+ models and shared training infra
- `unimatrix-engine` (extraction pipeline from col-013) calls into `unimatrix-learn` for classification/scoring

**Dependency graph:**
```
unimatrix-learn (new)
  ^           ^
  |           |
unimatrix-adapt   unimatrix-engine (extraction pipeline)
  ^                      ^
  |                      |
unimatrix-server    unimatrix-server
```

**Consequences:**
- New workspace member in `Cargo.toml`
- `unimatrix-adapt` gains a dependency on `unimatrix-learn`
- `unimatrix-engine` gains a dependency on `unimatrix-learn`
- ~200 lines of code moved from `unimatrix-adapt` to `unimatrix-learn`

### ADR-002: ML-Framework-Agnostic Model Trait (SR-01 mitigation)

**Context:** ruv-fann is v0.2.0 with limited adoption (SR-01). If RPROP implementation proves insufficient, we need to swap the ML backend without cascading changes through `ModelRegistry`, `ShadowEvaluator`, or the extraction pipeline.

**Decision:** Define a `NeuralModel` trait in `unimatrix-learn` that abstracts over the ML framework:

```rust
pub trait NeuralModel: Send + Sync {
    type Input;
    type Output;

    fn predict(&self, input: &Self::Input) -> Self::Output;
    fn save(&self, path: &Path) -> Result<(), String>;
    fn load(path: &Path) -> Result<Self, String> where Self: Sized;
    fn param_count(&self) -> usize;
    fn params_flat(&self) -> Vec<f32>;
}
```

`ModelRegistry`, `ShadowEvaluator`, and the extraction pipeline depend on this trait, not on ruv-fann types. The ruv-fann implementation is a concrete struct (`FannClassifier`, `FannScorer`) behind the trait.

**Rationale:**
- If ruv-fann fails, we implement `NdarrayClassifier` behind the same trait -- zero changes to consumers
- `params_flat()` enables EWC++ regularization regardless of framework (EwcState works on flat `Vec<f32>`)
- `save`/`load` enables framework-specific serialization without leaking format details

**Consequences:**
- Small overhead from trait indirection (negligible for <50ms inference)
- ruv-fann types are confined to model implementation files
- Fallback path is well-defined: implement same trait with ndarray

### ADR-003: Fixed-Width SignalDigest as [f32; 32]

**Context:** Neural models need a stable input format. The signal feature vector must be stable across model versions to avoid cold restarts on every schema change (resolved question 5 from SCOPE.md).

**Decision:** `SignalDigest` is a fixed-width `[f32; 32]` array. Slot assignments are documented in a canonical registry (`SIGNAL_SLOTS` const). crt-007 uses slots 0-6; remainder initialized to zero. New signals (crt-008/009) fill empty slots additively.

**Slot assignments (crt-007):**
| Slot | Name | Range | Source |
|------|------|-------|--------|
| 0 | search_miss_count | [0, 1] normalized | KnowledgeGapRule |
| 1 | co_access_density | [0, 1] | Co-access pairs / max_pairs |
| 2 | consistency_score | [0, 1] | Features matching / total features |
| 3 | feature_count | [0, 1] normalized | log(n+1) / log(max+1) |
| 4 | observation_count | [0, 1] normalized | log(n+1) / log(max+1) |
| 5 | age_days | [0, 1] | 1.0 - exp(-age/90) |
| 6 | rule_confidence | [0, 1] | Extraction rule confidence |
| 7-31 | reserved | 0.0 | Future signals |

**Rationale:**
- Power-of-2 alignment (32 floats = 128 bytes) for SIMD/cache efficiency
- Zero-initialized reserved slots are neutral to learned weights
- Append-only semantics: existing slots never reordered or repurposed
- `schema_version` field in model metadata detects incompatible changes
- 32 slots provide headroom for ~15 known roadmap features + undiscovered signals

**Consequences:**
- Models trained on 7-slot input work unchanged when slot 8 is added (weight for slot 8 starts at 0)
- Breaking change (removing/reordering slots) triggers `ModelRegistry` demotion and cold-start
- All input normalization must produce [0, 1] range values

### ADR-004: Shadow Evaluation in SQLite

**Context:** Shadow mode needs to persist evaluation records across sessions for accuracy computation and model promotion decisions. Options: SQLite table (JOINable, queryable) or flat file (simpler).

**Decision:** New `shadow_evaluations` table in the project SQLite database:

```sql
CREATE TABLE shadow_evaluations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    model_name TEXT NOT NULL,
    model_version INTEGER NOT NULL,
    ts_millis INTEGER NOT NULL,
    signal_digest BLOB NOT NULL,          -- 128 bytes (32 x f32)
    rule_prediction TEXT NOT NULL,         -- e.g., "convention"
    neural_prediction TEXT NOT NULL,       -- e.g., "noise"
    neural_confidence REAL NOT NULL,       -- max softmax probability
    ground_truth TEXT,                     -- filled by feedback (crt-008)
    feature_cycle TEXT
);
CREATE INDEX idx_shadow_model ON shadow_evaluations(model_name, model_version);
CREATE INDEX idx_shadow_ts ON shadow_evaluations(ts_millis);
```

Schema migration: v7 -> v8 (adds `shadow_evaluations` table).

**Rationale:**
- JOINable with `observations`, `sessions`, `entries` tables for rich analysis
- SQL aggregation queries for accuracy computation: `SELECT COUNT(*) WHERE rule_prediction = neural_prediction`
- `ground_truth` column supports crt-008 (feedback-to-label pipeline)
- Consistent with project pattern: all structured data in SQLite

**Consequences:**
- Schema migration required (v7 -> v8)
- ~10 lines of migration code following established `ALTER TABLE` + `CREATE TABLE` pattern
- Table grows linearly with extraction pipeline invocations (bounded by tick interval * model count)

### ADR-005: ModelRegistry Versioning Strategy

**Context:** Model versioning needs to support production/shadow/previous slots, promotion, and auto-rollback. Design must be simple enough for crt-007's two models but extensible to crt-009's five.

**Decision:** `ModelRegistry` manages named model slots:

```rust
pub struct ModelRegistry {
    models: HashMap<String, ModelSlot>,
    models_dir: PathBuf,
}

pub struct ModelSlot {
    pub production: Option<LoadedModel>,
    pub shadow: Option<LoadedModel>,
    pub previous_path: Option<PathBuf>,
    pub metrics: RollingMetrics,
    pub schema_version: u32,
}

pub struct LoadedModel {
    pub version: u32,
    pub path: PathBuf,
    pub accuracy: f64,
    pub evaluation_count: u64,
}

pub struct RollingMetrics {
    window: VecDeque<(bool, f64)>,  // (correct, confidence)
    capacity: usize,
}
```

**Promotion flow:**
1. Shadow model accumulates >= 20 evaluations
2. Shadow accuracy >= production accuracy (or rule-only baseline for first promotion)
3. No per-category regression (checked via shadow_evaluations table)
4. Promote: current production -> previous, shadow -> production, shadow slot cleared

**Rollback flow:**
1. Rolling accuracy (window=100) drops >5% below pre-promotion baseline
2. Rollback: previous -> production, log rollback event
3. Shadow slot cleared (failed model not retained)

**Retention policy:** Only production + previous + active shadow files retained. All other versions deleted on promotion.

**Rationale:**
- Simple slot-based design avoids complex version graph management
- Rolling metrics window (100 evaluations) smooths noise
- Retention policy bounds disk usage (max 3 files per model = ~21MB for crt-007)
- HashMap keyed by model name scales to 5+ models (crt-009)

**Consequences:**
- Model files stored as `{models_dir}/{model_name}/v{version}.bin`
- Registry state persisted as `{models_dir}/registry.json` (simple JSON, not bincode, for debuggability)
- Lazy loading: shadow model loaded only when shadow mode is active

### ADR-006: Conservative Baseline Weight Initialization

**Context:** Models ship with hand-tuned baseline weights (resolved question 4 from SCOPE.md). The bias must be set directly, not trained from data.

**Decision:** Set output layer biases to produce conservative predictions:

**Signal Classifier (5-class softmax):**
- Noise class bias: +2.0 (pre-softmax, yields ~60% probability for zero input)
- Convention class bias: -0.5
- Pattern class bias: -0.5
- Gap class bias: 0.0 (gaps are low-risk to extract)
- Dead class bias: 0.0 (dead knowledge flagging is low-risk)
- All hidden layer weights: Xavier/Glorot uniform initialization
- All hidden layer biases: 0.0

**Convention Scorer (1-output sigmoid):**
- Output bias: -1.0 (sigmoid(-1.0) = 0.27, below the 0.6 threshold for Active status)
- Hidden layer weights: Xavier/Glorot initialization
- Hidden layer biases: 0.0

**Rationale:**
- Output biases directly control the model's prior distribution without training data
- +2.0 noise bias means even moderately informative features must overcome a significant prior to change classification
- Convention scorer at 0.27 baseline means genuine conventions need clear signal to score above 0.6
- Xavier initialization for hidden layers ensures gradients flow properly when training begins (crt-008)

**Consequences:**
- Models are immediately functional (no training required)
- Initial predictions are heavily biased toward "noise" / low scores -- this is the desired conservative behavior
- Shadow mode will show initially low agreement with rule-based extraction (expected)
- Bias values are configurable in `NeuralConfig` for tuning

## Component Diagram

```
crates/unimatrix-learn/  (NEW)
  |-- lib.rs                     (pub mod declarations)
  |-- reservoir.rs               (TrainingReservoir<T>, extracted from adapt)
  |-- ewc.rs                     (EwcState, generalized for flat params)
  |-- registry.rs                (ModelRegistry, ModelSlot, LoadedModel, RollingMetrics)
  |-- persistence.rs             (atomic save/load helpers, extracted from adapt)
  |-- model.rs                   (NeuralModel trait)
  |-- digest.rs                  (SignalDigest, SIGNAL_SLOTS, normalization)
  |-- classifier.rs              (SignalClassifier: NeuralModel impl via ruv-fann)
  |-- scorer.rs                  (ConventionScorer: NeuralModel impl via ruv-fann)
  |-- shadow.rs                  (ShadowEvaluator, evaluation logging)
  |-- config.rs                  (NeuralConfig)

crates/unimatrix-adapt/  (REFACTORED)
  |-- training.rs                (removes TrainingReservoir, re-exports from learn)
  |-- regularization.rs          (removes EwcState, re-exports from learn)
  |-- persistence.rs             (uses learn::persistence helpers)
  |-- (all other files unchanged)

crates/unimatrix-engine/  (EXTENDED)
  |-- confidence.rs              (adds "neural" -> 0.40 to trust_score)

crates/unimatrix-server/  (EXTENDED)
  |-- background.rs              (extraction_tick gains neural enhancement step)
  |-- migrations.rs              (v7->v8: shadow_evaluations table)
```

## Data Flow

### Neural Enhancement (per extraction tick)

```
1. col-013 extraction rules produce Vec<ProposedEntry>
2. For each ProposedEntry:
   a. Build SignalDigest from entry metadata + observation context
   b. Classifier.predict(digest) -> probability distribution [5 classes]
   c. Scorer.predict(digest) -> convention confidence [0.0, 1.0]
3. If shadow mode:
   a. Log (rule_prediction, neural_prediction, confidence) to shadow_evaluations
   b. Pass original ProposedEntry unchanged to quality gates
4. If production mode:
   a. Neural classification overrides rule classification when neural confidence > 0.8
   b. Convention score supplements rule confidence
   c. Entries with neural enhancement: trust_source = "neural"
   d. Pass enhanced ProposedEntry to quality gates
```

### Model Lifecycle

```
Server startup:
  1. ModelRegistry loads registry.json (model metadata)
  2. Production models loaded into memory (lazy: shadow loaded on demand)
  3. If no models exist: create with baseline weights, enter observation mode

Per extraction tick:
  1. Check model state:
     - Observation mode (features < 5): skip neural step
     - Shadow mode: run predict, log comparison, don't influence output
     - Production mode: run predict, influence output
  2. After prediction: update RollingMetrics
  3. Check promotion criteria (shadow -> production)
  4. Check rollback criteria (production accuracy degradation)

Model state transitions:
  cold start -> observation (features 1-5) -> shadow (features 6+) -> production
  production -> rollback to previous (on accuracy drop)
```

### Feature Count Tracking

The model lifecycle depends on "feature count" to determine mode transitions. This is tracked by:
- Querying the `sessions` table for distinct `feature_cycle` values
- Counting features observed since model creation timestamp
- Stored in `ModelSlot.features_observed: u64`

## Integration Points

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `Cargo.toml` (workspace) | New member | `unimatrix-learn` added to workspace |
| `unimatrix-learn` | New crate | Shared training infra + neural models |
| `unimatrix-adapt/training.rs` | Refactor | `TrainingReservoir` moved to learn, re-exported |
| `unimatrix-adapt/regularization.rs` | Refactor | `EwcState` moved to learn, re-exported |
| `unimatrix-adapt/persistence.rs` | Refactor | Uses learn::persistence helpers |
| `unimatrix-adapt/Cargo.toml` | Dependency | `unimatrix-learn = { path = "../unimatrix-learn" }` |
| `unimatrix-engine/confidence.rs` | 1-line change | `trust_score: "neural" => 0.40` |
| `unimatrix-engine/Cargo.toml` | Dependency | `unimatrix-learn = { path = "../unimatrix-learn" }` |
| `unimatrix-server/background.rs` | Enhancement | Neural step in extraction_tick |
| `unimatrix-server/migrations.rs` | Schema v8 | `shadow_evaluations` table |

## Risk Mitigations (from Scope Risk Assessment)

| Risk | Architectural Mitigation |
|------|-------------------------|
| SR-01 (ruv-fann maturity) | ADR-002: NeuralModel trait abstracts ML framework. ruv-fann types confined to classifier.rs/scorer.rs |
| SR-02 (adapt refactoring) | ADR-001: Extract-then-redirect. Re-exports preserve public API. Serde-compatible persistence format |
| SR-03 (SignalDigest stability) | ADR-003: Fixed-width [f32; 32], append-only slots, schema version detection |
| SR-04 (shadow accuracy) | ADR-004: SQLite evaluation logs with side-by-side predictions. ground_truth column for crt-008 |
| SR-05 (col-013 integration) | SignalDigest is a crt-007-owned type. Thin adapter builds digest from ProposedEntry metadata |
| SR-06 (disk footprint) | ADR-005: Retention policy (3 files per model max). Registry prunes on promotion |
| SR-07 (conservative bias) | ADR-006: Configurable bias weights. Shadow mode logs prediction distributions for tuning |
