//! NeuralModel trait: lifecycle abstraction for all neural models.

/// Trait abstracting neural model lifecycle.
///
/// All models implement forward pass, training step, parameter access,
/// and serialization. Designed for future burn/candle implementations
/// behind feature gates.
pub trait NeuralModel: Send + Sync {
    /// Forward pass: input slice -> output vec.
    fn forward(&self, input: &[f32]) -> Vec<f32>;

    /// Single training step. Returns loss value.
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32;

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
