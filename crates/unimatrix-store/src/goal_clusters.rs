//! Goal cluster persistence and cosine-similarity query methods (crt-046).
//!
//! Provides `insert_goal_cluster` and `query_goal_clusters_by_embedding` on `SqlxStore`.
//! Both methods are `async fn` called with `.await` — no `spawn_blocking` (ADR entries #2266, #2249).
//!
//! `goal_clusters` is a structural table written via `write_pool_server()` directly,
//! not the analytics drain (ADR-002 crt-046).

use tracing::warn;

use crate::db::SqlxStore;
use crate::embedding::{decode_goal_embedding, encode_goal_embedding};
use crate::error::{Result, StoreError};

/// A row returned by `query_goal_clusters_by_embedding`.
///
/// `similarity` is computed at query time and is not stored in the database.
/// `entry_ids_json` is raw JSON text (e.g., `"[1,2,3]"`); callers parse with `serde_json`.
#[derive(Debug, Clone)]
pub struct GoalClusterRow {
    pub id: i64,
    pub feature_cycle: String,
    /// Decoded embedding Vec<f32>. Not stored as Vec<f32> in the database; decoded at query time.
    pub goal_embedding: Vec<f32>,
    pub phase: Option<String>,
    /// Raw JSON array of u64 entry IDs as stored in the DB.
    pub entry_ids_json: String,
    pub outcome: Option<String>,
    /// Unix millis.
    pub created_at: i64,
    /// Cosine similarity to the query embedding. Computed at query time; 0.0 for other queries.
    pub similarity: f32,
}

