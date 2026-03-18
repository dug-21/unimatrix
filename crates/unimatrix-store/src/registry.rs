//! Agent registry async methods on SqlxStore.
//!
//! Provides SQL-backed async access to the `agent_registry` table.
//! Replaces the old rusqlite-based `AgentRegistry` helper in unimatrix-server.

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};
use crate::schema::{AgentRecord, Capability, TrustLevel};

impl SqlxStore {
    /// Bootstrap default agents (system, human, cortical-implant) if they don't exist.
    ///
    /// Idempotent — safe to call on every startup.
    pub async fn agent_bootstrap_defaults(&self) -> Result<()> {
        let now = current_unix_seconds();
        let pool = self.write_pool_server();
        let mut txn = pool
            .begin()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Bootstrap "system"
        let system_caps = serde_json::to_string(&[
            Capability::Read as u8,
            Capability::Write as u8,
            Capability::Search as u8,
            Capability::Admin as u8,
        ])
        .map_err(|e| StoreError::Serialization(e.to_string()))?;
        sqlx::query(
            "INSERT OR IGNORE INTO agent_registry
             (agent_id, trust_level, capabilities, allowed_topics, allowed_categories,
              enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
        )
        .bind("system")
        .bind(TrustLevel::System as u8 as i64)
        .bind(&system_caps)
        .bind(now as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Bootstrap "human"
        sqlx::query(
            "INSERT OR IGNORE INTO agent_registry
             (agent_id, trust_level, capabilities, allowed_topics, allowed_categories,
              enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
        )
        .bind("human")
        .bind(TrustLevel::Privileged as u8 as i64)
        .bind(&system_caps)
        .bind(now as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Bootstrap "cortical-implant"
        let ci_caps = serde_json::to_string(&[Capability::Read as u8, Capability::Search as u8])
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        sqlx::query(
            "INSERT OR IGNORE INTO agent_registry
             (agent_id, trust_level, capabilities, allowed_topics, allowed_categories,
              enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
        )
        .bind("cortical-implant")
        .bind(TrustLevel::Internal as u8 as i64)
        .bind(&ci_caps)
        .bind(now as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Look up an agent by ID.
    pub async fn agent_get(&self, agent_id: &str) -> Result<Option<AgentRecord>> {
        let row = sqlx::query(
            "SELECT agent_id, trust_level, capabilities, allowed_topics,
                    allowed_categories, enrolled_at, last_seen_at, active
             FROM agent_registry WHERE agent_id = ?1",
        )
        .bind(agent_id)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(row.map(|r| agent_from_row(&r)))
    }

    /// Look up an agent by ID, auto-enrolling as Restricted if not found.
    ///
    /// Returns the final agent record (existing or newly enrolled).
    ///
    /// When `session_caps` is `Some`, the provided capability set is used for newly enrolled
    /// agents instead of the permissive/strict default. Existing agents are returned as-is.
    /// When `session_caps` is `None`, the existing permissive/strict branch runs unchanged.
    /// All pre-dsn-001 call sites pass `None` to preserve current behavior.
    pub async fn agent_resolve_or_enroll(
        &self,
        agent_id: &str,
        permissive: bool,
        session_caps: Option<&[Capability]>,
    ) -> Result<AgentRecord> {
        // Read-first: avoid write lock for existing agents.
        if let Some(record) = self.agent_get(agent_id).await? {
            return Ok(record);
        }

        let now = current_unix_seconds();
        let default_caps: Vec<u8> = match session_caps {
            Some(caps) => {
                // Config-supplied capability set overrides permissive/strict default.
                caps.iter().map(|c| *c as u8).collect()
            }
            None => {
                // Existing permissive/strict branch — unchanged behavior.
                if permissive {
                    vec![
                        Capability::Read as u8,
                        Capability::Write as u8,
                        Capability::Search as u8,
                    ]
                } else {
                    vec![Capability::Read as u8, Capability::Search as u8]
                }
            }
        };
        let caps_json = serde_json::to_string(&default_caps)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        let pool = self.write_pool_server();

        // INSERT OR IGNORE handles the race where another caller enrolled first.
        sqlx::query(
            "INSERT OR IGNORE INTO agent_registry
             (agent_id, trust_level, capabilities, allowed_topics, allowed_categories,
              enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
        )
        .bind(agent_id)
        .bind(TrustLevel::Restricted as u8 as i64)
        .bind(&caps_json)
        .bind(now as i64)
        .execute(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Re-read to return the canonical row.
        let row = sqlx::query(
            "SELECT agent_id, trust_level, capabilities, allowed_topics,
                    allowed_categories, enrolled_at, last_seen_at, active
             FROM agent_registry WHERE agent_id = ?1",
        )
        .bind(agent_id)
        .fetch_one(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(agent_from_row(&row))
    }

    /// Update the last_seen_at timestamp for an agent.
    pub async fn agent_update_last_seen(&self, agent_id: &str) -> Result<()> {
        let now = current_unix_seconds();
        sqlx::query("UPDATE agent_registry SET last_seen_at = ?1 WHERE agent_id = ?2")
            .bind(now as i64)
            .bind(agent_id)
            .execute(self.write_pool_server())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Enroll or update an agent's trust level and capabilities.
    ///
    /// Returns `(created, AgentRecord)`. Does NOT enforce protected-agent or
    /// self-lockout checks — callers must do that before calling.
    pub async fn agent_enroll(
        &self,
        target_id: &str,
        trust_level: TrustLevel,
        capabilities: Vec<Capability>,
    ) -> Result<(bool, AgentRecord)> {
        let existing = self.agent_get(target_id).await?;
        let now = current_unix_seconds();
        let caps_ints: Vec<u8> = capabilities.iter().map(|c| *c as u8).collect();
        let caps_json = serde_json::to_string(&caps_ints)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        let pool = self.write_pool_server();

        let (created, record) = match existing {
            Some(existing_record) => {
                sqlx::query(
                    "UPDATE agent_registry SET trust_level = ?1, capabilities = ?2,
                        last_seen_at = ?3 WHERE agent_id = ?4",
                )
                .bind(trust_level as u8 as i64)
                .bind(&caps_json)
                .bind(now as i64)
                .bind(target_id)
                .execute(pool)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

                let updated = AgentRecord {
                    agent_id: target_id.to_string(),
                    trust_level,
                    capabilities,
                    allowed_topics: existing_record.allowed_topics,
                    allowed_categories: existing_record.allowed_categories,
                    enrolled_at: existing_record.enrolled_at,
                    last_seen_at: now,
                    active: existing_record.active,
                };
                (false, updated)
            }
            None => {
                sqlx::query(
                    "INSERT INTO agent_registry
                     (agent_id, trust_level, capabilities, allowed_topics, allowed_categories,
                      enrolled_at, last_seen_at, active)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
                )
                .bind(target_id)
                .bind(trust_level as u8 as i64)
                .bind(&caps_json)
                .bind(now as i64)
                .execute(pool)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

                let new_agent = AgentRecord {
                    agent_id: target_id.to_string(),
                    trust_level,
                    capabilities,
                    allowed_topics: None,
                    allowed_categories: None,
                    enrolled_at: now,
                    last_seen_at: now,
                    active: true,
                };
                (true, new_agent)
            }
        };
        Ok((created, record))
    }
}

/// Construct an AgentRecord from a sqlx row.
fn agent_from_row(row: &sqlx::sqlite::SqliteRow) -> AgentRecord {
    let caps_json: String = row.get("capabilities");
    let cap_ints: Vec<u8> = serde_json::from_str(&caps_json).unwrap_or_default();
    let capabilities: Vec<Capability> = cap_ints
        .iter()
        .filter_map(|&v| Capability::try_from(v).ok())
        .collect();

    let topics_json: Option<String> = row.get("allowed_topics");
    let allowed_topics: Option<Vec<String>> =
        topics_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

    let cats_json: Option<String> = row.get("allowed_categories");
    let allowed_categories: Option<Vec<String>> =
        cats_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

    AgentRecord {
        agent_id: row.get("agent_id"),
        trust_level: TrustLevel::try_from(row.get::<i64, _>("trust_level") as u8)
            .unwrap_or(TrustLevel::Restricted),
        capabilities,
        allowed_topics,
        allowed_categories,
        enrolled_at: row.get::<i64, _>("enrolled_at") as u64,
        last_seen_at: row.get::<i64, _>("last_seen_at") as u64,
        active: row.get::<i64, _>("active") != 0,
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
    use crate::pool_config::PoolConfig;

    async fn open_test_store() -> SqlxStore {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::default())
            .await
            .expect("open test store");
        // Leak dir so the db file stays alive for the test.
        std::mem::forget(dir);
        store
    }

    /// IR-02: session_caps=None + permissive=true → existing full capability set (Read+Write+Search).
    #[tokio::test]
    async fn test_agent_resolve_or_enroll_none_caps_uses_permissive_default() {
        let store = open_test_store().await;
        let record = store
            .agent_resolve_or_enroll("test-agent-none", true, None)
            .await
            .unwrap();
        assert!(record.capabilities.contains(&Capability::Read));
        assert!(record.capabilities.contains(&Capability::Write));
        assert!(record.capabilities.contains(&Capability::Search));
    }

    /// IR-02: session_caps=None + permissive=false → strict (Read+Search, no Write).
    #[tokio::test]
    async fn test_agent_resolve_or_enroll_none_caps_strict_default() {
        let store = open_test_store().await;
        let record = store
            .agent_resolve_or_enroll("test-agent-strict", false, None)
            .await
            .unwrap();
        assert!(record.capabilities.contains(&Capability::Read));
        assert!(
            !record.capabilities.contains(&Capability::Write),
            "strict mode must not grant Write by default"
        );
    }

    /// R-14/AC-06: session_caps=Some([Read, Search]) overrides permissive=true default.
    #[tokio::test]
    async fn test_agent_resolve_or_enroll_some_caps_overrides_permissive() {
        let store = open_test_store().await;
        let caps = [Capability::Read, Capability::Search];
        let record = store
            .agent_resolve_or_enroll("test-agent-caps", true, Some(&caps))
            .await
            .unwrap();
        assert!(record.capabilities.contains(&Capability::Read));
        assert!(record.capabilities.contains(&Capability::Search));
        assert!(
            !record.capabilities.contains(&Capability::Write),
            "Some(session_caps) must override permissive default; Write must not be added"
        );
        assert_eq!(
            record.capabilities.len(),
            2,
            "capabilities must be exactly the provided set, no extras"
        );
    }

    /// R-14: session_caps=Some([Read]) only — exactly one capability stored.
    #[tokio::test]
    async fn test_agent_resolve_or_enroll_some_caps_read_only() {
        let store = open_test_store().await;
        let caps = [Capability::Read];
        let record = store
            .agent_resolve_or_enroll("test-agent-readonly", true, Some(&caps))
            .await
            .unwrap();
        assert_eq!(record.capabilities, vec![Capability::Read]);
    }
}
