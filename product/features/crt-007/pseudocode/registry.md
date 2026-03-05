# Pseudocode: registry (Wave 4)

## registry.rs

```rust
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

/// Model versioning registry with production/shadow/previous slots (ADR-005).
pub struct ModelRegistry {
    models: HashMap<String, ModelSlot>,
    models_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub struct ModelSlot {
    pub name: String,
    pub production: Option<LoadedModel>,
    pub shadow: Option<LoadedModel>,
    pub previous_path: Option<PathBuf>,
    pub metrics: RollingMetrics,
    pub schema_version: u32,
    pub features_observed: u64,
    pub state: ModelState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LoadedModel {
    pub version: u32,
    pub path: PathBuf,
    pub accuracy: f64,
    pub evaluation_count: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelState {
    Observation,
    Shadow,
    Production,
    RolledBack,
}

#[derive(Serialize, Deserialize)]
pub struct RollingMetrics {
    window: VecDeque<(bool, f64)>,  // (correct, confidence)
    capacity: usize,
}

impl RollingMetrics {
    fn new(capacity: usize) -> Self
    fn record(&mut self, correct: bool, confidence: f64)
        // push_back, pop_front if at capacity
    fn accuracy(&self) -> f64
        // proportion of correct=true in window
        // return 0.0 if window is empty
    fn mean_confidence(&self) -> f64
    fn count(&self) -> usize
}

impl ModelRegistry {
    pub fn new(models_dir: PathBuf) -> Self {
        // Create models_dir if doesn't exist
        // Initialize empty HashMap
    }

    pub fn register(&mut self, name: &str, version: u32, path: PathBuf, config: &NeuralConfig) -> Result<(), String> {
        // Create ModelSlot if not exists
        // Set production = Some(LoadedModel { version, path, accuracy: 0.0, ... })
        // Set state = Observation
    }

    pub fn get_production(&self, name: &str) -> Option<&LoadedModel>

    pub fn get_shadow(&self, name: &str) -> Option<&LoadedModel>

    pub fn state(&self, name: &str) -> ModelState {
        // Return slot.state, default Observation if not found
    }

    pub fn promote_shadow(&mut self, name: &str) -> Result<(), String> {
        // Precondition: shadow exists
        // current production -> previous (delete old previous file)
        // shadow -> production
        // shadow = None
        // state = Production
        // Save registry
    }

    pub fn rollback(&mut self, name: &str) -> Result<(), String> {
        // Precondition: previous_path exists
        // production -> (delete file)
        // previous -> production
        // previous_path = None
        // shadow = None
        // state = RolledBack
        // Save registry
    }

    pub fn check_promotion(&self, name: &str, shadow_accuracy: f64, shadow_eval_count: u64) -> bool {
        // shadow_eval_count >= config.shadow_min_evaluations (20)
        // shadow_accuracy >= production accuracy (or rule baseline for first promotion)
        // No per-category regression (caller must verify via ShadowEvaluator)
    }

    pub fn check_rollback(&self, name: &str) -> bool {
        // rolling accuracy dropped > rollback_accuracy_drop (5%) below promotion accuracy
        // OR NaN/Inf in params
    }

    pub fn update_features_observed(&mut self, name: &str, count: u64) {
        // Set features_observed
        // Transition state:
        //   if features_observed >= observation_feature_threshold && state == Observation:
        //     state = Shadow
    }

    pub fn record_evaluation(&mut self, name: &str, correct: bool, confidence: f64) {
        // slot.metrics.record(correct, confidence)
    }

    pub fn save_registry(&self) -> Result<(), String> {
        // Serialize to JSON: models_dir / "registry.json"
        // Use save_atomic from persistence module
        // JSON for debuggability (ADR-005)
    }

    pub fn load_registry(models_dir: &Path) -> Result<Self, String> {
        // Read models_dir / "registry.json"
        // If missing or corrupt: return fresh empty registry
        // Deserialize HashMap<String, ModelSlot>
    }

    fn cleanup_old_versions(&self, name: &str) -> Result<(), String> {
        // After promotion: delete all version files except
        // production, shadow, and previous
        // Retention policy: max 3 files per model (ADR-005)
    }
}
```

## Persistence Format (registry.json)

```json
{
  "signal_classifier": {
    "name": "signal_classifier",
    "production": {
      "version": 1,
      "path": "models/signal_classifier/v1.bin",
      "accuracy": 0.0,
      "evaluation_count": 0,
      "created_at": 1709654400000
    },
    "shadow": null,
    "previous_path": null,
    "metrics": { "window": [], "capacity": 100 },
    "schema_version": 1,
    "features_observed": 0,
    "state": "Observation"
  },
  "convention_scorer": { ... }
}
```

## Error Handling

- Missing models_dir: create it
- Corrupt registry.json: log warning, return fresh registry with baseline models
- Missing model file: fall back to baseline weights (ADR-006)
- Promotion with no shadow: return Err
- Rollback with no previous: return Err