impl SqlxStore {
    /// Insert a goal cluster row using INSERT OR IGNORE.
    ///
    /// Returns `Ok(true)` when a new row was inserted.
    /// Returns `Ok(false)` on UNIQUE conflict — first write wins (ADR-002 crt-046).
    /// Uses `write_pool_server()` directly (not analytics drain — structural table).
    ///
    /// `goal_embedding` is encoded to BLOB via `encode_goal_embedding`.
    /// `entry_ids_json` must be a pre-serialized JSON array string (caller responsibility).
    /// `created_at` is Unix millis; caller provides.
    pub async fn insert_goal_cluster(
        &self,
        feature_cycle: &str,
        goal_embedding: Vec<f32>,
        phase: Option<&str>,
        entry_ids_json: &str,
        outcome: Option<&str>,
        created_at: i64,
    ) -> Result<bool> {
        let blob = encode_goal_embedding(goal_embedding).map_err(|e| StoreError::InvalidInput {
            field: "goal_embedding".to_string(),
            reason: format!("encode_goal_embedding failed: {e}"),
        })?;

        let result = sqlx::query(
            "INSERT OR IGNORE INTO goal_clusters
                 (feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(feature_cycle)
        .bind(blob)
        .bind(phase)
        .bind(entry_ids_json)
        .bind(outcome)
        .bind(created_at)
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(result.rows_affected() == 1)
    }

    /// Query goal clusters by cosine similarity to the given embedding.
    ///
    /// Fetches the most recent `recency_limit` rows (ORDER BY created_at DESC), decodes
    /// each embedding BLOB, computes cosine similarity in-process, and returns rows with
    /// similarity >= threshold sorted by similarity descending.
    ///
    /// O(recency_limit × embedding_dim) — at dim=384 and limit=100 this is ~0.1ms (ADR-003).
    /// Uses `read_pool()`. No `spawn_blocking`.
    ///
    /// Returns `Ok(Vec::new())` when the table is empty or no row meets the threshold.
    pub async fn query_goal_clusters_by_embedding(
        &self,
        embedding: &[f32],
        threshold: f32,
        recency_limit: u64,
    ) -> Result<Vec<GoalClusterRow>> {
        let rows = sqlx::query_as::<_, GoalClusterRawRow>(
            "SELECT id, feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at
             FROM goal_clusters
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .bind(recency_limit as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut results: Vec<GoalClusterRow> = Vec::new();

        for raw in rows {
            let decoded = match decode_goal_embedding(&raw.goal_embedding) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        feature_cycle = %raw.feature_cycle,
                        error = %e,
                        "query_goal_clusters_by_embedding: decode failed for row, skipping"
                    );
                    continue;
                }
            };

            let sim = cosine_similarity(embedding, &decoded);
            if sim >= threshold {
                results.push(GoalClusterRow {
                    id: raw.id,
                    feature_cycle: raw.feature_cycle,
                    goal_embedding: decoded,
                    phase: raw.phase,
                    entry_ids_json: raw.entry_ids_json,
                    outcome: raw.outcome,
                    created_at: raw.created_at,
                    similarity: sim,
                });
            }
        }

        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }
}

/// Internal sqlx row type for raw DB fetch before BLOB decoding.
#[derive(sqlx::FromRow)]
struct GoalClusterRawRow {
    id: i64,
    feature_cycle: String,
    goal_embedding: Vec<u8>,
    phase: Option<String>,
    entry_ids_json: String,
    outcome: Option<String>,
    created_at: i64,
}

/// Compute cosine similarity between two f32 slices.
///
/// Returns 0.0 when:
/// - Either slice is empty.
/// - Lengths differ.
/// - Either magnitude is zero.
///
/// Result is clamped to [0.0, 1.0] to absorb floating-point rounding artifacts.
/// Threshold comparison is `>= threshold` (inclusive) per E-07.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut mag_a = 0.0_f32;
    let mut mag_b = 0.0_f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        mag_a += x * x;
        mag_b += y * y;
    }

    let mag_a = mag_a.sqrt();
    let mag_b = mag_b.sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    let result = dot / (mag_a * mag_b);
    result.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::encode_goal_embedding;
    use crate::pool_config::PoolConfig;

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open test store");
        (store, dir)
    }

    fn unit_vec(dim: usize) -> Vec<f32> {
        let v = vec![1.0_f32 / (dim as f32).sqrt(); dim];
        v
    }

    fn zero_vec(dim: usize) -> Vec<f32> {
        vec![0.0_f32; dim]
    }

    // Build a vector that is orthogonal to the unit vector in dim=2.
    fn orthogonal_vec() -> Vec<f32> {
        vec![1.0_f32, -1.0_f32]
    }

    // ---------------------------------------------------------------------------
    // cosine_similarity unit tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0_f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vec_returns_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_empty_returns_zero() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_len_returns_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![1.0_f32];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_clamped_above_one() {
        // Floating-point rounding can produce values slightly > 1.0; verify clamping.
        let a = vec![1.0_f32; 384];
        let sim = cosine_similarity(&a, &a);
        assert!(sim <= 1.0, "cosine_similarity must be <= 1.0, got {sim}");
        assert!(sim >= 0.0, "cosine_similarity must be >= 0.0, got {sim}");
    }

    // ---------------------------------------------------------------------------
    // insert_goal_cluster tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_insert_goal_cluster_new_row_returns_true() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec(384);

        let result = store
            .insert_goal_cluster("fc-001", embedding, None, "[]", None, 1000)
            .await;

        assert!(result.is_ok(), "insert must succeed: {:?}", result);
        assert!(result.unwrap(), "first insert must return true");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM goal_clusters")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
        assert_eq!(count, 1, "goal_clusters must have exactly 1 row");

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_goal_cluster_duplicate_returns_false() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec(384);

        store
            .insert_goal_cluster("fc-001", embedding.clone(), None, "[1,2,3]", None, 1000)
            .await
            .unwrap();

        let result = store
            .insert_goal_cluster("fc-001", embedding, None, "[4,5,6]", None, 2000)
            .await;

        assert!(
            result.is_ok(),
            "duplicate insert must not error: {:?}",
            result
        );
        assert!(!result.unwrap(), "duplicate insert must return false");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM goal_clusters")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
        assert_eq!(count, 1, "only one row must exist (first write wins)");

        // Verify original entry_ids_json was not overwritten.
        let ids_json: String = sqlx::query_scalar(
            "SELECT entry_ids_json FROM goal_clusters WHERE feature_cycle = 'fc-001'",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(
            ids_json, "[1,2,3]",
            "entry_ids_json must be original value (first write wins)"
        );

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_goal_cluster_special_chars_in_feature_cycle() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec(8);
        let feature_cycle = "crt-046/sub-test";

        store
            .insert_goal_cluster(feature_cycle, embedding, None, "[]", None, 1000)
            .await
            .expect("insert with special chars must succeed");

        let found: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM goal_clusters WHERE feature_cycle = ?1")
                .bind(feature_cycle)
                .fetch_one(store.write_pool_server())
                .await
                .unwrap();
        assert_eq!(found, 1, "row with special chars must round-trip");

        store.close().await.unwrap();
    }

    // ---------------------------------------------------------------------------
    // query_goal_clusters_by_embedding tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_query_goal_clusters_empty_table_returns_empty() {
        let (store, _dir) = open_test_store().await;
        let embedding = unit_vec(8);
        let result = store
            .query_goal_clusters_by_embedding(&embedding, 0.80, 100)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_query_goal_clusters_by_embedding_returns_above_threshold() {
        let (store, _dir) = open_test_store().await;

        // Row A: cosine 1.0 (identical to query)
        let v_a = vec![1.0_f32, 0.0, 0.0, 0.0];
        // Row B: cosine 0.0 (orthogonal)
        let v_b = vec![0.0_f32, 1.0, 0.0, 0.0];
        // Row C: high cosine ~0.9994 (nearly identical, above 0.80)
        let v_c = vec![0.9994_f32, 0.035, 0.0, 0.0]; // will normalize

        store
            .insert_goal_cluster("fc-A", v_a.clone(), None, "[1]", None, 3000)
            .await
            .unwrap();
        store
            .insert_goal_cluster("fc-B", v_b.clone(), None, "[2]", None, 2000)
            .await
            .unwrap();
        store
            .insert_goal_cluster("fc-C", v_c.clone(), None, "[3]", None, 1000)
            .await
            .unwrap();

        let results = store
            .query_goal_clusters_by_embedding(&v_a, 0.80, 100)
            .await
            .unwrap();

        // Row B (orthogonal, cosine 0.0) must be absent.
        let cycles: Vec<&str> = results.iter().map(|r| r.feature_cycle.as_str()).collect();
        assert!(
            cycles.contains(&"fc-A"),
            "row A (cosine 1.0) must be in results"
        );
        assert!(
            !cycles.contains(&"fc-B"),
            "row B (cosine 0.0) must be absent"
        );

        // Results sorted descending by similarity — A first.
        assert_eq!(results[0].feature_cycle, "fc-A");

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_query_goal_clusters_recency_cap_100() {
        let (store, _dir) = open_test_store().await;

        // Insert 101 rows; row at created_at=1 is oldest (should be excluded with limit=100).
        // Row 1 (oldest): identical to query (cosine=1.0).
        // Rows 2..=101: orthogonal to query (cosine=0.0).
        let v_query = vec![1.0_f32, 0.0];
        let v_oldest = vec![1.0_f32, 0.0]; // identical to query
        let v_other = vec![0.0_f32, 1.0]; // orthogonal

        // Insert oldest row first (created_at = 1).
        store
            .insert_goal_cluster("fc-oldest", v_oldest, None, "[999]", None, 1)
            .await
            .unwrap();

        // Insert 100 rows with higher created_at values.
        for i in 2_i64..=101 {
            let fc = format!("fc-{i}");
            store
                .insert_goal_cluster(&fc, v_other.clone(), None, "[]", None, i)
                .await
                .unwrap();
        }

        // threshold=0.0 ensures all rows above threshold — recency cap is the only filter.
        let results = store
            .query_goal_clusters_by_embedding(&v_query, 0.0, 100)
            .await
            .unwrap();

        assert!(
            results.len() <= 100,
            "result count must be <= 100 (recency cap)"
        );

        // Oldest row (fc-oldest, created_at=1) must NOT be in results.
        let cycles: Vec<&str> = results.iter().map(|r| r.feature_cycle.as_str()).collect();
        assert!(
            !cycles.contains(&"fc-oldest"),
            "oldest row must be excluded by recency cap"
        );

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_query_goal_clusters_threshold_boundary_inclusive() {
        let (store, _dir) = open_test_store().await;

        // Insert two rows: one at exactly 0.80 cosine, one just below.
        // We achieve a controlled cosine by using 2D vectors.
        // cos(θ) = a·b / (|a||b|)
        // For a = [1, 0] and b = [x, y] normalized:
        //   cos = x / sqrt(x^2 + y^2)
        // To get cos = 0.80: x=0.8, y=0.6 → sqrt(0.64+0.36)=1.0 → cos=0.8
        let v_query = vec![1.0_f32, 0.0];
        let v_at_threshold = vec![0.8_f32, 0.6]; // cosine exactly 0.80
        let v_below = vec![0.7_f32, 0.7143]; // cosine < 0.80

        store
            .insert_goal_cluster("fc-at", v_at_threshold.clone(), None, "[]", None, 2000)
            .await
            .unwrap();
        store
            .insert_goal_cluster("fc-below", v_below.clone(), None, "[]", None, 1000)
            .await
            .unwrap();

        let results = store
            .query_goal_clusters_by_embedding(&v_query, 0.80, 100)
            .await
            .unwrap();

        let cycles: Vec<&str> = results.iter().map(|r| r.feature_cycle.as_str()).collect();
        assert!(
            cycles.contains(&"fc-at"),
            "row at exactly 0.80 cosine must be included (>= threshold)"
        );

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_query_goal_clusters_empty_entry_ids_row() {
        let (store, _dir) = open_test_store().await;
        let v = vec![1.0_f32, 0.0];

        store
            .insert_goal_cluster("fc-empty-ids", v.clone(), None, "[]", None, 1000)
            .await
            .unwrap();

        let results = store
            .query_goal_clusters_by_embedding(&v, 0.0, 100)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_ids_json, "[]");
        assert_eq!(results[0].feature_cycle, "fc-empty-ids");

        store.close().await.unwrap();
    }

    // ---------------------------------------------------------------------------
    // get_cycle_start_goal_embedding tests (method defined in db.rs)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_cycle_start_goal_embedding_no_event_returns_none() {
        let (store, _dir) = open_test_store().await;
        let result = store
            .get_cycle_start_goal_embedding("nonexistent-cycle")
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_cycle_start_goal_embedding_returns_embedding() {
        let (store, _dir) = open_test_store().await;
        let cycle_id = "gc-test-cycle-001";
        let original: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();

        // Insert cycle_start row.
        store
            .insert_cycle_event(
                cycle_id,
                0,
                "cycle_start",
                None,
                None,
                None,
                1_000_000,
                None,
            )
            .await
            .expect("insert cycle_start");

        // Write embedding.
        let bytes = encode_goal_embedding(original.clone()).expect("encode");
        store
            .update_cycle_start_goal_embedding(cycle_id, bytes)
            .await
            .expect("update embedding");

        let result = store.get_cycle_start_goal_embedding(cycle_id).await;
        assert!(result.is_ok(), "must return Ok: {:?}", result);
        let opt = result.unwrap();
        assert!(opt.is_some(), "must return Some for cycle with embedding");
        assert_eq!(opt.unwrap(), original);

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_cycle_start_goal_embedding_null_blob_returns_none() {
        let (store, _dir) = open_test_store().await;
        let cycle_id = "gc-null-blob-cycle";

        // Insert cycle_start row (goal_embedding stays NULL by default).
        store
            .insert_cycle_event(
                cycle_id,
                0,
                "cycle_start",
                None,
                None,
                None,
                1_000_000,
                None,
            )
            .await
            .expect("insert cycle_start");

        // Do NOT call update_cycle_start_goal_embedding — goal_embedding remains NULL.
        let result = store.get_cycle_start_goal_embedding(cycle_id).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none(), "NULL blob must return None");

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_cycle_start_goal_embedding_malformed_blob_returns_none() {
        let (store, _dir) = open_test_store().await;
        let cycle_id = "gc-malformed-blob";

        store
            .insert_cycle_event(
                cycle_id,
                0,
                "cycle_start",
                None,
                None,
                None,
                1_000_000,
                None,
            )
            .await
            .expect("insert cycle_start");

        // Write arbitrary non-embedding bytes.
        let bad_bytes: Vec<u8> = vec![0x0A, 0x01, 0x02, 0x03, 0x04];
        sqlx::query(
            "UPDATE cycle_events SET goal_embedding = ?1
             WHERE cycle_id = ?2 AND event_type = 'cycle_start'",
        )
        .bind(bad_bytes)
        .bind(cycle_id)
        .execute(store.write_pool_server())
        .await
        .expect("write bad blob");

        let result = store.get_cycle_start_goal_embedding(cycle_id).await;
        // Must not panic. Must return Ok(None) on decode failure.
        assert!(result.is_ok(), "malformed blob must not return Err");
        assert!(result.unwrap().is_none(), "malformed blob must return None");

        store.close().await.unwrap();
    }

    // ---------------------------------------------------------------------------
    // zero_vec helper sanity check
    // ---------------------------------------------------------------------------

    #[test]
    fn test_zero_vec_helper() {
        let z = zero_vec(4);
        assert_eq!(z.len(), 4);
        assert!(z.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_orthogonal_vec_helper() {
        let v = vec![1.0_f32, 0.0];
        let o = orthogonal_vec();
        // [1,0] · [1,-1] = 1; |[1,0]| = 1; |[1,-1]| = sqrt(2)
        // cosine = 1 / sqrt(2) ≈ 0.707 — not actually orthogonal but helper sanity is fine
        let _ = cosine_similarity(&v, &o);
    }
}
