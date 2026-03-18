//! Topic delivery persistence for nxs-010.
//!
//! Provides CRUD operations on the `topic_deliveries` table for cross-session
//! topic aggregation. Consumed by col-020 (read + update counters) and
//! crt-018 (read).

use sqlx::Row;

use crate::analytics::AnalyticsWrite;
use crate::db::{SqlxStore, map_pool_timeout};
use crate::error::{PoolKind, Result, StoreError};

// -- Types --

/// Persistent record for a single topic delivery lifecycle.
#[derive(Debug, Clone)]
pub struct TopicDeliveryRecord {
    /// Natural key — the topic/feature-cycle name (e.g. "nxs-010").
    pub topic: String,
    /// Unix epoch seconds when this topic was first seen.
    pub created_at: u64,
    /// Unix epoch seconds when this topic was marked completed (if ever).
    pub completed_at: Option<u64>,
    /// Lifecycle status: "active" or "completed".
    pub status: String,
    /// Associated GitHub issue number (if any).
    pub github_issue: Option<i64>,
    /// Cumulative session count attributed to this topic.
    pub total_sessions: i64,
    /// Cumulative tool call count attributed to this topic.
    pub total_tool_calls: i64,
    /// Cumulative duration in seconds across all sessions.
    pub total_duration_secs: i64,
    /// Comma-separated list of completed phases (if any).
    pub phases_completed: Option<String>,
}

// -- Row helper --

