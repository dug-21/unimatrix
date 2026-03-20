//! Eval runner: in-process A/B scenario replay with metric computation (nan-007, D3).
//!
//! `run_eval` is the entry point: it validates inputs, constructs one
//! `EvalServiceLayer` per profile, polls for embed model readiness, replays
//! each scenario through each profile, and writes one JSON result file per
//! scenario to the output directory.
//!
//! Design invariants enforced here:
//! - `k == 0` rejected immediately (`EvalError::InvalidK`)
//! - Profile name collisions detected before any layer construction
//! - Kendall tau delegated to `unimatrix_engine::test_scenarios::kendall_tau`
//!   (C-10, FR-22, ADR-003)
//! - `AnalyticsMode::Suppressed` is enforced by `EvalServiceLayer::from_profile`
//! - Live-DB path guard applied in `run_eval` before async work begins (C-13)
//! - Embed model readiness polled before scenario replay (pseudocode lines 148-158)

mod layer;
mod metrics;
mod output;
mod replay;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_metrics;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use output::{ComparisonMetrics, ProfileResult, RankChange, ScenarioResult, ScoredEntry};

use crate::eval::profile::{EvalError, EvalProfile, EvalServiceLayer, parse_profile_toml};
use crate::export::block_export_sync;
use crate::project;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run in-process A/B evaluation across the supplied profile configs.
///
/// Steps:
/// 1. Validate `k >= 1`
/// 2. Apply live-DB path guard on `--db` (C-13, FR-44, ADR-001)
/// 3. Parse all profile TOMLs, detect name collisions
/// 4. Create output directory
/// 5. Bridge to async via `block_export_sync` for layer construction + replay
///
/// `configs` is an ordered slice of profile TOML paths. The first profile is
/// treated as the baseline; all others are candidates.
pub fn run_eval(
    db: &Path,
    scenarios: &Path,
    configs: &[PathBuf],
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Validate --k (EvalError::InvalidK if 0)
    if k == 0 {
        return Err(Box::new(EvalError::InvalidK(0)));
    }

    // 2. Live-DB path guard (C-13, FR-44, ADR-001)
    //    Skip guard if project paths cannot be resolved (eval scenarios model).
    if let Ok(paths) = project::ensure_data_directory(None, None) {
        let active_db =
            std::fs::canonicalize(&paths.db_path).unwrap_or_else(|_| paths.db_path.clone());
        let db_resolved = std::fs::canonicalize(db).map_err(EvalError::Io)?;
        if db_resolved == active_db {
            return Err(Box::new(EvalError::LiveDbPath {
                supplied: db.to_path_buf(),
                active: active_db,
            }));
        }
    }

    // 3. Parse all profile TOMLs
    let mut profiles: Vec<EvalProfile> = Vec::with_capacity(configs.len());
    for cfg_path in configs {
        let profile =
            parse_profile_toml(cfg_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        profiles.push(profile);
    }

    // 4. Detect profile name collisions before any layer construction
    {
        let mut seen_names: HashSet<&str> = HashSet::new();
        for profile in &profiles {
            if !seen_names.insert(profile.name.as_str()) {
                return Err(Box::new(EvalError::ProfileNameCollision(
                    profile.name.clone(),
                )));
            }
        }
    }

    // 5. Create output directory
    std::fs::create_dir_all(out)?;

    // 6. Bridge to async for profile construction + scenario replay
    block_export_sync(run_eval_async(db, scenarios, profiles, k, out))
}

// ---------------------------------------------------------------------------
// Async core
// ---------------------------------------------------------------------------

async fn run_eval_async(
    db: &Path,
    scenarios: &Path,
    profiles: Vec<EvalProfile>,
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Construct one EvalServiceLayer per profile, then wait for embed model
    let mut layers: Vec<EvalServiceLayer> = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        eprintln!(
            "eval run: constructing EvalServiceLayer for profile '{}'",
            profile.name
        );
        let layer = EvalServiceLayer::from_profile(db, profile, None::<&Path>)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        // Wait for embedding model to load before proceeding (pseudocode lines 148-158).
        // Mirrors TestHarness readiness pattern.
        let embed = layer.embed_handle();
        layer::wait_for_embed_model(&embed, &profile.name).await?;

        layers.push(layer);
    }

    // 2. Load scenarios from JSONL
    let scenario_records = replay::load_scenarios(scenarios)?;

    // 3. Print summary
    eprintln!(
        "eval run: {} profiles × {} scenarios",
        profiles.len(),
        scenario_records.len()
    );

    // 4. Replay each scenario through each profile
    replay::run_replay_loop(&profiles, &layers, &scenario_records, k, out).await?;

    eprintln!("eval run: complete. results in {}", out.display());
    Ok(())
}
