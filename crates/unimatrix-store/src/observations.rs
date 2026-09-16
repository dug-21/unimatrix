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
    /// vnc-049 ADR-001: persisted provider-first source_domain stamp. `None` (NULL column)
    /// for legacy/non-opencode rows; the read path (observation.rs) then falls back to
    /// read-derived resolution. `Some("opencode")` for ingest-stamped opencode rows.
    pub source_domain: Option<String>,
    /// vnc-049 ADR-002: persisted backend model identity (e.g. "ollama/qwen3-coder").
    /// `None` (NULL column) for non-opencode / legacy rows.
    pub model_id: Option<String>,
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
            "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet, \
                    source_domain, model_id
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
                source_domain: row.get(8),
                model_id: row.get(9),
            });
        }
        Ok((records, max_id))
    }

    /// Insert a single observation row. Used by hook IPC and tests.
    #[allow(clippy::too_many_arguments)]
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
            "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet, \
                    source_domain, model_id
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
                source_domain: row.get(8),
                model_id: row.get(9),
            })
            .collect())
    }

    /// Load observation stats for context_status: counts distinct sessions with
    /// feature-cycle-linked observations in two retention windows.
    ///
    /// Returns `(active_45d_count, active_60d_count)`.
    ///
    /// vnc-049 C6: this is a GROUP BY aggregate over `sessions`⋈`observations`; it projects
    /// `session_id, started_at, COUNT(o.id)`, NOT per-row observation columns, so the new
    /// `source_domain`/`model_id` columns are not (and should not be) selected here. It must
    /// still function correctly with the added columns present — pinned by
    /// `test_load_observation_session_stats_with_attribution_columns` (previously had no direct
    /// coverage).
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

#[cfg(test)]
mod tests {
    //! vnc-049 C6: store-layer SELECT coverage for the persisted attribution columns
    //! (`source_domain`, `model_id`). This module is net-new — `observations.rs` had no
    //! `#[cfg(test)]` today; coverage was indirect via server integration tests. Adding it is
    //! cumulative (test-plan R-04/R-17), and closes the `load_observation_session_stats` gap.
    use super::*;
    use crate::test_helpers::open_test_store;

    async fn insert_session(store: &SqlxStore, session_id: &str, started_at: i64) {
        sqlx::query(
            "INSERT INTO sessions (session_id, feature_cycle, started_at, status) \
             VALUES (?1, 'vnc-049', ?2, 0)",
        )
        .bind(session_id)
        .bind(started_at)
        .execute(store.write_pool_server())
        .await
        .expect("insert session");
    }

    async fn insert_obs(
        store: &SqlxStore,
        session_id: &str,
        ts_millis: i64,
        hook: &str,
        source_domain: Option<&str>,
        model_id: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO observations \
             (session_id, ts_millis, hook, tool, input, response_size, response_snippet, \
              source_domain, model_id) \
             VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, ?4, ?5)",
        )
        .bind(session_id)
        .bind(ts_millis)
        .bind(hook)
        .bind(source_domain)
        .bind(model_id)
        .execute(store.write_pool_server())
        .await
        .expect("insert observation");
    }

    /// C6: `fetch_observations_since` projects the new columns and returns their stored values;
    /// a NULL row surfaces None (no silent column loss, R-04).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_fetch_observations_since_selects_new_columns() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        insert_session(&store, "s1", 1000).await;
        insert_obs(
            &store,
            "s1",
            100,
            "PreToolUse",
            Some("opencode"),
            Some("ollama/qwen3-coder"),
        )
        .await;
        insert_obs(&store, "s1", 200, "PreToolUse", None, None).await;

        let (rows, watermark) = store.fetch_observations_since(0, 10).await.expect("fetch");
        assert_eq!(rows.len(), 2);
        assert!(watermark >= 2);

        let attributed = &rows[0];
        assert_eq!(attributed.source_domain.as_deref(), Some("opencode"));
        assert_eq!(attributed.model_id.as_deref(), Some("ollama/qwen3-coder"));

        let legacy = &rows[1];
        assert_eq!(legacy.source_domain, None);
        assert_eq!(legacy.model_id, None);
    }

    /// C6: `load_observations_for_sessions` projects the new columns and returns their values.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_observations_for_sessions_selects_new_columns() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        insert_session(&store, "s1", 1000).await;
        insert_obs(
            &store,
            "s1",
            100,
            "PostToolUse",
            Some("opencode"),
            Some("ollama/qwen3-coder"),
        )
        .await;

        let rows = store
            .load_observations_for_sessions(&["s1".to_string()])
            .await
            .expect("load");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_domain.as_deref(), Some("opencode"));
        assert_eq!(rows[0].model_id.as_deref(), Some("ollama/qwen3-coder"));
    }

    /// C6: NEW direct coverage for `load_observation_session_stats` (no test existed today).
    /// It is a GROUP BY aggregate that does not project the new columns; assert it still
    /// functions correctly with `source_domain`/`model_id` present on the joined rows.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_observation_session_stats_with_attribution_columns() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        insert_session(&store, "s1", 1000).await;
        insert_obs(
            &store,
            "s1",
            100,
            "PreToolUse",
            Some("opencode"),
            Some("ollama/qwen3-coder"),
        )
        .await;
        insert_obs(&store, "s1", 200, "PostToolUse", None, None).await;

        // started_at=1000 falls within [0, 2000].
        let stats = store
            .load_observation_session_stats(0, 2000)
            .await
            .expect("stats");
        assert_eq!(stats.len(), 1, "one session in the window");
        assert_eq!(stats[0].0, "s1");
        assert_eq!(stats[0].2, 2, "two observations counted for the session");
    }
}
