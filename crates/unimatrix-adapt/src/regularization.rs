//! EWC++ online regularization: prevents catastrophic forgetting.
//!
//! Maintains a diagonal Fisher information approximation and reference parameters
//! as running exponential averages. The penalty term constrains weight updates
//! to stay close to previously important parameter configurations.

use ndarray::{Array1, Array2, ArrayView1};

/// EWC++ regularization state.
///
/// Maintains the Fisher information diagonal and reference parameters,
/// updated online after each training step.
pub struct EwcState {
    fisher: Array1<f32>,
    reference_params: Array1<f32>,
    /// Exponential decay factor for Fisher updates.
    pub alpha: f32,
    /// Penalty weight (lambda) for the regularization term.
    pub lambda: f32,
    initialized: bool,
}

impl EwcState {
    /// Create a new EWC state with zero Fisher and reference params.
    pub fn new(param_count: usize, alpha: f32, lambda: f32) -> Self {
        Self {
            fisher: Array1::zeros(param_count),
            reference_params: Array1::zeros(param_count),
            alpha,
            lambda,
            initialized: false,
        }
    }

    /// Compute the EWC penalty: (lambda/2) * sum(F_i * (theta_i - theta*_i)^2).
    pub fn penalty(&self, current_params: &[f32]) -> f32 {
        if !self.initialized {
            return 0.0;
        }
        let current = ArrayView1::from(current_params);
        let diff = &current - &self.reference_params;
        let weighted: f32 = (&self.fisher * &diff * &diff).sum();
        (self.lambda / 2.0) * weighted
    }

    /// Compute the gradient contribution: lambda * F_i * (theta_i - theta*_i).
    pub fn gradient_contribution(&self, current_params: &[f32]) -> Vec<f32> {
        if !self.initialized {
            return vec![0.0; current_params.len()];
        }
        let current = ArrayView1::from(current_params);
        let diff = &current - &self.reference_params;
        let grad = self.lambda * &self.fisher * &diff;
        grad.to_vec()
    }

    /// Online EWC++ update after a training step.
    ///
    /// Updates the Fisher diagonal and reference parameters:
    /// - F_new = alpha * F_old + (1-alpha) * F_batch
    /// - theta*_new = alpha * theta*_old + (1-alpha) * theta_current
    pub fn update(
        &mut self,
        current_params: &[f32],
        grad_a: &Array2<f32>,
        grad_b: &Array2<f32>,
    ) {
        // Compute batch Fisher approximation: F_batch = grad^2
        let mut batch_fisher = Vec::with_capacity(current_params.len());
        for v in grad_a.iter().chain(grad_b.iter()) {
            batch_fisher.push(v * v);
        }
        let batch_fisher = Array1::from(batch_fisher);
        let current = Array1::from(current_params.to_vec());

        if !self.initialized {
            self.fisher = batch_fisher;
            self.reference_params = current;
            self.initialized = true;
        } else {
            // Online EWC++ update
            self.fisher =
                self.alpha * &self.fisher + (1.0 - self.alpha) * &batch_fisher;
            self.reference_params =
                self.alpha * &self.reference_params + (1.0 - self.alpha) * &current;
        }
    }

    /// Whether the EWC state has been initialized (at least one update).
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Serialize to flat vectors (for persistence).
    pub fn to_vecs(&self) -> (Vec<f32>, Vec<f32>) {
        (self.fisher.to_vec(), self.reference_params.to_vec())
    }

