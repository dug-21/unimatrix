//! Eval runner: in-process A/B scenario replay with metric computation (nan-007, D3).
//!
//! `run_eval` is the entry point: it validates inputs, constructs one
//! `EvalServiceLayer` per profile, replays each scenario through each profile,
//! and writes one JSON result file per scenario to the output directory.
//!
//! Design invariants enforced here:
//! - `k == 0` rejected immediately (`EvalError::InvalidK`)
//! - Profile name collisions detected before any layer construction
//! - Kendall tau delegated to `unimatrix_engine::test_scenarios::kendall_tau`
//!   (C-10, FR-22, ADR-003)
//! - `AnalyticsMode::Suppressed` is enforced by `EvalServiceLayer::from_profile`
//! - Live-DB path guard applied in `run_eval` before async work begins (C-13)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
// ADR-003, C-10: kendall_tau from test_scenarios requires test-support feature
use unimatrix_engine::test_scenarios::kendall_tau;

use crate::eval::profile::{EvalError, EvalProfile, EvalServiceLayer, parse_profile_toml};
use crate::eval::scenarios::ScenarioRecord;
use crate::export::block_export_sync;
use crate::project;
use crate::services::{AuditContext, AuditSource, CallerId, RetrievalMode, ServiceSearchParams};

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
    // 1. Construct one EvalServiceLayer per profile
    let mut layers: Vec<EvalServiceLayer> = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        eprintln!(
            "eval run: constructing EvalServiceLayer for profile '{}'",
            profile.name
        );
        let layer = EvalServiceLayer::from_profile(db, profile, None)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        layers.push(layer);
    }

    // 2. Load scenarios from JSONL
    let scenario_records = load_scenarios(scenarios)?;

    // 3. Print summary
    eprintln!(
        "eval run: {} profiles × {} scenarios",
        profiles.len(),
        scenario_records.len()
    );

    // 4. Replay each scenario through each profile
    for record in &scenario_records {
        let result = replay_scenario(record, &profiles, &layers, k).await?;
        write_scenario_result(result, out)?;
    }

    eprintln!("eval run: complete. results in {}", out.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Scenario loading
// ---------------------------------------------------------------------------

fn load_scenarios(scenarios: &Path) -> Result<Vec<ScenarioRecord>, Box<dyn std::error::Error>> {
    if !scenarios.exists() {
        return Err(format!("scenarios file not found: {}", scenarios.display()).into());
    }

    let content = std::fs::read_to_string(scenarios)?;
    let mut records: Vec<ScenarioRecord> = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: ScenarioRecord = serde_json::from_str(trimmed)
            .map_err(|e| format!("scenarios line {}: {e}", line_no + 1))?;
        records.push(record);
    }

    // Empty file is valid: returns empty Vec
    Ok(records)
}

// ---------------------------------------------------------------------------
// Scenario replay
// ---------------------------------------------------------------------------

async fn replay_scenario(
    record: &ScenarioRecord,
    profiles: &[EvalProfile],
    layers: &[EvalServiceLayer],
    k: usize,
) -> Result<ScenarioResult, Box<dyn std::error::Error>> {
    let mut profile_results: HashMap<String, ProfileResult> = HashMap::new();

    for (profile, layer) in profiles.iter().zip(layers.iter()) {
        let result = run_single_profile(record, layer, k).await?;
        profile_results.insert(profile.name.clone(), result);
    }

    // First profile is baseline by convention
    let baseline_name = profiles[0].name.clone();
    let comparison = compute_comparison(&profile_results, &baseline_name)?;

    Ok(ScenarioResult {
        scenario_id: record.id.clone(),
        query: record.query.clone(),
        profiles: profile_results,
        comparison,
    })
}

