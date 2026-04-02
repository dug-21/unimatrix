//! NLI graph edge helpers — shared utilities for NLI edge writes and metadata.
//!
//! This module provides three pub(crate) helpers consumed by `nli_detection_tick.rs`:
//! `write_nli_edge`, `format_nli_metadata`, and `current_timestamp_secs`.
//!
//! `run_post_store_nli`, `maybe_run_bootstrap_promotion`, and related functions
//! were removed in crt-038 (conf-boost-c formula migration).
//! Module merge into `nli_detection_tick.rs` is deferred to Group 2 (ADR-004).

use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_core::Store;
use unimatrix_embed::NliScores;

/// Write a single NLI-confirmed graph edge via `write_pool_server()` (SR-02).
///
/// Uses `INSERT OR IGNORE` for idempotency on `UNIQUE(source_id, target_id, relation_type)`.
/// Returns `true` if the insert succeeded (edge written or already existed).
pub(crate) async fn write_nli_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str, // "Supports" or "Contradicts"
    weight: f32,
    created_at: u64,
    metadata: &str, // JSON string: '{"nli_entailment": f32, "nli_contradiction": f32}'
) -> bool {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(weight as f64)
    .bind(created_at as i64)
    .bind(metadata)
    .execute(store.write_pool_server())
    .await;

    match result {
        Ok(_) => true,
        Err(e) => {
            // R-16: write pool contention or SQLite busy; log at warn, do NOT propagate.
            tracing::warn!(
                source_id = source_id,
                target_id = target_id,
                relation_type = relation_type,
                error = %e,
                "post-store NLI: failed to write graph edge"
            );
            false
        }
    }
}

/// Serialize NLI scores to the required GRAPH_EDGES metadata JSON format.
///
/// Uses `serde_json::to_string` to prevent SQL injection via string concatenation.
pub(crate) fn format_nli_metadata(scores: &NliScores) -> String {
    // Output: '{"nli_entailment": 0.85, "nli_contradiction": 0.07}'
    // Required fields per AC-11: nli_entailment and nli_contradiction (f32).
    serde_json::json!({
        "nli_entailment":    scores.entailment,
        "nli_contradiction": scores.contradiction,
    })
    .to_string()
}

/// Current Unix timestamp in seconds.
pub(crate) fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use unimatrix_embed::NliScores;

    use super::*;

    // ---------------------------------------------------------------------------
    // Unit tests: format_nli_metadata (AC-11)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_format_nli_metadata_contains_required_keys() {
        let scores = NliScores {
            entailment: 0.85,
            neutral: 0.08,
            contradiction: 0.07,
        };
        let json = format_nli_metadata(&scores);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed["nli_entailment"].is_number(),
            "must have nli_entailment"
        );
        assert!(
            parsed["nli_contradiction"].is_number(),
            "must have nli_contradiction"
        );
        let e = parsed["nli_entailment"].as_f64().unwrap();
        let c = parsed["nli_contradiction"].as_f64().unwrap();
        assert!(
            (e - 0.85f64).abs() < 1e-3,
            "entailment {e} not close to 0.85"
        );
        assert!(
            (c - 0.07f64).abs() < 1e-3,
            "contradiction {c} not close to 0.07"
        );
    }

    #[test]
    fn test_format_nli_metadata_is_valid_json() {
        let scores = NliScores {
            entailment: 0.7,
            neutral: 0.2,
            contradiction: 0.1,
        };
        let json = format_nli_metadata(&scores);
        let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(result.is_ok(), "metadata must be valid JSON: {json}");
    }
}
