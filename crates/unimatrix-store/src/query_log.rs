//! Query log persistence for nxs-010.
//!
//! Provides insert and scan operations on the `query_log` table.
//! The table uses SQLite AUTOINCREMENT for primary key allocation.
//! All operations are async.

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Row;

use crate::analytics::AnalyticsWrite;
use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

// -- Constants --

/// Milliseconds per day for ts_millis lookback arithmetic.
///
/// `observations.ts_millis` is millisecond-epoch (contrast with `query_log.ts`
/// which is second-epoch). MUST NOT be `86_400` — omitting the `* 1_000` factor
/// produces a 1000x-wide lookback window with no error logged (ADR-006, R-05).
///
/// Used in `query_phase_freq_observations` to pre-compute `cutoff_millis` in Rust.
pub(crate) const MILLIS_PER_DAY: i64 = 86_400 * 1_000;

// -- Types --

/// A single query log entry capturing search telemetry.
///
/// `query_id` is 0 on insert (AUTOINCREMENT allocates) and populated on read.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryLogRecord {
    /// Auto-allocated primary key. Set to 0 on insert; populated on read.
    pub query_id: i64,
    pub session_id: String,
    pub query_text: String,
    pub ts: u64,
    pub result_count: i64,
    pub result_entry_ids: String,
    pub similarity_scores: String,
    pub retrieval_mode: String,
    pub source: String,
    pub phase: Option<String>, // col-028: workflow phase at query time; None for UDS rows
}

/// Transient row returned by `query_phase_freq_observations`.
///
/// Used only during `PhaseFreqTable::rebuild`; not stored or returned to callers.
///
/// `freq` is i64 because SQLite `COUNT(*)` maps to i64 via sqlx 0.8.
/// Do NOT use u64 — sqlx deserialization will fail silently at runtime (R-13).
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    /// entry_id read as i64 from SQL (CAST result), then cast to u64.
    /// The SQL `CAST(json_extract(o.input, '$.id') AS INTEGER)` guarantees
    /// a non-negative integer value.
    pub entry_id: u64,
    /// COUNT(*) result — always i64 in sqlx 0.8 SQLite mapping.
    pub freq: i64,
}

/// One row from Query B: a `(phase, feature_cycle, outcome)` triple from
/// `cycle_events` joined to `sessions`.
///
/// Declared in `unimatrix-store` but NOT re-exported from the crate root (`lib.rs`).
/// Consumed only by `PhaseFreqTable::rebuild()` via `query_phase_outcome_map()`.
/// Callers outside this crate must import via the full module path:
/// `unimatrix_store::query_log::PhaseOutcomeRow`.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseOutcomeRow {
    pub phase: String,
    pub feature_cycle: String,
    pub outcome: String,
}

// -- Shared constructor --

impl QueryLogRecord {
    /// Construct a new `QueryLogRecord` with consistent field population.
    pub fn new(
        session_id: String,
        query_text: String,
        entry_ids: &[u64],
        similarity_scores: &[f64],
        retrieval_mode: &str,
        source: &str,
        phase: Option<String>, // col-028: workflow phase at query time; final parameter
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        QueryLogRecord {
            query_id: 0,
            session_id,
            query_text,
            ts: now,
            result_count: entry_ids.len() as i64,
            result_entry_ids: serde_json::to_string(entry_ids).unwrap_or_default(),
            similarity_scores: serde_json::to_string(similarity_scores).unwrap_or_default(),
            retrieval_mode: retrieval_mode.to_string(),
            source: source.to_string(),
            phase,
        }
    }
}

// -- Store methods --

impl SqlxStore {
    /// Enqueue a query log record (analytics write via enqueue_analytics).
    pub fn insert_query_log(&self, record: &QueryLogRecord) {
        self.enqueue_analytics(AnalyticsWrite::QueryLog {
            session_id: record.session_id.clone(),
            query_text: record.query_text.clone(),
            ts: record.ts as i64,
            result_count: record.result_count,
            result_entry_ids: if record.result_entry_ids.is_empty() {
                None
            } else {
                Some(record.result_entry_ids.clone())
            },
            similarity_scores: if record.similarity_scores.is_empty() {
                None
            } else {
                Some(record.similarity_scores.clone())
            },
            retrieval_mode: if record.retrieval_mode.is_empty() {
                None
            } else {
                Some(record.retrieval_mode.clone())
            },
            source: record.source.clone(),
            phase: record.phase.clone(), // col-028
        });
    }

    /// Scan query log records for multiple sessions, ordered by timestamp ascending.
    ///
    /// Session IDs are batched into chunks of 50 to avoid large IN clauses (R-11).
    /// Returns an empty Vec if `session_ids` is empty or no rows match.
    pub async fn scan_query_log_by_sessions(
        &self,
        session_ids: &[&str],
    ) -> Result<Vec<QueryLogRecord>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results: Vec<QueryLogRecord> = Vec::new();

