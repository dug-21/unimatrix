//! Observation and shadow evaluation async methods on SqlxStore.
//!
//! Provides SQL-backed async access to the `observations` and `shadow_evaluations`
//! tables. Used by the server background tick, observation service, and shadow
//! evaluation logging.

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

/// A single observation record fetched from the observations table.
#[derive(Debug, Clone)]
pub struct ObservationRow {
    pub id: u64,
    pub ts_millis: i64,
    pub hook: String,
    pub session_id: String,
    pub tool: Option<String>,
    pub input: Option<String>,
    pub response_size: Option<i64>,
    pub response_snippet: Option<String>,
}

/// A shadow evaluation to persist.
#[derive(Debug, Clone)]
pub struct ShadowEvalRow {
    pub timestamp: i64,
    pub rule_name: String,
    pub rule_category: String,
    pub neural_category: String,
    pub neural_confidence: f64,
    pub convention_score: f64,
    pub rule_accepted: i32,
    pub digest_bytes: Option<Vec<u8>>,
}

impl SqlxStore {
    /// Fetch observations with id > `watermark`, returning at most `limit` rows
    /// ordered by id ascending.
    ///
    /// Returns `(rows, new_watermark)` where `new_watermark` is the maximum id
    /// seen in the batch (unchanged from `watermark` if empty).
    pub async fn fetch_observations_since(
        &self,
        watermark: u64,
        limit: i64,
    ) -> Result<(Vec<ObservationRow>, u64)> {
        let rows = sqlx::query(
            "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet
             FROM observations WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
        )
        .bind(watermark as i64)
        .bind(limit)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut records = Vec::with_capacity(rows.len());
        let mut max_id = watermark;
        for row in rows {
            let id: i64 = row.get(0);
            if id as u64 > max_id {
                max_id = id as u64;
            }
            records.push(ObservationRow {
                id: id as u64,
                ts_millis: row.get(1),
                hook: row.get(2),
                session_id: row.get(3),
                tool: row.get(4),
                input: row.get(5),
                response_size: row.get(6),
                response_snippet: row.get(7),
            });
        }
        Ok((records, max_id))
    }

    /// Insert a single observation row. Used by hook IPC and tests.
    pub async fn insert_observation(
        &self,
        session_id: &str,
        ts_millis: i64,
        hook: &str,
        tool: Option<&str>,
        input: Option<&str>,
        response_size: Option<i64>,
        response_snippet: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO observations
             (session_id, ts_millis, hook, tool, input, response_size, response_snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(session_id)
        .bind(ts_millis)
        .bind(hook)
        .bind(tool)
        .bind(input)
        .bind(response_size)
        .bind(response_snippet)
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Load session IDs for a given feature cycle.
    pub async fn load_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
                .bind(feature_cycle)
                .fetch_all(self.read_pool())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    }

    /// Load observations for a list of session IDs.
    ///
    /// Returns rows ordered by ts_millis ASC.
    pub async fn load_observations_for_sessions(
        &self,
        session_ids: &[String],
    ) -> Result<Vec<ObservationRow>> {
        if session_ids.is_empty() {
            return Ok(vec![]);
        }
        // Build IN clause via repeated bind
        let placeholders = session_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet
             FROM observations WHERE session_id IN ({})
             ORDER BY ts_millis ASC",
            placeholders
        );

        let mut q = sqlx::query(&sql);
        for sid in session_ids {
            q = q.bind(sid);
        }
        let rows = q
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(rows
            .into_iter()
            .map(|row| ObservationRow {
                id: row.get::<i64, _>(0) as u64,
                ts_millis: row.get(1),
                hook: row.get(2),
                session_id: row.get(3),
                tool: row.get(4),
                input: row.get(5),
                response_size: row.get(6),
                response_snippet: row.get(7),
            })
            .collect())
    }

    /// Load observation stats for context_status: counts distinct sessions with
    /// feature-cycle-linked observations in two retention windows.
    ///
    /// Returns `(active_45d_count, active_60d_count)`.
    pub async fn load_observation_session_stats(
        &self,
        cutoff_45: i64,
        cutoff_60: i64,
    ) -> Result<Vec<(String, i64, i64)>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT s.session_id,
                    s.started_at,
                    COUNT(o.id) as obs_count
             FROM sessions s
             JOIN observations o ON o.session_id = s.session_id
             WHERE s.started_at BETWEEN ?1 AND ?2
             GROUP BY s.session_id, s.started_at",
        )
        .bind(cutoff_45)
        .bind(cutoff_60)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(rows)
    }

    /// Insert a batch of shadow evaluation rows.
    pub async fn insert_shadow_evaluations(&self, evals: &[ShadowEvalRow]) -> Result<()> {
        if evals.is_empty() {
            return Ok(());
        }

        for eval in evals {
            sqlx::query(
                "INSERT INTO shadow_evaluations
                 (timestamp, rule_name, rule_category, neural_category,
                  neural_confidence, convention_score, rule_accepted, digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(eval.timestamp)
            .bind(&eval.rule_name)
            .bind(&eval.rule_category)
            .bind(&eval.neural_category)
            .bind(eval.neural_confidence)
            .bind(eval.convention_score)
            .bind(eval.rule_accepted)
            .bind(eval.digest_bytes.as_deref())
            .execute(self.write_pool_server())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        }
        Ok(())
    }
}
