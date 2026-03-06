# Specification: crt-007 Neural Extraction Pipeline

## Objective

Extract shared training infrastructure from unimatrix-adapt into a reusable unimatrix-learn crate, implement two burn-based neural models (Signal Classifier, Convention Scorer) for knowledge extraction enhancement, and integrate them into the col-013 extraction pipeline via shadow mode with model versioning and auto-rollback.

## Functional Requirements

### FR-01: Shared Training Infrastructure Extraction

- FR-01.1: `TrainingReservoir<T>` generic over `T: Clone` with `new(capacity, seed)`, `add(&[T])`, `sample_batch(batch_size) -> Vec<&T>`, `len()`, `total_seen()`
- FR-01.2: `EwcState` with flat `Vec<f32>` parameter interface: `new(param_count, alpha, lambda)`, `penalty(&[f32])`, `gradient_contribution(&[f32])`, `update_from_flat(&[f32], &[f32])`, `to_vecs()`, `from_vecs()`
- FR-01.3: Atomic persistence helpers: `save_atomic(data, dir, filename)`, `load_file(dir, filename)`
- FR-01.4: All components in new `crates/unimatrix-learn/` crate

### FR-02: unimatrix-adapt Refactoring

- FR-02.1: `TrainingReservoir` in training.rs replaced with `unimatrix_learn::TrainingReservoir<TrainingPair>`
- FR-02.2: `EwcState` in regularization.rs replaced with `unimatrix_learn::EwcState`
- FR-02.3: Persistence helpers in persistence.rs use shared `save_atomic`/`load_file`
- FR-02.4: Public API of unimatrix-adapt unchanged (same function signatures)
- FR-02.5: All 174+ existing unimatrix-adapt tests pass without modification

### FR-03: Signal Classifier MLP

- FR-03.1: Burn module with topology: `Linear(32, 64)` -> `Sigmoid` -> `Linear(64, 32)` -> `ReLU` -> `Linear(32, 5)` -> `Softmax`
- FR-03.2: Input: `SignalDigest` (32-element `[f32; 32]` vector)
- FR-03.3: Output: `ClassificationResult` with `category: SignalCategory` (highest probability), `probabilities: [f32; 5]` (full distribution), `confidence: f32` (max probability)
- FR-03.4: `SignalCategory` enum: `Convention`, `Pattern`, `Gap`, `Dead`, `Noise`
- FR-03.5: Cold-start baseline weights bias output toward `Noise` class (output layer bias for Noise slot set to +2.0, others to 0.0)

### FR-04: Convention Scorer MLP

- FR-04.1: Burn module with topology: `Linear(32, 32)` -> `ReLU` -> `Linear(32, 1)` -> `Sigmoid`
- FR-04.2: Input: `SignalDigest` (same 32-element vector)
- FR-04.3: Output: `f32` in [0.0, 1.0] representing convention confidence
- FR-04.4: Cold-start baseline weights bias output toward low scores (output layer bias set to -2.0)

### FR-05: SignalDigest

- FR-05.1: Fixed-width struct: `pub struct SignalDigest { pub features: [f32; 32] }`
- FR-05.2: Constructor `from_proposed(entry: &ProposedEntry) -> Self` populating slots 0-6
- FR-05.3: All features normalized to [0.0, 1.0] range
- FR-05.4: Slots 7-31 initialized to 0.0 (reserved for crt-008/009)
- FR-05.5: Slot assignment documented in code and architecture

### FR-06: Shadow Mode

- FR-06.1: `NeuralEnhancer` wraps classifier + scorer, operates in `Shadow` or `Active` mode
- FR-06.2: In `Shadow` mode: models run, predictions logged to `shadow_evaluations` table, but ProposedEntry is passed through unmodified
- FR-06.3: In `Active` mode: classifier can suppress entries (reclassify as Noise with confidence > 0.8), scorer overrides extraction_confidence
- FR-06.4: `ShadowEvaluator` tracks per-category accuracy by comparing neural classification against rule-based ground truth
- FR-06.5: Shadow log schema: `(id INTEGER PRIMARY KEY, timestamp INTEGER, rule_name TEXT, rule_category TEXT, neural_category TEXT, neural_confidence REAL, convention_score REAL, rule_accepted INTEGER, digest BLOB)`

### FR-07: Model Registry

- FR-07.1: Three slots per model: Production, Shadow, Previous
- FR-07.2: `ModelVersion` metadata: generation, timestamp, accuracy, burn_version, slot
- FR-07.3: Promotion: shadow -> production (old production -> previous) when: accuracy >= rule-only accuracy AND minimum 20 evaluations AND no per-category regression
- FR-07.4: Rollback: production -> shadow (previous -> production) when: rolling accuracy drops > 5% below pre-promotion baseline
- FR-07.5: Registry state persisted to `~/.unimatrix/{project_hash}/models/registry.json`
- FR-07.6: Burn version stored in ModelVersion for deserialization compatibility detection

