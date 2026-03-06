//! Neural model definitions for extraction pipeline.

pub mod classifier;
pub mod digest;
pub mod scorer;
pub mod traits;

pub use classifier::{ClassificationResult, SignalCategory, SignalClassifier};
pub use digest::SignalDigest;
pub use scorer::ConventionScorer;
pub use traits::NeuralModel;
