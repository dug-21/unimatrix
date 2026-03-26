//! Markdown report generation for eval results (nan-007 D4).
//!
//! Reads per-scenario JSON result files from a `--results` directory, aggregates
//! across all scenarios, and writes a Markdown report with five required sections:
//!
//! 1. Summary
//! 2. Notable Ranking Changes
//! 3. Latency Distribution
//! 4. Entry-Level Analysis
//! 5. Zero-Regression Check
//! 6. Distribution Analysis
//!
//! This module is entirely synchronous: pure filesystem reads and string formatting.
//! No database, no sqlx, no tokio runtime, no async. Dispatched directly in the sync
//! branch of `run_eval_command` (no `block_export_sync` needed).
//!
//! Zero-regression check uses OR semantics (AC-09, R-12): a scenario is a regression
//! if candidate MRR < baseline MRR OR candidate P@K < baseline P@K.
//!
//! `run_report` always returns `Ok(())` — C-07, FR-29.

mod aggregate;
mod render;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use aggregate::{
    compute_aggregate_stats, compute_cc_at_k_scenario_rows, compute_entry_rank_changes,
    compute_latency_buckets, find_regressions,
};
use render::render_report;

// ---------------------------------------------------------------------------
// JSON deserialization types (mirror eval/runner.rs schema)
//
// These are local copies defined here for deserialization from the result JSON
// files. They intentionally mirror the runner.rs public types so that
// report.rs can deserialize result files without a hard compile-time
// dependency on runner.rs's internal layout.
// ---------------------------------------------------------------------------

/// A scored entry in a profile result.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ScoredEntry {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub final_score: f64,
    #[serde(default)]
    pub similarity: f64,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub nli_rerank_delta: Option<f64>,
}

/// A rank change between baseline and candidate.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RankChange {
    pub entry_id: u64,
    pub from_rank: usize,
    pub to_rank: usize,
}

/// Comparison metrics computed across profiles.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ComparisonMetrics {
    #[serde(default)]
    pub kendall_tau: f64,
    #[serde(default)]
    pub rank_changes: Vec<RankChange>,
    #[serde(default)]
    pub mrr_delta: f64,
    #[serde(default)]
    pub p_at_k_delta: f64,
    #[serde(default)]
    pub latency_overhead_ms: i64,
    #[serde(default)]
    pub cc_at_k_delta: f64,
    #[serde(default)]
    pub icd_delta: f64,
}

/// Metrics for one profile on one scenario.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ProfileResult {
    #[serde(default)]
    pub entries: Vec<ScoredEntry>,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub p_at_k: f64,
    #[serde(default)]
    pub mrr: f64,
    #[serde(default)]
    pub cc_at_k: f64,
    #[serde(default)]
    pub icd: f64,
}

/// Per-scenario result JSON file schema.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ScenarioResult {
    pub scenario_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileResult>,
    #[serde(default = "default_comparison")]
    pub comparison: ComparisonMetrics,
}

