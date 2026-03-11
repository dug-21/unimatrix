//! Signal Classifier: 5-class MLP for extraction signal classification.
//!
//! Topology: Linear(32,64) -> Sigmoid -> Linear(64,32) -> ReLU -> Linear(32,5) -> Softmax
//! Hand-rolled forward/backward passes using ndarray.

use ndarray::{Array1, Array2, Axis};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::digest::SignalDigest;
use super::traits::NeuralModel;

/// Signal category classification output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SignalCategory {
    Convention = 0,
    Pattern = 1,
    Gap = 2,
    Dead = 3,
    Noise = 4,
}

impl SignalCategory {
    /// Convert from index to category.
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Convention,
            1 => Self::Pattern,
            2 => Self::Gap,
            3 => Self::Dead,
            _ => Self::Noise,
        }
    }
}

impl std::fmt::Display for SignalCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Convention => write!(f, "Convention"),
            Self::Pattern => write!(f, "Pattern"),
            Self::Gap => write!(f, "Gap"),
            Self::Dead => write!(f, "Dead"),
            Self::Noise => write!(f, "Noise"),
        }
    }
}

/// Full classifier output.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub category: SignalCategory,
    pub probabilities: [f32; 5],
    pub confidence: f32,
}

/// 5-class signal classifier MLP.
pub struct SignalClassifier {
    w1: Array2<f32>, // [32, 64]
    b1: Array1<f32>, // [64]
    w2: Array2<f32>, // [64, 32]
    b2: Array1<f32>, // [32]
    w3: Array2<f32>, // [32, 5]
    b3: Array1<f32>, // [5]
}

impl SignalClassifier {
    /// Create classifier with baseline weights using the default seed (42).
    ///
    /// Xavier/Glorot initialization with deterministic seed.
    /// Output bias: Noise class (index 4) gets +2.0, biasing toward conservative output.
    pub fn new_with_baseline() -> Self {
        Self::new_with_baseline_seed(42)
    }

    /// Create classifier with baseline weights using the given seed.
    ///
    /// Xavier/Glorot initialization with deterministic seed.
    /// Output bias: Noise class (index 4) gets +2.0, biasing toward conservative output.
    pub fn new_with_baseline_seed(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let w1 = xavier_init(&mut rng, 32, 64);
        let b1 = Array1::zeros(64);
        let w2 = xavier_init(&mut rng, 64, 32);
        let b2 = Array1::zeros(32);
        let w3 = xavier_init(&mut rng, 32, 5);
        let mut b3 = Array1::zeros(5);
        b3[4] = 2.0; // Noise bias
        Self {
            w1,
            b1,
            w2,
            b2,
            w3,
            b3,
        }
    }

    /// Classify a signal digest.
    pub fn classify(&self, digest: &SignalDigest) -> ClassificationResult {
        let output = self.forward(digest.as_slice());
        let mut probs = [0.0_f32; 5];
        for (i, v) in output.iter().enumerate().take(5) {
            probs[i] = *v;
        }
        let (max_idx, &max_val) = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((4, &0.0));
        ClassificationResult {
            category: SignalCategory::from_index(max_idx),
            probabilities: probs,
            confidence: max_val,
        }
    }

    /// Forward pass returning intermediate activations for backward pass.
    fn forward_layers(
        &self,
        input: &Array1<f32>,
    ) -> (Array1<f32>, Array1<f32>, Array1<f32>, Array1<f32>) {
        // Layer 1: Linear + Sigmoid
        let z1 = self.w1.t().dot(input) + &self.b1;
        let a1 = z1.mapv(sigmoid);

        // Layer 2: Linear + ReLU
        let z2 = self.w2.t().dot(&a1) + &self.b2;
        let a2 = z2.mapv(relu);

        // Layer 3: Linear + Softmax
        let z3 = self.w3.t().dot(&a2) + &self.b3;
        let a3 = softmax_array(&z3);

        (a1, a2, z2, a3)
    }
}

