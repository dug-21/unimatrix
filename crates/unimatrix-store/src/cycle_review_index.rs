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
///   - crt-047: bumped 1 → 2 to trigger stale-record advisory for all rows
///     written before curation health columns were added.
///   - crt-049: bumped 2 → 3; adding explicit_read_count, explicit_read_by_category,
///     and redefining total_served (search exposures no longer contribute).
pub const SUMMARY_SCHEMA_VERSION: u32 = 3;

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
///
/// The seven curation health fields (`corrections_total`, `corrections_agent`,
/// `corrections_human`, `corrections_system`, `deprecations_total`,
/// `orphan_deprecations`, `first_computed_at`) were added in crt-047 (v24).
/// Pre-v24 rows migrated by `migration.rs` will have DEFAULT 0 for all seven.
/// `Default` is derived so callers that construct the struct with partial field
/// syntax can use `..Default::default()` for the new fields.
#[derive(Debug, Clone, Default)]
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

    // --- crt-047 curation health fields (v24) ---
    /// Sum of `corrections_agent + corrections_human`. Stored as a column for
    /// baseline window queries; `corrections_system` is excluded from this total.
    pub corrections_total: i64,
    /// Count of corrections where `trust_source = 'agent'` in the cycle window.
    pub corrections_agent: i64,
    /// Count of corrections where `trust_source IN ('human', 'privileged')`.
    pub corrections_human: i64,
    /// Count of corrections for all other `trust_source` values (informational only;
    /// excluded from `corrections_total` and σ baseline computation).
    pub corrections_system: i64,
    /// All entries with `status = 'deprecated'` in the cycle window (orphan + chain).
    pub deprecations_total: i64,
    /// Entries deprecated AND `superseded_by IS NULL` in the cycle window.
    pub orphan_deprecations: i64,
    /// Unix timestamp seconds set once on the first INSERT; never updated on
    /// subsequent overwrites (ADR-001, crt-047). Pre-crt-047 rows keep DEFAULT 0
    /// after migration — do NOT "fix" this on `force=true` of historical rows.
    /// Rows with `first_computed_at = 0` are excluded from baseline window queries.
    pub first_computed_at: i64,
}

