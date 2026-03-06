//! Shared ML infrastructure and neural models for Unimatrix.
//!
//! Provides:
//! - `TrainingReservoir<T>`: Generic reservoir sampling buffer
//! - `EwcState`: EWC++ online regularization
//! - `save_atomic`/`load_file`: Atomic persistence helpers
//! - `ModelRegistry`: Three-slot model versioning
//! - Neural models: `SignalClassifier`, `ConventionScorer`

pub mod config;
pub mod ewc;
pub mod models;
pub mod persistence;
pub mod registry;
pub mod reservoir;

pub use config::LearnConfig;
pub use ewc::EwcState;
pub use persistence::{load_file, save_atomic};
pub use registry::{ModelRegistry, ModelSlot, ModelVersion, RegistryError};
pub use reservoir::TrainingReservoir;
