//! Cycle review index persistence for crt-033.
//!
//! Provides memoized storage for `context_cycle_review` results.
//! The `cycle_review_index` table is a keyed archive (one row per feature_cycle)
//! allowing idempotent retrospective report retrieval and a purge gate for GH #409.
//!
//! All read methods use `read_pool()`. The write method uses `write_pool_server()`
//! directly in the caller's async context — MUST NOT be called from `spawn_blocking`
//! (ADR-001, entries #2266, #2249).

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Unified schema version covering both `RetrospectiveReport` serialization
/// format and hotspot detection rule logic.
///
/// Defined here only — no import from `unimatrix-observe`, no definition in
/// `tools.rs` (C-04, FR-12, ADR-002).
///
/// Bump policy:
///   - Bump when any field on `RetrospectiveReport` or a nested type changes
///     JSON round-trip fidelity (add, remove, rename).
///   - Bump when any hotspot detection rule in `unimatrix-observe` changes logic.
///   - Do NOT bump for threshold-only changes that leave stored results valid.
pub const SUMMARY_SCHEMA_VERSION: u32 = 1;

/// 4MB ceiling for stored `summary_json` (NFR-03).
const SUMMARY_JSON_MAX_BYTES: usize = 4 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single row from the `cycle_review_index` table.
///
/// Stores the full memoized `RetrospectiveReport` JSON for a feature cycle.
/// `raw_signals_available` is `i32` (not `bool`) to match sqlx's SQLite
/// INTEGER→i32 binding. Consumers that need bool semantics use
/// `record.raw_signals_available != 0`.
#[derive(Debug, Clone)]
pub struct CycleReviewRecord {
    /// Primary key — matches `cycle_events.cycle_id`.
    pub feature_cycle: String,
    /// `SUMMARY_SCHEMA_VERSION` at compute time. Used to detect stale records.
    pub schema_version: u32,
    /// Unix timestamp seconds when this record was computed.
    pub computed_at: i64,
    /// SQLite INTEGER: 1 = live signals present, 0 = signals purged (GH #409).
    pub raw_signals_available: i32,
    /// Full `RetrospectiveReport` JSON. No `evidence_limit` truncation applied.
    pub summary_json: String,
}

// ---------------------------------------------------------------------------
// Store methods
// ---------------------------------------------------------------------------

impl SqlxStore {
    /// Look up a stored cycle review by `feature_cycle`.
    ///
    /// Uses `read_pool()` — read-only query, no write contention (entry #3619).
    ///
    /// Returns `None` if no row exists for the given `feature_cycle`.
    /// Returns `Err` only on genuine SQL infrastructure failure.
    ///
    /// On `Err` at the call site: treat as a cache miss (fall through to full
    /// pipeline computation). Do NOT abort the handler on a read failure.
    pub async fn get_cycle_review(&self, feature_cycle: &str) -> Result<Option<CycleReviewRecord>> {
        let row = sqlx::query(
            "SELECT feature_cycle, schema_version, computed_at, \
                    raw_signals_available, summary_json \
             FROM cycle_review_index \
             WHERE feature_cycle = ?1",
        )
        .bind(feature_cycle)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        match row {
            None => Ok(None),
            Some(r) => Ok(Some(CycleReviewRecord {
                feature_cycle: r.get::<String, _>(0),
                schema_version: r.get::<i64, _>(1) as u32,
                computed_at: r.get::<i64, _>(2),
                raw_signals_available: r.get::<i32, _>(3),
                summary_json: r.get::<String, _>(4),
            })),
        }
    }