pub(crate) fn default_comparison() -> ComparisonMetrics {
    ComparisonMetrics {
        kendall_tau: 1.0,
        rank_changes: Vec::new(),
        mrr_delta: 0.0,
        p_at_k_delta: 0.0,
        latency_overhead_ms: 0,
        cc_at_k_delta: 0.0,
        icd_delta: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Internal aggregate types (used by aggregate.rs and render.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub(super) struct AggregateStats {
    pub profile_name: String,
    pub scenario_count: usize,
    pub mean_p_at_k: f64,
    pub mean_mrr: f64,
    pub mean_latency_ms: f64,
    pub p_at_k_delta: f64,
    pub mrr_delta: f64,
    pub latency_delta_ms: f64,
    pub mean_cc_at_k: f64,
    pub mean_icd: f64,
    pub cc_at_k_delta: f64,
    pub icd_delta: f64,
}

/// Per-scenario row for CC@k comparison in the Distribution Analysis section.
#[derive(Debug)]
pub(super) struct CcAtKScenarioRow {
    pub scenario_id: String,
    pub query: String,
    pub baseline_cc_at_k: f64,
    pub candidate_cc_at_k: f64,
    pub cc_at_k_delta: f64,
}

#[derive(Debug)]
pub(super) struct RegressionRecord {
    pub scenario_id: String,
    pub query: String,
    pub profile_name: String,
    pub baseline_mrr: f64,
    pub candidate_mrr: f64,
    pub baseline_p_at_k: f64,
    pub candidate_p_at_k: f64,
    pub reason: String,
}

#[derive(Debug)]
pub(super) struct LatencyBucket {
    pub le_ms: u64,
    pub count: usize,
}

#[derive(Debug)]
pub(super) struct EntryRankSummary {
    pub most_promoted: Vec<(u64, String, i64)>,
    pub most_demoted: Vec<(u64, String, i64)>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Generate a Markdown eval report from per-scenario JSON result files.
///
/// - `results`: directory containing `*.json` result files from `eval run`
/// - `scenarios`: optional JSONL file for annotating scenario queries by ID
/// - `out`: path to write the Markdown report
///
/// Always returns `Ok(())` — never exits non-zero due to regression count (C-07, FR-29).
pub fn run_report(
    results: &Path,
    scenarios: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Enumerate all *.json files in the results directory.
    let mut result_files: Vec<std::path::PathBuf> = std::fs::read_dir(results)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .map(|e| e.path())
        .collect();

    // Sort for deterministic ordering across platforms.
    result_files.sort();

    if result_files.is_empty() {
        eprintln!("WARN: no result JSON files found in {}", results.display());
    }

    // Step 2: Deserialize result files, skipping malformed ones with WARN.
    let mut scenario_results: Vec<ScenarioResult> = Vec::new();
    for path in &result_files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("WARN: skipping {} (read error: {e})", path.display());
                continue;
            }
        };
        match serde_json::from_str::<ScenarioResult>(&content) {
            Ok(r) => scenario_results.push(r),
            Err(e) => {
                eprintln!("WARN: skipping {} (parse error: {e})", path.display());
            }
        }
    }

    // Step 3: Load optional scenarios JSONL for query annotation.
    let query_map: HashMap<String, String> = match scenarios {
        Some(p) => load_scenario_query_map(p)?,
        None => HashMap::new(),
    };

    // Step 4: Aggregate.
    let aggregate_stats = compute_aggregate_stats(&scenario_results);
    let regressions = find_regressions(&scenario_results, &query_map);
    let latency_buckets = compute_latency_buckets(&scenario_results);
    let entry_rank_changes = compute_entry_rank_changes(&scenario_results);
    let cc_at_k_rows = compute_cc_at_k_scenario_rows(&scenario_results);

    // Step 5: Render.
    let md = render_report(
        &aggregate_stats,
        &scenario_results,
        &regressions,
        &latency_buckets,
        &entry_rank_changes,
        &query_map,
        &cc_at_k_rows,
    );

    // Step 6: Write output.
    let mut out_file = std::fs::File::create(out)?;
    out_file.write_all(md.as_bytes())?;

    // Step 7: Confirm written.
    eprintln!("eval report: written to {}", out.display());

    // Step 8: Always Ok(()) — C-07, FR-29.
    Ok(())
}

// ---------------------------------------------------------------------------
// load_scenario_query_map
// ---------------------------------------------------------------------------

/// Load a JSONL scenarios file and build a map from scenario ID to query text.
fn load_scenario_query_map(
    path: &Path,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<serde_json::Value>(line)
            && let (Some(id), Some(query)) = (
                record.get("id").and_then(|v| v.as_str()),
                record.get("query").and_then(|v| v.as_str()),
            )
        {
            map.insert(id.to_string(), query.to_string());
        }
    }
    Ok(map)
}
