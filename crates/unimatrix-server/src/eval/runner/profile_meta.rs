//! Profile metadata sidecar writer (nan-010).
//!
//! Produces `profile-meta.json` in the eval output directory after all scenario
//! replay completes. The sidecar carries distribution_change flag and
//! DistributionTargets per profile so that `eval report` can select the correct
//! Section 5 gate without re-reading TOMLs.
//!
//! Atomic write: write to `.tmp` then `fs::rename` (ADR-004).
//! Separate serde types from EvalProfile types (ADR-002).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::eval::profile::{EvalError, EvalProfile};

// ---------------------------------------------------------------------------
// Serde types (JSON representation — distinct from in-memory EvalProfile types)
// ---------------------------------------------------------------------------

/// JSON representation of distribution targets in the sidecar.
///
/// Separate from in-memory `DistributionTargets` (no serde on profile types per ADR-002).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistributionTargetsJson {
    pub cc_at_k_min: f64,
    pub icd_min: f64,
    pub mrr_floor: f64,
}

/// Per-profile entry in `profile-meta.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileMetaEntry {
    pub distribution_change: bool,
    pub distribution_targets: Option<DistributionTargetsJson>,
}

/// Top-level `profile-meta.json` structure.
///
/// `version` is a top-level field, not per-entry (ADR-002, design decision #2).
#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileMetaFile {
    /// Schema version — always 1.
    pub version: u32,
    pub profiles: HashMap<String, ProfileMetaEntry>,
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Write `profile-meta.json` atomically to `{out}/profile-meta.json`.
///
/// Builds a [`ProfileMetaFile`] from the given profiles slice and serializes it
/// to JSON. Uses a write-to-tmp + rename strategy to avoid partial writes (ADR-004).
/// Falls back to `fs::copy` + `fs::remove_file` on cross-device rename error.
///
/// Returns `Err(EvalError::Io(_))` on I/O failure or
/// `Err(EvalError::ConfigInvariant(_))` on JSON serialization failure.
pub fn write_profile_meta(profiles: &[EvalProfile], out: &Path) -> Result<(), EvalError> {
    // Step 1: Build ProfileMetaFile from EvalProfile slice.
    let mut profiles_map: HashMap<String, ProfileMetaEntry> =
        HashMap::with_capacity(profiles.len());

    for profile in profiles {
        let entry = ProfileMetaEntry {
            distribution_change: profile.distribution_change,
            distribution_targets: profile.distribution_targets.as_ref().map(|dt| {
                DistributionTargetsJson {
                    cc_at_k_min: dt.cc_at_k_min,
                    icd_min: dt.icd_min,
                    mrr_floor: dt.mrr_floor,
                }
            }),
        };
        profiles_map.insert(profile.name.clone(), entry);
    }

    let meta_file = ProfileMetaFile {
        version: 1,
        profiles: profiles_map,
    };

    // Step 2: Serialize to pretty JSON (consistent with skipped.json in runner/mod.rs).
    let json_str = serde_json::to_string_pretty(&meta_file).map_err(|e| {
        EvalError::ConfigInvariant(format!("failed to serialize profile-meta.json: {e}"))
    })?;

    // Step 3: Atomic write — write to .tmp, then rename.
    let tmp_path = out.join("profile-meta.json.tmp");
    let final_path = out.join("profile-meta.json");

    std::fs::write(&tmp_path, &json_str).map_err(EvalError::Io)?;

    // Step 4: Rename (atomic on POSIX same-filesystem).
    if let Err(_rename_err) = std::fs::rename(&tmp_path, &final_path) {
        // Cross-device fallback: fs::copy then fs::remove_file.
        // This edge case should not occur for local eval output directories
        // but is required for correctness (ADR-004).
        std::fs::copy(&tmp_path, &final_path).map_err(EvalError::Io)?;
        // If copy succeeded but remove fails, a .tmp artifact is left.
        // This is cosmetically untidy but functionally harmless — report reads
        // only profile-meta.json, not .tmp. Do not surface remove failure.
        let _ = std::fs::remove_file(&tmp_path);
    }

    Ok(())
}
