# Pseudocode Overview: crt-007 Neural Extraction Pipeline

## Component Interaction

```
unimatrix-learn (NEW crate)
  lib.rs          -- pub mod declarations, re-exports
  config.rs       -- NeuralConfig struct
  reservoir.rs    -- TrainingReservoir<T> (extracted from adapt)
  ewc.rs          -- EwcState (generalized from adapt)
  persistence.rs  -- save_atomic / load_bytes (extracted from adapt)
  model.rs        -- NeuralModel trait
  digest.rs       -- SignalDigest, SIGNAL_SLOTS, normalization fns
  classifier.rs   -- SignalClassifier (NeuralModel impl via ruv-fann)
  scorer.rs       -- ConventionScorer (NeuralModel impl via ruv-fann)
  registry.rs     -- ModelRegistry, ModelSlot, LoadedModel, RollingMetrics
  shadow.rs       -- ShadowEvaluator (SQLite logging)

unimatrix-adapt (REFACTORED)
  training.rs     -- TrainingReservoir removed, re-exports from learn
  regularization.rs -- EwcState removed, re-exports from learn
  persistence.rs  -- uses learn::persistence helpers

unimatrix-engine (EXTENDED)
  confidence.rs   -- trust_score: "neural" => 0.40

unimatrix-store (EXTENDED)
  migration.rs    -- v7->v8: shadow_evaluations table
  db.rs           -- create_tables: shadow_evaluations

unimatrix-server (EXTENDED)
  background.rs   -- neural enhancement step in extraction_tick
```

## Data Flow

```
extraction_tick:
  observations -> extraction_rules -> Vec<ProposedEntry>
      |
      v
  For each ProposedEntry:
    build_signal_digest(entry) -> SignalDigest
      |
      v
    ModelRegistry.state(name) ->
      Observation: skip
      Shadow:
        classifier.predict(digest) -> ClassificationResult
        scorer.predict(digest)     -> ConventionScore
        ShadowEvaluator.evaluate(rule_pred, neural_pred, confidence)
        pass entry unchanged
      Production:
        classifier.predict(digest) -> ClassificationResult
        scorer.predict(digest)     -> ConventionScore
        if neural_confidence > threshold && disagrees: override classification
        set trust_source = "neural"
      |
      v
    quality_gate(entry) -> Accept | Reject
```

## Shared Types

```
SignalDigest {
    features: [f32; 32],
    schema_version: u32,
    source_rule: String,
    feature_cycle: String,
}

ClassificationResult {
    probabilities: [f32; 5],  // [convention, pattern, gap, dead, noise]
    predicted_class: SignalClass,
    confidence: f32,
}

SignalClass { Convention, Pattern, Gap, Dead, Noise }

ConventionScore { score: f32 }

ModelState { Observation, Shadow, Production, RolledBack }

NeuralConfig { models_dir, topologies, biases, thresholds, ... }
```

## Dependency Graph

```
unimatrix-learn (no internal deps)
  ^           ^
  |           |
unimatrix-adapt   unimatrix-engine
  ^                      ^
  |                      |
unimatrix-server    unimatrix-server
```

## Integration Harness Plan

For integration testing:
- Existing infra-001 suites: smoke tests apply (server startup, basic operations)
- New integration tests needed in `crates/unimatrix-learn/tests/`:
  - `integration_shadow.rs`: end-to-end shadow mode pipeline
  - `integration_registry.rs`: model lifecycle (promote, rollback, persist)
- Server-level integration: verify extraction_tick with neural enhancement does not regress existing behavior

## Patterns Used

- **Extract-then-redirect** (ADR-001): Move code to learn, re-export from adapt
- **New table migration** (Unimatrix #390): v7->v8, same pattern as col-012
- **Trait abstraction** (ADR-002): NeuralModel trait isolates ML framework
- **Atomic persistence** (adapt pattern): tmp+rename for model files
- **#![forbid(unsafe_code)]**: All crates
- **No .unwrap() in non-test code**: .map_err() chains