        for chunk in session_ids.chunks(50) {
            let placeholders: String = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");

            let sql = format!(
                "SELECT query_id, session_id, query_text, ts, result_count, \
                        result_entry_ids, similarity_scores, retrieval_mode, source, phase \
                 FROM query_log \
                 WHERE session_id IN ({placeholders}) \
                 ORDER BY ts ASC"
            );

            let mut query = sqlx::query(&sql);
            for &id in chunk {
                query = query.bind(id);
            }

            let rows = query
                .fetch_all(self.read_pool())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

            for row in rows {
                all_results.push(row_to_query_log(&row)?);
            }
        }

        Ok(all_results)
    }

    /// Scan all query log records for a given session, ordered by timestamp ascending.
    pub async fn scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>> {
        let rows = sqlx::query(
            "SELECT query_id, session_id, query_text, ts, result_count, \
                    result_entry_ids, similarity_scores, retrieval_mode, source, phase \
             FROM query_log \
             WHERE session_id = ?1 \
             ORDER BY ts ASC",
        )
        .bind(session_id)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter().map(row_to_query_log).collect()
    }

    /// Aggregate `(phase, category, entry_id, freq)` from explicit-read observations.
    ///
    /// Replaces `query_phase_freq_table`. Sources from `observations` (deliberate
    /// agent reads via `context_get` / `context_lookup`) instead of `query_log`
    /// search exposures.
    ///
    /// # SQL
    ///
    /// Filters to `PreToolUse` hook events for `context_get` and `context_lookup`
    /// tools (4-entry IN clause — bare and `mcp__unimatrix__` prefix variants).
    /// `CAST(json_extract(o.input, '$.id') AS INTEGER)` is MANDATORY in the JOIN
    /// predicate — omitting it causes a silent zero-row return (col-031 R-05).
    /// `o.hook = 'PreToolUse'` (NOT `o.hook_event` — the DB column is `hook`, ADR-007).
    ///
    /// # Parameters
    ///
    /// `lookback_days` is converted to `cutoff_millis` (`i64`) in Rust using
    /// `MILLIS_PER_DAY`. Bound as `?1` (`i64`, not `u32` — sqlx 0.8 INTEGER
    /// mapping, ADR-006).
    ///
    /// # Returns
    ///
    /// Empty Vec when no matching observations exist within the lookback window.
    /// Caller (`PhaseFreqTable::rebuild`) treats empty as `use_fallback=true`.
    /// Results pre-sorted by `(phase, category, freq DESC)` — caller uses this
    /// ordering directly for rank-based normalization (col-031 ADR-001).
    pub async fn query_phase_freq_observations(
        &self,
        lookback_days: u32,
    ) -> Result<Vec<PhaseFreqRow>> {
        // Pre-compute lookback cutoff in Rust (ADR-006).
        // observations.ts_millis is millisecond-epoch; MILLIS_PER_DAY converts days to ms.
        let now_millis: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let cutoff_millis: i64 = now_millis - (lookback_days as i64) * MILLIS_PER_DAY;

        let sql = "
            SELECT o.phase,
                   e.category,
                   CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
                   COUNT(*) AS freq
            FROM observations o
              JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
            WHERE o.phase IS NOT NULL
              AND o.hook = 'PreToolUse'
              AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                             'context_lookup', 'mcp__unimatrix__context_lookup')
              AND json_extract(o.input, '$.id') IS NOT NULL
              AND o.ts_millis > ?1
            GROUP BY o.phase, e.category, entry_id
            ORDER BY o.phase, e.category, freq DESC
        ";

        // Bind cutoff_millis as i64 (?1 — sqlx 0.8 INTEGER mapping; u32 would fail at runtime).
        let rows = sqlx::query(sql)
            .bind(cutoff_millis)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Column order from SELECT:
        //   0: o.phase    -> String
        //   1: e.category -> String
        //   2: entry_id   -> i64 (CAST result), cast to u64
        //   3: freq       -> i64 (COUNT(*) always i64 in sqlx 0.8)
        rows.iter().map(row_to_phase_freq_row).collect()
    }

    /// Count distinct `(phase, session_id)` pairs in `observations` within the
    /// lookback window.
    ///
    /// Used by `warn_observations_coverage()` in `status.rs` to check whether the
    /// explicit-read signal is sufficiently populated before rebuild. When the count
    /// falls below `InferenceConfig::min_phase_session_pairs`, the caller emits a
    /// `tracing::warn!` and sets `use_fallback = true`.
    ///
    /// Only counts rows matching the same filters as `query_phase_freq_observations`:
    /// `hook = 'PreToolUse'`, tool IN 4-entry clause, `json_extract(input,'$.id') IS NOT NULL`,
    /// `phase IS NOT NULL`, and `ts_millis > cutoff_millis`.
    pub async fn count_phase_session_pairs(&self, lookback_days: u32) -> Result<i64> {
        let now_millis: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let cutoff_millis: i64 = now_millis - (lookback_days as i64) * MILLIS_PER_DAY;

        let sql = "
            SELECT COUNT(DISTINCT (phase || '|' || session_id))
            FROM observations
            WHERE phase IS NOT NULL
              AND hook = 'PreToolUse'
              AND tool IN ('context_get', 'mcp__unimatrix__context_get',
                           'context_lookup', 'mcp__unimatrix__context_lookup')
              AND json_extract(input, '$.id') IS NOT NULL
              AND ts_millis > ?1
        ";

        let row = sqlx::query(sql)
            .bind(cutoff_millis)
            .fetch_one(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let count: i64 = row
            .try_get::<i64, _>(0)
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(count)
    }

    /// Fetch `(phase, feature_cycle, outcome)` triples for outcome-weight computation.
    ///
    /// Returns all `cycle_phase_end` rows joined to sessions that have a non-NULL
    /// `feature_cycle`. Pre-col-022 sessions (NULL `feature_cycle`) are excluded by
    /// the WHERE clause — those sessions contribute no outcome weight (default `1.0`).
    ///
    /// # Returns
    ///
    /// Empty Vec is valid (no `cycle_phase_end` history or all sessions pre-col-022).
    /// Store error MUST propagate — do NOT return empty Vec on error (constraint C-7,
    /// architecture constraint #12). The caller (`PhaseFreqTable::rebuild`) must
    /// return `Err` and retain the previous table (retain-on-error semantics).
    pub async fn query_phase_outcome_map(&self) -> Result<Vec<PhaseOutcomeRow>> {
        let sql = "
            SELECT ce.phase, s.feature_cycle, ce.outcome
            FROM cycle_events ce
              JOIN sessions s ON s.feature_cycle = ce.cycle_id
            WHERE ce.event_type = 'cycle_phase_end'
              AND ce.phase IS NOT NULL
              AND ce.outcome IS NOT NULL
              AND s.feature_cycle IS NOT NULL
        ";

        let rows = sqlx::query(sql)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter().map(row_to_phase_outcome_row).collect()
    }
}