    /// Write or overwrite a cycle review record.
    ///
    /// Uses `write_pool_server()` directly in the caller's async context.
    /// MUST NOT be called from `spawn_blocking` — sqlx async queries require an
    /// async context; `block_in_place` risks pool starvation (ADR-001, #2266, #2249).
    ///
    /// Uses `INSERT OR REPLACE` to support both first-call writes and `force=true`
    /// overwrites (FR-03, FR-04).
    ///
    /// Enforces the 4MB ceiling on `summary_json` before any DB call (NFR-03).
    /// Returns `Err` (not panic) when the ceiling is exceeded.
    pub async fn store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()> {
        // 4MB ceiling check (NFR-03). Return Err, not panic.
        if record.summary_json.len() > SUMMARY_JSON_MAX_BYTES {
            return Err(StoreError::InvalidInput {
                field: "summary_json".to_string(),
                reason: format!(
                    "summary_json exceeds 4MB ceiling ({} bytes)",
                    record.summary_json.len()
                ),
            });
        }

        // Acquire write connection from write_pool_server().
        // This is a direct pool acquire — not spawn_blocking, not block_in_place.
        // The handler's async context drives the await (ADR-001).
        let mut conn = self
            .write_pool_server()
            .acquire()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        sqlx::query(
            "INSERT OR REPLACE INTO cycle_review_index \
                 (feature_cycle, schema_version, computed_at, \
                  raw_signals_available, summary_json) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&record.feature_cycle)
        .bind(record.schema_version as i64)
        .bind(record.computed_at)
        .bind(record.raw_signals_available)
        .bind(&record.summary_json)
        .execute(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(())
    }

    /// Return cycle IDs that have a `cycle_start` event in the K-window but no
    /// stored review in `cycle_review_index`.
    ///
    /// Uses `read_pool()` — read-only set-difference query (ADR-004, entry #3619).
    /// Pre-`cycle_events` cycles (no `cycle_events` rows) are excluded by definition.
    /// `SELECT DISTINCT` prevents duplicates when multiple `cycle_start` events
    /// exist for the same `cycle_id` (RISK-TEST-STRATEGY edge case).
    ///
    /// `k_window_cutoff`: unix timestamp seconds = `now - PENDING_REVIEWS_K_WINDOW_SECS`.
    /// Cycles with `cycle_start.timestamp < k_window_cutoff` are excluded.
    pub async fn pending_cycle_reviews(&self, k_window_cutoff: i64) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT DISTINCT ce.cycle_id \
             FROM cycle_events ce \
             WHERE ce.event_type = 'cycle_start' \
               AND ce.timestamp >= ?1 \
               AND ce.cycle_id NOT IN (SELECT feature_cycle FROM cycle_review_index) \
             ORDER BY ce.cycle_id",
        )
        .bind(k_window_cutoff)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let cycle_ids: Vec<String> = rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        Ok(cycle_ids)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::open_test_store;

    // -----------------------------------------------------------------------
    // CRS-U-02: SUMMARY_SCHEMA_VERSION is 1
    // -----------------------------------------------------------------------

    #[test]
    fn test_summary_schema_version_is_one() {
        assert_eq!(
            SUMMARY_SCHEMA_VERSION, 1u32,
            "SUMMARY_SCHEMA_VERSION must be 1"
        );
    }

    // -----------------------------------------------------------------------
    // CRS-U-03: 4MB ceiling — over limit returns Err (not panic)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_cycle_review_4mb_ceiling_exceeded() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let record = CycleReviewRecord {
            feature_cycle: "crt-033-ceiling-test".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            // One byte over the 4MB ceiling
            summary_json: "x".repeat(SUMMARY_JSON_MAX_BYTES + 1),
        };

        let result = store.store_cycle_review(&record).await;
        assert!(
            result.is_err(),
            "store_cycle_review must return Err when summary_json exceeds 4MB"
        );

        // Verify it is the expected variant
        match result.unwrap_err() {
            StoreError::InvalidInput { field, reason } => {
                assert_eq!(field, "summary_json");
                assert!(
                    reason.contains("4MB ceiling"),
                    "error reason must mention 4MB ceiling, got: {reason}"
                );
            }
            other => panic!("expected InvalidInput, got: {other}"),
        }

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-U-04: 4MB ceiling — exactly at limit returns Ok
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_cycle_review_4mb_ceiling_boundary() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let record = CycleReviewRecord {
            feature_cycle: "crt-033-boundary-test".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            // Exactly at the 4MB ceiling
            summary_json: "x".repeat(SUMMARY_JSON_MAX_BYTES),
        };

