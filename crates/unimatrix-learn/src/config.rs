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
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            shadow_min_evaluations: 20,
            rollback_threshold: 0.05,
            rollback_window: 50,
        }
    }
}
