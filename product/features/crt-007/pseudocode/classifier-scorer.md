# Pseudocode: classifier-scorer (Wave 3)

## classifier.rs — SignalClassifier

```rust
use fann::Fann;  // ruv-fann crate
use crate::config::NeuralConfig;
use crate::digest::SignalDigest;
use crate::model::NeuralModel;

pub struct SignalClassifier {
    network: Fann,
}

impl SignalClassifier {
    /// Create a new classifier with baseline weights (ADR-006).
    pub fn new(config: &NeuralConfig) -> Self {
        // 1. Create Fann with topology from config.classifier_topology
        //    Default: [32, 64, 32, 5]
        //    Activation: sigmoid-symmetric for hidden, softmax for output
        //
        // 2. Initialize hidden layer weights: Xavier/Glorot uniform
        //    range = sqrt(6 / (fan_in + fan_out))
        //    Use randomize_weights with calculated range
        //
        // 3. Set output layer biases (ADR-006):
        //    [convention=-0.5, pattern=-0.5, gap=0.0, dead=0.0, noise=+2.0]
        //    Access via set_weight or direct bias manipulation
        //
        // NOTE: ruv-fann may not expose direct bias setting.
        // Fallback: use set_weight on bias connections (neuron index for bias node)
        // or accept default initialization and tune via training.
        //
        // If ruv-fann doesn't support bias manipulation:
        //   Option A: Use randomize_weights and accept default biases
        //   Option B: Run a single training step with synthetic data that
        //             achieves the desired bias effect
        //   Option C: Switch to ndarray fallback for full control

        Self { network }
    }

    /// Create from baseline weights (for fallback on corrupt file).
    pub fn baseline(config: &NeuralConfig) -> Self {
        Self::new(config)
    }

    /// Apply softmax to raw outputs.
    fn softmax(raw: &[f32]) -> [f32; 5] {
        let max = raw.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = raw.iter().map(|x| (x - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let mut result = [0.0; 5];
        for (i, e) in exps.iter().enumerate() {
            result[i] = e / sum;
        }
        result
    }
}

impl NeuralModel for SignalClassifier {
    type Input = SignalDigest;
    type Output = ClassificationResult;

    fn predict(&self, input: &SignalDigest) -> ClassificationResult {
        // 1. Run forward pass: network.run(&input.features)
        // 2. Apply softmax to raw outputs -> probabilities
        // 3. Find argmax -> predicted_class
        // 4. confidence = max probability
        //
        // Guard: if any output is NaN/Inf, return all-noise result
        let raw = self.network.run(&input.features);
        // if raw has NaN: return noise fallback
        let probabilities = Self::softmax(&raw);
        let (max_idx, &confidence) = probabilities.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((4, &1.0)); // default to noise
        ClassificationResult {
            probabilities,
            predicted_class: SignalClass::from_index(max_idx),
            confidence,
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        // Create parent dir if needed
        // self.network.save(path)
        // Verify file was written
        Ok(())
    }

    fn load(path: &Path) -> Result<Self, String> {
        // Fann::load(path)
        // Validate param count matches expected topology
        // Check for NaN/Inf in params
        Ok(Self { network })
    }

    fn param_count(&self) -> usize {
        // network.get_total_connections()
    }

    fn params_flat(&self) -> Vec<f32> {
        // Extract all weights from network as flat Vec<f32>
        // ruv-fann: iterate connections
    }
}
```

## scorer.rs — ConventionScorer

```rust
pub struct ConventionScorer {
    network: Fann,
}

impl ConventionScorer {
    /// Create with baseline weights (ADR-006).
    pub fn new(config: &NeuralConfig) -> Self {
        // 1. Create Fann with topology from config.scorer_topology
        //    Default: [32, 32, 1]
        //    Activation: sigmoid-symmetric for hidden, sigmoid for output
        //
        // 2. Xavier/Glorot initialization for hidden weights
        //
        // 3. Output bias: -1.0 (sigmoid(-1.0) ~= 0.27)
        //    Same bias-setting considerations as classifier

        Self { network }
    }

    pub fn baseline(config: &NeuralConfig) -> Self {
        Self::new(config)
    }

    fn sigmoid(x: f32) -> f32 {
        1.0 / (1.0 + (-x).exp())
    }
}

impl NeuralModel for ConventionScorer {
    type Input = SignalDigest;
    type Output = ConventionScore;

    fn predict(&self, input: &SignalDigest) -> ConventionScore {
        // 1. Forward pass: network.run(&input.features)
        // 2. Apply sigmoid to output (if not already applied by ruv-fann)
        // 3. Clamp to [0.0, 1.0]
        //
        // Guard: NaN/Inf -> return score 0.0
        let raw = self.network.run(&input.features);
        let score = Self::sigmoid(raw[0]).clamp(0.0, 1.0);
        ConventionScore { score }
    }

    fn save(&self, path: &Path) -> Result<(), String> { ... }
    fn load(path: &Path) -> Result<Self, String> { ... }
    fn param_count(&self) -> usize { ... }
    fn params_flat(&self) -> Vec<f32> { ... }
}
```

## Baseline Weight Verification Tests

```
#[test] fn classifier_strong_signal_not_noise()
    // Input: consistency=1.0, feature_count=10, rule_confidence=0.9
    // Expected: predicted_class != Noise
    // This validates ADR-006 bias calibration

#[test] fn classifier_weak_signal_biased_to_noise()
    // Input: all features near 0
    // Expected: noise probability > other classes

#[test] fn scorer_strong_signal_above_half()
    // Input: strong convention indicators
    // Expected: score > 0.5

#[test] fn scorer_weak_signal_below_threshold()
    // Input: near-zero features
    // Expected: score < 0.6 (below Active threshold)
```
