//! Topic delivery persistence for nxs-010.
//!
//! Provides CRUD operations on the `topic_deliveries` table for cross-session
//! topic aggregation. Consumed by col-020 (read + update counters) and
//! crt-018 (read).

use rusqlite::OptionalExtension;

use crate::db::Store;
use crate::error::{Result, StoreError};

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

fn row_to_topic_delivery(row: &rusqlite::Row<'_>) -> rusqlite::Result<TopicDeliveryRecord> {
    Ok(TopicDeliveryRecord {
        topic: row.get(0)?,
        created_at: row.get::<_, i64>(1)? as u64,
        completed_at: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
        status: row.get(3)?,
        github_issue: row.get(4)?,
        total_sessions: row.get(5)?,
        total_tool_calls: row.get(6)?,
        total_duration_secs: row.get(7)?,
        phases_completed: row.get(8)?,
    })
}

const TOPIC_DELIVERY_COLUMNS: &str = "topic, created_at, completed_at, status, github_issue, \
     total_sessions, total_tool_calls, total_duration_secs, phases_completed";

// -- Store methods (SQLite backend) --

impl Store {
    /// Insert or fully replace a topic delivery record.
    ///
    /// Uses INSERT OR REPLACE semantics: if a record with the same topic
    /// already exists, it is completely overwritten (including counter fields).
    /// Callers must not call this concurrently with `update_topic_delivery_counters`
    /// for the same topic (R-10).
    pub fn upsert_topic_delivery(&self, record: &TopicDeliveryRecord) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO topic_deliveries \
                (topic, created_at, completed_at, status, github_issue, \
                 total_sessions, total_tool_calls, total_duration_secs, phases_completed) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                &record.topic,
                record.created_at as i64,
                record.completed_at.map(|v| v as i64),
                &record.status,
                record.github_issue,
                record.total_sessions,
                record.total_tool_calls,
                record.total_duration_secs,
                &record.phases_completed,
            ],
        )
        .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Retrieve a single topic delivery by topic name.
    ///
    /// Returns `None` if no record exists for the given topic.
    pub fn get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>> {
        let conn = self.lock_conn();
        conn.query_row(
            &format!(
                "SELECT {} FROM topic_deliveries WHERE topic = ?1",
                TOPIC_DELIVERY_COLUMNS
            ),
            rusqlite::params![topic],
            row_to_topic_delivery,
        )
        .optional()
        .map_err(StoreError::Sqlite)
    }

    /// Atomically increment (or decrement) the counter fields for a topic.
    ///
    /// Returns an error if the topic does not exist (R-07). This prevents
    /// silent failures when downstream features try to update counters for
    /// a topic that was never created.
    pub fn update_topic_delivery_counters(
        &self,
        topic: &str,
        sessions_delta: i64,
        tool_calls_delta: i64,
        duration_delta: i64,
    ) -> Result<()> {
        let conn = self.lock_conn();
        let rows_affected = conn
            .execute(
                "UPDATE topic_deliveries \
                 SET total_sessions = total_sessions + ?1, \
                     total_tool_calls = total_tool_calls + ?2, \
                     total_duration_secs = total_duration_secs + ?3 \
                 WHERE topic = ?4",
                rusqlite::params![sessions_delta, tool_calls_delta, duration_delta, topic],
            )
            .map_err(StoreError::Sqlite)?;

        if rows_affected == 0 {
            return Err(StoreError::Deserialization(format!(
                "topic_delivery not found: {topic}"
            )));
        }
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
    pub fn set_topic_delivery_counters(
        &self,
        topic: &str,
        total_sessions: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
    ) -> Result<()> {
        let conn = self.lock_conn();
        let rows_affected = conn
            .execute(
                "UPDATE topic_deliveries \
                 SET total_sessions = ?1, \
                     total_tool_calls = ?2, \
                     total_duration_secs = ?3 \
                 WHERE topic = ?4",
                rusqlite::params![total_sessions, total_tool_calls, total_duration_secs, topic],
            )
            .map_err(StoreError::Sqlite)?;

        if rows_affected == 0 {
            return Err(StoreError::Deserialization(format!(
                "topic_delivery not found: {topic}"
            )));
        }
        Ok(())
    }

    /// List all topic deliveries ordered by created_at descending (newest first).
    pub fn list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM topic_deliveries ORDER BY created_at DESC",
                TOPIC_DELIVERY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], row_to_topic_delivery)
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::TestDb;

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

    #[test]
    fn test_upsert_topic_delivery_insert() {
        let db = TestDb::new();
        let store = db.store();

        let record = make_record("nxs-010", 1000);
        store.upsert_topic_delivery(&record).unwrap();

        let fetched = store.get_topic_delivery("nxs-010").unwrap().unwrap();
        assert_eq!(fetched.topic, "nxs-010");
        assert_eq!(fetched.created_at, 1000);
        assert_eq!(fetched.status, "active");
        assert_eq!(fetched.total_sessions, 0);
        assert_eq!(fetched.total_tool_calls, 0);
        assert_eq!(fetched.total_duration_secs, 0);
        assert!(fetched.completed_at.is_none());
        assert!(fetched.github_issue.is_none());
        assert!(fetched.phases_completed.is_none());
    }

    #[test]
    fn test_upsert_topic_delivery_replace() {
        let db = TestDb::new();
        let store = db.store();

        let mut record = make_record("nxs-010", 1000);
        record.total_sessions = 5;
        record.status = "active".to_string();
        store.upsert_topic_delivery(&record).unwrap();

        // Replace with different status and reset sessions
        record.status = "completed".to_string();
        record.total_sessions = 0;
        store.upsert_topic_delivery(&record).unwrap();

        let fetched = store.get_topic_delivery("nxs-010").unwrap().unwrap();
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.total_sessions, 0);
    }

    #[test]
    fn test_upsert_replace_overwrites_counters() {
        let db = TestDb::new();
        let store = db.store();

        let mut record = make_record("nxs-010", 1000);
        record.total_sessions = 5;
        record.total_tool_calls = 10;
        store.upsert_topic_delivery(&record).unwrap();

        // Increment counters
        store
            .update_topic_delivery_counters("nxs-010", 3, 5, 0)
            .unwrap();

        // Now upsert with zeroed counters — should overwrite
        record.total_sessions = 0;
        record.total_tool_calls = 0;
        store.upsert_topic_delivery(&record).unwrap();

        let fetched = store.get_topic_delivery("nxs-010").unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 0);
        assert_eq!(fetched.total_tool_calls, 0);
    }

    #[test]
    fn test_get_topic_delivery_not_found() {
        let db = TestDb::new();
        let store = db.store();

        let result = store.get_topic_delivery("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_topic_delivery_all_fields() {
        let db = TestDb::new();
        let store = db.store();

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
        store.upsert_topic_delivery(&record).unwrap();

        let fetched = store.get_topic_delivery("nxs-010").unwrap().unwrap();
        assert_eq!(fetched.topic, "nxs-010");
        assert_eq!(fetched.created_at, 1000);
        assert_eq!(fetched.completed_at, Some(2000));
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.github_issue, Some(42));
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
        assert_eq!(fetched.phases_completed.as_deref(), Some("design,delivery"));
    }

    #[test]
    fn test_update_topic_delivery_counters_increment() {
        let db = TestDb::new();
        let store = db.store();

        let mut record = make_record("topic-a", 1000);
        record.total_sessions = 2;
        record.total_tool_calls = 10;
        record.total_duration_secs = 500;
        store.upsert_topic_delivery(&record).unwrap();

        store
            .update_topic_delivery_counters("topic-a", 3, 5, 100)
            .unwrap();

        let fetched = store.get_topic_delivery("topic-a").unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 15);
        assert_eq!(fetched.total_duration_secs, 600);
    }

    #[test]
    fn test_update_topic_delivery_counters_decrement() {
        let db = TestDb::new();
        let store = db.store();

        let mut record = make_record("topic-b", 1000);
        record.total_sessions = 10;
        record.total_tool_calls = 20;
        record.total_duration_secs = 1000;
        store.upsert_topic_delivery(&record).unwrap();

        store
            .update_topic_delivery_counters("topic-b", -3, -5, -100)
            .unwrap();

        let fetched = store.get_topic_delivery("topic-b").unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 7);
        assert_eq!(fetched.total_tool_calls, 15);
        assert_eq!(fetched.total_duration_secs, 900);
    }

    #[test]
    fn test_update_topic_delivery_counters_nonexistent_topic_returns_error() {
        let db = TestDb::new();
        let store = db.store();

        let result = store.update_topic_delivery_counters("missing", 1, 1, 1);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("topic_delivery not found"),
            "expected 'topic_delivery not found' in: {err_msg}"
        );
    }

    #[test]
    fn test_list_topic_deliveries_empty() {
        let db = TestDb::new();
        let store = db.store();

        let result = store.list_topic_deliveries().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_topic_deliveries_ordered_by_created_at_desc() {
        let db = TestDb::new();
        let store = db.store();

        store
            .upsert_topic_delivery(&make_record("topic-old", 1000))
            .unwrap();
        store
            .upsert_topic_delivery(&make_record("topic-new", 3000))
            .unwrap();
        store
            .upsert_topic_delivery(&make_record("topic-mid", 2000))
            .unwrap();

        let result = store.list_topic_deliveries().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].created_at, 3000);
        assert_eq!(result[0].topic, "topic-new");
        assert_eq!(result[1].created_at, 2000);
        assert_eq!(result[1].topic, "topic-mid");
        assert_eq!(result[2].created_at, 1000);
        assert_eq!(result[2].topic, "topic-old");
    }

    // -- set_topic_delivery_counters tests (col-020 C4, ADR-002) --

    #[test]
    fn test_set_topic_delivery_counters_basic() {
        let db = TestDb::new();
        let store = db.store();

        store
            .upsert_topic_delivery(&make_record("test-topic", 1000))
            .unwrap();
        store
            .set_topic_delivery_counters("test-topic", 5, 100, 3600)
            .unwrap();

        let fetched = store.get_topic_delivery("test-topic").unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
    }

    #[test]
    fn test_set_topic_delivery_counters_idempotent() {
        let db = TestDb::new();
        let store = db.store();

        store
            .upsert_topic_delivery(&make_record("idem-topic", 1000))
            .unwrap();

        store
            .set_topic_delivery_counters("idem-topic", 5, 100, 3600)
            .unwrap();
        store
            .set_topic_delivery_counters("idem-topic", 5, 100, 3600)
            .unwrap();

        let fetched = store.get_topic_delivery("idem-topic").unwrap().unwrap();
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
    }

    #[test]
    fn test_set_topic_delivery_counters_overwrite() {
        let db = TestDb::new();
        let store = db.store();

        store
            .upsert_topic_delivery(&make_record("overwrite-topic", 1000))
            .unwrap();

        store
            .set_topic_delivery_counters("overwrite-topic", 5, 100, 3600)
            .unwrap();
        store
            .set_topic_delivery_counters("overwrite-topic", 10, 200, 7200)
            .unwrap();

        let fetched = store
            .get_topic_delivery("overwrite-topic")
            .unwrap()
            .unwrap();
        // Absolute set, not additive: values should be (10, 200, 7200), not (15, 300, 10800)
        assert_eq!(fetched.total_sessions, 10);
        assert_eq!(fetched.total_tool_calls, 200);
        assert_eq!(fetched.total_duration_secs, 7200);
    }

    #[test]
    fn test_set_topic_delivery_counters_missing_record() {
        let db = TestDb::new();
        let store = db.store();

        let result = store.set_topic_delivery_counters("nonexistent", 1, 1, 1);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("topic_delivery not found"),
            "expected 'topic_delivery not found' in: {err_msg}"
        );
    }

    #[test]
    fn test_set_topic_delivery_counters_preserves_non_counter_fields() {
        let db = TestDb::new();
        let store = db.store();

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
        store.upsert_topic_delivery(&record).unwrap();

        store
            .set_topic_delivery_counters("preserve-topic", 5, 100, 3600)
            .unwrap();

        let fetched = store.get_topic_delivery("preserve-topic").unwrap().unwrap();
        assert_eq!(fetched.status, "completed");
        assert_eq!(fetched.github_issue, Some(42));
        assert_eq!(fetched.completed_at, Some(2000));
        assert_eq!(fetched.phases_completed.as_deref(), Some("design,delivery"));
        // Counters updated
        assert_eq!(fetched.total_sessions, 5);
        assert_eq!(fetched.total_tool_calls, 100);
        assert_eq!(fetched.total_duration_secs, 3600);
    }

    #[test]
    fn test_upsert_topic_delivery_nullable_fields() {
        let db = TestDb::new();
        let store = db.store();

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
        store.upsert_topic_delivery(&record).unwrap();

        let fetched = store.get_topic_delivery("nullable-test").unwrap().unwrap();
        assert!(fetched.completed_at.is_none());
        assert!(fetched.github_issue.is_none());
        assert!(fetched.phases_completed.is_none());
    }
}