    /// Restore from flat vectors.
    pub fn from_vecs(
        fisher: Vec<f32>,
        reference: Vec<f32>,
        alpha: f32,
        lambda: f32,
    ) -> Self {
        let initialized = fisher.iter().any(|v| *v != 0.0);
        Self {
            fisher: Array1::from(fisher),
            reference_params: Array1::from(reference),
            alpha,
            lambda,
            initialized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-REG-01: Construction and initial state
    #[test]
    fn construction_initial_state() {
        let ewc = EwcState::new(3072, 0.95, 0.5);
        assert_eq!(ewc.fisher.len(), 3072);
        assert_eq!(ewc.reference_params.len(), 3072);
        assert!(!ewc.initialized);

        let params = vec![1.0_f32; 3072];
        assert_eq!(ewc.penalty(&params), 0.0);

        let grad = ewc.gradient_contribution(&params);
        assert!(grad.iter().all(|v| *v == 0.0));
    }

    // T-REG-02: Penalty computation with known values
    #[test]
    fn penalty_known_values() {
        let ewc = EwcState {
            fisher: Array1::from(vec![1.0, 2.0, 3.0]),
            reference_params: Array1::from(vec![0.0, 0.0, 0.0]),
            alpha: 0.95,
            lambda: 0.5,
            initialized: true,
        };

        let params = vec![1.0, 1.0, 1.0];
        let penalty = ewc.penalty(&params);
        // (0.5/2) * (1*1 + 2*1 + 3*1) = 0.25 * 6 = 1.5
        assert!(
            (penalty - 1.5).abs() < 1e-6,
            "penalty should be 1.5, got {penalty}"
        );
    }

    // T-REG-03: Gradient contribution with known values
    #[test]
    fn gradient_contribution_known_values() {
        let ewc = EwcState {
            fisher: Array1::from(vec![1.0, 2.0, 3.0]),
            reference_params: Array1::from(vec![0.0, 0.0, 0.0]),
            alpha: 0.95,
            lambda: 0.5,
            initialized: true,
        };

        let params = vec![1.0, 1.0, 1.0];
        let grad = ewc.gradient_contribution(&params);
        // lambda * F * (theta - theta*) = 0.5 * [1,2,3] * [1,1,1] = [0.5, 1.0, 1.5]
        assert!((grad[0] - 0.5).abs() < 1e-6);
        assert!((grad[1] - 1.0).abs() < 1e-6);
        assert!((grad[2] - 1.5).abs() < 1e-6);
    }

    // T-REG-04: Long-sequence Fisher stability
    #[test]
    fn long_sequence_stability() {
        let mut ewc = EwcState::new(100, 0.95, 0.5);

        // Simulate 10,000 updates with random-ish batch Fisher values
        for i in 0..10_000 {
            let params: Vec<f32> = (0..100).map(|j| ((i + j) as f32 * 0.01).sin()).collect();
            // Fake grad matrices that produce batch fisher in [0, 1]
            let grad_a = Array2::from_shape_fn((10, 5), |(_r, _c)| {
                ((i as f32 * 0.001) + 0.1).sin().abs().sqrt()
            });
            let grad_b = Array2::from_shape_fn((5, 10), |(_r, _c)| {
                ((i as f32 * 0.002) + 0.2).cos().abs().sqrt()
            });
            ewc.update(&params, &grad_a, &grad_b);
        }

        // Verify no NaN or Inf
        assert!(!ewc.fisher.iter().any(|v| v.is_nan() || v.is_infinite()));
        assert!(!ewc.reference_params.iter().any(|v| v.is_nan() || v.is_infinite()));
        // Fisher values should be non-negative
        assert!(ewc.fisher.iter().all(|v| *v >= 0.0));
    }

    // T-REG-05: EWC regularization effectiveness
    #[test]
    fn regularization_effectiveness() {
        let mut ewc = EwcState::new(10, 0.95, 0.5);

        // Do some initial updates
        for i in 0..100 {
            let params: Vec<f32> = (0..10).map(|j| (i + j) as f32 * 0.01).collect();
            let grad_a = Array2::from_elem((2, 2), 0.5_f32);
            let grad_b = Array2::from_elem((2, 3), 0.3_f32);
            ewc.update(&params, &grad_a, &grad_b);
        }

        let ref_params: Vec<f32> = ewc.reference_params.to_vec();

        // Small perturbation
        let small_perturb: Vec<f32> = ref_params.iter().map(|v| v + 0.01).collect();
        let small_penalty = ewc.penalty(&small_perturb);

        // Large perturbation
        let large_perturb: Vec<f32> = ref_params.iter().map(|v| v + 1.0).collect();
        let large_penalty = ewc.penalty(&large_perturb);

        assert!(
            large_penalty > small_penalty,
            "large perturbation should have higher penalty: small={small_penalty}, large={large_penalty}"
        );
    }

    // T-REG-06: First update initializes state
    #[test]
    fn first_update_initializes() {
        let mut ewc = EwcState::new(4, 0.95, 0.5);
        assert!(!ewc.initialized);

        let params = vec![1.0, 2.0, 3.0, 4.0];
        let grad_a = Array2::from_elem((1, 2), 0.5_f32);
        let grad_b = Array2::from_elem((1, 2), 0.3_f32);
        ewc.update(&params, &grad_a, &grad_b);

        assert!(ewc.initialized);
        // First update: fisher = batch_fisher directly (no alpha blending)
        // batch_fisher = [0.25, 0.25, 0.09, 0.09]
        assert!((ewc.fisher[0] - 0.25).abs() < 1e-6);
        assert!((ewc.fisher[2] - 0.09).abs() < 1e-6);
    }

    // T-REG-07: EWC++ update formula correctness
    #[test]
    fn ewc_update_formula() {
        let mut ewc = EwcState::new(4, 0.95, 0.5);

        // First update: grad produces batch_fisher = [1, 0, 0, 0]
        let grad_a1 = Array2::from_shape_vec((1, 2), vec![1.0, 0.0]).unwrap();
        let grad_b1 = Array2::from_shape_vec((1, 2), vec![0.0, 0.0]).unwrap();
        ewc.update(&[1.0, 0.0, 0.0, 0.0], &grad_a1, &grad_b1);

        assert!((ewc.fisher[0] - 1.0).abs() < 1e-6);
        assert!((ewc.fisher[1] - 0.0).abs() < 1e-6);

        // Second update: batch_fisher = [0, 0, 1, 0]
        let grad_a2 = Array2::from_shape_vec((1, 2), vec![0.0, 0.0]).unwrap();
        let grad_b2 = Array2::from_shape_vec((1, 2), vec![1.0, 0.0]).unwrap();
        ewc.update(&[0.0, 0.0, 1.0, 0.0], &grad_a2, &grad_b2);

        // Fisher: 0.95 * [1,0,0,0] + 0.05 * [0,0,1,0] = [0.95, 0, 0.05, 0]
        assert!((ewc.fisher[0] - 0.95).abs() < 1e-6, "fisher[0]={}", ewc.fisher[0]);
        assert!((ewc.fisher[2] - 0.05).abs() < 1e-6, "fisher[2]={}", ewc.fisher[2]);
    }

    // T-REG-08: Serialization round-trip
    #[test]
    fn serialization_roundtrip() {
        let mut ewc = EwcState::new(10, 0.95, 0.5);
        // Do some updates
        for i in 0..5 {
            let params: Vec<f32> = (0..10).map(|j| (i + j) as f32 * 0.1).collect();
            let grad_a = Array2::from_elem((2, 2), 0.1_f32);
            let grad_b = Array2::from_elem((2, 3), 0.2_f32);
            ewc.update(&params, &grad_a, &grad_b);
        }

        let test_params = vec![0.5_f32; 10];
        let penalty_before = ewc.penalty(&test_params);

        let (fisher, reference) = ewc.to_vecs();
        let restored = EwcState::from_vecs(fisher, reference, 0.95, 0.5);
        let penalty_after = restored.penalty(&test_params);

        assert!(
            (penalty_before - penalty_after).abs() < 1e-6,
            "penalty mismatch: {penalty_before} vs {penalty_after}"
        );
    }

    // T-REG-09: Zero gradient produces zero Fisher contribution
    #[test]
    fn zero_gradient_update() {
        let mut ewc = EwcState::new(4, 0.95, 0.5);
        // Initialize with non-zero fisher
        let grad_a = Array2::from_elem((1, 2), 1.0_f32);
        let grad_b = Array2::from_elem((1, 2), 1.0_f32);
        ewc.update(&[1.0; 4], &grad_a, &grad_b);

        let fisher_before: Vec<f32> = ewc.fisher.to_vec();

        // Update with zero gradients
        let zero_a = Array2::zeros((1, 2));
        let zero_b = Array2::zeros((1, 2));
        ewc.update(&[1.0; 4], &zero_a, &zero_b);

        // Fisher should decrease by factor alpha (blending with zero)
        for (before, after) in fisher_before.iter().zip(ewc.fisher.iter()) {
            let expected = 0.95 * before; // alpha * old + (1-alpha) * 0
            assert!(
                (after - expected).abs() < 1e-6,
                "fisher decay: expected {expected}, got {after}"
            );
        }
    }
}
