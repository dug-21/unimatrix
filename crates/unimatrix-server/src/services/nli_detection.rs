//! NLI graph edge helpers — shared utilities for NLI edge writes and metadata.
//!
//! This module provides pub(crate) helpers consumed by `nli_detection_tick.rs`:
//! `write_nli_edge`, `write_graph_edge`, `format_nli_metadata`, and `current_timestamp_secs`.
//!
//! `write_nli_edge`: hardcodes source='nli'; used by Path A and Path B callers.
//! `write_graph_edge`: accepts source as a parameter; used by Path C (crt-040) and
//!   future edge signal sources. Adding a new source: call `write_graph_edge` with the
//!   corresponding `EDGE_SOURCE_*` constant from `unimatrix-store`. Do NOT add source
//!   parameters to `write_nli_edge` (pattern #4025).
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

/// Write a single graph edge via `write_pool_server()` with a caller-supplied source tag.
///
/// This is the generalized sibling of `write_nli_edge`. Path C (cosine Supports) calls
/// this with `source = EDGE_SOURCE_COSINE_SUPPORTS`. Future edge signal origins should
/// also call this function with their own `EDGE_SOURCE_*` constant rather than
/// parameterizing `write_nli_edge` (pattern #4025, ADR-001 crt-040).
///
/// Uses `INSERT OR IGNORE` for idempotency on `UNIQUE(source_id, target_id, relation_type)`.
///
/// # Return value
/// - `true`  — row was inserted (`rows_affected = 1`)
/// - `false` — UNIQUE conflict (`rows_affected = 0`): silent, no `warn!`
/// - `false` — SQL error: emits `warn!` with structured context, does not propagate
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0, ?7)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(weight as f64)
    .bind(created_at as i64)
    .bind(source) // bound to ?6 — used for BOTH created_by and source
    .bind(metadata)
    .execute(store.write_pool_server())
    .await;

    match result {
        Ok(query_result) => query_result.rows_affected() > 0,
        Err(e) => {
            tracing::warn!(
                source_id = source_id,
                target_id = target_id,
                relation_type = relation_type,
                source = source,
                error = %e,
                "write_graph_edge: failed to write graph edge"
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

    // ---------------------------------------------------------------------------
    // Unit tests: write_graph_edge (crt-040)
    // ---------------------------------------------------------------------------

    /// TC-01: write_graph_edge writes the passed source value (AC-11, R-02).
    ///
    /// Verifies that source and created_by columns are both set to the caller-supplied
    /// source string — NOT the hardcoded 'nli' value from write_nli_edge.
    #[tokio::test]
    async fn test_write_graph_edge_writes_cosine_supports_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();

        let wrote = write_graph_edge(
            &store,
            1,
            2,
            "Supports",
            0.70,
            ts,
            "cosine_supports",
            r#"{"cosine":0.70}"#,
        )
        .await;

        assert!(
            wrote,
            "TC-01: write_graph_edge must return true on fresh insert"
        );

        let edges = store.query_graph_edges().await.unwrap();
        let row = edges
            .iter()
            .find(|e| e.source_id == 1 && e.target_id == 2 && e.relation_type == "Supports")
            .expect("TC-01: row must exist in graph_edges");

        assert_eq!(
            row.source, "cosine_supports",
            "TC-01: source must be 'cosine_supports'"
        );
        assert_eq!(
            row.created_by, "cosine_supports",
            "TC-01: created_by must equal source"
        );
        assert_eq!(
            row.relation_type, "Supports",
            "TC-01: relation_type must be 'Supports'"
        );
        assert!(
            (row.weight - 0.70_f32).abs() < 1e-5,
            "TC-01: weight={} expected≈0.70",
            row.weight
        );
    }

    /// TC-02: write_nli_edge still writes source='nli' after adding write_graph_edge (R-02).
    ///
    /// Mandatory regression guard: a refactor that accidentally routes write_nli_edge through
    /// write_graph_edge with a wrong source would compile but fail here.
    #[tokio::test]
    async fn test_write_nli_edge_still_writes_nli_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();

        let wrote = write_nli_edge(
            &store,
            10,
            20,
            "Supports",
            0.85,
            ts,
            r#"{"nli_entailment":0.85}"#,
        )
        .await;

        assert!(
            wrote,
            "TC-02: write_nli_edge must return true on fresh insert"
        );

        let edges = store.query_graph_edges().await.unwrap();
        let row = edges
            .iter()
            .find(|e| e.source_id == 10 && e.target_id == 20 && e.relation_type == "Supports")
            .expect("TC-02: row must exist in graph_edges");

        assert_eq!(
            row.source, "nli",
            "TC-02: write_nli_edge must still write source='nli'"
        );
        assert_eq!(
            row.created_by, "nli",
            "TC-02: write_nli_edge must still write created_by='nli'"
        );
    }

    /// TC-03: write_graph_edge and write_nli_edge produce distinct source values (R-02).
    ///
    /// Isolation test: both functions write different pairs; two rows with different sources.
    #[tokio::test]
    async fn test_write_graph_edge_and_write_nli_edge_distinct_sources() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();

        write_nli_edge(
            &store,
            30,
            40,
            "Informs",
            0.75,
            ts,
            r#"{"nli_entailment":0.75}"#,
        )
        .await;
        write_graph_edge(
            &store,
            50,
            60,
            "Supports",
            0.68,
            ts,
            "cosine_supports",
            r#"{"cosine":0.68}"#,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();

        let nli_row = edges
            .iter()
            .find(|e| e.source_id == 30 && e.target_id == 40)
            .expect("TC-03: nli row must exist");
        let cosine_row = edges
            .iter()
            .find(|e| e.source_id == 50 && e.target_id == 60)
            .expect("TC-03: cosine row must exist");

        assert_eq!(nli_row.source, "nli", "TC-03: nli row source must be 'nli'");
        assert_eq!(
            cosine_row.source, "cosine_supports",
            "TC-03: cosine row source must be 'cosine_supports'"
        );
    }

    /// TC-04: Second write_graph_edge call for same triple returns false (INSERT OR IGNORE).
    ///
    /// Verifies that rows_affected()=0 on a UNIQUE conflict causes write_graph_edge to
    /// return false (not true), and that the DB retains exactly one row.
    #[tokio::test]
    async fn test_write_graph_edge_duplicate_returns_false_no_warn() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();

        let first = write_graph_edge(
            &store,
            1,
            2,
            "Supports",
            0.70,
            ts,
            "cosine_supports",
            r#"{"cosine":0.70}"#,
        )
        .await;
        assert!(first, "TC-04: first write must return true");

        let second = write_graph_edge(
            &store,
            1,
            2,
            "Supports",
            0.71,
            ts,
            "cosine_supports",
            r#"{"cosine":0.71}"#,
        )
        .await;
        assert!(
            !second,
            "TC-04: duplicate write must return false (UNIQUE conflict, not an error)"
        );

        let count = store
            .query_graph_edges()
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.source_id == 1 && e.target_id == 2 && e.relation_type == "Supports")
            .count();
        assert_eq!(
            count, 1,
            "TC-04: exactly one row after duplicate write attempt"
        );
    }

    /// TC-05: write_graph_edge returns false on SQL error, does not panic (R-07 failure mode).
    ///
    /// Uses open_readonly which produces SQLITE_READONLY on any write — triggers the Err
    /// branch inside write_graph_edge, which must emit warn! and return false.
    #[tokio::test]
    async fn test_write_graph_edge_sql_error_returns_false() {
        let tmp = tempfile::TempDir::new().unwrap();
        // First open with write access to create the schema.
        let _writable = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        // Open the same DB read-only; writes will fail with SQLITE_READONLY.
        // open_test_store creates test.db under tmp.path().
        let readonly = unimatrix_store::SqlxStore::open_readonly(tmp.path().join("test.db"))
            .await
            .expect("TC-05: open_readonly must succeed");

        let ts = current_timestamp_secs();
        let result = write_graph_edge(
            &readonly,
            1,
            2,
            "Supports",
            0.70,
            ts,
            "cosine_supports",
            r#"{"cosine":0.70}"#,
        )
        .await;

        assert!(!result, "TC-05: SQL error must return false, not panic");
    }

    /// TC-06: write_graph_edge stores metadata column correctly (FR-10).
    #[tokio::test]
    async fn test_write_graph_edge_metadata_format() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();
        let expected_meta = r#"{"cosine":0.71}"#;

        write_graph_edge(
            &store,
            7,
            8,
            "Supports",
            0.71,
            ts,
            "cosine_supports",
            expected_meta,
        )
        .await;

        // query_graph_edges does not return metadata; query directly.
        let row: Option<String> = sqlx::query_scalar(
            "SELECT metadata FROM graph_edges WHERE source_id=7 AND target_id=8 \
             AND relation_type='Supports'",
        )
        .fetch_optional(store.write_pool_server())
        .await
        .unwrap();

        assert_eq!(
            row.as_deref(),
            Some(expected_meta),
            "TC-06: metadata column must contain the exact JSON string passed in"
        );
    }

    /// TC-07: write_graph_edge is generic — accepts any relation_type (FR-06).
    #[tokio::test]
    async fn test_write_graph_edge_informs_relation_type() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let ts = current_timestamp_secs();

        write_graph_edge(
            &store,
            3,
            4,
            "Informs",
            0.66,
            ts,
            "cosine_supports",
            r#"{"cosine":0.66}"#,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let row = edges
            .iter()
            .find(|e| e.source_id == 3 && e.target_id == 4)
            .expect("TC-07: row must exist");
        assert_eq!(
            row.relation_type, "Informs",
            "TC-07: relation_type must be 'Informs' as passed"
        );
    }
}
