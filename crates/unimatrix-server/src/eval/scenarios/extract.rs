//! Row extraction from `query_log` table (nan-007).
//!
//! # Notes on actual `query_log` schema
//!
//! The real schema (from `migration.rs`) has these columns:
//! `query_id`, `session_id`, `query_text`, `ts`, `result_count`,
//! `result_entry_ids`, `similarity_scores`, `retrieval_mode`, `source`.
//!
//! There is no `agent_id` or `feature_cycle` column. The pseudocode assumed
//! those columns — they are absent. `ScenarioContext.agent_id` is populated
//! from `session_id` as the closest available identifier, and `feature_cycle`
//! defaults to `""`.

use sqlx::Row;

use super::types::{ScenarioBaseline, ScenarioContext, ScenarioRecord};

/// Map a `query_log` row to a `ScenarioRecord`.
///
/// Handles length parity enforcement (R-16): if `result_entry_ids` and
/// `similarity_scores` arrays differ in length, both are truncated to the
/// minimum length and a warning is printed to stderr.
pub(crate) fn build_scenario_record(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ScenarioRecord, Box<dyn std::error::Error>> {
    let query_id: i64 = row.try_get("query_id")?;
    let session_id: String = row.try_get("session_id")?;
    let query_text: String = row.try_get("query_text")?;
    let retrieval_mode: String = row
        .try_get::<Option<String>, _>("retrieval_mode")?
        .unwrap_or_else(|| "flexible".to_string());
    let source: String = row.try_get("source")?;

    // Parse entry_ids JSON array (may be NULL)
    let entry_ids_json: String = row
        .try_get::<Option<String>, _>("result_entry_ids")?
        .unwrap_or_default();

    // Parse scores JSON array (may be NULL)
    let scores_json: String = row
        .try_get::<Option<String>, _>("similarity_scores")?
        .unwrap_or_default();

    let mut entry_ids: Vec<u64> = if entry_ids_json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&entry_ids_json)
            .map_err(|e| format!("failed to parse result_entry_ids for row {query_id}: {e}"))?
    };

    let mut scores: Vec<f32> = if scores_json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&scores_json)
            .map_err(|e| format!("failed to parse similarity_scores for row {query_id}: {e}"))?
    };

    // Length parity check (R-16)
    if !entry_ids.is_empty() && entry_ids.len() != scores.len() {
        eprintln!(
            "WARN: query_log row {query_id}: entry_ids.len()={} != scores.len()={}, \
             truncating to min",
            entry_ids.len(),
            scores.len()
        );
        let min_len = std::cmp::min(entry_ids.len(), scores.len());
        entry_ids.truncate(min_len);
        scores.truncate(min_len);
    }

    // Build baseline only when results exist
    let baseline = if entry_ids.is_empty() {
        None
    } else {
        Some(ScenarioBaseline { entry_ids, scores })
    };

    Ok(ScenarioRecord {
        id: format!("qlog-{query_id}"),
        query: query_text,
        context: ScenarioContext {
            // No agent_id column in query_log; use session_id as proxy.
            agent_id: session_id.clone(),
            // No feature_cycle column in query_log.
            feature_cycle: String::new(),
            session_id,
            retrieval_mode,
        },
        baseline,
        source,
        expected: None,
    })
}
