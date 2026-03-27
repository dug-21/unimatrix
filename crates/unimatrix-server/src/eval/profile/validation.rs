//! Profile validation helpers: weight sum check and TOML parsing (nan-007).

use std::path::Path;

use crate::infra::config::UnimatrixConfig;

use super::error::EvalError;
use super::types::{DistributionTargets, EvalProfile};

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

    // Extract [profile].distribution_change and [profile.distribution_targets]
    // BEFORE the [profile] section is stripped (FR-04, SR-07, constraint 3).
    // Extraction must precede the table.remove("profile") call below.
    let profile_section = raw.get("profile");

    let distribution_change: bool = profile_section
        .and_then(|p| p.get("distribution_change"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let distribution_targets: Option<DistributionTargets> = if distribution_change {
        // Baseline profiles must not declare distribution_change = true (ADR-001,
        // constraint 8).
        if name.eq_ignore_ascii_case("baseline") {
            return Err(EvalError::ConfigInvariant(
                "baseline profile must not declare `distribution_change = true`".to_string(),
            ));
        }

        // [profile.distribution_targets] sub-table is required when flag is set.
        let targets_table = profile_section
            .and_then(|p| p.get("distribution_targets"))
            .and_then(|v| v.as_table());

        let targets_table = targets_table.ok_or_else(|| {
            EvalError::ConfigInvariant(
                "[profile.distribution_targets] is required when distribution_change = true"
                    .to_string(),
            )
        })?;

        // All three fields are required; name the missing one explicitly (NFR-06).
        let cc_at_k_min = targets_table
            .get("cc_at_k_min")
            .and_then(|v| v.as_float())
            .ok_or_else(|| {
                EvalError::ConfigInvariant(
                    "[profile.distribution_targets].cc_at_k_min is required".to_string(),
                )
            })?;

        let icd_min = targets_table
            .get("icd_min")
            .and_then(|v| v.as_float())
            .ok_or_else(|| {
                EvalError::ConfigInvariant(
                    "[profile.distribution_targets].icd_min is required".to_string(),
                )
            })?;

        let mrr_floor = targets_table
            .get("mrr_floor")
            .and_then(|v| v.as_float())
            .ok_or_else(|| {
                EvalError::ConfigInvariant(
                    "[profile.distribution_targets].mrr_floor is required".to_string(),
                )
            })?;

        Some(DistributionTargets {
            cc_at_k_min,
            icd_min,
            mrr_floor,
        })
    } else {
        // distribution_change = false or absent → targets not needed.
        None
    };

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
        distribution_change,
        distribution_targets,
    })
}