### FR-08: Pipeline Integration

- FR-08.1: Neural enhancement inserted between rule evaluation and quality gate in `extraction_tick()`
- FR-08.2: Entries enhanced by neural models in Active mode use `trust_source: "neural"` (vs `"auto"` for rule-only)
- FR-08.3: `"neural"` trust_source weight = 0.40 in confidence scoring (crt-002 integration)
- FR-08.4: Neural enhancement is optional -- if models fail to load, pipeline operates rule-only

## Non-Functional Requirements

### NFR-01: Performance

- NFR-01.1: Signal Classifier inference < 50ms per entry on CPU
- NFR-01.2: Convention Scorer inference < 10ms per entry on CPU
- NFR-01.3: Combined neural enhancement < 100ms per extraction tick batch
- NFR-01.4: Model loading from disk < 500ms on cold start

### NFR-02: Resource Constraints

- NFR-02.1: Classifier model file < 5MB on disk
- NFR-02.2: Scorer model file < 2MB on disk
- NFR-02.3: Combined model memory footprint < 20MB at runtime
- NFR-02.4: No GPU dependencies -- CPU-only via burn-ndarray backend

### NFR-03: Binary Size

- NFR-03.1: burn + burn-ndarray dependency adds < 15MB to release binary
- NFR-03.2: If binary delta exceeds 15MB, burn should be feature-gated (SR-01)

### NFR-04: Compatibility

- NFR-04.1: No breaking changes to unimatrix-adapt public API
- NFR-04.2: No changes to MCP tool interfaces
- NFR-04.3: Extraction pipeline degrades gracefully to rule-only when models unavailable
- NFR-04.4: Shadow evaluation data queryable via standard SQL (not custom binary format)

## Acceptance Criteria

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-01 | `unimatrix-learn` crate exists with `TrainingReservoir<T>`, `EwcState`, `ModelRegistry`, persistence helpers | Cargo build + unit tests |
| AC-02 | `unimatrix-adapt` depends on `unimatrix-learn` and uses shared implementations | Grep for imports; no duplicated reservoir/ewc code |
| AC-03 | All existing unimatrix-adapt tests pass after refactoring | `cargo test -p unimatrix-adapt` -- 174+ tests pass |
| AC-04 | Signal Classifier MLP constructed with hand-tuned baseline weights via burn | Unit test: construct, verify output shape |
| AC-05 | Convention Scorer MLP constructed with hand-tuned baseline weights via burn | Unit test: construct, verify output in [0,1] |
| AC-06 | `SignalDigest` struct defined with all input features for both models | Unit test: from_proposed produces valid 32-element vector |
| AC-07 | Classifier inference produces probability distribution over 5 categories in < 50ms | Benchmark test: 1000 inferences, p99 < 50ms |
| AC-08 | Scorer inference produces convention confidence in < 10ms | Benchmark test: 1000 inferences, p99 < 10ms |
| AC-09 | Shadow mode runs both models on extraction pipeline without affecting stored entries | Integration test: shadow mode produces same store output as rule-only |
| AC-10 | Shadow evaluation logs persist predictions with ground truth | Unit test: log entries written to SQLite, queryable |
| AC-11 | ModelRegistry manages production/shadow/previous slots per model with promotion criteria | Unit test: promote/rollback state transitions |
| AC-12 | Auto-rollback triggers when rolling accuracy drops > 5% below pre-promotion baseline | Unit test: simulate accuracy drop, verify rollback |
| AC-13 | Models stored in `~/.unimatrix/{project_hash}/models/{model_name}/` with versioned filenames | Unit test: save/load round-trip at expected path |
| AC-14 | Cold-start baseline weights bias classifier toward `noise` and scorer toward low scores | Unit test: all-zero digest -> Noise classification; scorer < 0.3 |
| AC-15 | crt-002 confidence scoring includes `"neural" -> 0.40` trust_source weight | Unit test: neural entry scored with 0.40 weight |
| AC-16 | Neural-enhanced entries use `trust_source: "neural"` | Integration test: active-mode entry stored with trust_source "neural" |
| AC-17 | Unit tests for classifier inference, scorer inference, shadow evaluation, model registry | `cargo test -p unimatrix-learn` passes |
| AC-18 | Integration test: end-to-end shadow mode (rules -> digest -> classify -> log -> no store impact) | Integration test in unimatrix-server |

## Domain Models

### Key Entities