/// Slim projection from `cycle_review_index` for baseline computation.
///
/// Produced by `get_curation_baseline_window()`. `schema_version` is included
/// so callers can distinguish rows written before crt-047 (schema_version < 2,
/// all snapshot fields = DEFAULT 0) from real zero-correction cycles
/// (schema_version = 2, all zeros are genuine measured zeros).
#[derive(Debug, Clone)]
pub struct CurationBaselineRow {
    pub corrections_total: i64,
    pub corrections_agent: i64,
    pub corrections_human: i64,
    pub deprecations_total: i64,
    pub orphan_deprecations: i64,
    /// `schema_version` from the row — used to exclude legacy DEFAULT-0 rows
    /// when `schema_version < 2` AND all snapshot columns equal zero.
    /// A real zero-correction cycle at `schema_version = 2` IS included.
    pub schema_version: i64,
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
                    raw_signals_available, summary_json, \
                    corrections_total, corrections_agent, corrections_human, \
                    corrections_system, deprecations_total, orphan_deprecations, \
                    first_computed_at \
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
                corrections_total: r.get::<i64, _>(5),
                corrections_agent: r.get::<i64, _>(6),
                corrections_human: r.get::<i64, _>(7),
                corrections_system: r.get::<i64, _>(8),
                deprecations_total: r.get::<i64, _>(9),
                orphan_deprecations: r.get::<i64, _>(10),
                first_computed_at: r.get::<i64, _>(11),
            })),
        }
    }

    /// Write or overwrite a cycle review record using a two-step upsert.
    ///
    /// Uses `write_pool_server()` directly in the caller's async context.
    /// MUST NOT be called from `spawn_blocking` — sqlx async queries require an
    /// async context; `block_in_place` risks pool starvation (ADR-001, #2266, #2249).
    ///
    /// **Two-step upsert** (ADR-001, crt-047): plain `INSERT OR REPLACE` deletes
    /// then reinserts the row, which would reset `first_computed_at` to the new
    /// value on every force=true overwrite. Instead:
    ///
    /// 1. Read existing `first_computed_at` (if any).
    /// 2. No existing row: INSERT with `first_computed_at` from `record`.
    /// 3. Existing row: UPDATE all mutable columns; `first_computed_at` is
    ///    intentionally excluded from the SET clause (first write wins).
    ///
    /// Edge case: pre-crt-047 rows have `first_computed_at = 0` from migration
    /// DEFAULT 0. On force=true for historical cycles those rows keep `0` — they
    /// remain excluded from `get_curation_baseline_window()`. Do NOT overwrite
    /// `first_computed_at` with a real timestamp for these rows.
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

        // Step 1: Check whether a row already exists and read existing first_computed_at.
        // Uses the same write connection to avoid a TOCTOU race — write_pool_server
        // is a single-connection serializer, so no other writer can interleave here.
        let existing_first_computed_at: Option<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT first_computed_at FROM cycle_review_index WHERE feature_cycle = ?1",
        )
        .bind(&record.feature_cycle)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        match existing_first_computed_at {
            None => {
                // Step 2a: No existing row — INSERT with first_computed_at from record.
                // record.first_computed_at is set by the caller (context_cycle_review)
                // to cycle_start_ts if available, or to now() as fallback.
                sqlx::query(
                    "INSERT INTO cycle_review_index \
                         (feature_cycle, schema_version, computed_at, \
                          raw_signals_available, summary_json, \
                          corrections_total, corrections_agent, corrections_human, \
                          corrections_system, deprecations_total, orphan_deprecations, \
                          first_computed_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )
                .bind(&record.feature_cycle)
                .bind(record.schema_version as i64)
                .bind(record.computed_at)
                .bind(record.raw_signals_available)
                .bind(&record.summary_json)
                .bind(record.corrections_total)
                .bind(record.corrections_agent)
                .bind(record.corrections_human)
                .bind(record.corrections_system)
                .bind(record.deprecations_total)
                .bind(record.orphan_deprecations)
                .bind(record.first_computed_at)
                .execute(&mut *conn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
            }
            Some(_preserved_first_computed_at) => {
                // Step 2b: Existing row found — UPDATE all mutable columns.
                // first_computed_at is intentionally excluded from the SET clause (ADR-001).
                //
                // Edge case for pre-crt-047 rows (force=true on historical cycles):
                // _preserved_first_computed_at will be 0 (from migration DEFAULT 0).
                // We do NOT overwrite it with a real timestamp. The row remains excluded
                // from get_curation_baseline_window() (WHERE first_computed_at > 0).
                // This is intentional per ADR-001 — no backfilling of historical rows.
                sqlx::query(
                    "UPDATE cycle_review_index \
                     SET schema_version        = ?2, \
                         computed_at           = ?3, \
                         raw_signals_available = ?4, \
                         summary_json          = ?5, \
                         corrections_total     = ?6, \
                         corrections_agent     = ?7, \
                         corrections_human     = ?8, \
                         corrections_system    = ?9, \
                         deprecations_total    = ?10, \
                         orphan_deprecations   = ?11 \
                     WHERE feature_cycle = ?1",
                )
                // Note: first_computed_at is NOT in the SET clause (ADR-001, crt-047).
                .bind(&record.feature_cycle)
                .bind(record.schema_version as i64)
                .bind(record.computed_at)
                .bind(record.raw_signals_available)
                .bind(&record.summary_json)
                .bind(record.corrections_total)
                .bind(record.corrections_agent)
                .bind(record.corrections_human)
                .bind(record.corrections_system)
                .bind(record.deprecations_total)
                .bind(record.orphan_deprecations)
                .execute(&mut *conn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
            }
        }

        Ok(())
    }

    /// Read the last `n` rows from `cycle_review_index` ordered by
    /// `first_computed_at DESC`.
    ///
    /// Excludes rows where `first_computed_at = 0`. These are legacy pre-v24 rows
    /// whose `first_computed_at` was set to DEFAULT 0 by the v23→v24 migration.
    /// They have no temporal anchor and must not influence the ordering or count
    /// of the baseline window. Operators can populate a real `first_computed_at`
    /// by running `context_cycle_review force=true` on each historical cycle.
    ///
    /// Uses `read_pool()` — read-only query (pool discipline per architecture).
    /// Returns `Ok(vec![])` when no qualifying rows exist. Never returns `Err`
    /// for an empty result — only for SQL infrastructure failures.
    pub async fn get_curation_baseline_window(&self, n: usize) -> Result<Vec<CurationBaselineRow>> {
        let rows = sqlx::query(
            "SELECT corrections_total, corrections_agent, corrections_human, \
                    deprecations_total, orphan_deprecations, schema_version \
             FROM cycle_review_index \
             WHERE first_computed_at > 0 \
             ORDER BY first_computed_at DESC \
             LIMIT ?1",
        )
        .bind(n as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let result = rows
            .into_iter()
            .map(|r| CurationBaselineRow {
                corrections_total: r.get::<i64, _>(0),
                corrections_agent: r.get::<i64, _>(1),
                corrections_human: r.get::<i64, _>(2),
                deprecations_total: r.get::<i64, _>(3),
                orphan_deprecations: r.get::<i64, _>(4),
                schema_version: r.get::<i64, _>(5),
            })
            .collect::<Vec<_>>();

        Ok(result)
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
    // CRS-U-01 substitute: CycleReviewRecord store + retrieve round-trip
    //
    // CycleReviewRecord intentionally does NOT derive Serialize/Deserialize.
    // It is a DB-boundary type: handler code serializes the higher-level
    // RetrospectiveReport directly; serde on this struct is not needed and
    // would create a misleading serialization surface. The CRS-U-01 test plan
    // item (serde JSON round-trip) does not apply to the actual design.
    //
    // This test covers the equivalent concern: a fully-populated record stored
    // via store_cycle_review() and retrieved via get_cycle_review() must have
    // all fields byte-identical, including summary_json.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cycle_review_record_round_trip() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let original = CycleReviewRecord {
            feature_cycle: "crt-033-serde-substitute".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_711_700_000,
            raw_signals_available: 1,
            summary_json:
                r#"{"feature_cycle":"crt-033-serde-substitute","schema_version":1,"hotspots":[]}"#
                    .to_string(),
            ..Default::default()
        };

        store
            .store_cycle_review(&original)
            .await
            .expect("store must succeed");

        let retrieved = store
            .get_cycle_review(&original.feature_cycle)
            .await
            .expect("get must not error")
            .expect("get must return Some after store");

        assert_eq!(
            retrieved.feature_cycle, original.feature_cycle,
            "feature_cycle must survive store/retrieve"
        );
        assert_eq!(
            retrieved.schema_version, original.schema_version,
            "schema_version must survive store/retrieve"
        );
        assert_eq!(
            retrieved.computed_at, original.computed_at,
            "computed_at must survive store/retrieve"
        );
        assert_eq!(
            retrieved.raw_signals_available, original.raw_signals_available,
            "raw_signals_available must survive store/retrieve"
        );
        assert_eq!(
            retrieved.summary_json, original.summary_json,
            "summary_json must survive store/retrieve byte-identical"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-01 (replaces CRS-U-02): SUMMARY_SCHEMA_VERSION is 3
    // -----------------------------------------------------------------------

    #[test]
    fn test_summary_schema_version_is_three() {
        assert_eq!(
            SUMMARY_SCHEMA_VERSION, 3u32,
            "SUMMARY_SCHEMA_VERSION must be 3 (bumped in crt-049: \
             added explicit_read_count, explicit_read_by_category, \
             redefined total_served)"
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
    // CRS-I-03: Two-step upsert overwrites mutable columns on prior record
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
            first_computed_at: 1_700_000_000,
            ..Default::default()
        };

        store.store_cycle_review(&v1).await.expect("first store");

        let v2 = CycleReviewRecord {
            feature_cycle: "crt-033-overwrite".to_string(),
            schema_version: 2,
            computed_at: 1_700_000_100,
            raw_signals_available: 1,
            summary_json: r#"{"version":2}"#.to_string(),
            first_computed_at: 1_700_000_999,
            ..Default::default()
        };

        store.store_cycle_review(&v2).await.expect("second store");

        let fetched = store
            .get_cycle_review("crt-033-overwrite")
            .await
            .expect("get must not error")
            .expect("must return Some");

        assert_eq!(
            fetched.computed_at, 1_700_000_100,
            "two-step upsert must overwrite prior record — computed_at must be T2"
        );
        assert_eq!(fetched.schema_version, 2);
        assert_eq!(fetched.summary_json, r#"{"version":2}"#);
        // first_computed_at must be from v1 (first write wins)
        assert_eq!(
            fetched.first_computed_at, 1_700_000_000,
            "first_computed_at must be from first write, not second write"
        );

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
            ..Default::default()
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
                ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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

    // -----------------------------------------------------------------------
    // CRS-V24-U-02: CycleReviewRecord round-trip includes all seven new fields
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cycle_review_record_v24_round_trip() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let record = CycleReviewRecord {
            feature_cycle: "crt-047-v24-roundtrip".to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at: 1_720_000_000,
            raw_signals_available: 1,
            summary_json: r#"{"v":"24"}"#.to_string(),
            corrections_total: 11,
            corrections_agent: 7,
            corrections_human: 4,
            corrections_system: 2,
            deprecations_total: 5,
            orphan_deprecations: 3,
            first_computed_at: 1_719_000_000,
        };

        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        let fetched = store
            .get_cycle_review(&record.feature_cycle)
            .await
            .expect("get must not error")
            .expect("must return Some after store");

        assert_eq!(
            fetched.corrections_total, 11,
            "corrections_total round-trip"
        );
        assert_eq!(fetched.corrections_agent, 7, "corrections_agent round-trip");
        assert_eq!(fetched.corrections_human, 4, "corrections_human round-trip");
        assert_eq!(
            fetched.corrections_system, 2,
            "corrections_system round-trip"
        );
        assert_eq!(
            fetched.deprecations_total, 5,
            "deprecations_total round-trip"
        );
        assert_eq!(
            fetched.orphan_deprecations, 3,
            "orphan_deprecations round-trip"
        );
        assert_eq!(
            fetched.first_computed_at, 1_719_000_000,
            "first_computed_at round-trip"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-03: first_computed_at preserved on overwrite (R-07)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_cycle_review_preserves_first_computed_at_on_overwrite() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // First write: first_computed_at = 1_700_000_000
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "v24-preserve-test".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_700_000_000,
                raw_signals_available: 1,
                summary_json: r#"{"v":1}"#.to_string(),
                corrections_total: 3,
                corrections_agent: 2,
                corrections_human: 1,
                first_computed_at: 1_700_000_000,
                ..Default::default()
            })
            .await
            .expect("first store");

        // Second write (force=true): first_computed_at = 1_800_000_000 (newer value)
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "v24-preserve-test".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_800_000_000,
                raw_signals_available: 1,
                summary_json: r#"{"v":2}"#.to_string(),
                corrections_total: 5,
                corrections_agent: 3,
                corrections_human: 2,
                first_computed_at: 1_800_000_000,
                ..Default::default()
            })
            .await
            .expect("second store");

        let fetched = store
            .get_cycle_review("v24-preserve-test")
            .await
            .expect("get must not error")
            .expect("must return Some");

        // Critical assertion: first_computed_at must be from first write
        assert_eq!(
            fetched.first_computed_at, 1_700_000_000,
            "first_computed_at must be preserved from first write (first write wins)"
        );
        // Other fields must reflect the second write
        assert_eq!(fetched.computed_at, 1_800_000_000, "computed_at must be T2");
        assert_eq!(
            fetched.corrections_total, 5,
            "corrections_total must be from second write"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-04: First write sets first_computed_at from caller-supplied ts
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_store_cycle_review_first_write_sets_first_computed_at() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "v24-first-write-test".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_750_000_000,
                raw_signals_available: 1,
                summary_json: r#"{"ok":true}"#.to_string(),
                first_computed_at: 1_750_000_000,
                ..Default::default()
            })
            .await
            .expect("store must succeed");

        let fetched = store
            .get_cycle_review("v24-first-write-test")
            .await
            .expect("get must not error")
            .expect("must return Some");

        assert_eq!(
            fetched.first_computed_at, 1_750_000_000,
            "first write must set first_computed_at from caller-supplied timestamp"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-05: get_curation_baseline_window excludes first_computed_at=0
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_curation_baseline_window_excludes_zero_first_computed_at() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Two legacy rows with first_computed_at = 0
        for i in 0..2u32 {
            store
                .store_cycle_review(&CycleReviewRecord {
                    feature_cycle: format!("legacy-{i}"),
                    schema_version: 1,
                    computed_at: 1_600_000_000 + i as i64,
                    raw_signals_available: 1,
                    summary_json: r#"{}"#.to_string(),
                    first_computed_at: 0,
                    ..Default::default()
                })
                .await
                .expect("store legacy row");
        }

        // One qualifying row with first_computed_at > 0
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "qualifying".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_700_000_000,
                raw_signals_available: 1,
                summary_json: r#"{}"#.to_string(),
                first_computed_at: 1_700_000_000,
                ..Default::default()
            })
            .await
            .expect("store qualifying row");

        let result = store
            .get_curation_baseline_window(10)
            .await
            .expect("must not error");

        assert_eq!(result.len(), 1, "only qualifying row must be returned");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-06: get_curation_baseline_window ordered by first_computed_at DESC
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_curation_baseline_window_ordered_by_first_computed_at_desc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        for (fc, ts) in [
            ("cycle-a", 1_000_i64),
            ("cycle-b", 2_000_i64),
            ("cycle-c", 3_000_i64),
        ] {
            store
                .store_cycle_review(&CycleReviewRecord {
                    feature_cycle: fc.to_string(),
                    schema_version: SUMMARY_SCHEMA_VERSION,
                    computed_at: ts,
                    raw_signals_available: 1,
                    summary_json: r#"{}"#.to_string(),
                    first_computed_at: ts,
                    ..Default::default()
                })
                .await
                .expect("store row");
        }

        let result = store
            .get_curation_baseline_window(10)
            .await
            .expect("must not error");

        assert_eq!(result.len(), 3, "all three rows must be returned");
        // Verify DESC ordering via schema_version field used as a proxy —
        // schema_version is the same for all, so we use corrections_total
        // to track identity. Actually use the fact that we stored corrections_total=0
        // for all. We need a distinguishing feature — use the fact that the row
        // with the most recent first_computed_at should come first. Since we can't
        // directly read first_computed_at from CurationBaselineRow, we verify
        // ordering is consistent (no panics, 3 rows, stable). Use corrections_total
        // as a proxy by setting them differently.
        //
        // Since CurationBaselineRow doesn't include first_computed_at, we verify
        // ordering by inserting rows with distinct corrections_total values
        // and checking the order. The test already exercises the ordering contract
        // implicitly — all 3 rows must be present and length == 3.
        // The ordering assertion is covered in the next test with distinct values.
        assert_eq!(result.len(), 3);

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-06b: Ordering verified via distinct corrections_total values
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_curation_baseline_window_ordering_verified() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert rows with distinct corrections_total to track ordering:
        // cycle-old: ts=1_000, corrections_total=100
        // cycle-mid: ts=2_000, corrections_total=200
        // cycle-new: ts=3_000, corrections_total=300
        for (fc, ts, ct) in [
            ("cycle-old", 1_000_i64, 100_i64),
            ("cycle-mid", 2_000_i64, 200_i64),
            ("cycle-new", 3_000_i64, 300_i64),
        ] {
            store
                .store_cycle_review(&CycleReviewRecord {
                    feature_cycle: fc.to_string(),
                    schema_version: SUMMARY_SCHEMA_VERSION,
                    computed_at: ts,
                    raw_signals_available: 1,
                    summary_json: r#"{}"#.to_string(),
                    corrections_total: ct,
                    first_computed_at: ts,
                    ..Default::default()
                })
                .await
                .expect("store row");
        }

        let result = store
            .get_curation_baseline_window(10)
            .await
            .expect("must not error");

        assert_eq!(result.len(), 3);
        // DESC order: newest first (ts=3_000, ct=300)
        assert_eq!(
            result[0].corrections_total, 300,
            "first row must be newest (corrections_total=300)"
        );
        assert_eq!(
            result[1].corrections_total, 200,
            "second row must be middle (corrections_total=200)"
        );
        assert_eq!(
            result[2].corrections_total, 100,
            "third row must be oldest (corrections_total=100)"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-07: get_curation_baseline_window caps at n (R-11 boundary)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_curation_baseline_window_caps_at_n() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert 12 rows with distinct first_computed_at values
        for i in 0..12_i64 {
            store
                .store_cycle_review(&CycleReviewRecord {
                    feature_cycle: format!("cap-cycle-{i:02}"),
                    schema_version: SUMMARY_SCHEMA_VERSION,
                    computed_at: 1_700_000_000 + i,
                    raw_signals_available: 1,
                    summary_json: r#"{}"#.to_string(),
                    first_computed_at: 1_700_000_000 + i,
                    ..Default::default()
                })
                .await
                .expect("store row");
        }

        let result = store
            .get_curation_baseline_window(10)
            .await
            .expect("must not error");

        assert_eq!(
            result.len(),
            10,
            "LIMIT must cap at n=10 even when 12 rows exist"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-08: force=true historical does not perturb baseline window order
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_force_true_historical_does_not_perturb_baseline_window_order() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // current-cycle: first_computed_at = 2_000
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "current-cycle".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 2_000,
                raw_signals_available: 1,
                summary_json: r#"{"current":true}"#.to_string(),
                corrections_total: 20,
                first_computed_at: 2_000,
                ..Default::default()
            })
            .await
            .expect("store current-cycle");

        // historical-cycle: first_computed_at = 1_000
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "historical-cycle".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_000,
                raw_signals_available: 1,
                summary_json: r#"{"historical":true}"#.to_string(),
                corrections_total: 10,
                first_computed_at: 1_000,
                ..Default::default()
            })
            .await
            .expect("store historical-cycle");

        // Simulate force=true on historical-cycle: new computed_at, new snapshot values,
        // but first_computed_at in record is newer — must NOT overwrite stored value.
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "historical-cycle".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 9_999,
                raw_signals_available: 1,
                summary_json: r#"{"historical":"recomputed"}"#.to_string(),
                corrections_total: 15,
                first_computed_at: 9_999, // caller passes new ts, but it must be ignored
                ..Default::default()
            })
            .await
            .expect("force-true recompute");

        let result = store
            .get_curation_baseline_window(2)
            .await
            .expect("must not error");

        assert_eq!(result.len(), 2, "both cycles must appear");
        // current-cycle (first_computed_at=2_000) must be first in DESC order
        assert_eq!(
            result[0].corrections_total, 20,
            "current-cycle must appear first (first_computed_at=2_000 > 1_000)"
        );
        // historical-cycle (first_computed_at=1_000) must be second
        assert_eq!(
            result[1].corrections_total, 15,
            "historical-cycle must appear second with updated snapshot (corrections_total=15)"
        );

        // Verify first_computed_at was preserved on historical-cycle
        let hist = store
            .get_cycle_review("historical-cycle")
            .await
            .expect("get must not error")
            .expect("must return Some");
        assert_eq!(
            hist.first_computed_at, 1_000,
            "historical-cycle first_computed_at must be preserved at 1_000"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-09: corrections_system survives round-trip (R-09)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_corrections_system_round_trips_through_store() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "sys-roundtrip".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_700_000_000,
                raw_signals_available: 1,
                summary_json: r#"{}"#.to_string(),
                corrections_total: 9,
                corrections_agent: 5,
                corrections_human: 4,
                corrections_system: 7,
                deprecations_total: 3,
                orphan_deprecations: 1,
                first_computed_at: 1_700_000_000,
            })
            .await
            .expect("store must succeed");

        let fetched = store
            .get_cycle_review("sys-roundtrip")
            .await
            .expect("get must not error")
            .expect("must return Some");

        assert_eq!(
            fetched.corrections_system, 7,
            "corrections_system must not be silently dropped in SQL projection"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // CRS-V24-U-10: Empty window returns empty slice (FM-03)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_curation_baseline_window_empty_when_no_qualifying_rows() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert only a row with first_computed_at = 0 (legacy)
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "legacy-only".to_string(),
                schema_version: 1,
                computed_at: 1_600_000_000,
                raw_signals_available: 1,
                summary_json: r#"{}"#.to_string(),
                first_computed_at: 0,
                ..Default::default()
            })
            .await
            .expect("store legacy row");

        let result = store
            .get_curation_baseline_window(10)
            .await
            .expect("must not error — empty result is Ok, not Err");

        assert!(
            result.is_empty(),
            "all rows have first_computed_at=0 — result must be empty slice"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // EC-04: Concurrent force=true — first_computed_at always preserved
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_concurrent_force_true_preserves_first_computed_at() {
        use std::sync::Arc;

        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = Arc::new(open_test_store(&dir).await);

        // Insert initial row with known first_computed_at
        store
            .store_cycle_review(&CycleReviewRecord {
                feature_cycle: "concurrent-force-true".to_string(),
                schema_version: SUMMARY_SCHEMA_VERSION,
                computed_at: 1_000,
                raw_signals_available: 1,
                summary_json: r#"{"initial":true}"#.to_string(),
                first_computed_at: 1_000,
                ..Default::default()
            })
            .await
            .expect("initial store");

        let store_a = Arc::clone(&store);
        let store_b = Arc::clone(&store);

        // Two concurrent force=true writes with different first_computed_at values
        let (r1, r2) = tokio::join!(
            async move {
                store_a
                    .store_cycle_review(&CycleReviewRecord {
                        feature_cycle: "concurrent-force-true".to_string(),
                        schema_version: SUMMARY_SCHEMA_VERSION,
                        computed_at: 2_000,
                        raw_signals_available: 1,
                        summary_json: r#"{"writer":"A"}"#.to_string(),
                        first_computed_at: 2_000,
                        ..Default::default()
                    })
                    .await
            },
            async move {
                store_b
                    .store_cycle_review(&CycleReviewRecord {
                        feature_cycle: "concurrent-force-true".to_string(),
                        schema_version: SUMMARY_SCHEMA_VERSION,
                        computed_at: 3_000,
                        raw_signals_available: 1,
                        summary_json: r#"{"writer":"B"}"#.to_string(),
                        first_computed_at: 3_000,
                        ..Default::default()
                    })
                    .await
            }
        );

        assert!(r1.is_ok(), "concurrent force-true A must not error: {r1:?}");
        assert!(r2.is_ok(), "concurrent force-true B must not error: {r2:?}");

        let fetched = Arc::try_unwrap(store)
            .expect("no other Arc refs")
            .get_cycle_review("concurrent-force-true")
            .await
            .expect("get must not error")
            .expect("must return Some");

        // first_computed_at must always be from the initial insert (1_000)
        assert_eq!(
            fetched.first_computed_at, 1_000,
            "concurrent force=true must never overwrite first_computed_at (serializer ensures this)"
        );
    }
}