        let result = store.store_cycle_review(&record).await;
        assert!(
            result.is_ok(),
            "store_cycle_review must return Ok when summary_json is exactly 4MB"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-01: get_cycle_review returns None for missing feature_cycle
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_cycle_review_missing_returns_none() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let result = store.get_cycle_review("nonexistent-cycle").await;
        assert!(result.is_ok(), "get_cycle_review must not error on miss");
        assert!(
            result.unwrap().is_none(),
            "get_cycle_review must return None for unknown feature_cycle"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-02: store then get returns identical record (round-trip)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_and_get_cycle_review_round_trip() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let record = CycleReviewRecord {
            feature_cycle: "crt-033-round-trip".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            summary_json: r#"{"feature_cycle":"crt-033-round-trip"}"#.to_string(),
        };

        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        let fetched = store
            .get_cycle_review(&record.feature_cycle)
            .await
            .expect("get must not error")
            .expect("get must return Some after store");

        assert_eq!(fetched.feature_cycle, record.feature_cycle);
        assert_eq!(fetched.schema_version, SUMMARY_SCHEMA_VERSION);
        assert_eq!(fetched.computed_at, record.computed_at);
        assert_eq!(fetched.raw_signals_available, 1);
        assert_eq!(fetched.summary_json, record.summary_json);

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // raw_signals_available = 0 round-trip (confirms i32 binding)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_raw_signals_available_zero_round_trip() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let record = CycleReviewRecord {
            feature_cycle: "crt-033-purged".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_700_000_001,
            raw_signals_available: 0,
            summary_json: r#"{"status":"purged"}"#.to_string(),
        };

        store
            .store_cycle_review(&record)
            .await
            .expect("store with raw_signals_available=0 must succeed");

        let fetched = store
            .get_cycle_review("crt-033-purged")
            .await
            .expect("get must not error")
            .expect("must return Some");

        assert_eq!(
            fetched.raw_signals_available, 0,
            "raw_signals_available=0 must round-trip as 0 (sqlx INTEGER→i32 binding)"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-03: INSERT OR REPLACE overwrites prior record
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_cycle_review_overwrites_prior() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let v1 = CycleReviewRecord {
            feature_cycle: "crt-033-overwrite".to_string(),
            schema_version: 1,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            summary_json: r#"{"version":1}"#.to_string(),
        };

        store.store_cycle_review(&v1).await.expect("first store");

        let v2 = CycleReviewRecord {
            feature_cycle: "crt-033-overwrite".to_string(),
            schema_version: 2,
            computed_at: 1_700_000_100,
            raw_signals_available: 1,
            summary_json: r#"{"version":2}"#.to_string(),
        };

        store.store_cycle_review(&v2).await.expect("second store");

        let fetched = store
            .get_cycle_review("crt-033-overwrite")
            .await
            .expect("get must not error")
            .expect("must return Some");

        assert_eq!(
            fetched.computed_at, 1_700_000_100,
            "INSERT OR REPLACE must overwrite prior record — computed_at must be T2"
        );
        assert_eq!(fetched.schema_version, 2);
        assert_eq!(fetched.summary_json, r#"{"version":2}"#);

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-04: pending_cycle_reviews — happy path set-difference
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_returns_unreviewed_cycles() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let one_day = 86_400_i64;
        let two_days = 2 * one_day;
        let ninety_days = 90 * one_day;

        // Insert two cycle_start events within the K-window
        store
            .insert_cycle_event(
                "feat-A",
                0,
                "cycle_start",
                None,
                None,
                None,
                now - one_day,
                None,
            )
            .await
            .expect("insert feat-A");
        store
            .insert_cycle_event(
                "feat-B",
                0,
                "cycle_start",
                None,
                None,
                None,
                now - two_days,
                None,
            )
            .await
            .expect("insert feat-B");

        // Store a review for feat-A only
        let review_a = CycleReviewRecord {
            feature_cycle: "feat-A".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: now,
            raw_signals_available: 1,
            summary_json: r#"{"reviewed":true}"#.to_string(),
        };
        store
            .store_cycle_review(&review_a)
            .await
            .expect("store feat-A review");

        // K-window cutoff = now - 90 days
        let cutoff = now - ninety_days;
        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert_eq!(
            pending,
            vec!["feat-B".to_string()],
            "feat-A has a review; only feat-B must be in pending list"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-05: pending_cycle_reviews — empty when all reviewed
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_empty_when_all_reviewed() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let one_day = 86_400_i64;
        let ninety_days = 90 * one_day;

        store
            .insert_cycle_event(
                "rev-A",
                0,
                "cycle_start",
                None,
                None,
                None,
                now - one_day,
                None,
            )
            .await
            .expect("insert rev-A");
        store
            .insert_cycle_event(
                "rev-B",
                0,
                "cycle_start",
                None,
                None,
                None,
                now - one_day,
                None,
            )
            .await
            .expect("insert rev-B");

        for fc in &["rev-A", "rev-B"] {
            let r = CycleReviewRecord {
                feature_cycle: fc.to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: now,
                raw_signals_available: 1,
                summary_json: r#"{}"#.to_string(),
            };
            store.store_cycle_review(&r).await.expect("store review");
        }

        let cutoff = now - ninety_days;
        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert!(
            pending.is_empty(),
            "all cycles have reviews — pending list must be empty"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-06: pending_cycle_reviews excludes cycles outside K-window
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_excludes_outside_k_window() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let one_day = 86_400_i64;
        let ninety_days = 90 * one_day;

        // 91 days ago — outside the 90-day K-window
        let old_ts = now - (91 * one_day);
        store
            .insert_cycle_event(
                "old-cycle",
                0,
                "cycle_start",
                None,
                None,
                None,
                old_ts,
                None,
            )
            .await
            .expect("insert old-cycle");

        let cutoff = now - ninety_days;
        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert!(
            pending.is_empty(),
            "old-cycle is outside the K-window — must not be in pending list"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-07: pending_cycle_reviews excludes cycles with only cycle_end events
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_excludes_cycle_end_only() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let one_day = 86_400_i64;
        let ninety_days = 90 * one_day;

        store
            .insert_cycle_event(
                "end-only-cycle",
                0,
                "cycle_end",
                None,
                None,
                None,
                now - one_day,
                None,
            )
            .await
            .expect("insert cycle_end event");

        let cutoff = now - ninety_days;
        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert!(
            pending.is_empty(),
            "cycle_end-only event must not qualify — only cycle_start events count"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-08: pending_cycle_reviews K-window boundary is inclusive
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_boundary_is_inclusive() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let ninety_days = 90 * 86_400_i64;

        // Timestamp exactly equals the cutoff
        let cutoff = now - ninety_days;
        store
            .insert_cycle_event(
                "boundary-cycle",
                0,
                "cycle_start",
                None,
                None,
                None,
                cutoff,
                None,
            )
            .await
            .expect("insert boundary-cycle");

        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert_eq!(
            pending,
            vec!["boundary-cycle".to_string()],
            "timestamp == cutoff must be inclusive (>= condition)"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-09: pending_cycle_reviews DISTINCT on multiple cycle_start events
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pending_cycle_reviews_distinct_on_cycle_id() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let one_day = 86_400_i64;
        let ninety_days = 90 * one_day;

        // Three cycle_start events for the same cycle_id
        for seq in 0..3_i64 {
            store
                .insert_cycle_event(
                    "dup-cycle",
                    seq,
                    "cycle_start",
                    None,
                    None,
                    None,
                    now - one_day,
                    None,
                )
                .await
                .expect("insert dup-cycle event");
        }

        let cutoff = now - ninety_days;
        let pending = store
            .pending_cycle_reviews(cutoff)
            .await
            .expect("pending_cycle_reviews must not error");

        assert_eq!(
            pending.len(),
            1,
            "DISTINCT must collapse three cycle_start events for the same cycle_id to one entry"
        );
        assert_eq!(pending[0], "dup-cycle");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-I-10: concurrent store for same cycle — last writer wins, no error
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_concurrent_store_same_cycle_last_writer_wins() {
        use std::sync::Arc;

        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = Arc::new(open_test_store(&dir).await);

        let store_a = Arc::clone(&store);
        let store_b = Arc::clone(&store);

        let (r1, r2) = tokio::join!(
            async move {
                store_a
                    .store_cycle_review(&CycleReviewRecord {
                        feature_cycle: "concurrent-cycle".to_string(),
                        schema_version: SUMMARY_SCHEMA_VERSION,
                        computed_at: 1_000,
                        raw_signals_available: 1,
                        summary_json: r#"{"writer":"A"}"#.to_string(),
                    })
                    .await
            },
            async move {
                store_b
                    .store_cycle_review(&CycleReviewRecord {
                        feature_cycle: "concurrent-cycle".to_string(),
                        schema_version: SUMMARY_SCHEMA_VERSION,
                        computed_at: 2_000,
                        raw_signals_available: 1,
                        summary_json: r#"{"writer":"B"}"#.to_string(),
                    })
                    .await
            }
        );

        // Both must succeed (no error, no panic)
        assert!(r1.is_ok(), "concurrent write A must not error: {r1:?}");
        assert!(r2.is_ok(), "concurrent write B must not error: {r2:?}");

        // Exactly one row must exist
        let fetched = store
            .get_cycle_review("concurrent-cycle")
            .await
            .expect("get must not error")
            .expect("must have exactly one row");

        // The row must be valid (one of the two writers won)
        assert!(
            fetched.computed_at == 1_000 || fetched.computed_at == 2_000,
            "last writer wins — row must be from one of the two writers"
        );

        Arc::try_unwrap(store)
            .expect("no other Arc refs")
            .close()
            .await
            .unwrap();
    }

}
