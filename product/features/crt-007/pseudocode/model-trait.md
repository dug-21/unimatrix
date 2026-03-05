# Pseudocode: model-trait (Wave 2)

## model.rs

```rust
use std::path::Path;

/// Framework-agnostic neural model trait (ADR-002).
///
/// Abstracts over ML framework (ruv-fann, ndarray, etc).
/// All consumers (ModelRegistry, ShadowEvaluator, extraction pipeline)
/// depend on this trait, not on framework-specific types.
pub trait NeuralModel: Send + Sync {
    type Input;
    type Output;

    /// Run forward pass.
    fn predict(&self, input: &Self::Input) -> Self::Output;

    /// Persist model to disk.
    fn save(&self, path: &Path) -> Result<(), String>;

    /// Load model from disk.
    fn load(path: &Path) -> Result<Self, String> where Self: Sized;

    /// Number of trainable parameters.
    fn param_count(&self) -> usize;

    /// Flat parameter vector (for EWC regularization).
    fn params_flat(&self) -> Vec<f32>;
}
```

## digest.rs

```rust
/// Number of signal slots in the fixed-width feature vector.
pub const SIGNAL_SLOTS: usize = 32;

/// Schema version for slot assignments.
pub const DIGEST_SCHEMA_VERSION: u32 = 1;

/// Slot index constants for crt-007 features.
pub const SLOT_SEARCH_MISS_COUNT: usize = 0;
pub const SLOT_CO_ACCESS_DENSITY: usize = 1;
pub const SLOT_CONSISTENCY_SCORE: usize = 2;
pub const SLOT_FEATURE_COUNT: usize = 3;
pub const SLOT_OBSERVATION_COUNT: usize = 4;
pub const SLOT_AGE_DAYS: usize = 5;
pub const SLOT_RULE_CONFIDENCE: usize = 6;

/// Fixed-width feature vector consumed by neural models (ADR-003).
pub struct SignalDigest {
    pub features: [f32; SIGNAL_SLOTS],
    pub schema_version: u32,
    pub source_rule: String,
    pub feature_cycle: String,
}

impl SignalDigest {
    /// Build a digest from raw feature values.
    ///
    /// All normalization happens here. Inputs are raw counts/values,
    /// outputs are [0.0, 1.0] normalized per slot.
    pub fn new(
        search_miss_count: u32,
        co_access_density: f32,
        consistency_score: f32,
        feature_count: u32,
        observation_count: u32,
        age_days: f32,
        rule_confidence: f32,
        source_rule: String,
        feature_cycle: String,
    ) -> Self {
        let mut features = [0.0_f32; SIGNAL_SLOTS];
        features[SLOT_SEARCH_MISS_COUNT] = normalize_log(search_miss_count, 100);
        features[SLOT_CO_ACCESS_DENSITY] = co_access_density.clamp(0.0, 1.0);
        features[SLOT_CONSISTENCY_SCORE] = consistency_score.clamp(0.0, 1.0);
        features[SLOT_FEATURE_COUNT] = normalize_log(feature_count, 50);
        features[SLOT_OBSERVATION_COUNT] = normalize_log(observation_count, 1000);
        features[SLOT_AGE_DAYS] = normalize_age(age_days);
        features[SLOT_RULE_CONFIDENCE] = rule_confidence.clamp(0.0, 1.0);
        Self { features, schema_version: DIGEST_SCHEMA_VERSION, source_rule, feature_cycle }
    }

    /// Convert features to byte slice for SQLite BLOB storage.
    pub fn to_bytes(&self) -> Vec<u8>
        // unsafe { std::slice::from_raw_parts ... } -- NO, forbid(unsafe_code)
        // Use bytemuck or manual: features.iter().flat_map(|f| f.to_le_bytes())

    /// Restore features from byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<[f32; SIGNAL_SLOTS], String>
        // Read 32 f32 values from le bytes
}

/// Normalization: log(n+1) / log(max+1), clamped to [0, 1].
fn normalize_log(value: u32, max: u32) -> f32 {
    if value == 0 { return 0.0; }
    let num = (1.0 + value as f64).ln();
    let den = (1.0 + max as f64).ln();
    (num / den).min(1.0) as f32
}

/// Age normalization: 1.0 - exp(-age_days / 90.0).
fn normalize_age(age_days: f32) -> f32 {
    if age_days <= 0.0 { return 0.0; }
    (1.0 - (-age_days / 90.0_f32).exp()).clamp(0.0, 1.0)
}
```

## ClassificationResult and SignalClass

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalClass {
    Convention,
    Pattern,
    Gap,
    Dead,
    Noise,
}

impl SignalClass {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Convention, 1 => Pattern, 2 => Gap,
            3 => Dead, _ => Noise,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Convention => "convention", Pattern => "pattern",
            Gap => "gap", Dead => "dead", Noise => "noise",
        }
    }
}

pub struct ClassificationResult {
    pub probabilities: [f32; 5],
    pub predicted_class: SignalClass,
    pub confidence: f32,
}

pub struct ConventionScore {
    pub score: f32,
}
```

## ruv-fann Validation (Wave 2 gate)

In unit tests for model.rs or a separate test file:

```
#[test] fn fann_classifier_topology_constructs()
    // Create ruv-fann Fann with layers [32, 64, 32, 5]
    // Verify it doesn't panic

#[test] fn fann_scorer_topology_constructs()
    // Create ruv-fann Fann with layers [32, 32, 1]

#[test] fn fann_forward_pass_zero_input_finite()
    // Run forward([0.0; 32]), assert all outputs finite

#[test] fn fann_forward_pass_random_input_valid()
    // Run forward with random [0,1] inputs
    // Classifier: assert sum of outputs ~= 1.0 (softmax)
    // Scorer: assert output in [0, 1] (sigmoid)
```

If any of these fail, the fallback path (ndarray MLP) would be implemented.
