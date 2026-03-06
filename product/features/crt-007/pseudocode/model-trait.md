# Pseudocode: model-trait (NeuralModel Trait + SignalDigest)

## Pattern: Trait abstraction for model lifecycle

NeuralModel trait in unimatrix-learn defines the contract all models implement.
SignalDigest is the canonical input format for extraction models.

## Files

### crates/unimatrix-learn/src/models/mod.rs

```pseudo
pub mod traits;
pub mod digest;
pub mod classifier;
pub mod scorer;

pub use traits::NeuralModel;
pub use digest::SignalDigest;
pub use classifier::{SignalClassifier, ClassificationResult, SignalCategory};
pub use scorer::ConventionScorer;
```

### crates/unimatrix-learn/src/models/traits.rs

```pseudo
/// Trait abstracting neural model lifecycle.
/// All models implement forward, train_step, parameter access, serialization.
/// Designed for future burn/candle implementations behind feature gates.
pub trait NeuralModel: Send + Sync {
    /// Forward pass: input slice -> output vec
    fn forward(&self, input: &[f32]) -> Vec<f32>;

    /// Single training step: returns loss value
    /// input: feature vector, target: expected output, lr: learning rate
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32;

    /// Flatten all model parameters into a single Vec<f32>
    /// Order: layer-by-layer, weights then biases
    fn flat_parameters(&self) -> Vec<f32>;

    /// Set all parameters from a flat Vec<f32>
    /// Must match the order of flat_parameters()
    fn set_parameters(&mut self, params: &[f32]);

    /// Serialize model to bytes (bincode)
    fn serialize(&self) -> Vec<u8>;

    /// Deserialize model from bytes
    fn deserialize(data: &[u8]) -> Result<Self, String> where Self: Sized;
}
```

### crates/unimatrix-learn/src/models/digest.rs

```pseudo
/// Fixed-width 32-slot feature vector (ADR-003).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct SignalDigest {
    pub features: [f32; 32],
}

/// Category ordinal encoding for slot 3.
fn category_ordinal(category: &str) -> f32 {
    match category {
        "convention" => 0.0,
        "pattern"    => 0.2,
        "lesson-learned" => 0.4,
        "gap"        => 0.6,
        "decision"   => 0.8,
        _            => 1.0,
    }
}

/// Rule ordinal encoding for slot 4.
fn rule_ordinal(rule: &str) -> f32 {
    match rule {
        "knowledge-gap"         => 0.0,
        "implicit-convention"   => 0.2,
        "dead-knowledge"        => 0.4,
        "recurring-friction"    => 0.6,
        "file-dependency"       => 0.8,
        _                       => 1.0,
    }
}

impl SignalDigest {
    /// Construct digest from extraction ProposedEntry fields.
    /// Uses raw field values; does NOT take &ProposedEntry to avoid
    /// cross-crate dependency on unimatrix-observe.
    pub fn from_fields(
        extraction_confidence: f64,
        source_feature_count: usize,
        content_length: usize,
        category: &str,
        source_rule: &str,
        title_length: usize,
        tag_count: usize,
    ) -> Self {
        let mut features = [0.0_f32; 32];
        features[0] = extraction_confidence as f32;           // [0,1] already
        features[1] = (source_feature_count as f32 / 10.0).min(1.0);
        features[2] = (content_length as f32 / 1000.0).min(1.0);
        features[3] = category_ordinal(category);
        features[4] = rule_ordinal(source_rule);
        features[5] = (title_length as f32 / 200.0).min(1.0);
        features[6] = (tag_count as f32 / 10.0).min(1.0);
        // slots 7-31: reserved, zero-initialized
        Self { features }
    }

    /// All-zero digest (useful for baseline testing).
    pub fn zeros() -> Self {
        Self { features: [0.0; 32] }
    }

    /// Return features as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.features
    }
}
```

Note: `from_fields` takes raw values instead of `&ProposedEntry` to avoid
unimatrix-learn depending on unimatrix-observe. The observe crate's
`NeuralEnhancer` calls `SignalDigest::from_fields(entry.extraction_confidence, ...)`.

## Open Questions

None.
