//! Eval profile types and `EvalServiceLayer` construction (nan-007).
//!
//! Defines the type-level foundation for the eval engine:
//! - `AnalyticsMode` — structural guarantee that eval never writes analytics (ADR-002)
//! - `EvalProfile` — parsed profile TOML with config overrides
//! - `EvalServiceLayer` — restricted ServiceLayer variant for eval replay
//! - `EvalError` — all structured errors for the eval subsystem
//!
//! `EvalServiceLayer::from_profile()` is the single construction gateway.
//! All invariant validation (live-DB guard, model paths, weight sum) occurs
//! here. If construction returns `Ok`, the layer is safe to use for replay.

pub(crate) mod error;
pub(crate) mod layer;
pub(crate) mod types;
pub(crate) mod validation;

#[cfg(test)]
mod layer_graph_tests;
#[cfg(test)]
mod layer_tests;
#[cfg(test)]
mod tests;

// Public re-exports consumed by eval/mod.rs and runner.rs.
pub use error::EvalError;
pub use layer::EvalServiceLayer;
pub use types::{AnalyticsMode, DistributionTargets, EvalProfile};
pub(crate) use validation::parse_profile_toml;
