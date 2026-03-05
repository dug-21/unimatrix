# Pseudocode: learn-crate (Wave 1)

## Cargo.toml

```toml
[package]
name = "unimatrix-learn"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
ndarray = "0.16"
rand = "0.9"
serde = { workspace = true }
serde_json = "1"
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

## lib.rs

```rust
#![forbid(unsafe_code)]

pub mod config;
pub mod reservoir;
pub mod ewc;
pub mod persistence;
pub mod model;
pub mod digest;
pub mod classifier;
pub mod scorer;
pub mod registry;
pub mod shadow;

pub use config::NeuralConfig;
pub use digest::SignalDigest;
pub use model::NeuralModel;
pub use registry::ModelRegistry;
```

## reservoir.rs

Extract TrainingReservoir from adapt/training.rs and genericize.

```
// Generic reservoir sampling buffer
struct TrainingReservoir<T: Clone> {
    items: Vec<T>,
    capacity: usize,
    total_seen: u64,
    rng: StdRng,
}

impl<T: Clone> TrainingReservoir<T> {
    fn new(capacity, seed) -> Self
    fn add(&mut self, items: &[T])
        // For each item:
        //   total_seen += 1
        //   if items.len() < capacity: push
        //   else: reservoir sampling with probability capacity/total_seen
    fn sample_batch(&mut self, batch_size) -> Vec<&T>
        // Random sampling with replacement, capped at items.len()
    fn len() -> usize
    fn is_empty() -> bool
    fn clear(&mut self)
    fn total_seen() -> u64
}
```

Key change from adapt: `TrainingPair` -> generic `T: Clone`. The `add` method
takes `&[T]` instead of `&[(u64, u64, u32)]`.

## ewc.rs

Extract EwcState from adapt/regularization.rs. Generalize `update` to take
flat gradient vectors.

```
struct EwcState {
    fisher: Array1<f32>,
    reference_params: Array1<f32>,
    alpha: f32,
    lambda: f32,
    initialized: bool,
}

impl EwcState {
    fn new(param_count, alpha, lambda) -> Self
    fn penalty(&self, current_params: &[f32]) -> f32
        // Same formula: (lambda/2) * sum(F_i * (theta_i - theta*_i)^2)
    fn gradient_contribution(&self, current_params: &[f32]) -> Vec<f32>
        // Same formula: lambda * F_i * (theta_i - theta*_i)
    fn update(&mut self, current_params: &[f32], batch_gradients: &[f32])
        // CHANGED: takes flat &[f32] instead of (Array2, Array2)
        // batch_fisher = batch_gradients.iter().map(|g| g*g)
        // if !initialized: fisher = batch_fisher, ref = current, initialized = true
        // else: fisher = alpha*fisher + (1-alpha)*batch_fisher
        //        ref   = alpha*ref + (1-alpha)*current
    fn is_initialized() -> bool
    fn to_vecs() -> (Vec<f32>, Vec<f32>)
    fn from_vecs(fisher, reference, alpha, lambda) -> Self
}
```

Key change: `update` now takes `&[f32]` gradient vector. The
adapt crate's `update(&[f32], &Array2, &Array2)` becomes a thin wrapper
that flattens the two Array2 arguments before calling the learn version.

## persistence.rs

Extract atomic save/load helpers.

```
fn save_atomic(data: &[u8], path: &Path) -> Result<(), String>
    // Write to {path}.tmp, then fs::rename to path
    // Create parent directories if needed

fn load_bytes(path: &Path) -> Result<Option<Vec<u8>>, String>
    // If path doesn't exist: return Ok(None)
    // Read bytes, if empty: return Ok(None)
    // Return Ok(Some(bytes))
```

## config.rs

```
struct NeuralConfig {
    models_dir: PathBuf,                  // default: {data_dir}/models/
    classifier_topology: Vec<u32>,        // default: [32, 64, 32, 5]
    scorer_topology: Vec<u32>,            // default: [32, 32, 1]
    classifier_noise_bias: f32,           // default: 2.0
    scorer_output_bias: f32,              // default: -1.0
    shadow_min_evaluations: u64,          // default: 20
    shadow_promotion_threshold: f64,      // default: 0.0
    rollback_accuracy_drop: f64,          // default: 0.05
    rolling_window_size: usize,           // default: 100
    observation_feature_threshold: u64,   // default: 5
    neural_override_confidence: f32,      // default: 0.8
}

impl Default for NeuralConfig { ... }
```

## Adapt Refactoring

### adapt/training.rs changes

```
// Keep TrainingPair where it is (it's adapt-specific)
// Replace TrainingReservoir with:
pub use unimatrix_learn::reservoir::TrainingReservoir;

// The existing add() calls use &[(u64, u64, u32)] tuples.
// Convert: create a wrapper type or helper that converts tuples to TrainingPair
// and calls reservoir.add(&pairs).
//
// Actually: keep the existing TrainingReservoir in adapt as a thin wrapper
// that delegates to learn::TrainingReservoir<TrainingPair>, adapting the
// &[(u64, u64, u32)] interface:

pub struct TrainingReservoir {
    inner: unimatrix_learn::reservoir::TrainingReservoir<TrainingPair>,
}

impl TrainingReservoir {
    pub fn new(capacity, seed) -> Self
    pub fn add(&mut self, pairs: &[(u64, u64, u32)])
        // Convert tuples to TrainingPair, call inner.add
    pub fn sample_batch(&mut self, batch_size) -> Vec<&TrainingPair>
        // Delegate to inner
    pub fn len() -> usize
    pub fn total_seen() -> u64
}
```

### adapt/regularization.rs changes

```
// Keep EwcState but delegate to learn::ewc::EwcState internally.
// The adapt version needs to maintain the (Array2, Array2) update interface
// for backward compatibility with execute_training_step.

pub use unimatrix_learn::ewc::EwcState;

// No wrapper needed IF we can change execute_training_step to flatten
// the gradients before calling ewc.update. This is internal to adapt,
// so it's safe to change.

// In execute_training_step: replace
//   ewc.update(&params, &grad_a, &grad_b)
// with:
//   let flat_grads: Vec<f32> = grad_a.iter().chain(grad_b.iter()).map(|v| v*v... )
//   Actually: the new update takes raw gradients, not squared.
//   The squaring happens inside learn::ewc::EwcState::update.
//   So flatten: grad_a.iter().chain(grad_b.iter()).cloned().collect()
//   Then call ewc.update(&params, &flat_grads)
```

### adapt/persistence.rs changes

```
// Use learn::persistence::{save_atomic, load_bytes} for the low-level I/O.
// Keep save_state/load_state/snapshot_state/restore_state as-is since they
// are adapt-specific (AdaptationState serialization).
// Replace the inline tmp+rename logic in save_state with save_atomic.
```
