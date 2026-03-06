# Pseudocode: learn-crate (Wave 1 — Shared Training Infrastructure)

## Pattern: Crate extraction with re-export bridge

Follows existing workspace pattern: new crate under `crates/`, workspace member
via `crates/*` glob. Existing adapt code refactored to depend on learn crate.

## Files

### crates/unimatrix-learn/Cargo.toml

```toml
[package]
name = "unimatrix-learn"
# workspace edition, rust-version, license

[dependencies]
ndarray = "0.16"
rand = "0.9"
serde = { workspace = true }
serde_json = { workspace = true }
bincode = { workspace = true }
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

### crates/unimatrix-learn/src/lib.rs

```
pub mod reservoir;
pub mod ewc;
pub mod persistence;
pub mod registry;
pub mod config;
pub mod models;

pub use reservoir::TrainingReservoir;
pub use ewc::EwcState;
pub use persistence::{save_atomic, load_file};
pub use registry::{ModelRegistry, ModelVersion, ModelSlot, RegistryError};
pub use config::LearnConfig;
```

### crates/unimatrix-learn/src/reservoir.rs

Extracted from `unimatrix-adapt/src/training.rs::TrainingReservoir`.
Generalized from `TrainingPair` to generic `T: Clone`.

```pseudo
struct TrainingReservoir<T: Clone> {
    items: Vec<T>,
    capacity: usize,
    total_seen: u64,
    rng: StdRng,
}

impl<T: Clone> TrainingReservoir<T> {
    fn new(capacity, seed) -> Self
        // Same as current TrainingReservoir::new, replacing pairs->items

    fn add(&mut self, items: &[T])
        // Same reservoir sampling algorithm as current add()
        // For each item in items:
        //   total_seen += 1
        //   if items.len() < capacity: push
        //   else: j = rng.random_range(0..total_seen); if j < capacity: replace

    fn sample_batch(&mut self, batch_size) -> Vec<&T>
        // Same as current sample_batch, replacing TrainingPair with T

    fn len(&self) -> usize
    fn total_seen(&self) -> u64
}
```

Key change: `add(&mut self, items: &[T])` accepts `&[T]` directly, not `&[(u64, u64, u32)]`.

### crates/unimatrix-learn/src/ewc.rs

Extracted from `unimatrix-adapt/src/regularization.rs::EwcState`.
Added `update_from_flat` method for generic model parameter interface.

```pseudo
struct EwcState {
    fisher: Array1<f32>,
    reference_params: Array1<f32>,
    alpha: f32,
    lambda: f32,
    initialized: bool,
}

impl EwcState {
    fn new(param_count, alpha, lambda) -> Self
        // Identical to current

    fn penalty(&self, current_params: &[f32]) -> f32
        // Identical to current

    fn gradient_contribution(&self, current_params: &[f32]) -> Vec<f32>
        // Identical to current

    fn update(&mut self, current_params, grad_a, grad_b)
        // Identical to current (MicroLoRA backward compat)

    fn update_from_flat(&mut self, params: &[f32], grad_squared: &[f32])
        // NEW: ADR-002 flat parameter interface for NeuralModel
        // batch_fisher = Array1::from(grad_squared.to_vec())
        // if !initialized: fisher = batch_fisher, reference = params, initialized = true
        // else: fisher = alpha * fisher + (1-alpha) * batch_fisher
        //        reference = alpha * reference + (1-alpha) * params

    fn is_initialized(&self) -> bool
    fn to_vecs(&self) -> (Vec<f32>, Vec<f32>)
    fn from_vecs(fisher, reference, alpha, lambda) -> Self
        // All identical to current
}
```

### crates/unimatrix-learn/src/persistence.rs

Extracted atomic save/load from `unimatrix-adapt/src/persistence.rs`.
Only the generic file I/O, NOT `AdaptationState`.

```pseudo
fn save_atomic(data: &[u8], dir: &Path, filename: &str) -> Result<(), String>
    // tmp_path = dir.join(format!("{filename}.tmp"))
    // target = dir.join(filename)
    // fs::create_dir_all(dir)
    // fs::write(tmp_path, data)
    // fs::rename(tmp_path, target)

fn load_file(dir: &Path, filename: &str) -> Result<Option<Vec<u8>>, String>
    // path = dir.join(filename)
    // if !path.exists(): return Ok(None)
    // fs::read(path) -> Ok(bytes) or warn + return Ok(None)
    // if bytes.is_empty(): return Ok(None)
    // return Ok(Some(bytes))
```

### crates/unimatrix-learn/src/config.rs

```pseudo
struct LearnConfig {
    pub models_dir: PathBuf,        // default: ~/.unimatrix/{hash}/models/
    pub shadow_min_evaluations: u32, // default: 20
    pub rollback_threshold: f64,    // default: 0.05 (5% accuracy drop)
    pub rollback_window: usize,     // default: 50 predictions
}

impl Default for LearnConfig {
    // reasonable defaults
}
```

## Adapt Refactoring

### crates/unimatrix-adapt/Cargo.toml

Add dependency: `unimatrix-learn = { path = "../unimatrix-learn" }`

### crates/unimatrix-adapt/src/training.rs

```pseudo
// TrainingPair stays here (MicroLoRA-specific)
// TrainingReservoir removed — now imported from learn

use unimatrix_learn::TrainingReservoir;

// Existing add() call sites change from:
//   reservoir.add(&[(id_a, id_b, count)])
// to:
//   reservoir.add(&[TrainingPair { entry_id_a: id_a, entry_id_b: id_b, count }])

// execute_training_step: change ewc.update() call to match learn's EwcState
// (same signature, so no change needed — update() still takes grad_a, grad_b)
```

### crates/unimatrix-adapt/src/regularization.rs

```pseudo
// Remove entire EwcState impl.
// Replace with re-export:
pub use unimatrix_learn::EwcState;
```

### crates/unimatrix-adapt/src/persistence.rs

```pseudo
// save_state() uses learn::save_atomic internally:
//   let bytes = bincode::encode(state);
//   save_atomic(&bytes, dir, STATE_FILENAME)?;
//
// load_state() uses learn::load_file internally:
//   let bytes = load_file(dir, STATE_FILENAME)?;
//   if let Some(bytes) = bytes { deserialize... } else { Ok(None) }
//
// AdaptationState, snapshot_state, restore_state stay here (MicroLoRA-specific)
```

## Validation Gate

- `cargo test -p unimatrix-adapt` -- all 174+ tests pass
- `cargo build --workspace` -- no compilation errors
- No public API changes visible to unimatrix-adapt consumers
