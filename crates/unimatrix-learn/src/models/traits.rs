//! NeuralModel trait: lifecycle abstraction for all neural models.

/// Trait abstracting neural model lifecycle.
///
/// All models implement forward pass, training step, parameter access,
/// and serialization. Designed for future burn/candle implementations
/// behind feature gates.
///
/// # Parameter Ordering Contract (ADR-002)
///
/// All parameter-level operations use a canonical flat ordering:
/// layer-by-layer, weights then biases, row-major. This ordering is
/// shared by `flat_parameters()`, `set_parameters()`, `compute_gradients()`,
/// and `apply_gradients()`. EWC correctness depends on this alignment.
pub trait NeuralModel: Send + Sync {
    /// Forward pass: input slice -> output vec.
    fn forward(&self, input: &[f32]) -> Vec<f32>;

    /// Compute loss and flat gradient vector without updating weights.
    ///
    /// Returns `(loss, gradient_vector)` where the gradient vector uses
    /// the same ordering as `flat_parameters()`.
    fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>);

    /// Apply a flat gradient vector as SGD weight update.
    ///
    /// `gradients` must use the same ordering as `flat_parameters()`.
    fn apply_gradients(&mut self, gradients: &[f32], lr: f32);

    /// Single training step. Returns loss value.
    ///
    /// Default implementation calls `compute_gradients` then `apply_gradients`.
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
        let (loss, grads) = self.compute_gradients(input, target);
        self.apply_gradients(&grads, lr);
        loss
    }

    /// Flatten all model parameters into a single Vec.
    /// Order: layer-by-layer, weights then biases.
    fn flat_parameters(&self) -> Vec<f32>;

    /// Set all parameters from a flat Vec.
    /// Must match the order of `flat_parameters()`.
    fn set_parameters(&mut self, params: &[f32]);

    /// Serialize model to bytes.
    fn serialize(&self) -> Vec<u8>;

    /// Deserialize model from bytes.
    fn deserialize(data: &[u8]) -> Result<Self, String>
    where
        Self: Sized;
}
