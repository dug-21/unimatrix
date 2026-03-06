//! Convention Scorer: binary confidence MLP.
//!
//! Topology: Linear(32,32) -> ReLU -> Linear(32,1) -> Sigmoid
//! Hand-rolled forward/backward passes using ndarray.

use ndarray::{Array1, Array2, Axis};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::digest::SignalDigest;
use super::traits::NeuralModel;

/// Binary convention confidence scorer.
pub struct ConventionScorer {
    w1: Array2<f32>, // [32, 32]
    b1: Array1<f32>, // [32]
    w2: Array2<f32>, // [32, 1]
    b2: Array1<f32>, // [1]
}

impl ConventionScorer {
    /// Create scorer with baseline weights.
    ///
    /// Xavier/Glorot initialization with deterministic seed.
    /// Output bias: -2.0 biases toward low scores (conservative).
    pub fn new_with_baseline() -> Self {
        let mut rng = StdRng::seed_from_u64(123);
        let w1 = xavier_init(&mut rng, 32, 32);
        let b1 = Array1::zeros(32);
        let w2 = xavier_init(&mut rng, 32, 1);
        let b2 = Array1::from(vec![-2.0]);
        Self { w1, b1, w2, b2 }
    }

    /// Score a signal digest. Returns value in [0.0, 1.0].
    pub fn score(&self, digest: &SignalDigest) -> f32 {
        let output = self.forward(digest.as_slice());
        output[0]
    }

    /// Forward pass returning intermediate activations for backward pass.
    fn forward_layers(
        &self,
        input: &Array1<f32>,
    ) -> (Array1<f32>, Array1<f32>, Array1<f32>) {
        // Layer 1: Linear + ReLU
        let z1 = self.w1.t().dot(input) + &self.b1;
        let a1 = z1.mapv(relu);

        // Layer 2: Linear + Sigmoid
        let z2 = self.w2.t().dot(&a1) + &self.b2;
        let a2 = z2.mapv(sigmoid);

        (a1, z1, a2)
    }
}