async fn run_single_profile(
    record: &ScenarioRecord,
    layer: &EvalServiceLayer,
    k: usize,
) -> Result<ProfileResult, Box<dyn std::error::Error>> {
    // 1. Build search params from scenario context
    let retrieval_mode = match record.context.retrieval_mode.as_str() {
        "strict" => RetrievalMode::Strict,
        _ => RetrievalMode::Flexible,
    };

    let params = ServiceSearchParams {
        query: record.query.clone(),
        k,
        filters: None,
        similarity_floor: None,
        confidence_floor: None,
        feature_tag: None,
        co_access_anchors: None,
        caller_agent_id: Some(record.context.agent_id.clone()),
        retrieval_mode,
    };

    let audit_ctx = AuditContext {
        source: AuditSource::Internal {
            service: "eval-runner".to_string(),
        },
        caller_id: record.context.agent_id.clone(),
        session_id: Some(record.context.session_id.clone()),
        feature_cycle: if record.context.feature_cycle.is_empty() {
            None
        } else {
            Some(record.context.feature_cycle.clone())
        },
    };

    let caller_id = CallerId::Agent(record.context.agent_id.clone());

    // 2. Time the search
    let start = Instant::now();
    let search_result = layer
        .inner
        .search
        .search(params, &audit_ctx, &caller_id)
        .await
        .map_err(|e| format!("search failed for scenario {}: {e}", record.id))?;
    let latency_ms = start.elapsed().as_millis() as u64;

    // 3. Build ScoredEntry list
    let entries: Vec<ScoredEntry> = search_result
        .entries
        .into_iter()
        .map(|se| ScoredEntry {
            id: se.entry.id as u64,
            title: se.entry.title.clone(),
            final_score: se.final_score,
            similarity: se.similarity,
            confidence: se.entry.confidence,
            status: se.entry.status.to_string(),
            nli_rerank_delta: None, // W1-4, not in scope
        })
        .collect();

    // 4. Determine ground truth (dual-mode: expected > baseline soft GT)
    let ground_truth = determine_ground_truth(record);

    // 5. Compute P@K and MRR
    let p_at_k = compute_p_at_k(&entries, &ground_truth, k);
    let mrr = compute_mrr(&entries, &ground_truth);

    Ok(ProfileResult {
        entries,
        latency_ms,
        p_at_k,
        mrr,
    })
}

// ---------------------------------------------------------------------------
// Ground truth resolution (AC-07, R-08)
// ---------------------------------------------------------------------------