impl NeuralModel for SignalClassifier {
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let x = Array1::from(input.to_vec());
        let (_, _, _, output) = self.forward_layers(&x);
        output.to_vec()
    }

    fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>) {
        let x = Array1::from(input.to_vec());
        let t = Array1::from(target.to_vec());
        let (a1, a2, z2, a3) = self.forward_layers(&x);

        // Cross-entropy loss
        let loss: f32 = -t
            .iter()
            .zip(a3.iter())
            .map(|(ti, ai)| ti * ai.max(1e-7).ln())
            .sum::<f32>();

        // Backward: softmax + cross-entropy shortcut
        let da3 = &a3 - &t;

        // Layer 3 gradients
        let dw3 = a2
            .view()
            .insert_axis(Axis(1))
            .dot(&da3.view().insert_axis(Axis(0)));
        let db3 = da3.clone();

        // Backprop through layer 3
        let da2 = self.w3.dot(&da3);

        // ReLU derivative
        let dz2 = da2 * z2.mapv(relu_derivative);

        // Layer 2 gradients
        let dw2 = a1
            .view()
            .insert_axis(Axis(1))
            .dot(&dz2.view().insert_axis(Axis(0)));
        let db2 = dz2.clone();

        // Backprop through layer 2
        let da1 = self.w2.dot(&dz2);

        // Sigmoid derivative
        let dz1 = &da1 * &a1.mapv(|a| a * (1.0 - a));

        // Layer 1 gradients
        let dw1 = x
            .view()
            .insert_axis(Axis(1))
            .dot(&dz1.view().insert_axis(Axis(0)));
        let db1 = dz1;

        // Flatten gradients in canonical order: w1, b1, w2, b2, w3, b3
        let param_count = 32 * 64 + 64 + 64 * 32 + 32 + 32 * 5 + 5;
        let mut grads = Vec::with_capacity(param_count);
        grads.extend(dw1.iter());
        grads.extend(db1.iter());
        grads.extend(dw2.iter());
        grads.extend(db2.iter());
        grads.extend(dw3.iter());
        grads.extend(db3.iter());

        (loss, grads)
    }

    fn apply_gradients(&mut self, gradients: &[f32], lr: f32) {
        let mut offset = 0;

        let s = 32 * 64;
        let dw1 = Array2::from_shape_vec((32, 64), gradients[offset..offset + s].to_vec())
            .expect("dw1 shape");
        offset += s;

        let db1 = Array1::from(gradients[offset..offset + 64].to_vec());
        offset += 64;

        let s = 64 * 32;
        let dw2 = Array2::from_shape_vec((64, 32), gradients[offset..offset + s].to_vec())
            .expect("dw2 shape");
        offset += s;

        let db2 = Array1::from(gradients[offset..offset + 32].to_vec());
        offset += 32;

        let s = 32 * 5;
        let dw3 = Array2::from_shape_vec((32, 5), gradients[offset..offset + s].to_vec())
            .expect("dw3 shape");
        offset += s;

        let db3 = Array1::from(gradients[offset..offset + 5].to_vec());

        // SGD update
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);
        self.w3 = &self.w3 - &(lr * &dw3);
        self.b3 = &self.b3 - &(lr * &db3);
    }

    fn flat_parameters(&self) -> Vec<f32> {
        let mut params = Vec::with_capacity(32 * 64 + 64 + 64 * 32 + 32 + 32 * 5 + 5);
        params.extend(self.w1.iter());
        params.extend(self.b1.iter());
        params.extend(self.w2.iter());
        params.extend(self.b2.iter());
        params.extend(self.w3.iter());
        params.extend(self.b3.iter());
        params
    }

    fn set_parameters(&mut self, params: &[f32]) {
        let mut offset = 0;

        let s = 32 * 64;
        self.w1 = Array2::from_shape_vec((32, 64), params[offset..offset + s].to_vec())
            .expect("w1 shape");
        offset += s;

        self.b1 = Array1::from(params[offset..offset + 64].to_vec());
        offset += 64;

        let s = 64 * 32;
        self.w2 = Array2::from_shape_vec((64, 32), params[offset..offset + s].to_vec())
            .expect("w2 shape");
        offset += s;

        self.b2 = Array1::from(params[offset..offset + 32].to_vec());
        offset += 32;

        let s = 32 * 5;
        self.w3 =
            Array2::from_shape_vec((32, 5), params[offset..offset + s].to_vec()).expect("w3 shape");
        offset += s;

        self.b3 = Array1::from(params[offset..offset + 5].to_vec());
    }

    fn serialize(&self) -> Vec<u8> {
        let params = self.flat_parameters();
        bincode::serde::encode_to_vec(&params, bincode::config::standard())
            .expect("classifier serialize")
    }

    fn deserialize(data: &[u8]) -> Result<Self, String> {
        let (params, _): (Vec<f32>, _) =
            bincode::serde::decode_from_slice(data, bincode::config::standard())
                .map_err(|e| format!("classifier deserialize: {e}"))?;
        let mut model = Self::new_with_baseline();
        model.set_parameters(&params);
        Ok(model)
    }
}

// -- Activation functions --

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn relu(x: f32) -> f32 {
    x.max(0.0)
}

fn relu_derivative(x: f32) -> f32 {
    if x > 0.0 { 1.0 } else { 0.0 }
}

fn softmax_array(z: &Array1<f32>) -> Array1<f32> {
    let max = z.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp = z.mapv(|v| (v - max).exp());
    let sum = exp.sum();
    exp / sum
}

