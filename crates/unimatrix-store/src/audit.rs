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

        // Defensive guard: metadata must never be stored as empty string (NFR-06).
        // Callers must supply "{}" as the minimum; warn and substitute if they don't.
        let metadata = if event.metadata.is_empty() {
            tracing::warn!(
                event_id = id,
                operation = %event.operation,
                "AuditEvent.metadata is empty; substituting '{{}}' sentinel"
            );
            "{}".to_string()
        } else {
            event.metadata
        };

        sqlx::query(
            "INSERT INTO audit_log
                (event_id, timestamp, session_id, agent_id,
                 operation, target_ids, outcome, detail,
                 credential_type, capability_used, agent_attribution, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(id as i64)
        .bind(now as i64)
        .bind(&event.session_id)
        .bind(&event.agent_id)
        .bind(&event.operation)
        .bind(&target_ids_json)
        .bind(event.outcome as u8 as i64)
        .bind(&event.detail)
        .bind(&event.credential_type)
        .bind(&event.capability_used)
        .bind(&event.agent_attribution)
        .bind(&metadata)
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
                    target_ids, outcome, detail,
                    credential_type, capability_used, agent_attribution, metadata
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
                    credential_type: r.get("credential_type"),
                    capability_used: r.get("capability_used"),
                    agent_attribution: r.get("agent_attribution"),
                    metadata: r.get("metadata"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Outcome;
    use crate::test_helpers::open_test_store;

    // -- AE-U-01: AuditEvent::default() yields correct sentinel values --

    #[test]
    fn test_audit_event_default_sentinel_credential_type_is_none() {
        let ae = AuditEvent::default();
        assert_eq!(
            ae.credential_type, "none",
            "credential_type sentinel must be 'none', not empty string"
        );
    }

    #[test]
    fn test_audit_event_default_sentinel_metadata_is_empty_object() {
        let ae = AuditEvent::default();
        assert_eq!(
            ae.metadata, "{}",
            "metadata sentinel must be '{{}}', not empty string"
        );
    }

    #[test]
    fn test_audit_event_default_sentinel_capability_used_is_empty() {
        let ae = AuditEvent::default();
        assert_eq!(ae.capability_used, "");
    }

    #[test]
    fn test_audit_event_default_sentinel_agent_attribution_is_empty() {
        let ae = AuditEvent::default();
        assert_eq!(ae.agent_attribution, "");
    }

    // -- AE-U-02: #[serde(default)] — 8-field legacy JSON gives empty-string defaults --

    #[test]
    fn test_audit_event_serde_default_legacy_json_gives_empty_strings() {
        // Simulate a pre-v25 AuditEvent JSON with only the original 8 fields.
        // Note: Outcome serializes as a variant string ("Success"), not a number.
        let json = r#"{
            "event_id": 1,
            "timestamp": 1000,
            "session_id": "mcp::test-session",
            "agent_id": "test-agent",
            "operation": "context_search",
            "target_ids": [],
            "outcome": "Success",
            "detail": "test detail"
        }"#;
        let ae: AuditEvent =
            serde_json::from_str(json).expect("must deserialize 8-field legacy JSON");
        // serde(default) for String gives "" — intentionally different from Default impl sentinels
        assert_eq!(
            ae.credential_type, "",
            "serde default for missing field must be '' (String::default), not 'none'"
        );
        assert_eq!(ae.capability_used, "");
        assert_eq!(ae.agent_attribution, "");
        assert_eq!(
            ae.metadata, "",
            "serde default for missing field must be '' (String::default), not '{{}}'"
        );
    }

    // Confirm serde path and Default path are distinct (R-13)
    #[test]
    fn test_audit_event_serde_default_differs_from_impl_default_sentinels() {
        // Outcome serializes as a variant string ("Success"), not a number.
        let json = r#"{"event_id":0,"timestamp":0,"session_id":"","agent_id":"","operation":"","target_ids":[],"outcome":"Success","detail":""}"#;
        let from_serde: AuditEvent = serde_json::from_str(json).unwrap();
        let from_default = AuditEvent::default();

        // serde gives "" for credential_type; Default gives "none"
        assert_ne!(from_serde.credential_type, from_default.credential_type);
        // serde gives "" for metadata; Default gives "{}"
        assert_ne!(from_serde.metadata, from_default.metadata);
    }

    // -- AE-I-01: log_audit_event → read_audit_event round-trip with all four fields --

    #[tokio::test]
    async fn test_audit_event_roundtrip_all_four_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let event = AuditEvent {
            event_id: 0, // assigned by log_audit_event
            timestamp: 0,
            session_id: "mcp::test-session".to_string(),
            agent_id: "test-agent".to_string(),
            operation: "context_store".to_string(),
            target_ids: vec![42],
            outcome: Outcome::Success,
            detail: "stored entry 42".to_string(),
            credential_type: "none".to_string(),
            capability_used: "write".to_string(),
            agent_attribution: "codex-mcp-client".to_string(),
            metadata: r#"{"client_type":"codex-mcp-client"}"#.to_string(),
        };

        let id = store
            .log_audit_event(event)
            .await
            .expect("log_audit_event must succeed");
        let returned = store
            .read_audit_event(id)
            .await
            .expect("read must succeed")
            .expect("event must be present");

        assert_eq!(returned.credential_type, "none");
        assert_eq!(returned.capability_used, "write");
        assert_eq!(returned.agent_attribution, "codex-mcp-client");
        assert_eq!(returned.metadata, r#"{"client_type":"codex-mcp-client"}"#);
        // metadata must be valid JSON
        serde_json::from_str::<serde_json::Value>(&returned.metadata)
            .expect("metadata must be valid JSON");
    }

    // -- AE-I-02: round-trip with metadata = "{}" — minimum value preserved --

    #[tokio::test]
    async fn test_audit_event_roundtrip_metadata_minimum_value() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let event = AuditEvent {
            metadata: "{}".to_string(),
            ..AuditEvent::default()
        };

        let id = store
            .log_audit_event(event)
            .await
            .expect("log must succeed");
        let returned = store.read_audit_event(id).await.unwrap().unwrap();

        assert_eq!(returned.metadata, "{}");
        serde_json::from_str::<serde_json::Value>(&returned.metadata)
            .expect("metadata must be valid JSON");
    }

    // -- AE-I-03: round-trip with AuditEvent::default() — all sentinel defaults preserved --

    #[tokio::test]
    async fn test_audit_event_roundtrip_default_sentinels_preserved() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let event = AuditEvent {
            operation: "test_op".to_string(),
            ..AuditEvent::default()
        };

        let id = store
            .log_audit_event(event)
            .await
            .expect("log must succeed");
        let returned = store.read_audit_event(id).await.unwrap().unwrap();

        assert_eq!(returned.credential_type, "none");
        assert_eq!(returned.capability_used, "");
        assert_eq!(returned.agent_attribution, "");
        assert_eq!(returned.metadata, "{}");
    }

    // -- AE-I-04: INSERT binds ?9–?12 — no column count mismatch --

    #[tokio::test]
    async fn test_audit_event_insert_all_twelve_fields_no_bind_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: "mcp::s1".to_string(),
            agent_id: "agent-x".to_string(),
            operation: "context_search".to_string(),
            target_ids: vec![1, 2, 3],
            outcome: Outcome::Success,
            detail: "detail text".to_string(),
            credential_type: "none".to_string(),
            capability_used: "search".to_string(),
            agent_attribution: "gemini-cli".to_string(),
            metadata: r#"{"client_type":"gemini-cli"}"#.to_string(),
        };

        let result = store.log_audit_event(event).await;
        assert!(
            result.is_ok(),
            "INSERT with 12 bindings must succeed: {result:?}"
        );
    }

    // -- AE-I-05: metadata JSON injection resistance via serde_json::json! --

    #[test]
    fn test_audit_event_metadata_json_injection_embedded_quotes() {
        let ct = r#"client"with"quotes"#;
        let metadata = serde_json::json!({"client_type": ct}).to_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata).expect("must be valid JSON");
        assert_eq!(parsed["client_type"].as_str().unwrap(), ct);
    }

    #[test]
    fn test_audit_event_metadata_json_injection_backslashes() {
        let ct = r"client\with\backslash";
        let metadata = serde_json::json!({"client_type": ct}).to_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata).expect("must be valid JSON");
        assert_eq!(parsed["client_type"].as_str().unwrap(), ct);
    }

    #[test]
    fn test_audit_event_metadata_json_injection_newlines() {
        let ct = "client\nwith\nnewline";
        let metadata = serde_json::json!({"client_type": ct}).to_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata).expect("must be valid JSON");
        assert_eq!(parsed["client_type"].as_str().unwrap(), ct);
    }

    #[test]
    fn test_audit_event_metadata_json_injection_attempt_treated_as_single_value() {
        // JSON injection attempt: value contains comma and colon
        let ct = r#"a","b":"c"#;
        let metadata = serde_json::json!({"client_type": ct}).to_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata).expect("must be valid JSON");
        // The entire string is treated as one value — no injection
        assert_eq!(parsed["client_type"].as_str().unwrap(), ct);
        // Must NOT have a "b" key at the top level
        assert!(
            parsed.get("b").is_none(),
            "JSON injection must not create extra keys"
        );
    }

    // -- AE-I-06: empty clientInfo.name → metadata = "{}" --

    #[test]
    fn test_audit_event_metadata_empty_client_type_gives_empty_object() {
        // When client_type is None (no map entry), metadata construction uses "{}"
        let client_type: Option<&str> = None;
        let metadata = match client_type.filter(|s| !s.is_empty()) {
            Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
            None => "{}".to_string(),
        };
        assert_eq!(metadata, "{}");
        // no client_type key in parsed result
        let parsed: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert!(parsed.get("client_type").is_none());
    }
}