fn row_to_topic_delivery(row: &sqlx::sqlite::SqliteRow) -> Result<TopicDeliveryRecord> {
    Ok(TopicDeliveryRecord {
        topic: row.try_get(0).map_err(|e| StoreError::Database(e.into()))?,
        created_at: row
            .try_get::<i64, _>(1)
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        completed_at: row
            .try_get::<Option<i64>, _>(2)
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64),
        status: row.try_get(3).map_err(|e| StoreError::Database(e.into()))?,
        github_issue: row.try_get(4).map_err(|e| StoreError::Database(e.into()))?,
        total_sessions: row.try_get(5).map_err(|e| StoreError::Database(e.into()))?,
        total_tool_calls: row.try_get(6).map_err(|e| StoreError::Database(e.into()))?,
        total_duration_secs: row.try_get(7).map_err(|e| StoreError::Database(e.into()))?,
        phases_completed: row
            .try_get::<Option<String>, _>(8)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

// -- Store methods --

impl SqlxStore {
    /// Insert or fully replace a topic delivery record (analytics write via enqueue_analytics).
    ///
    /// Uses INSERT OR REPLACE semantics via the drain task: if a record with the same topic
    /// already exists, it is completely overwritten (including counter fields).
    pub fn upsert_topic_delivery(&self, record: &TopicDeliveryRecord) {
        self.enqueue_analytics(AnalyticsWrite::TopicDelivery {
            topic: record.topic.clone(),
            created_at: record.created_at as i64,
            completed_at: record.completed_at.map(|v| v as i64),
            status: record.status.clone(),
            github_issue: record.github_issue,
            total_sessions: record.total_sessions,
            total_tool_calls: record.total_tool_calls,
            total_duration_secs: record.total_duration_secs,
            phases_completed: record.phases_completed.clone(),
        });
    }

    /// Retrieve a single topic delivery by topic name.
    ///
    /// Returns `None` if no record exists for the given topic.
    pub async fn get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>> {
        let row = sqlx::query(
            "SELECT topic, created_at, completed_at, status, github_issue, \
                    total_sessions, total_tool_calls, total_duration_secs, phases_completed \
             FROM topic_deliveries WHERE topic = ?1",
        )
        .bind(topic)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        match row {
            Some(r) => Ok(Some(row_to_topic_delivery(&r)?)),
            None => Ok(None),
        }
    }

    /// Atomically increment (or decrement) the counter fields for a topic.
    ///
    /// Returns an error if the topic does not exist (R-07). This prevents
    /// silent failures when downstream features try to update counters for
    /// a topic that was never created.
    ///
    /// Uses write_pool directly (not enqueue_analytics) because we need to
    /// check rows_affected and report an error if the topic doesn't exist.
    pub async fn update_topic_delivery_counters(
        &self,
        topic: &str,
        sessions_delta: i64,
        tool_calls_delta: i64,
        duration_delta: i64,
    ) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let result = sqlx::query(
            "UPDATE topic_deliveries \
             SET total_sessions = total_sessions + ?1, \
                 total_tool_calls = total_tool_calls + ?2, \
                 total_duration_secs = total_duration_secs + ?3 \
             WHERE topic = ?4",
        )
        .bind(sessions_delta)
        .bind(tool_calls_delta)
        .bind(duration_delta)
        .bind(topic)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        if result.rows_affected() == 0 {
            return Err(StoreError::Deserialization(format!(
                "topic_delivery not found: {topic}"
            )));
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Set the counter fields for a topic to absolute values (ADR-002: idempotent).
    ///
    /// Unlike `update_topic_delivery_counters` which applies additive deltas,
    /// this method overwrites the counter fields with the provided values.
    /// Repeated calls with the same values produce the same result.
    ///
    /// Returns an error if the topic does not exist. The caller should ensure
    /// the record exists (via `upsert_topic_delivery`) before calling this.
    ///
    /// Uses write_pool directly (not enqueue_analytics) because we need to
    /// check rows_affected and report an error if the topic doesn't exist.
    pub async fn set_topic_delivery_counters(
        &self,
        topic: &str,
        total_sessions: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
    ) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let result = sqlx::query(
            "UPDATE topic_deliveries \
             SET total_sessions = ?1, \
                 total_tool_calls = ?2, \
                 total_duration_secs = ?3 \
             WHERE topic = ?4",
        )
        .bind(total_sessions)
        .bind(total_tool_calls)
        .bind(total_duration_secs)
        .bind(topic)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        if result.rows_affected() == 0 {
            return Err(StoreError::Deserialization(format!(
                "topic_delivery not found: {topic}"
            )));
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// List all topic deliveries ordered by created_at descending (newest first).
    pub async fn list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>> {
        let rows = sqlx::query(
            "SELECT topic, created_at, completed_at, status, github_issue, \
                    total_sessions, total_tool_calls, total_duration_secs, phases_completed \
             FROM topic_deliveries ORDER BY created_at DESC",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter().map(row_to_topic_delivery).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::open_test_store;

    fn make_record(topic: &str, created_at: u64) -> TopicDeliveryRecord {
        TopicDeliveryRecord {
            topic: topic.to_string(),
            created_at,
            completed_at: None,
            status: "active".to_string(),
            github_issue: None,
            total_sessions: 0,
            total_tool_calls: 0,
            total_duration_secs: 0,
            phases_completed: None,
        }
    }

    /// Flush the analytics drain for a test store.
    ///
    /// Closes the store (triggers drain shutdown flush) and re-opens it.
    /// Returns the path so the caller can re-open if needed.
    async fn flush_analytics(store: SqlxStore, dir: &tempfile::TempDir) -> SqlxStore {
        store.close().await.expect("close failed");
        let path = dir.path().join("test.db");
        SqlxStore::open(&path, Default::default())
            .await
            .expect("re-open failed")
    }

    #[tokio::test]
    async fn test_upsert_topic_delivery_insert() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let record = make_record("nxs-010", 1000);
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        let fetched = store.get_topic_delivery("nxs-010").await.unwrap().unwrap();
        assert_eq!(fetched.topic, "nxs-010");
        assert_eq!(fetched.created_at, 1000);
        assert_eq!(fetched.status, "active");
        assert_eq!(fetched.total_sessions, 0);
        assert_eq!(fetched.total_tool_calls, 0);
        assert_eq!(fetched.total_duration_secs, 0);
        assert!(fetched.completed_at.is_none());
        assert!(fetched.github_issue.is_none());
        assert!(fetched.phases_completed.is_none());
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_topic_delivery_replace() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let mut record = make_record("nxs-010", 1000);
        record.total_sessions = 5;
        record.status = "active".to_string();
        store.upsert_topic_delivery(&record);

        // Replace with different status and reset sessions
        record.status = "completed".to_string();
        record.total_sessions = 0;
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        let fetched = store.get_topic_delivery("nxs-010").await.unwrap().unwrap();
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.total_sessions, 0);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_replace_overwrites_counters() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let mut record = make_record("nxs-010", 1000);
        record.total_sessions = 5;
        record.total_tool_calls = 10;
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        // Increment counters via direct write
        store
            .update_topic_delivery_counters("nxs-010", 3, 5, 0)
            .await
            .unwrap();

        // Now upsert with zeroed counters — should overwrite
        record.total_sessions = 0;
        record.total_tool_calls = 0;
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        let fetched = store.get_topic_delivery("nxs-010").await.unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 0);
        assert_eq!(fetched.total_tool_calls, 0);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_topic_delivery_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let result = store.get_topic_delivery("nonexistent").await.unwrap();
        assert!(result.is_none());
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_topic_delivery_all_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let record = TopicDeliveryRecord {
            topic: "nxs-010".to_string(),
            created_at: 1000,
            completed_at: Some(2000),
            status: "completed".to_string(),
            github_issue: Some(42),
            total_sessions: 5,
            total_tool_calls: 100,
            total_duration_secs: 3600,
            phases_completed: Some("design,delivery".to_string()),
        };
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        let fetched = store.get_topic_delivery("nxs-010").await.unwrap().unwrap();
        assert_eq!(fetched.topic, "nxs-010");
        assert_eq!(fetched.created_at, 1000);
        assert_eq!(fetched.completed_at, Some(2000));
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.github_issue, Some(42));
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
        assert_eq!(fetched.phases_completed.as_deref(), Some("design,delivery"));
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_topic_delivery_counters_increment() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let mut record = make_record("topic-a", 1000);
        record.total_sessions = 2;
        record.total_tool_calls = 10;
        record.total_duration_secs = 500;
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        store
            .update_topic_delivery_counters("topic-a", 3, 5, 100)
            .await
            .unwrap();

        let fetched = store.get_topic_delivery("topic-a").await.unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 15);
        assert_eq!(fetched.total_duration_secs, 600);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_topic_delivery_counters_decrement() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let mut record = make_record("topic-b", 1000);
        record.total_sessions = 10;
        record.total_tool_calls = 20;
        record.total_duration_secs = 1000;
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        store
            .update_topic_delivery_counters("topic-b", -3, -5, -100)
            .await
            .unwrap();

        let fetched = store.get_topic_delivery("topic-b").await.unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 7);
        assert_eq!(fetched.total_tool_calls, 15);
        assert_eq!(fetched.total_duration_secs, 900);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_update_topic_delivery_counters_nonexistent_topic_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let result = store
            .update_topic_delivery_counters("missing", 1, 1, 1)
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("topic_delivery not found"),
            "expected 'topic_delivery not found' in: {err_msg}"
        );
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_topic_deliveries_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let result = store.list_topic_deliveries().await.unwrap();
        assert!(result.is_empty());
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_topic_deliveries_ordered_by_created_at_desc() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        store.upsert_topic_delivery(&make_record("topic-old", 1000));
        store.upsert_topic_delivery(&make_record("topic-new", 3000));
        store.upsert_topic_delivery(&make_record("topic-mid", 2000));
        let store = flush_analytics(store, &dir).await;

        let result = store.list_topic_deliveries().await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].created_at, 3000);
        assert_eq!(result[0].topic, "topic-new");
        assert_eq!(result[1].created_at, 2000);
        assert_eq!(result[1].topic, "topic-mid");
        assert_eq!(result[2].created_at, 1000);
        assert_eq!(result[2].topic, "topic-old");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_set_topic_delivery_counters_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        store.upsert_topic_delivery(&make_record("test-topic", 1000));
        let store = flush_analytics(store, &dir).await;

        store
            .set_topic_delivery_counters("test-topic", 5, 100, 3600)
            .await
            .unwrap();

        let fetched = store
            .get_topic_delivery("test-topic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_set_topic_delivery_counters_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        store.upsert_topic_delivery(&make_record("idem-topic", 1000));
        let store = flush_analytics(store, &dir).await;

        store
            .set_topic_delivery_counters("idem-topic", 5, 100, 3600)
            .await
            .unwrap();
        store
            .set_topic_delivery_counters("idem-topic", 5, 100, 3600)
            .await
            .unwrap();

        let fetched = store
            .get_topic_delivery("idem-topic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_set_topic_delivery_counters_overwrite() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        store.upsert_topic_delivery(&make_record("overwrite-topic", 1000));
        let store = flush_analytics(store, &dir).await;

        store
            .set_topic_delivery_counters("overwrite-topic", 5, 100, 3600)
            .await
            .unwrap();
        store
            .set_topic_delivery_counters("overwrite-topic", 10, 200, 7200)
            .await
            .unwrap();

        let fetched = store
            .get_topic_delivery("overwrite-topic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.total_sessions, 10);
        assert_eq!(fetched.total_tool_calls, 200);
        assert_eq!(fetched.total_duration_secs, 7200);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_set_topic_delivery_counters_missing_record() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let result = store
            .set_topic_delivery_counters("nonexistent", 1, 1, 1)
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("topic_delivery not found"),
            "expected 'topic_delivery not found' in: {err_msg}"
        );
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_set_topic_delivery_counters_preserves_non_counter_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let record = TopicDeliveryRecord {
            topic: "preserve-topic".to_string(),
            created_at: 1000,
            completed_at: Some(2000),
            status: "completed".to_string(),
            github_issue: Some(42),
            total_sessions: 0,
            total_tool_calls: 0,
            total_duration_secs: 0,
            phases_completed: Some("design,delivery".to_string()),
        };
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        store
            .set_topic_delivery_counters("preserve-topic", 5, 100, 3600)
            .await
            .unwrap();

        let fetched = store
            .get_topic_delivery("preserve-topic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.github_issue, Some(42));
        assert_eq!(fetched.completed_at, Some(2000));
        assert_eq!(fetched.phases_completed.as_deref(), Some("design,delivery"));
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_topic_delivery_nullable_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let record = TopicDeliveryRecord {
            topic: "nullable-test".to_string(),
            created_at: 500,
            completed_at: None,
            status: "active".to_string(),
            github_issue: None,
            total_sessions: 0,
            total_tool_calls: 0,
            total_duration_secs: 0,
            phases_completed: None,
        };
        store.upsert_topic_delivery(&record);
        let store = flush_analytics(store, &dir).await;

        let fetched = store
            .get_topic_delivery("nullable-test")
            .await
            .unwrap()
            .unwrap();
        assert!(fetched.completed_at.is_none());
        assert!(fetched.github_issue.is_none());
        assert!(fetched.phases_completed.is_none());
        store.close().await.unwrap();
    }
}
