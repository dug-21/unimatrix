//! Profile validation helpers: weight sum check and TOML parsing (nan-007).

use std::path::Path;

use crate::infra::config::UnimatrixConfig;

use super::error::EvalError;
use super::types::EvalProfile;

/// Expected sum of the six confidence weight fields (ADR-005 invariant).
pub(super) const EXPECTED_WEIGHT_SUM: f64 = 0.92;

/// Floating-point tolerance for weight sum validation (C-06, C-15).
pub(super) const WEIGHT_SUM_TOLERANCE: f64 = 1e-9;

/// Validate the `ConfidenceWeights` sum invariant.
///
/// Only validates if a `[confidence]` section with weights is present in the
/// profile. An empty `UnimatrixConfig` (baseline profile) is always valid.
///
/// The six fields (`base`, `usage`, `fresh`, `help`, `corr`, `trust`) must
/// sum to `0.92 ± 1e-9`. Returns `EvalError::ConfigInvariant` with a
/// user-readable message on failure (C-06, C-15, SR-08).
pub(crate) fn validate_confidence_weights(config: &UnimatrixConfig) -> Result<(), EvalError> {
    let weights = match &config.confidence.weights {
        Some(w) => w,
        // No [confidence] section → baseline profile → always valid.
        None => return Ok(()),
    };

    let sum =
        weights.base + weights.usage + weights.fresh + weights.help + weights.corr + weights.trust;

    if (sum - EXPECTED_WEIGHT_SUM).abs() > WEIGHT_SUM_TOLERANCE {
        return Err(EvalError::ConfigInvariant(format!(
            "confidence weights sum to {sum:.10}, expected {EXPECTED_WEIGHT_SUM:.2} ± 1e-9\n\
             fields: base={}, usage={}, fresh={}, help={}, corr={}, trust={}",
            weights.base, weights.usage, weights.fresh, weights.help, weights.corr, weights.trust,
        )));
    }

    Ok(())
}

/// Parse a profile TOML file into an `EvalProfile`.
///
/// The TOML must contain a `[profile]` section with at minimum a `name` field.
/// Remaining sections (`[confidence]`, `[inference]`) are deserialized as
/// `UnimatrixConfig` overrides. Missing sections use compiled defaults.
///
/// Returns `EvalError::ConfigInvariant` for parse failures and missing `name`.
/// Returns `EvalError::Io` for file read failures.
///
/// Used by Wave 2 `runner.rs`.
pub(crate) fn parse_profile_toml(path: &Path) -> Result<EvalProfile, EvalError> {
    let content = std::fs::read_to_string(path).map_err(EvalError::Io)?;

    let raw: toml::Value = toml::from_str(&content).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to parse profile TOML at {}: {e}",
            path.display()
        ))
    })?;

    // Extract [profile].name (required).
    let name = raw
        .get("profile")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            EvalError::ConfigInvariant("[profile].name is required in profile TOML".to_string())
        })?
        .to_string();

    // Extract [profile].description (optional).
    let description = raw
        .get("profile")
        .and_then(|p| p.get("description"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    // Build config_overrides by stripping [profile] section then deserializing
    // the remainder as UnimatrixConfig. This allows [confidence] and [inference]
    // sections to flow through to the UnimatrixConfig defaults.
    let mut config_value = raw.clone();
    if let Some(table) = config_value.as_table_mut() {
        table.remove("profile");
    }

    let config_str = toml::to_string(&config_value).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to serialize config subset from {}: {e}",
            path.display()
        ))
    })?;

    let config_overrides: UnimatrixConfig = toml::from_str(&config_str).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to deserialize config overrides from {}: {e}",
            path.display()
        ))
    })?;

    Ok(EvalProfile {
        name,
        description,
        config_overrides,
    })
}
