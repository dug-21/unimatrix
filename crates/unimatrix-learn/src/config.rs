//! Shared ML configuration for neural models.

use std::path::PathBuf;

/// Configuration for the learning pipeline.
#[derive(Debug, Clone)]
pub struct LearnConfig {
    /// Directory for model storage.
    pub models_dir: PathBuf,
    /// Minimum shadow evaluations before promotion is considered.
    pub shadow_min_evaluations: u32,
    /// Rolling accuracy drop threshold for auto-rollback (0.05 = 5%).
    pub rollback_threshold: f64,
    /// Number of predictions in rolling accuracy window.
    pub rollback_window: usize,

    // -- crt-008: Continuous Self-Retraining fields --
    /// Signal count threshold to trigger classifier retraining.
    pub classifier_retrain_threshold: u64,
    /// Batch size for classifier training step.
    pub classifier_batch_size: usize,
    /// Signal count threshold to trigger scorer retraining.
    pub scorer_retrain_threshold: u64,
    /// Batch size for scorer training step.
    pub scorer_batch_size: usize,
    /// EWC++ exponential decay factor for Fisher updates.
    pub ewc_alpha: f32,
    /// EWC++ penalty weight (lambda).
    pub ewc_lambda: f32,
    /// Per-class accuracy regression threshold for promotion rejection.
    pub per_class_regression_threshold: f64,
    /// Weight for weak labels (feature outcomes, stale entries).
    pub weak_label_weight: f32,
    /// Learning rate for training steps.
    pub training_lr: f32,
    /// Reservoir capacity per model.
    pub reservoir_capacity: usize,
    /// Reservoir RNG seed (scorer uses seed + 1).
    pub reservoir_seed: u64,
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            shadow_min_evaluations: 20,
            rollback_threshold: 0.05,
            rollback_window: 50,
            classifier_retrain_threshold: 20,
            classifier_batch_size: 16,
            scorer_retrain_threshold: 5,
            scorer_batch_size: 4,
            ewc_alpha: 0.95,
            ewc_lambda: 0.5,
            per_class_regression_threshold: 0.10,
            weak_label_weight: 0.3,
            training_lr: 0.01,
            reservoir_capacity: 500,
            reservoir_seed: 42,
        }
    }
}
