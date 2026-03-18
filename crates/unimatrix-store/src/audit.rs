//! Audit log async methods on SqlxStore.
//!
//! Provides SQL-backed async access to the `audit_log` table.
//! Replaces the old rusqlite-based `AuditLog` helper in unimatrix-server.

use sqlx::Row;

use crate::counters;
use crate::db::SqlxStore;
use crate::error::{Result, StoreError};
use crate::schema::{AuditEvent, Outcome};

impl SqlxStore {
    /// Append an audit event to the audit_log table.
    ///
    /// Assigns `event_id` (monotonically increasing via `next_audit_event_id` counter)
    /// and `timestamp` (current unix seconds). Returns the assigned event_id.
    pub async fn log_audit_event(&self, event: AuditEvent) -> Result<u64> {
        let pool = self.write_pool_server();
        let mut txn = pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let current_id = counters::read_counter(&mut *txn, "next_audit_event_id").await?;
        let id = if current_id == 0 { 1 } else { current_id };
        counters::set_counter(&mut *txn, "next_audit_event_id", id + 1).await?;

        let target_ids_json = serde_json::to_string(&event.target_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        let now = current_unix_seconds();

        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
                operation, target_ids, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(id as i64)
        .bind(now as i64)
        .bind(&event.session_id)
        .bind(&event.agent_id)
        .bind(&event.operation)
        .bind(&target_ids_json)
        .bind(event.outcome as u8 as i64)
        .bind(&event.detail)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(id)
    }

    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Only counts `context_store` and `context_correct` operations.
    pub async fn audit_write_count_since(&self, agent_id: &str, since: u64) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log
             WHERE agent_id = ?1 AND timestamp >= ?2
             AND operation IN ('context_store', 'context_correct')",
        )
        .bind(agent_id)
        .bind(since as i64)
        .fetch_one(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(count as u64)
    }

    /// Read an audit event by event_id (for tests).
    pub async fn read_audit_event(&self, event_id: u64) -> Result<Option<AuditEvent>> {
        let row = sqlx::query(
            "SELECT event_id, timestamp, session_id, agent_id, operation,
                    target_ids, outcome, detail
             FROM audit_log WHERE event_id = ?1",
        )
        .bind(event_id as i64)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        match row {
            None => Ok(None),
            Some(r) => {
                let target_ids_json: String = r.get("target_ids");
                let target_ids: Vec<u64> =
                    serde_json::from_str(&target_ids_json).unwrap_or_default();
                let outcome_byte = r.get::<i64, _>("outcome") as u8;
                let outcome = Outcome::try_from(outcome_byte).unwrap_or(Outcome::Error);
                Ok(Some(AuditEvent {
                    event_id: r.get::<i64, _>("event_id") as u64,
                    timestamp: r.get::<i64, _>("timestamp") as u64,
                    session_id: r.get("session_id"),
                    agent_id: r.get("agent_id"),
                    operation: r.get("operation"),
                    target_ids,
                    outcome,
                    detail: r.get("detail"),
                }))
            }
        }
    }
}

fn current_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
