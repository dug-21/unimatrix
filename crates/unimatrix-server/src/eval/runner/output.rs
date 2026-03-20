//! Result types for eval runner output (nan-007).
//!
//! All types derive `Serialize` + `Deserialize` and are written as pretty-printed
//! JSON per scenario to the output directory.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Per-entry result produced by one profile's search replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredEntry {
    pub id: u64,
    pub title: String,
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    /// Always `None` in nan-007 — NLI re-ranking is W1-4.
    pub nli_rerank_delta: Option<f64>,
}

/// Metric result for one profile against one scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: u64,
    pub p_at_k: f64,
    pub mrr: f64,
}

/// Rank change record for a single entry between baseline and candidate profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankChange {
    pub entry_id: u64,
    /// 1-indexed position in baseline result list (or `baseline_len + 1` if absent).
    pub from_rank: usize,
    /// 1-indexed position in candidate result list (or `candidate_len + 1` if absent).
    pub to_rank: usize,
}

/// Comparison metrics across profiles for one scenario.
///
/// First profile is baseline by convention. Comparison is baseline vs. first
/// non-baseline profile. All profiles stored in `ScenarioResult.profiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonMetrics {
    pub kendall_tau: f64,
    pub rank_changes: Vec<RankChange>,
    /// `candidate.mrr - baseline.mrr`
    pub mrr_delta: f64,
    /// `candidate.p_at_k - baseline.p_at_k`
    pub p_at_k_delta: f64,
    /// `candidate.latency_ms as i64 - baseline.latency_ms as i64`
    pub latency_overhead_ms: i64,
}

/// Complete result for one scenario across all profiles.
///
/// Written as a pretty-printed JSON file per scenario to the output directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Write one JSON result file for a scenario to the output directory.
///
/// Filename is derived from `scenario_id` with path separators replaced by `_`.
pub(super) fn write_scenario_result(
    result: ScenarioResult,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = result.scenario_id.replace('/', "_").replace('\\', "_") + ".json";
    let out_path = out_dir.join(&filename);
    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&out_path, json.as_bytes())?;
    Ok(())
}
