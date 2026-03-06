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

    /// Online EWC++ update after a training step using gradient matrices.
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
            self.fisher = self.alpha * &self.fisher + (1.0 - self.alpha) * &batch_fisher;
            self.reference_params =
                self.alpha * &self.reference_params + (1.0 - self.alpha) * &current;
        }
    }

    /// Online EWC++ update using flat parameter and gradient-squared vectors.
    ///
    /// This is the generic interface for NeuralModel implementations (ADR-002).
    pub fn update_from_flat(&mut self, params: &[f32], grad_squared: &[f32]) {
        let batch_fisher = Array1::from(grad_squared.to_vec());
        let current = Array1::from(params.to_vec());

        if !self.initialized {
            self.fisher = batch_fisher;
            self.reference_params = current;
            self.initialized = true;
        } else {
            self.fisher = self.alpha * &self.fisher + (1.0 - self.alpha) * &batch_fisher;
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
    pub fn from_vecs(fisher: Vec<f32>, reference: Vec<f32>, alpha: f32, lambda: f32) -> Self {
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

    // T-LC-04: EwcState update_from_flat known values
    #[test]
    fn update_from_flat_known_values() {
        let mut ewc = EwcState::new(4, 0.95, 0.5);

        // First update: fisher = batch_fisher directly
        ewc.update_from_flat(&[1.0, 0.0, 0.0, 0.0], &[1.0, 0.0, 0.0, 0.0]);
        assert!((ewc.fisher[0] - 1.0).abs() < 1e-6);
        assert!((ewc.fisher[1] - 0.0).abs() < 1e-6);
        assert!(ewc.is_initialized());

        // Second update: alpha-blended
        ewc.update_from_flat(&[0.0, 0.0, 1.0, 0.0], &[0.0, 0.0, 1.0, 0.0]);
        // fisher = 0.95 * [1,0,0,0] + 0.05 * [0,0,1,0] = [0.95, 0, 0.05, 0]
        assert!(
            (ewc.fisher[0] - 0.95).abs() < 1e-6,
            "fisher[0]={}",
            ewc.fisher[0]
        );
        assert!(
            (ewc.fisher[2] - 0.05).abs() < 1e-6,
            "fisher[2]={}",
            ewc.fisher[2]
        );
    }

    // T-LC-05: EwcState update_from_flat penalty matches update()
    #[test]
    fn update_from_flat_matches_update() {
        let mut ewc1 = EwcState::new(4, 0.95, 0.5);
        let mut ewc2 = EwcState::new(4, 0.95, 0.5);

        let params = vec![1.0, 2.0, 3.0, 4.0];
        let grad_a = Array2::from_shape_vec((1, 2), vec![0.5, 0.3]).expect("shape");
        let grad_b = Array2::from_shape_vec((1, 2), vec![0.2, 0.4]).expect("shape");

        // Compute grad_squared as update() would
        let grad_squared: Vec<f32> = grad_a
            .iter()
            .chain(grad_b.iter())
            .map(|v| v * v)
            .collect();

        ewc1.update(&params, &grad_a, &grad_b);
        ewc2.update_from_flat(&params, &grad_squared);

        let test_params = vec![0.5, 1.0, 1.5, 2.0];
        let p1 = ewc1.penalty(&test_params);
        let p2 = ewc2.penalty(&test_params);
        assert!(
            (p1 - p2).abs() < 1e-6,
            "penalty mismatch: {p1} vs {p2}"
        );
    }

    #[test]
    fn construction_initial_state() {
        let ewc = EwcState::new(100, 0.95, 0.5);
        assert!(!ewc.is_initialized());
        assert_eq!(ewc.penalty(&vec![1.0; 100]), 0.0);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut ewc = EwcState::new(10, 0.95, 0.5);
        let grad_a = Array2::from_elem((2, 2), 0.1_f32);
        let grad_b = Array2::from_elem((2, 3), 0.2_f32);
        for i in 0..5 {
            let params: Vec<f32> = (0..10).map(|j| (i + j) as f32 * 0.1).collect();
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
}
