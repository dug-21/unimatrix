//! MicroLoRA engine: low-rank adaptation of embedding vectors.
//!
//! Forward: `output = input + scale * (input @ A @ B)`
//! A: down-projection (d x r), Xavier init
//! B: up-projection (r x d), near-zero init (identity at start)

use std::sync::RwLock;

use ndarray::{Array2, ArrayView1, Axis};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand_distr::{Distribution, Normal};

/// LoRA-specific configuration (subset of AdaptConfig).
#[derive(Debug, Clone)]
pub struct LoraConfig {
    pub rank: u8,
    pub dimension: u16,
    pub scale: f32,
}

/// Weight matrices.
#[derive(Debug, Clone)]
struct LoraWeights {
    a: Array2<f32>, // d x r (down-projection)
    b: Array2<f32>, // r x d (up-projection)
}

/// MicroLoRA engine for low-rank embedding adaptation.
///
/// Thread-safe: forward pass takes a read lock, weight update takes a write lock.
/// Multiple concurrent forward passes do not block each other.
pub struct MicroLoRA {
    config: LoraConfig,
    weights: RwLock<LoraWeights>,
}

impl MicroLoRA {
    /// Create a new MicroLoRA with Xavier init for A, near-zero for B.
    pub fn new(config: LoraConfig) -> Self {
        Self::with_seed(config, 42)
    }

    /// Create with explicit RNG seed (for testing).
    pub fn with_seed(config: LoraConfig, seed: u64) -> Self {
        let d = config.dimension as usize;
        let r = config.rank as usize;
        let mut rng = StdRng::seed_from_u64(seed);

        // Xavier normal: std = sqrt(2 / (d + r))
        let std_a = (2.0 / (d + r) as f32).sqrt();
        let normal_a = Normal::new(0.0_f32, std_a).expect("valid std for Normal");
        let a = Array2::from_shape_fn((d, r), |_| normal_a.sample(&mut rng));

        // Near-zero for B: scale 1e-4
        let normal_b = Normal::new(0.0_f32, 1e-4).expect("valid std for Normal");
        let b = Array2::from_shape_fn((r, d), |_| normal_b.sample(&mut rng));

        Self {
            config,
            weights: RwLock::new(LoraWeights { a, b }),
        }
    }

    /// Forward pass: `output = input + scale * (input @ A @ B)`.
    ///
    /// Caller is responsible for L2 normalization of the output.
    /// Takes a read lock on weights -- multiple concurrent calls do not block.
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        let d = self.config.dimension as usize;
        assert_eq!(input.len(), d, "input dimension mismatch");

        let weights = self.weights.read().expect("weights lock poisoned");
        let input_arr = ArrayView1::from(input);

        // Step 1: down-project: A^T @ input -> (r,)
        let down = weights.a.t().dot(&input_arr);

        // Step 2: up-project: B^T @ down -> (d,)
        let up = weights.b.t().dot(&down);

        // Step 3: residual connection: output = input + scale * up
        let mut output = input.to_vec();
        let scale = self.config.scale;
        for i in 0..d {
            output[i] += scale * up[i];
        }

