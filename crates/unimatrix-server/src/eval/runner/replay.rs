//! Scenario replay logic for eval runner (nan-007).
//!
//! Loads scenarios from JSONL, replays each through each profile's service layer,
//! and assembles per-scenario result files.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::eval::profile::{EvalProfile, EvalServiceLayer};
use crate::eval::scenarios::ScenarioRecord;
use crate::services::{AuditContext, AuditSource, CallerId, RetrievalMode, ServiceSearchParams};

use super::metrics::{compute_comparison, compute_mrr, compute_p_at_k, determine_ground_truth};
use super::output::{ProfileResult, ScenarioResult, ScoredEntry, write_scenario_result};

// ---------------------------------------------------------------------------
// Scenario loading
// ---------------------------------------------------------------------------

pub(super) fn load_scenarios(
    scenarios: &Path,
) -> Result<Vec<ScenarioRecord>, Box<dyn std::error::Error>> {
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

pub(super) async fn replay_scenario(
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
// Replay loop (used by mod.rs run_eval_async)
// ---------------------------------------------------------------------------

/// Replay all scenarios through all profiles and write per-scenario JSON results.
pub(super) async fn run_replay_loop(
    profiles: &[EvalProfile],
    layers: &[EvalServiceLayer],
    scenario_records: &[ScenarioRecord],
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for record in scenario_records {
        let result = replay_scenario(record, profiles, layers, k).await?;
        write_scenario_result(result, out)?;
    }
    Ok(())
}
