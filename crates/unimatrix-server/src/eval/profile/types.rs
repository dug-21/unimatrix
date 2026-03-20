//! `AnalyticsMode` and `EvalProfile` type definitions (nan-007).

use crate::infra::config::UnimatrixConfig;

// ---------------------------------------------------------------------------
// AnalyticsMode
// ---------------------------------------------------------------------------

/// Controls whether the analytics write queue is active in a ServiceLayer.
///
/// `Suppressed` is the only mode used in nan-007. `Live` exists for future
/// use in a hypothetical `eval live` command where analytics recording is
/// acceptable (ADR-002, SR-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsMode {
    /// Normal SqlxStore behaviour — drain task active, analytics writes occur.
    /// Reserved for future `eval live` mode. NOT used in nan-007.
    Live,
    /// No drain task spawned; `enqueue_analytics` calls are no-ops.
    /// Always used in `EvalServiceLayer` construction (ADR-002, SR-07).
    Suppressed,
}

// ---------------------------------------------------------------------------
// EvalProfile
// ---------------------------------------------------------------------------

/// A named eval profile parsed from a TOML file.
///
/// An empty TOML body (with only `[profile]` name/description) represents
/// the baseline profile and uses all compiled defaults from `UnimatrixConfig`.
///
/// Profile TOML format:
/// ```toml
/// [profile]
/// name = "candidate-weights-v1"
/// description = "Test higher base weight"   # optional
///
/// [confidence.weights]
/// # All six weight fields required if [confidence.weights] present (C-06).
/// # Fields match ConfidenceWeights struct: base, usage, fresh, help, corr, trust
/// base  = 0.20
/// usage = 0.15
/// fresh = 0.17
/// help  = 0.15
/// corr  = 0.15
/// trust = 0.10
/// # sum must be 0.92 ± 1e-9
///
/// [inference]
/// # Optional; rayon_pool_size validated at from_profile() time (C-14).
/// rayon_pool_size = 1
/// ```
#[derive(Debug, Clone)]
pub struct EvalProfile {
    /// Profile identifier. Must be unique across all profiles in a single
    /// `eval run` invocation (checked by run_eval, not by from_profile).
    pub name: String,
    /// Optional human-readable description of what this profile tests.
    pub description: Option<String>,
    /// Config overrides. Absent sections use compiled defaults.
    /// An empty `UnimatrixConfig` → all compiled defaults → baseline profile.
    pub config_overrides: UnimatrixConfig,
}