        output
    }

    /// Backward pass: compute gradients of loss wrt A and B.
    ///
    /// Returns (grad_A, grad_B) given input and dL/d(output).
    pub fn backward(&self, input: &[f32], grad_output: &[f32]) -> (Array2<f32>, Array2<f32>) {
        let weights = self.weights.read().expect("weights lock poisoned");
        let input_arr = ArrayView1::from(input);
        let grad_out = ArrayView1::from(grad_output);
        let scale = self.config.scale;

        // Recompute forward intermediate: down = A^T @ input
        let down = weights.a.t().dot(&input_arr); // (r,)

        // dL/dB = scale * outer(down, grad_output)  -- (r, d)
        let grad_b = scale * outer(&down.view(), &grad_out);

        // dL/d(down) = scale * B @ grad_output  -- (r,)
        let grad_down = scale * weights.b.dot(&grad_out);

        // dL/dA = outer(input, grad_down)  -- (d, r)
        let grad_a = outer(&input_arr, &grad_down.view());

        (grad_a, grad_b)
    }

    /// Update weights via SGD with separate learning rates (LoRA+).
    ///
    /// NaN/Inf check: if any new weight is NaN or Inf, the update is aborted
    /// and a warning is logged.
    pub fn update_weights(&self, grad_a: &Array2<f32>, grad_b: &Array2<f32>, lr_a: f32, lr_b: f32) {
        // Read current weights to compute new values
        let current = {
            let w = self.weights.read().expect("weights lock poisoned");
            (w.a.clone(), w.b.clone())
        };

        let new_a = &current.0 - &(lr_a * grad_a);
        let new_b = &current.1 - &(lr_b * grad_b);

        // NaN/Inf guard
        if contains_nan_or_inf(&new_a) || contains_nan_or_inf(&new_b) {
            tracing::warn!("NaN/Inf detected in weight update, skipping");
            return;
        }

        // Atomic swap under write lock
        let mut weights = self.weights.write().expect("weights lock poisoned");
        weights.a = new_a;
        weights.b = new_b;
    }

    /// Flatten all parameters into a single vector for EWC.
    ///
    /// Layout: [A.flatten(), B.flatten()], total length = 2 * d * r.
    pub fn parameters_flat(&self) -> Vec<f32> {
        let weights = self.weights.read().expect("weights lock poisoned");
        let d = self.config.dimension as usize;
        let r = self.config.rank as usize;
        let mut flat = Vec::with_capacity(2 * d * r);
        flat.extend(weights.a.iter());
        flat.extend(weights.b.iter());
        flat
    }

    /// Get a copy of the A matrix (for persistence/inspection).
    pub fn weights_a(&self) -> Array2<f32> {
        self.weights
            .read()
            .expect("weights lock poisoned")
            .a
            .clone()
    }

    /// Get a copy of the B matrix (for persistence/inspection).
    pub fn weights_b(&self) -> Array2<f32> {
        self.weights
            .read()
            .expect("weights lock poisoned")
            .b
            .clone()
    }

    /// Set weight matrices (for state restoration).
    pub fn set_weights(&self, a: Array2<f32>, b: Array2<f32>) {
        let mut weights = self.weights.write().expect("weights lock poisoned");
        weights.a = a;
        weights.b = b;
    }

    /// Get the LoRA config.
    pub fn config(&self) -> &LoraConfig {
        &self.config
    }
}

/// Outer product of two 1D arrays: (n,) x (m,) -> (n, m).
fn outer(a: &ArrayView1<f32>, b: &ArrayView1<f32>) -> Array2<f32> {
    let n = a.len();
    let m = b.len();
    let a_col = a.to_owned().insert_axis(Axis(1)); // (n, 1)
    let b_row = b.to_owned().insert_axis(Axis(0)); // (1, m)
    let mut result = Array2::zeros((n, m));
    ndarray::Zip::from(&mut result)
        .and(&a_col.broadcast((n, m)).unwrap())
        .and(&b_row.broadcast((n, m)).unwrap())
        .for_each(|r, &a_val, &b_val| {
            *r = a_val * b_val;
        });
    result
}