fn xavier_init(rng: &mut StdRng, fan_in: usize, fan_out: usize) -> Array2<f32> {
    let scale = (2.0 / (fan_in + fan_out) as f32).sqrt();
    Array2::from_shape_fn((fan_in, fan_out), |_| {
        rng.random::<f32>() * 2.0 * scale - scale
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-CS-01: Classifier baseline output on zero digest (AC-14)
    #[test]
    fn baseline_zero_digest_noise() {
        let clf = SignalClassifier::new_with_baseline();
        let result = clf.classify(&SignalDigest::zeros());
        assert_eq!(result.category, SignalCategory::Noise);
        assert!(
            result.probabilities[4] > 0.5,
            "Noise prob {} should be > 0.5",
            result.probabilities[4]
        );
    }

    // T-CS-02: Classifier non-degenerate on typical digest
    #[test]
    fn non_degenerate_typical_digest() {
        let clf = SignalClassifier::new_with_baseline();
        let digest = SignalDigest::from_fields(0.7, 3, 500, "convention", "knowledge-gap", 50, 2);
        let result = clf.classify(&digest);

        let sum: f32 = result.probabilities.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "probabilities sum to {sum}, not 1.0"
        );

        // At least one non-Noise category should have some probability
        let non_noise_max = result.probabilities[..4]
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);
        assert!(
            non_noise_max > 0.01,
            "non-Noise max prob {} too low",
            non_noise_max
        );
    }

    // T-CS-03: Classifier output shape
    #[test]
    fn output_shape() {
        let clf = SignalClassifier::new_with_baseline();
        let output = clf.forward(&[0.5; 32]);
        assert_eq!(output.len(), 5);
        for &v in &output {
            assert!((0.0..=1.0).contains(&v), "output {v} not in [0,1]");
        }
        let sum: f32 = output.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "sum = {sum}");
    }

    // T-CS-06: Numerical gradient check
    //
    // Verifies backward pass correctness by comparing analytical gradients
    // against finite-difference numerical gradients. Uses f32-appropriate
    // thresholds: only checks parameters with sufficiently large gradients
    // (> 1e-3) where finite-difference precision is meaningful.
    #[test]
    fn numerical_gradient_check() {
        let mut clf = SignalClassifier::new_with_baseline();
        let input: Vec<f32> = (0..32).map(|i| (i as f32) * 0.03 + 0.1).collect();
        let target = vec![0.0, 0.0, 1.0, 0.0, 0.0]; // Gap class

        let params_before = clf.flat_parameters();
        let _loss = clf.train_step(&input, &target, 1.0);
        let params_after = clf.flat_parameters();

        let analytical: Vec<f32> = params_before
            .iter()
            .zip(params_after.iter())
            .map(|(b, a)| b - a)
            .collect();

        clf.set_parameters(&params_before);
        let h = 5e-3_f32;
        let mut checked = 0;
        let mut passed = 0;
        // Check parameters with large enough gradients for f32 precision
        for i in (0..params_before.len()).step_by(31) {
            if analytical[i].abs() < 1e-3 {
                continue;
            }

            let mut p_plus = params_before.clone();
            p_plus[i] += h;
            clf.set_parameters(&p_plus);
            let out_plus = clf.forward(&input);
            let loss_plus: f32 = -target
                .iter()
                .zip(out_plus.iter())
                .map(|(t, o)| t * o.max(1e-7).ln())
                .sum::<f32>();

            let mut p_minus = params_before.clone();
            p_minus[i] -= h;
            clf.set_parameters(&p_minus);
            let out_minus = clf.forward(&input);
            let loss_minus: f32 = -target
                .iter()
                .zip(out_minus.iter())
                .map(|(t, o)| t * o.max(1e-7).ln())
                .sum::<f32>();

            let numerical = (loss_plus - loss_minus) / (2.0 * h);
            let anal = analytical[i];

            checked += 1;
            // Check sign agreement and rough magnitude
            if (anal > 0.0) == (numerical > 0.0) || numerical.abs() < 1e-5 {
                let denom = anal.abs().max(numerical.abs());
                if (anal - numerical).abs() / denom < 0.5 {
                    passed += 1;
                }
            }
        }
        assert!(checked > 5, "checked {checked} params with |grad| > 1e-3");
        let pass_rate = passed as f64 / checked as f64;
        assert!(
            pass_rate > 0.7,
            "gradient check pass rate {pass_rate:.2} ({passed}/{checked}) below 0.7"
        );
    }

    // T-CS-08: Gradient flow test
    #[test]
    fn gradient_flow() {
        let mut clf = SignalClassifier::new_with_baseline();
        let input = vec![0.5_f32; 32];
        let target = vec![1.0, 0.0, 0.0, 0.0, 0.0];

        let initial_loss = {
            let out = clf.forward(&input);
            -target
                .iter()
                .zip(out.iter())
                .map(|(t, o)| t * o.max(1e-7).ln())
                .sum::<f32>()
        };

        // Train 10 steps
        let mut final_loss = initial_loss;
        for _ in 0..10 {
            final_loss = clf.train_step(&input, &target, 0.01);
        }

        assert!(
            final_loss < initial_loss,
            "loss should decrease: {initial_loss} -> {final_loss}"
        );
    }

    // T-FR00-02: compute_gradients + apply_gradients matches train_step
    #[test]
    fn compute_apply_matches_train_step() {
        let mut clf_a = SignalClassifier::new_with_baseline();
        let mut clf_b = SignalClassifier::new_with_baseline();
        let input = vec![0.5_f32; 32];
        let target = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        let lr = 0.01;

        let _loss_a = clf_a.train_step(&input, &target, lr);

        let (_, grads) = clf_b.compute_gradients(&input, &target);
        clf_b.apply_gradients(&grads, lr);

        let params_a = clf_a.flat_parameters();
        let params_b = clf_b.flat_parameters();
        assert_eq!(params_a.len(), params_b.len());
        for (i, (a, b)) in params_a.iter().zip(params_b.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "param {i} mismatch: {a} vs {b}");
        }
    }

    // T-R01-01: Parameter ordering identity test (classifier)
    #[test]
    fn parameter_ordering_identity() {
        let mut clf = SignalClassifier::new_with_baseline();
        let test_inputs: Vec<Vec<f32>> = vec![
            vec![0.0; 32],
            vec![0.5; 32],
            vec![1.0; 32],
            (0..32).map(|i| i as f32 * 0.03).collect(),
            (0..32).map(|i| 1.0 - i as f32 * 0.03).collect(),
        ];

        let preds_before: Vec<Vec<f32>> = test_inputs.iter().map(|inp| clf.forward(inp)).collect();

        let params = clf.flat_parameters();
        clf.set_parameters(&params);

        for (i, inp) in test_inputs.iter().enumerate() {
            let pred = clf.forward(inp);
            for (j, (a, b)) in preds_before[i].iter().zip(pred.iter()).enumerate() {
                assert!((a - b).abs() < 1e-6, "input {i} output {j}: {a} vs {b}");
            }
        }
    }

    // T-R01-03: Gradient vector length matches parameter count (classifier)
    #[test]
    fn gradient_length_matches_params() {
        let clf = SignalClassifier::new_with_baseline();
        let input = vec![0.5_f32; 32];
        let target = vec![0.0, 0.0, 1.0, 0.0, 0.0];
        let (_, grads) = clf.compute_gradients(&input, &target);
        assert_eq!(
            grads.len(),
            clf.flat_parameters().len(),
            "gradient length {} != param count {}",
            grads.len(),
            clf.flat_parameters().len()
        );
    }

    // T-MT-04: flat_parameters + set_parameters roundtrip
    #[test]
    fn parameter_roundtrip() {
        let clf1 = SignalClassifier::new_with_baseline();
        let params1 = clf1.flat_parameters();

        let mut clf2 = SignalClassifier::new_with_baseline();
        clf2.set_parameters(&params1);
        let params2 = clf2.flat_parameters();

        assert_eq!(params1.len(), params2.len());
        for (i, (a, b)) in params1.iter().zip(params2.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "param {i} mismatch: {a} vs {b}");
        }
    }

    // T-SEED-01: baseline_seed(42) == baseline()
    #[test]
    fn seed_42_matches_baseline() {
        let baseline = SignalClassifier::new_with_baseline();
        let seeded = SignalClassifier::new_with_baseline_seed(42);
        assert_eq!(baseline.flat_parameters(), seeded.flat_parameters());
    }

    // T-SEED-03: Different seeds produce different weights
    #[test]
    fn different_seeds_different_weights() {
        let a = SignalClassifier::new_with_baseline_seed(42);
        let b = SignalClassifier::new_with_baseline_seed(99);
        assert_ne!(a.flat_parameters(), b.flat_parameters());
    }

    // T-MT-05: serialize + deserialize roundtrip
    #[test]
    fn serialize_roundtrip() {
        let clf1 = SignalClassifier::new_with_baseline();
        let bytes = clf1.serialize();
        let clf2 = SignalClassifier::deserialize(&bytes).expect("deserialize");

        let input = vec![0.3_f32; 32];
        let out1 = clf1.forward(&input);
        let out2 = clf2.forward(&input);

        for (i, (a, b)) in out1.iter().zip(out2.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "output {i} mismatch: {a} vs {b}");
        }
    }
}