| Entity | Definition |
|--------|-----------|
| `SignalDigest` | Fixed-width 32-element f32 vector representing structured features of a ProposedEntry. The canonical input for all neural extraction models. |
| `SignalCategory` | Classification output: Convention, Pattern, Gap, Dead, Noise. Maps to extraction pipeline categories. |
| `ClassificationResult` | Full classifier output: winning category, probability distribution, confidence (max probability). |
| `NeuralPrediction` | Combined output of classifier + scorer for a single ProposedEntry. |
| `ModelSlot` | Lifecycle position: Production (active inference), Shadow (evaluation only), Previous (rollback target). |
| `ModelVersion` | Immutable metadata for a saved model: generation counter, timestamp, accuracy metrics, burn version. |
| `ModelRegistry` | Manages model slot assignments and promotion/rollback logic. One registry per project. |
| `NeuralEnhancer` | Pipeline component wrapping classifier + scorer. Operates in Shadow or Active mode. |
| `ShadowEvaluator` | Tracks neural vs rule agreement. Computes per-category accuracy and overall precision. |
| `ProposedEntry` | (Existing, from col-013) Rule output -- title, content, category, confidence. Input to neural enhancement. |
| `TrainingReservoir<T>` | Generic bounded-capacity buffer using reservoir sampling. Used by both MicroLoRA and (future) neural model training. |
| `EwcState` | EWC++ regularization state: Fisher diagonal + reference parameters. Prevents catastrophic forgetting during incremental retraining. |

### Relationship Map

```
ProposedEntry (col-013) --> SignalDigest --> SignalClassifier --> ClassificationResult
                                       \--> ConventionScorer --> f32 score

ClassificationResult + f32 score --> NeuralPrediction

ModelRegistry manages ModelVersion instances for each model name
  - "signal-classifier" -> {Production: v3, Shadow: v4, Previous: v2}
  - "convention-scorer"  -> {Production: v1, Shadow: v2, Previous: None}

ShadowEvaluator consumes NeuralPrediction + rule ground truth -> accuracy metrics
  accuracy metrics feed into ModelRegistry promotion/rollback decisions
```

## User Workflows

### W1: First Run (Cold Start)

1. Server starts, `ModelRegistry` finds no saved models
2. `SignalClassifier` and `ConventionScorer` initialized with baseline weights
3. Models saved to `~/.unimatrix/{project_hash}/models/` as generation 0
4. `NeuralEnhancer` starts in Shadow mode
5. Extraction pipeline runs rule-only; neural predictions logged but not applied

### W2: Shadow Evaluation Phase

1. Each extraction tick, `NeuralEnhancer` runs classifier + scorer on every ProposedEntry
2. `ShadowEvaluator` compares neural category against rule source_rule mapping
3. After 20+ evaluations, `ShadowEvaluator` computes accuracy metrics
4. When promotion criteria met, `ModelRegistry::promote()` transitions Shadow -> Production
5. `NeuralEnhancer` mode switches to Active

### W3: Active Mode Operation

1. Extraction tick runs rules, produces ProposedEntries
2. `NeuralEnhancer` classifies each entry
3. If classifier says Noise with > 0.8 confidence, entry is suppressed
4. For non-suppressed entries, convention score supplements rule confidence
5. Entries stored with `trust_source: "neural"`

### W4: Accuracy Regression (Auto-Rollback)

1. Active-mode neural predictions logged to shadow_evaluations table
2. Rolling accuracy computed (window of last 50 predictions)
3. If accuracy drops > 5% below pre-promotion baseline
4. `ModelRegistry::rollback()` reverts: Production -> Shadow, Previous -> Production
5. `NeuralEnhancer` continues in Active mode with the restored previous model

## Constraints

- **col-013 dependency**: Extraction pipeline (background tick, ProposedEntry, quality gate) must be complete and merged
- **crt-006 dependency**: unimatrix-adapt must exist for refactoring
- **No breaking changes**: unimatrix-adapt public API unchanged
- **CPU only**: No GPU. burn-ndarray backend only
- **Per-repo isolation**: Models in `~/.unimatrix/{project_hash}/models/`
- **~800 lines total**: ~250 shared infra extraction, ~350 neural models, ~200 shadow mode
- **Binary size**: burn dependency should add < 15MB; feature-gate if larger

## Dependencies

| Crate | Role |
|-------|------|
| `burn 0.16` | Neural model definition, training traits |
| `burn-ndarray 0.16` | CPU-only tensor backend |
| `ndarray 0.16` | Shared infra (EwcState internal) |
| `unimatrix-store` | SQLite access for shadow logs |
| `unimatrix-observe` | Extraction pipeline (ProposedEntry, ExtractionRule) |
| `unimatrix-adapt` | Refactoring target for shared infra |
| `serde`, `bincode` | Serialization |

## NOT in Scope

- Continuous self-retraining (crt-008)
- Duplicate Detector, Pattern Merger, Entry Writer Scorer (crt-009)
- LLM API integration (crt-009)
- `context_review` MCP tool (crt-009)
- GPU acceleration
- Multi-repository model sharing
- Daemon mode (models run within session-scoped server)
- Training from utilization feedback (crt-008)