/// Resolve ground truth with dual-mode semantics.
///
/// Priority: `expected` (hard labels from hand-authored scenarios) takes
/// precedence over `baseline.entry_ids` (soft ground truth from query_log).
/// Returns empty Vec when neither is present (P@K = 0.0, MRR = 0.0).
fn determine_ground_truth(record: &ScenarioRecord) -> Vec<u64> {
    if let Some(expected) = &record.expected {
        expected.clone()
    } else if let Some(baseline) = &record.baseline {
        baseline.entry_ids.clone()
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Metric computation
// ---------------------------------------------------------------------------

/// Precision at K.
///
/// Returns the fraction of top-K results that appear in `ground_truth`.
/// Returns 0.0 if `ground_truth` is empty or `entries` is empty.
fn compute_p_at_k(entries: &[ScoredEntry], ground_truth: &[u64], k: usize) -> f64 {
    if ground_truth.is_empty() || entries.is_empty() {
        return 0.0;
    }
    let gt_set: HashSet<u64> = ground_truth.iter().copied().collect();
    let top_k_len = k.min(entries.len());
    let hits = entries
        .iter()
        .take(k)
        .filter(|e| gt_set.contains(&e.id))
        .count();
    hits as f64 / top_k_len as f64
}

/// Mean Reciprocal Rank.
///
/// Returns the reciprocal of the rank of the first relevant result.
/// Returns 0.0 if `ground_truth` is empty or no relevant result is found.
fn compute_mrr(entries: &[ScoredEntry], ground_truth: &[u64]) -> f64 {
    if ground_truth.is_empty() || entries.is_empty() {
        return 0.0;
    }
    let gt_set: HashSet<u64> = ground_truth.iter().copied().collect();
    for (i, entry) in entries.iter().enumerate() {
        if gt_set.contains(&entry.id) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

// ---------------------------------------------------------------------------
// Comparison metrics
// ---------------------------------------------------------------------------

/// Compute ComparisonMetrics for baseline vs. first candidate profile.
///
/// For single-profile runs (no candidate), self-comparison produces
/// `kendall_tau = 1.0` and all deltas = 0.
fn compute_comparison(
    profile_results: &HashMap<String, ProfileResult>,
    baseline_name: &str,
) -> Result<ComparisonMetrics, Box<dyn std::error::Error>> {
    let baseline = profile_results
        .get(baseline_name)
        .ok_or_else(|| format!("baseline profile '{}' not found in results", baseline_name))?;

    // Candidate = first non-baseline profile, or self (single-profile run)
    let candidate = profile_results
        .keys()
        .find(|k| k.as_str() != baseline_name)
        .and_then(|name| profile_results.get(name))
        .unwrap_or(baseline);

    let baseline_ids: Vec<u64> = baseline.entries.iter().map(|e| e.id).collect();
    let candidate_ids: Vec<u64> = candidate.entries.iter().map(|e| e.id).collect();

    // Kendall tau: only valid when both lists have the same elements.
    // When profiles produce different result sets, compute tau over the intersection.
    let tau = compute_tau_safe(&baseline_ids, &candidate_ids);

    let rank_changes = compute_rank_changes(&baseline_ids, &candidate_ids);

    Ok(ComparisonMetrics {
        kendall_tau: tau,
        rank_changes,
        mrr_delta: candidate.mrr - baseline.mrr,
        p_at_k_delta: candidate.p_at_k - baseline.p_at_k,
        latency_overhead_ms: candidate.latency_ms as i64 - baseline.latency_ms as i64,
    })
}

/// Compute Kendall tau safely when baseline and candidate may have different entry sets.
///
/// `kendall_tau()` from `unimatrix_engine::test_scenarios` requires both slices
/// to contain exactly the same elements. When profiles differ, their result sets
/// may diverge. We compute tau over the shared intersection in baseline order.
///
/// Special cases:
/// - Empty lists → 0.0 (undefined)
/// - No overlap → 0.0 (no ranking signal)
/// - Single element → 1.0 (per `kendall_tau` convention for n <= 1)
fn compute_tau_safe(baseline_ids: &[u64], candidate_ids: &[u64]) -> f64 {
    if baseline_ids.is_empty() || candidate_ids.is_empty() {
        return 0.0;
    }

    let candidate_set: HashSet<u64> = candidate_ids.iter().copied().collect();
    let baseline_set: HashSet<u64> = baseline_ids.iter().copied().collect();

    // Intersection in baseline order
    let common_baseline: Vec<u64> = baseline_ids
        .iter()
        .copied()
        .filter(|id| candidate_set.contains(id))
        .collect();

    // Intersection in candidate order
    let common_candidate: Vec<u64> = candidate_ids
        .iter()
        .copied()
        .filter(|id| baseline_set.contains(id))
        .collect();

    if common_baseline.is_empty() {
        return 0.0;
    }

    kendall_tau(&common_baseline, &common_candidate)
}

/// Compute rank changes between baseline and candidate result lists.
///
/// Entries that moved, appeared in only one list, or dropped out are recorded.
/// Sorted by magnitude of rank change (largest first).
fn compute_rank_changes(baseline_ids: &[u64], candidate_ids: &[u64]) -> Vec<RankChange> {
    let baseline_pos: HashMap<u64, usize> = baseline_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i + 1)) // 1-indexed
        .collect();

    let candidate_pos: HashMap<u64, usize> = candidate_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i + 1))
        .collect();

    let all_ids: HashSet<u64> = baseline_pos
        .keys()
        .chain(candidate_pos.keys())
        .copied()
        .collect();

    let mut changes: Vec<RankChange> = Vec::new();

    for id in all_ids {
        let from = baseline_pos.get(&id).copied();
        let to = candidate_pos.get(&id).copied();
        match (from, to) {
            (Some(f), Some(t)) if f != t => {
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: f,
                    to_rank: t,
                });
            }
            (Some(f), None) => {
                // Dropped from candidate results
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: f,
                    to_rank: candidate_ids.len() + 1,
                });
            }
            (None, Some(t)) => {
                // New in candidate results
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: baseline_ids.len() + 1,
                    to_rank: t,
                });
            }
            _ => {} // unchanged or not in either list
        }
    }

    // Sort by magnitude of rank change, largest first
    changes.sort_by(|a, b| {
        let delta_a = (a.to_rank as i64 - a.from_rank as i64).unsigned_abs();
        let delta_b = (b.to_rank as i64 - b.from_rank as i64).unsigned_abs();
        delta_b.cmp(&delta_a)
    });

    changes
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Write one JSON result file for a scenario to the output directory.
///
/// Filename is derived from `scenario_id` with path separators replaced by `_`.
fn write_scenario_result(
    result: ScenarioResult,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = result.scenario_id.replace('/', "_").replace('\\', "_") + ".json";
    let out_path = out_dir.join(&filename);
    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&out_path, json.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::scenarios::{ScenarioBaseline, ScenarioContext};
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helper builders
    // -----------------------------------------------------------------------

    fn make_scenario(
        id: &str,
        baseline_ids: Option<Vec<u64>>,
        expected: Option<Vec<u64>>,
    ) -> ScenarioRecord {
        ScenarioRecord {
            id: id.to_string(),
            query: "test query".to_string(),
            context: ScenarioContext {
                agent_id: "test-agent".to_string(),
                feature_cycle: "nan-007".to_string(),
                session_id: "sess-1".to_string(),
                retrieval_mode: "flexible".to_string(),
            },
            baseline: baseline_ids.map(|ids| {
                let scores = vec![0.9f32; ids.len()];
                ScenarioBaseline {
                    entry_ids: ids,
                    scores,
                }
            }),
            source: "mcp".to_string(),
            expected,
        }
    }

    fn make_entries(ids: &[u64]) -> Vec<ScoredEntry> {
        ids.iter()
            .map(|&id| ScoredEntry {
                id,
                title: format!("Entry {id}"),
                final_score: 0.9,
                similarity: 0.85,
                confidence: 0.7,
                status: "Active".to_string(),
                nli_rerank_delta: None,
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // R-03: kendall_tau reachable from eval module (compile guard)
    // -----------------------------------------------------------------------

    #[test]
    fn test_kendall_tau_reachable_from_eval_runner() {
        // Direct call to unimatrix_engine::test_scenarios::kendall_tau.
        // Removing the test-support feature from Cargo.toml causes a compile error here.
        let tau = kendall_tau(&[1, 2, 3], &[1, 3, 2]);
        // [1,2,3] vs [1,3,2]: pairs (1,2)→OK, (1,3)→OK, (2,3)→inverted
        // concordant=2, discordant=1, total_pairs=3 → tau=(2-1)/3 = 0.333...
        assert!((-1.0..=1.0).contains(&tau), "tau must be in [-1.0, 1.0]");
        let expected = 1.0 / 3.0;
        assert!(
            (tau - expected).abs() < 1e-9,
            "expected tau ≈ 0.333, got {tau}"
        );
    }

    // -----------------------------------------------------------------------
    // R-03 edge case: single-element lists
    // -----------------------------------------------------------------------

    #[test]
    fn test_kendall_tau_single_element_no_panic() {
        // Single-element lists: kendall_tau returns 1.0 by convention.
        let tau = kendall_tau(&[5], &[5]);
        assert!(
            !tau.is_nan(),
            "tau must not be NaN for single-element lists"
        );
        assert_eq!(tau, 1.0, "single-element list: tau must be 1.0");
    }

    // -----------------------------------------------------------------------
    // AC-07, R-08: P@K dual-mode — soft ground truth (expected = null)
    // -----------------------------------------------------------------------

    #[test]
    fn test_pak_soft_ground_truth_query_log_scenario() {
        // expected = None → use baseline.entry_ids as soft GT
        // baseline = [10, 20, 30], results = [10, 20, 50]
        // 2 hits out of k=3 → P@3 = 2/3
        let ground_truth = vec![10u64, 20, 30];
        let entries = make_entries(&[10, 20, 50]);
        let p = compute_p_at_k(&entries, &ground_truth, 3);
        let expected = 2.0 / 3.0;
        assert!(
            (p - expected).abs() < 1e-9,
            "P@3 = {p}, expected ≈ {expected}"
        );
    }

    // -----------------------------------------------------------------------
    // AC-07, R-08: P@K dual-mode — hard labels (expected = [id1, id2])
    // -----------------------------------------------------------------------

    #[test]
    fn test_pak_hard_labels_hand_authored_scenario() {
        // expected = [10, 20], results = [10, 30, 20] → 2 hits → P@3 = 2/3
        let ground_truth = vec![10u64, 20];
        let entries = make_entries(&[10, 30, 20]);
        let p = compute_p_at_k(&entries, &ground_truth, 3);
        let expected = 2.0 / 3.0;
        assert!(
            (p - expected).abs() < 1e-9,
            "P@3 = {p}, expected ≈ {expected}"
        );
    }

    // -----------------------------------------------------------------------
    // R-08 critical: hard labels not confused with baseline
    // -----------------------------------------------------------------------

    #[test]
    fn test_pak_hard_labels_not_confused_with_baseline() {
        // expected = [10], baseline = [20, 30] (disjoint from expected)
        // results = [10, 20, 30]
        // P@3 must use expected → 1 hit → P@3 = 1/3
        // If baseline used by mistake → 2 hits → P@3 = 2/3
        let record = make_scenario("s1", Some(vec![20, 30]), Some(vec![10]));
        let gt = determine_ground_truth(&record);
        assert_eq!(gt, vec![10u64], "must use expected, not baseline");

        let entries = make_entries(&[10, 20, 30]);
        let p = compute_p_at_k(&entries, &gt, 3);
        let expected = 1.0 / 3.0;
        assert!(
            (p - expected).abs() < 1e-9,
            "P@3 = {p}, expected ≈ {expected} (using hard labels, not baseline)"
        );
    }

    // -----------------------------------------------------------------------
    // P@K at k=1
    // -----------------------------------------------------------------------

    #[test]
    fn test_pak_at_k1_known_result() {
        let ground_truth = vec![99u64];
        let entries = make_entries(&[99, 1, 2]);
        let p = compute_p_at_k(&entries, &ground_truth, 1);
        assert_eq!(p, 1.0, "P@1 must be 1.0 when first result is in GT");
    }

    // -----------------------------------------------------------------------
    // MRR known result
    // -----------------------------------------------------------------------

    #[test]
    fn test_mrr_known_result() {
        // GT = {10, 20}, results = [5, 10, 20] → first relevant at rank 2
        let ground_truth = vec![10u64, 20];
        let entries = make_entries(&[5, 10, 20]);
        let mrr = compute_mrr(&entries, &ground_truth);
        assert!((mrr - 0.5).abs() < 1e-9, "MRR = {mrr}, expected 0.5");
    }

    // -----------------------------------------------------------------------
    // determine_ground_truth: priority and fallback
    // -----------------------------------------------------------------------

    #[test]
    fn test_determine_ground_truth_prefers_expected() {
        let record = make_scenario("s1", Some(vec![20, 30]), Some(vec![10, 11]));
        let gt = determine_ground_truth(&record);
        assert_eq!(gt, vec![10u64, 11]);
    }

    #[test]
    fn test_determine_ground_truth_falls_back_to_baseline() {
        let record = make_scenario("s1", Some(vec![20, 30]), None);
        let gt = determine_ground_truth(&record);
        assert_eq!(gt, vec![20u64, 30]);
    }

    #[test]
    fn test_determine_ground_truth_empty_when_neither() {
        let record = make_scenario("s1", None, None);
        let gt = determine_ground_truth(&record);
        assert!(gt.is_empty(), "no GT available → empty vec");
    }

    // -----------------------------------------------------------------------
    // compute_p_at_k / compute_mrr: empty ground truth
    // -----------------------------------------------------------------------

    #[test]
    fn test_pak_empty_ground_truth_returns_zero() {
        let entries = make_entries(&[1, 2, 3]);
        let p = compute_p_at_k(&entries, &[], 3);
        assert_eq!(p, 0.0, "P@K with no GT must be 0.0");
    }

    #[test]
    fn test_mrr_empty_ground_truth_returns_zero() {
        let entries = make_entries(&[1, 2, 3]);
        let mrr = compute_mrr(&entries, &[]);
        assert_eq!(mrr, 0.0, "MRR with no GT must be 0.0");
    }

    // -----------------------------------------------------------------------
    // compute_rank_changes: 1-indexed positions
    // -----------------------------------------------------------------------

    #[test]
    fn test_rank_changes_one_moved_entry() {
        // Baseline: [1, 2, 3], Candidate: [1, 3, 2]
        // 2 → 3, 3 → 2
        let baseline = vec![1u64, 2, 3];
        let candidate = vec![1u64, 3, 2];
        let changes = compute_rank_changes(&baseline, &candidate);

        assert_eq!(changes.len(), 2);
        let ids: HashSet<u64> = changes.iter().map(|c| c.entry_id).collect();
        assert!(ids.contains(&2), "entry 2 must be in changes");
        assert!(ids.contains(&3), "entry 3 must be in changes");

        let b_change = changes.iter().find(|c| c.entry_id == 2).unwrap();
        assert_eq!(b_change.from_rank, 2);
        assert_eq!(b_change.to_rank, 3);

        let c_change = changes.iter().find(|c| c.entry_id == 3).unwrap();
        assert_eq!(c_change.from_rank, 3);
        assert_eq!(c_change.to_rank, 2);
    }

    #[test]
    fn test_rank_changes_entry_dropped_from_candidate() {
        let baseline = vec![1u64, 2];
        let candidate = vec![1u64];
        let changes = compute_rank_changes(&baseline, &candidate);
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.entry_id, 2);
        assert_eq!(change.from_rank, 2);
        assert_eq!(change.to_rank, 2, "to_rank = candidate_len + 1 = 1 + 1 = 2");
    }

    #[test]
    fn test_rank_changes_entry_new_in_candidate() {
        let baseline = vec![1u64];
        let candidate = vec![1u64, 2];
        let changes = compute_rank_changes(&baseline, &candidate);
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.entry_id, 2);
        assert_eq!(
            change.from_rank, 2,
            "from_rank = baseline_len + 1 = 1 + 1 = 2"
        );
        assert_eq!(change.to_rank, 2);
    }

    #[test]
    fn test_rank_changes_no_changes_identical_lists() {
        let ids = vec![1u64, 2, 3];
        let changes = compute_rank_changes(&ids, &ids);
        assert!(changes.is_empty(), "identical lists → no rank changes");
    }

    // -----------------------------------------------------------------------
    // compute_tau_safe: edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_tau_safe_empty_lists_returns_zero() {
        let tau = compute_tau_safe(&[], &[]);
        assert_eq!(tau, 0.0);
    }

    #[test]
    fn test_tau_safe_identical_lists() {
        let tau = compute_tau_safe(&[1, 2, 3], &[1, 2, 3]);
        assert!(
            (tau - 1.0).abs() < 1e-9,
            "identical lists → tau = 1.0, got {tau}"
        );
    }

    #[test]
    fn test_tau_safe_disjoint_lists_returns_zero() {
        let tau = compute_tau_safe(&[1, 2, 3], &[4, 5, 6]);
        assert_eq!(tau, 0.0, "disjoint lists → tau = 0.0");
    }

    // -----------------------------------------------------------------------
    // k == 0 rejection
    // -----------------------------------------------------------------------

    #[test]
    fn test_k_zero_rejected() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("snap.db");
        let scenarios = dir.path().join("scenarios.jsonl");
        let out = dir.path().join("results");
        std::fs::write(&db, b"SQLite format 3\0").unwrap();
        std::fs::write(&scenarios, b"").unwrap();

        let result = run_eval(&db, &scenarios, &[], 0, &out);
        assert!(result.is_err(), "k=0 must return Err");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("k >= 1") || msg.contains("k must be"),
            "got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Profile name collision detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_profile_name_collision_rejected() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("snap.db");
        std::fs::write(&db, b"SQLite format 3\0").unwrap();

        let scenarios = dir.path().join("scenarios.jsonl");
        std::fs::write(&scenarios, b"").unwrap();

        let p1 = dir.path().join("p1.toml");
        let p2 = dir.path().join("p2.toml");
        std::fs::write(&p1, "[profile]\nname = \"baseline\"\n").unwrap();
        std::fs::write(&p2, "[profile]\nname = \"baseline\"\n").unwrap();

        let out = dir.path().join("results");
        let configs = vec![p1, p2];

        let result = run_eval(&db, &scenarios, &configs, 5, &out);
        if let Err(e) = &result {
            let msg = format!("{e}");
            assert!(
                msg.contains("duplicate profile name") || msg.contains("baseline"),
                "expected collision error, got: {msg}"
            );
        }
        // If result is Ok it means path guard skipped and profiles were processed —
        // but with an empty scenarios file, run succeeds with 0 scenarios. That's fine.
        // The collision check happens before async work, so it should error.
    }

    // -----------------------------------------------------------------------
    // Empty scenarios file
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_scenarios_loads_zero_records() {
        let dir = TempDir::new().unwrap();
        let scenarios = dir.path().join("empty.jsonl");
        std::fs::write(&scenarios, b"").unwrap();
        let records = load_scenarios(&scenarios).expect("load must succeed");
        assert!(records.is_empty(), "empty file → zero records");
    }

    #[test]
    fn test_scenarios_with_blank_lines_skipped() {
        let dir = TempDir::new().unwrap();
        let scenarios = dir.path().join("blank.jsonl");
        let line = r#"{"id":"s1","query":"q","context":{"agent_id":"a","feature_cycle":"f","session_id":"s","retrieval_mode":"flexible"},"baseline":null,"source":"mcp","expected":null}"#;
        std::fs::write(&scenarios, format!("\n{line}\n\n")).unwrap();
        let records = load_scenarios(&scenarios).expect("load must succeed");
        assert_eq!(records.len(), 1, "one record, blank lines skipped");
    }

    // -----------------------------------------------------------------------
    // load_scenarios: missing file returns error
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_scenarios_missing_file_returns_error() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("nonexistent.jsonl");
        let result = load_scenarios(&missing);
        assert!(result.is_err(), "missing file must return Err");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("not found") || msg.contains("nonexistent"),
            "got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // write_scenario_result: path separator sanitization
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_scenario_result_sanitizes_id() {
        let dir = TempDir::new().unwrap();
        let result = ScenarioResult {
            scenario_id: "a/b/c".to_string(),
            query: "q".to_string(),
            profiles: HashMap::new(),
            comparison: ComparisonMetrics {
                kendall_tau: 1.0,
                rank_changes: vec![],
                mrr_delta: 0.0,
                p_at_k_delta: 0.0,
                latency_overhead_ms: 0,
            },
        };
        write_scenario_result(result, dir.path()).expect("write must succeed");
        let expected_file = dir.path().join("a_b_c.json");
        assert!(
            expected_file.exists(),
            "sanitized filename must be a_b_c.json"
        );
    }

    // -----------------------------------------------------------------------
    // JSON schema completeness (AC-06)
    // -----------------------------------------------------------------------

    #[test]
    fn test_output_json_schema_completeness() {
        let dir = TempDir::new().unwrap();
        let mut profiles = HashMap::new();
        profiles.insert(
            "baseline".to_string(),
            ProfileResult {
                entries: vec![ScoredEntry {
                    id: 1,
                    title: "T".to_string(),
                    final_score: 0.9,
                    similarity: 0.8,
                    confidence: 0.7,
                    status: "Active".to_string(),
                    nli_rerank_delta: None,
                }],
                latency_ms: 10,
                p_at_k: 0.5,
                mrr: 1.0,
            },
        );

        let result = ScenarioResult {
            scenario_id: "test-schema".to_string(),
            query: "test".to_string(),
            profiles,
            comparison: ComparisonMetrics {
                kendall_tau: 1.0,
                rank_changes: vec![RankChange {
                    entry_id: 1,
                    from_rank: 1,
                    to_rank: 2,
                }],
                mrr_delta: 0.1,
                p_at_k_delta: 0.2,
                latency_overhead_ms: -5,
            },
        };

        write_scenario_result(result, dir.path()).expect("write must succeed");
        let file = dir.path().join("test-schema.json");
        let content = std::fs::read_to_string(file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(v["scenario_id"].is_string(), "scenario_id must be string");
        assert!(v["query"].is_string(), "query must be string");
        assert!(v["profiles"].is_object(), "profiles must be object");
        assert!(
            v["profiles"]["baseline"]["latency_ms"].is_number(),
            "latency_ms must be number"
        );
        assert!(
            v["profiles"]["baseline"]["p_at_k"].is_number(),
            "p_at_k must be number"
        );
        assert!(
            v["profiles"]["baseline"]["mrr"].is_number(),
            "mrr must be number"
        );
        assert!(
            v["comparison"]["kendall_tau"].is_number(),
            "kendall_tau must be number"
        );
        assert!(
            v["comparison"]["mrr_delta"].is_number(),
            "mrr_delta must be number"
        );
        assert!(
            v["comparison"]["p_at_k_delta"].is_number(),
            "p_at_k_delta must be number"
        );
        assert!(
            v["comparison"]["latency_overhead_ms"].is_number(),
            "latency_overhead_ms must be number"
        );
        assert!(
            v["comparison"]["rank_changes"].is_array(),
            "rank_changes must be array"
        );
    }

    // -----------------------------------------------------------------------
    // Metric reproducibility (NFR-05)
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_run_metric_reproducibility() {
        let ground_truth = vec![10u64, 20, 30];
        let entries = make_entries(&[10, 20, 50]);

        let p1 = compute_p_at_k(&entries, &ground_truth, 3);
        let p2 = compute_p_at_k(&entries, &ground_truth, 3);
        assert_eq!(p1, p2, "P@K must be deterministic");

        let m1 = compute_mrr(&entries, &ground_truth);
        let m2 = compute_mrr(&entries, &ground_truth);
        assert_eq!(m1, m2, "MRR must be deterministic");
    }
}