/// Deserialize a single SQL row into `PhaseFreqRow`.
///
/// Column positions must match the SELECT clause in `query_phase_freq_observations`:
///   0: phase    (String)
///   1: category (String)
///   2: entry_id (i64, cast to u64)
///   3: freq     (i64)
///
/// `entry_id` is read as `i64` and cast to `u64` because:
///   - SQLite INTEGER is always signed i64 in sqlx 0.8
///   - Entry IDs are non-negative by construction
///   - The `CAST(json_extract(o.input, '$.id') AS INTEGER)` SQL expression produces
///     INTEGER affinity
fn row_to_phase_freq_row(row: &sqlx::sqlite::SqliteRow) -> Result<PhaseFreqRow> {
    Ok(PhaseFreqRow {
        phase: row
            .try_get::<String, _>(0)
            .map_err(|e| StoreError::Database(e.into()))?,
        category: row
            .try_get::<String, _>(1)
            .map_err(|e| StoreError::Database(e.into()))?,
        entry_id: row
            .try_get::<i64, _>(2)
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        freq: row
            .try_get::<i64, _>(3)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

/// Deserialize a single SQL row into `PhaseOutcomeRow`.
///
/// Column positions must match the SELECT clause in `query_phase_outcome_map`:
///   0: ce.phase          (String)
///   1: s.feature_cycle   (String)
///   2: ce.outcome        (String)
fn row_to_phase_outcome_row(row: &sqlx::sqlite::SqliteRow) -> Result<PhaseOutcomeRow> {
    Ok(PhaseOutcomeRow {
        phase: row
            .try_get::<String, _>(0)
            .map_err(|e| StoreError::Database(e.into()))?,
        feature_cycle: row
            .try_get::<String, _>(1)
            .map_err(|e| StoreError::Database(e.into()))?,
        outcome: row
            .try_get::<String, _>(2)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

fn row_to_query_log(row: &sqlx::sqlite::SqliteRow) -> Result<QueryLogRecord> {
    Ok(QueryLogRecord {
        query_id: row.try_get(0).map_err(|e| StoreError::Database(e.into()))?,
        session_id: row.try_get(1).map_err(|e| StoreError::Database(e.into()))?,
        query_text: row.try_get(2).map_err(|e| StoreError::Database(e.into()))?,
        ts: row
            .try_get::<i64, _>(3)
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        result_count: row.try_get(4).map_err(|e| StoreError::Database(e.into()))?,
        result_entry_ids: row
            .try_get::<Option<String>, _>(5)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        similarity_scores: row
            .try_get::<Option<String>, _>(6)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        retrieval_mode: row
            .try_get::<Option<String>, _>(7)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        source: row.try_get(8).map_err(|e| StoreError::Database(e.into()))?,
        // col-028: phase at index 9 — must match SELECT column list order (AC-17, SR-01 guard).
        // source is at index 8; phase is at index 9. Do NOT swap.
        phase: row
            .try_get::<Option<String>, _>(9)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

#[cfg(test)]
#[path = "query_log_tests.rs"]
mod tests;
