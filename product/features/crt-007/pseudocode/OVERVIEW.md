# Pseudocode Overview: crt-007 Neural Extraction Pipeline

## Component Interaction

```
                    unimatrix-learn (NEW crate)
                   /          |          \
          reservoir.rs    ewc.rs    persistence.rs   (Wave 1: extracted from adapt)
                          |
          models/traits.rs  models/digest.rs         (Wave 2: NeuralModel trait + SignalDigest)
          models/classifier.rs  models/scorer.rs     (Wave 2: MLP models)
          registry.rs  config.rs                     (Wave 2: model versioning)
                          |
                    unimatrix-adapt (REFACTORED)
                    - training.rs uses learn::TrainingReservoir<TrainingPair>
                    - regularization.rs uses learn::EwcState
                    - persistence.rs uses learn::save_atomic/load_file
                          |
                    unimatrix-observe (EXTENDED)
                    - extraction/neural.rs: NeuralEnhancer
                    - extraction/shadow.rs: ShadowEvaluator
                          |
                    unimatrix-engine (MODIFIED)
                    - confidence.rs: add "neural" => 0.40
                          |
                    unimatrix-server (INTEGRATION)
                    - background.rs: wire NeuralEnhancer into extraction_tick
```

## Data Flow

1. `extraction_tick()` queries observations, runs rules -> `Vec<ProposedEntry>`
2. For each proposal: `SignalDigest::from_proposed(&entry)` -> 32-element f32 vector
3. `NeuralEnhancer::enhance(&entry)` runs classifier + scorer -> `NeuralPrediction`
4. Shadow mode: log prediction to `shadow_evaluations`, pass entry unchanged
5. Active mode: suppress Noise (>0.8 confidence), override trust_source to "neural"
6. Quality gate checks 1-6 proceed as before
7. Store accepted entries

## Shared Types

```rust
// unimatrix-learn/src/models/mod.rs
pub struct SignalDigest { pub features: [f32; 32] }
pub enum SignalCategory { Convention, Pattern, Gap, Dead, Noise }
pub struct ClassificationResult {
    pub category: SignalCategory,
    pub probabilities: [f32; 5],
    pub confidence: f32,
}
pub struct NeuralPrediction {
    pub classification: ClassificationResult,
    pub convention_score: f32,
    pub digest: SignalDigest,
}
pub enum ModelSlot { Production, Shadow, Previous }
pub struct ModelVersion {
    pub generation: u64,
    pub timestamp: u64,
    pub accuracy: Option<f64>,
    pub schema_version: u32,
    pub slot: ModelSlot,
}
pub enum EnhancerMode { Shadow, Active }
```

## Component List

| Component | Pseudocode | Crate(s) |
|-----------|-----------|----------|
| learn-crate | pseudocode/learn-crate.md | unimatrix-learn (new) + unimatrix-adapt (refactor) |
| model-trait | pseudocode/model-trait.md | unimatrix-learn |
| classifier-scorer | pseudocode/classifier-scorer.md | unimatrix-learn |
| registry | pseudocode/registry.md | unimatrix-learn |
| shadow | pseudocode/shadow.md | unimatrix-observe |
| integration | pseudocode/integration.md | unimatrix-engine + unimatrix-server |

## Integration Harness Plan

Suites relevant to crt-007 (per USAGE-PROTOCOL.md):
- `confidence` -- trust_source "neural" weight affects confidence scoring
- `lifecycle` -- schema version bump for shadow_evaluations table
- `smoke` -- minimum gate, regression baseline

New integration tests needed:
- None in infra-001 -- shadow mode is an internal pipeline concern. Integration
  verification is via unit tests in unimatrix-learn and unimatrix-observe, plus
  the confidence suite covering the "neural" trust_source weight.

## Open Questions

None. All ADRs resolved during Session 1 design.