/// Check if any element is NaN or Inf.
fn contains_nan_or_inf(arr: &Array2<f32>) -> bool {
    arr.iter().any(|v| v.is_nan() || v.is_infinite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> LoraConfig {
        LoraConfig {
            rank: 4,
            dimension: 384,
            scale: 1.0,
        }
    }

    fn small_config() -> LoraConfig {
        LoraConfig {
            rank: 4,
            dimension: 16,
            scale: 1.0,
        }
    }

    // T-LOR-01: Construction at various ranks
    #[test]
    fn construction_various_ranks() {
        for rank in [2, 4, 8, 16] {
            let config = LoraConfig {
                rank,
                dimension: 384,
                scale: 1.0,
            };
            let lora = MicroLoRA::new(config);
            let a = lora.weights_a();
            let b = lora.weights_b();
            assert_eq!(a.shape(), &[384, rank as usize]);
            assert_eq!(b.shape(), &[rank as usize, 384]);
            assert!(!contains_nan_or_inf(&a));
            assert!(!contains_nan_or_inf(&b));
        }
    }

    // T-LOR-02: Forward pass output dimension
    #[test]
    fn forward_pass_output_dimension() {
        let lora = MicroLoRA::new(default_config());
        let input = vec![0.1_f32; 384];
        let output = lora.forward(&input);
        assert_eq!(output.len(), 384);
        assert!(!output.iter().any(|v| v.is_nan() || v.is_infinite()));
    }

    // T-LOR-03: Near-identity at initialization
    #[test]
    fn near_identity_at_init() {
        let lora = MicroLoRA::new(default_config());
        for seed in 0..10 {
            let input: Vec<f32> = (0..384)
                .map(|i| ((i as f32 + seed as f32) * 0.01).sin())
                .collect();
            let mut normalized = input.clone();
            let norm: f32 = normalized.iter().map(|v| v * v).sum::<f32>().sqrt();
            for v in &mut normalized {
                *v /= norm;
            }

            let output = lora.forward(&normalized);
            let cos_sim = cosine_sim(&normalized, &output);
            assert!(
                cos_sim > 0.99,
                "near-identity failed: cos_sim={cos_sim} for seed {seed}"
            );
        }
    }

    // T-LOR-04: Gradient correctness via finite differences
    #[test]
    fn gradient_correctness_finite_diff() {
        for &rank in &[2u8, 4, 8] {
            let config = LoraConfig {
                rank,
                dimension: 16,
                scale: 1.0,
            };
            let lora = MicroLoRA::with_seed(config.clone(), 123);
            let d = 16;
            let r = rank as usize;

            let input: Vec<f32> = (0..d).map(|i| (i as f32 * 0.1).sin()).collect();
            let grad_output: Vec<f32> = (0..d).map(|i| (i as f32 * 0.2).cos()).collect();

            // Analytical gradients
            let (grad_a, grad_b) = lora.backward(&input, &grad_output);

            // Numerical gradients for A
            let eps = 1e-4_f32;
            for i in 0..d {
                for j in 0..r {
                    let original = lora.weights_a()[[i, j]];

                    let mut a_plus = lora.weights_a();
                    a_plus[[i, j]] = original + eps;
                    lora.set_weights(a_plus, lora.weights_b());
                    let out_plus = lora.forward(&input);
                    let f_plus: f32 = out_plus.iter().zip(&grad_output).map(|(o, g)| o * g).sum();

                    let mut a_minus = lora.weights_a();
                    a_minus[[i, j]] = original - eps;
                    lora.set_weights(a_minus, lora.weights_b());
                    let out_minus = lora.forward(&input);
                    let f_minus: f32 = out_minus.iter().zip(&grad_output).map(|(o, g)| o * g).sum();

                    // Restore
                    let mut a_restore = lora.weights_a();
                    a_restore[[i, j]] = original;
                    lora.set_weights(a_restore, lora.weights_b());

                    let numerical = (f_plus - f_minus) / (2.0 * eps);
                    let analytical = grad_a[[i, j]];
                    let diff = (numerical - analytical).abs();
                    assert!(
                        diff < 0.05,
                        "A grad mismatch at [{i},{j}] rank={rank}: analytical={analytical}, numerical={numerical}, diff={diff}"
                    );
                }
            }

            // Numerical gradients for B
            for i in 0..r {
                for j in 0..d {
                    let original = lora.weights_b()[[i, j]];

                    let mut b_plus = lora.weights_b();
                    b_plus[[i, j]] = original + eps;
                    lora.set_weights(lora.weights_a(), b_plus);
                    let out_plus = lora.forward(&input);
                    let f_plus: f32 = out_plus.iter().zip(&grad_output).map(|(o, g)| o * g).sum();

                    let mut b_minus = lora.weights_b();
                    b_minus[[i, j]] = original - eps;
                    lora.set_weights(lora.weights_a(), b_minus);
                    let out_minus = lora.forward(&input);
                    let f_minus: f32 = out_minus.iter().zip(&grad_output).map(|(o, g)| o * g).sum();

                    // Restore
                    let mut b_restore = lora.weights_b();
                    b_restore[[i, j]] = original;
                    lora.set_weights(lora.weights_a(), b_restore);

                    let numerical = (f_plus - f_minus) / (2.0 * eps);
                    let analytical = grad_b[[i, j]];
                    let diff = (numerical - analytical).abs();
                    assert!(
                        diff < 0.05,
                        "B grad mismatch at [{i},{j}] rank={rank}: analytical={analytical}, numerical={numerical}, diff={diff}"
                    );
                }
            }
        }
    }

    // T-LOR-06: NaN guard in weight update
    #[test]
    fn nan_guard_weight_update() {
        let lora = MicroLoRA::new(small_config());
        let initial_params = lora.parameters_flat();

        // NaN gradients
        let mut nan_grad = Array2::zeros((16, 4));
        nan_grad[[0, 0]] = f32::NAN;
        lora.update_weights(&nan_grad, &Array2::zeros((4, 16)), 0.01, 0.16);
        assert_eq!(
            lora.parameters_flat(),
            initial_params,
            "NaN gradient should not change weights"
        );

        // Inf gradients
        let mut inf_grad = Array2::zeros((4, 16));
        inf_grad[[0, 0]] = f32::INFINITY;
        lora.update_weights(&Array2::zeros((16, 4)), &inf_grad, 0.01, 0.16);
        assert_eq!(
            lora.parameters_flat(),
            initial_params,
            "Inf gradient should not change weights"
        );
    }

    // T-LOR-08: Parameters flat round-trip
    #[test]
    fn parameters_flat_roundtrip() {
        let lora = MicroLoRA::new(small_config());
        let flat = lora.parameters_flat();
        assert_eq!(flat.len(), 2 * 16 * 4);

        let a = lora.weights_a();
        let b = lora.weights_b();
        let a_flat: Vec<f32> = a.iter().cloned().collect();
        let b_flat: Vec<f32> = b.iter().cloned().collect();

        assert_eq!(&flat[..64], &a_flat[..]);
        assert_eq!(&flat[64..], &b_flat[..]);
    }

    // T-LOR-09: Weight update correctness
    #[test]
    fn weight_update_correctness() {
        let lora = MicroLoRA::with_seed(small_config(), 99);
        let old_a = lora.weights_a();
        let old_b = lora.weights_b();

        let grad_a = Array2::from_elem((16, 4), 1.0_f32);
        let grad_b = Array2::from_elem((4, 16), 1.0_f32);
        lora.update_weights(&grad_a, &grad_b, 0.01, 0.16);

        let new_a = lora.weights_a();
        let new_b = lora.weights_b();

        for ((o, n), g) in old_a.iter().zip(new_a.iter()).zip(grad_a.iter()) {
            let expected = o - 0.01 * g;
            assert!(
                (n - expected).abs() < 1e-6,
                "A update: old={o}, expected={expected}, got={n}"
            );
        }
        for ((o, n), g) in old_b.iter().zip(new_b.iter()).zip(grad_b.iter()) {
            let expected = o - 0.16 * g;
            assert!(
                (n - expected).abs() < 1e-6,
                "B update: old={o}, expected={expected}, got={n}"
            );
        }
    }

    // T-LOR-10: Forward pass determinism
    #[test]
    fn forward_pass_determinism() {
        let lora = MicroLoRA::new(small_config());
        let input: Vec<f32> = (0..16).map(|i| (i as f32 * 0.1).sin()).collect();
        let out1 = lora.forward(&input);
        let out2 = lora.forward(&input);
        assert_eq!(out1, out2, "Forward pass should be deterministic");
    }

    // Zero input vector edge case
    #[test]
    fn forward_zero_input() {
        let lora = MicroLoRA::new(small_config());
        let input = vec![0.0_f32; 16];
        let output = lora.forward(&input);
        assert_eq!(output.len(), 16);
        assert!(!output.iter().any(|v| v.is_nan()));
    }

    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na < 1e-12 || nb < 1e-12 {
            return 0.0;
        }
        dot / (na * nb)
    }
}