impl NeuralModel for ConventionScorer {
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let x = Array1::from(input.to_vec());
        let (_, _, output) = self.forward_layers(&x);
        output.to_vec()
    }

    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
        let x = Array1::from(input.to_vec());
        let t = target[0];
        let (a1, z1, a2) = self.forward_layers(&x);
        let y = a2[0];

        // Binary cross-entropy loss
        let loss = -(t * y.max(1e-7).ln() + (1.0 - t) * (1.0 - y).max(1e-7).ln());

        // Backward: sigmoid + BCE shortcut
        let da2 = Array1::from(vec![y - t]);

        // Layer 2 gradients
        let dw2 = a1
            .view()
            .insert_axis(Axis(1))
            .dot(&da2.view().insert_axis(Axis(0)));
        let db2 = da2.clone();

        // Backprop through layer 2
        let da1 = self.w2.dot(&da2);

        // ReLU derivative
        let dz1 = da1 * z1.mapv(relu_derivative);

        // Layer 1 gradients
        let dw1 = x
            .view()
            .insert_axis(Axis(1))
            .dot(&dz1.view().insert_axis(Axis(0)));
        let db1 = dz1;

        // SGD update
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);

        loss
    }

    fn flat_parameters(&self) -> Vec<f32> {
        let mut params = Vec::with_capacity(32 * 32 + 32 + 32 + 1);
        params.extend(self.w1.iter());
        params.extend(self.b1.iter());
        params.extend(self.w2.iter());
        params.extend(self.b2.iter());
        params
    }

    fn set_parameters(&mut self, params: &[f32]) {
        let mut offset = 0;

        let s = 32 * 32;
        self.w1 = Array2::from_shape_vec((32, 32), params[offset..offset + s].to_vec())
            .expect("w1 shape");
        offset += s;

        self.b1 = Array1::from(params[offset..offset + 32].to_vec());
        offset += 32;

        let s = 32;
        self.w2 = Array2::from_shape_vec((32, 1), params[offset..offset + s].to_vec())
            .expect("w2 shape");
        offset += s;

        self.b2 = Array1::from(params[offset..offset + 1].to_vec());
    }

    fn serialize(&self) -> Vec<u8> {
        let params = self.flat_parameters();
        bincode::serde::encode_to_vec(&params, bincode::config::standard())
            .expect("scorer serialize")
    }

    fn deserialize(data: &[u8]) -> Result<Self, String> {
        let (params, _): (Vec<f32>, _) =
            bincode::serde::decode_from_slice(data, bincode::config::standard())
                .map_err(|e| format!("scorer deserialize: {e}"))?;
        let mut model = Self::new_with_baseline();
        model.set_parameters(&params);
        Ok(model)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn relu(x: f32) -> f32 {
    x.max(0.0)
}

fn relu_derivative(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else {
        0.0
    }
}

fn xavier_init(rng: &mut StdRng, fan_in: usize, fan_out: usize) -> Array2<f32> {
    let scale = (2.0 / (fan_in + fan_out) as f32).sqrt();
    Array2::from_shape_fn((fan_in, fan_out), |_| rng.random::<f32>() * 2.0 * scale - scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-CS-04: Scorer baseline output on zero digest (AC-14)
    #[test]
    fn baseline_zero_digest_low_score() {
        let scorer = ConventionScorer::new_with_baseline();
        let score = scorer.score(&SignalDigest::zeros());
        assert!(
            score < 0.3,
            "zero-digest score {} should be < 0.3",
            score
        );
    }

    // T-CS-05: Scorer output in [0,1] (AC-05)
    #[test]
    fn output_in_range() {
        let scorer = ConventionScorer::new_with_baseline();
        let test_inputs: Vec<Vec<f32>> = vec![
            vec![0.0; 32],
            vec![1.0; 32],
            vec![0.5; 32],
            SignalDigest::from_fields(0.9, 5, 800, "pattern", "implicit-convention", 100, 5)
                .features
                .to_vec(),
        ];
        for input in &test_inputs {
            let output = scorer.forward(input);
            assert_eq!(output.len(), 1);
            assert!(
                (0.0..=1.0).contains(&output[0]),
                "output {} not in [0,1]",
                output[0]
            );
        }
    }

    // T-CS-07: Scorer numerical gradient check
    #[test]
    fn numerical_gradient_check() {
        let mut scorer = ConventionScorer::new_with_baseline();
        let input: Vec<f32> = (0..32).map(|i| (i as f32) * 0.01).collect();
        let target = vec![0.7_f32]; // moderate target for numerical stability

        let params_before = scorer.flat_parameters();
        let _loss = scorer.train_step(&input, &target, 1.0);
        let params_after = scorer.flat_parameters();

        let analytical: Vec<f32> = params_before
            .iter()
            .zip(params_after.iter())
            .map(|(b, a)| b - a)
            .collect();

        scorer.set_parameters(&params_before);
        let h = 1e-3_f32;
        let mut max_rel_error = 0.0_f32;
        let mut checked = 0;
        for i in (0..params_before.len()).step_by(23) {
            if analytical[i].abs() < 1e-6 {
                continue;
            }

            let mut p_plus = params_before.clone();
            p_plus[i] += h;
            scorer.set_parameters(&p_plus);
            let out_plus = scorer.forward(&input);
            let y_plus = out_plus[0];
            let loss_plus =
                -(target[0] * y_plus.max(1e-7).ln()
                    + (1.0 - target[0]) * (1.0 - y_plus).max(1e-7).ln());

            let mut p_minus = params_before.clone();
            p_minus[i] -= h;
            scorer.set_parameters(&p_minus);
            let out_minus = scorer.forward(&input);
            let y_minus = out_minus[0];
            let loss_minus =
                -(target[0] * y_minus.max(1e-7).ln()
                    + (1.0 - target[0]) * (1.0 - y_minus).max(1e-7).ln());

            let numerical = (loss_plus - loss_minus) / (2.0 * h);
            let anal = analytical[i];
            let denom = anal.abs().max(numerical.abs()).max(1e-5);
            let rel_error = (anal - numerical).abs() / denom;
            if rel_error > max_rel_error {
                max_rel_error = rel_error;
            }
            checked += 1;
        }
        assert!(checked > 5, "checked {checked} parameters, need at least 5");
        assert!(
            max_rel_error < 0.15,
            "max relative gradient error {max_rel_error} exceeds 0.15 ({checked} params checked)"
        );
    }

    // T-CS-09: Scorer gradient flow test
    #[test]
    fn gradient_flow() {
        let mut scorer = ConventionScorer::new_with_baseline();
        let input = vec![0.5_f32; 32];
        let target = vec![1.0_f32];

        let initial_loss = {
            let out = scorer.forward(&input);
            let y = out[0];
            -(target[0] * y.max(1e-7).ln() + (1.0 - target[0]) * (1.0 - y).max(1e-7).ln())
        };

        let mut final_loss = initial_loss;
        for _ in 0..10 {
            final_loss = scorer.train_step(&input, &target, 0.01);
        }

        assert!(
            final_loss < initial_loss,
            "loss should decrease: {initial_loss} -> {final_loss}"
        );
    }

    // Scorer serialize roundtrip
    #[test]
    fn serialize_roundtrip() {
        let s1 = ConventionScorer::new_with_baseline();
        let bytes = s1.serialize();
        let s2 = ConventionScorer::deserialize(&bytes).expect("deserialize");

        let input = vec![0.3_f32; 32];
        let out1 = s1.forward(&input);
        let out2 = s2.forward(&input);

        assert!(
            (out1[0] - out2[0]).abs() < 1e-6,
            "output mismatch: {} vs {}",
            out1[0],
            out2[0]
        );
    }

    // Parameter roundtrip
    #[test]
    fn parameter_roundtrip() {
        let s1 = ConventionScorer::new_with_baseline();
        let params = s1.flat_parameters();

        let mut s2 = ConventionScorer::new_with_baseline();
        s2.set_parameters(&params);

        let p2 = s2.flat_parameters();
        assert_eq!(params.len(), p2.len());
        for (i, (a, b)) in params.iter().zip(p2.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "param {i}: {a} vs {b}");
        }
    }
}
