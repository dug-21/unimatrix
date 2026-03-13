//! Agent registry: identity, trust levels, and capabilities.
//!
//! Uses direct SQL against the agent_registry table (ADR-004, nxs-008).
//! Types re-exported from unimatrix_store::schema.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_store::Store;
use unimatrix_store::rusqlite::{self, OptionalExtension};

// Re-export types so existing `use crate::infra::registry::*` imports keep working.
pub use unimatrix_store::{AgentRecord, Capability, TrustLevel};

use crate::error::ServerError;

/// Result of an enrollment operation.
pub struct EnrollResult {
    /// Whether this was a create (true) or update (false).
    pub created: bool,
    /// The final agent record after enrollment.
    pub agent: AgentRecord,
}

/// When true, unknown agents auto-enroll with [Read, Write, Search].
/// When false (production), unknown agents auto-enroll with [Read, Search] only.
const PERMISSIVE_AUTO_ENROLL: bool = true;

/// Agent IDs that cannot be modified via enrollment (ADR-002).
const PROTECTED_AGENTS: &[&str] = &["system", "human"];

/// Manages agent identity, trust levels, and capabilities.
pub struct AgentRegistry {
    store: Arc<Store>,
}

impl AgentRegistry {
    /// Create a new registry backed by the given store.
    pub fn new(store: Arc<Store>) -> Result<Self, ServerError> {
        Ok(AgentRegistry { store })
    }

    /// Bootstrap default agents if they don't already exist.
    ///
    /// Creates "system" (System trust), "human" (Privileged trust), and
    /// "cortical-implant" (Internal trust) agents on first run.
    /// Idempotent -- safe to call on every startup.
    pub fn bootstrap_defaults(&self) -> Result<(), ServerError> {
        let now = current_unix_seconds();
        let conn = self.store.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| ServerError::Registry(e.to_string()))?;

        let result = (|| -> Result<(), ServerError> {
            // Bootstrap "system" if not present
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM agent_registry WHERE agent_id = 'system'",
                    [],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|e| ServerError::Registry(e.to_string()))?
                .unwrap_or(false);

            if !exists {
                let caps_json = serde_json::to_string(&[
                    Capability::Read as u8,
                    Capability::Write as u8,
                    Capability::Search as u8,
                    Capability::Admin as u8,
                ])
                .map_err(|e| ServerError::Registry(e.to_string()))?;
                conn.execute(
                    "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
                        allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
                    rusqlite::params![
                        "system",
                        TrustLevel::System as u8 as i64,
                        &caps_json,
                        now as i64
                    ],
                )
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            }

            // Bootstrap "human" if not present
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM agent_registry WHERE agent_id = 'human'",
                    [],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|e| ServerError::Registry(e.to_string()))?
                .unwrap_or(false);

            if !exists {
                let caps_json = serde_json::to_string(&[
                    Capability::Read as u8,
                    Capability::Write as u8,
                    Capability::Search as u8,
                    Capability::Admin as u8,
                ])
                .map_err(|e| ServerError::Registry(e.to_string()))?;
                conn.execute(
                    "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
                        allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
                    rusqlite::params![
                        "human",
                        TrustLevel::Privileged as u8 as i64,
                        &caps_json,
                        now as i64
                    ],
                )
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            }

            // Bootstrap "cortical-implant" if not present (col-006)
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM agent_registry WHERE agent_id = 'cortical-implant'",
                    [],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|e| ServerError::Registry(e.to_string()))?
                .unwrap_or(false);

            if !exists {
                let caps_json =
                    serde_json::to_string(&[Capability::Read as u8, Capability::Search as u8])
                        .map_err(|e| ServerError::Registry(e.to_string()))?;
                conn.execute(
                    "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
                        allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
                    rusqlite::params![
                        "cortical-implant",
                        TrustLevel::Internal as u8 as i64,
                        &caps_json,
                        now as i64
                    ],
                )
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(|e| ServerError::Registry(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Look up an agent by ID, auto-enrolling as Restricted if unknown.
    ///
    /// Uses a read-first optimization to avoid write transactions for known agents.
    pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError> {
        let conn = self.store.lock_conn();

        // Read-first: check if agent exists
        let record = conn
            .query_row(
                "SELECT agent_id, trust_level, capabilities, allowed_topics,
                        allowed_categories, enrolled_at, last_seen_at, active
                 FROM agent_registry WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                agent_from_row,
            )
            .optional()
            .map_err(|e| ServerError::Registry(e.to_string()))?;

        if let Some(r) = record {
            return Ok(r);
        }

        // Not found: auto-enroll as Restricted
        let now = current_unix_seconds();
        let default_caps = if PERMISSIVE_AUTO_ENROLL {
            vec![Capability::Read, Capability::Write, Capability::Search]
        } else {
            vec![Capability::Read, Capability::Search]
        };
        let caps_json = serialize_capabilities(&default_caps)?;

        conn.execute(
            "INSERT OR IGNORE INTO agent_registry (agent_id, trust_level, capabilities,
                allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
            rusqlite::params![
                agent_id,
                TrustLevel::Restricted as u8 as i64,
                &caps_json,
                now as i64
            ],
        )
        .map_err(|e| ServerError::Registry(e.to_string()))?;

        // Re-read (handles INSERT OR IGNORE race where another thread inserted first)
        conn.query_row(
            "SELECT agent_id, trust_level, capabilities, allowed_topics,
                    allowed_categories, enrolled_at, last_seen_at, active
             FROM agent_registry WHERE agent_id = ?1",
            rusqlite::params![agent_id],
            agent_from_row,
        )
        .map_err(|e| ServerError::Registry(e.to_string()))
    }

    /// Check if an agent has a specific capability.
    pub fn has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool, ServerError> {
        let conn = self.store.lock_conn();
        let record = conn
            .query_row(
                "SELECT agent_id, trust_level, capabilities, allowed_topics,
                        allowed_categories, enrolled_at, last_seen_at, active
                 FROM agent_registry WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                agent_from_row,
            )
            .optional()
            .map_err(|e| ServerError::Registry(e.to_string()))?
            .ok_or_else(|| ServerError::Registry(format!("agent '{agent_id}' not found")))?;
        Ok(record.capabilities.contains(&cap))
    }

    /// Require an agent to have a specific capability.
    ///
    /// Returns `Ok(())` if the agent has the capability, or
    /// `Err(ServerError::CapabilityDenied)` if not.
    pub fn require_capability(&self, agent_id: &str, cap: Capability) -> Result<(), ServerError> {
        if !self.has_capability(agent_id, cap)? {
            return Err(ServerError::CapabilityDenied {
                agent_id: agent_id.to_string(),
                capability: cap,
            });
        }
        Ok(())
    }

    /// Update the last_seen_at timestamp for an agent.
    pub fn update_last_seen(&self, agent_id: &str) -> Result<(), ServerError> {
        let now = current_unix_seconds();
        let conn = self.store.lock_conn();
        conn.execute(
            "UPDATE agent_registry SET last_seen_at = ?1 WHERE agent_id = ?2",
            rusqlite::params![now as i64, agent_id],
        )
        .map_err(|e| ServerError::Registry(e.to_string()))?;
        Ok(())
    }

    /// Enroll a new agent or update an existing agent's trust level and capabilities.
    ///
    /// Protected bootstrap agents ("system", "human") cannot be modified (ADR-002).
    /// Self-lockout is prevented: if caller equals target, Admin must remain in capabilities.
    pub fn enroll_agent(
        &self,
        caller_id: &str,
        target_id: &str,
        trust_level: TrustLevel,
        capabilities: Vec<Capability>,
    ) -> Result<EnrollResult, ServerError> {
        // 1. Protected agent check (ADR-002)
        if PROTECTED_AGENTS.contains(&target_id) {
            return Err(ServerError::ProtectedAgent {
                agent_id: target_id.to_string(),
            });
        }

        // 2. Self-lockout prevention
        if caller_id == target_id && !capabilities.contains(&Capability::Admin) {
            return Err(ServerError::SelfLockout);
        }

        let conn = self.store.lock_conn();

        // 3. Check if target already exists
        let existing = conn
            .query_row(
                "SELECT agent_id, trust_level, capabilities, allowed_topics,
                        allowed_categories, enrolled_at, last_seen_at, active
                 FROM agent_registry WHERE agent_id = ?1",
                rusqlite::params![target_id],
                agent_from_row,
            )
            .optional()
            .map_err(|e| ServerError::Registry(e.to_string()))?;

        let now = current_unix_seconds();
        let caps_json = serialize_capabilities(&capabilities)?;

        // 4. Build the agent record and persist
        let (created, record) = match existing {
            Some(existing_record) => {
                // UPDATE: preserve enrolled_at, active, allowed_topics, allowed_categories
                conn.execute(
                    "UPDATE agent_registry SET trust_level = ?1, capabilities = ?2,
                        last_seen_at = ?3 WHERE agent_id = ?4",
                    rusqlite::params![trust_level as u8 as i64, &caps_json, now as i64, target_id],
                )
                .map_err(|e| ServerError::Registry(e.to_string()))?;

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
                // CREATE: new agent with defaults
                conn.execute(
                    "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
                        allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
                    rusqlite::params![target_id, trust_level as u8 as i64, &caps_json, now as i64],
                )
                .map_err(|e| ServerError::Registry(e.to_string()))?;

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

        Ok(EnrollResult {
            created,
            agent: record,
        })
    }
}

/// Get the current time as unix seconds.
fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Construct an AgentRecord from a SQL row.
fn agent_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    let caps_json: String = row.get("capabilities")?;
    let cap_ints: Vec<u8> = serde_json::from_str(&caps_json).unwrap_or_default();
    let capabilities: Vec<Capability> = cap_ints
        .iter()
        .filter_map(|&v| Capability::try_from(v).ok())
        .collect();

    let topics_json: Option<String> = row.get("allowed_topics")?;
    let allowed_topics: Option<Vec<String>> =
        topics_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

    let cats_json: Option<String> = row.get("allowed_categories")?;
    let allowed_categories: Option<Vec<String>> =
        cats_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

    Ok(AgentRecord {
        agent_id: row.get("agent_id")?,
        trust_level: TrustLevel::try_from(row.get::<_, i64>("trust_level")? as u8)
            .unwrap_or(TrustLevel::Restricted),
        capabilities,
        allowed_topics,
        allowed_categories,
        enrolled_at: row.get::<_, i64>("enrolled_at")? as u64,
        last_seen_at: row.get::<_, i64>("last_seen_at")? as u64,
        active: row.get::<_, i64>("active")? != 0,
    })
}

/// Serialize capabilities as JSON array of u8 discriminants.
fn serialize_capabilities(capabilities: &[Capability]) -> Result<String, ServerError> {
    let cap_ints: Vec<u8> = capabilities.iter().map(|c| *c as u8).collect();
    serde_json::to_string(&cap_ints).map_err(|e| ServerError::Registry(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Arc<Store> {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        // Leak TempDir to keep it alive for the test
        let store = Store::open(&path).unwrap();
        std::mem::forget(dir);
        Arc::new(store)
    }

    #[test]
    fn test_bootstrap_creates_system_and_human() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let system = registry.resolve_or_enroll("system").unwrap();
        assert_eq!(system.trust_level, TrustLevel::System);
        assert_eq!(
            system.capabilities,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::Admin
            ]
        );

        let human = registry.resolve_or_enroll("human").unwrap();
        assert_eq!(human.trust_level, TrustLevel::Privileged);
    }

    #[test]
    fn test_bootstrap_idempotent() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let first = registry.resolve_or_enroll("system").unwrap();
        registry.bootstrap_defaults().unwrap();
        let second = registry.resolve_or_enroll("system").unwrap();

        assert_eq!(first.enrolled_at, second.enrolled_at);
    }

    #[test]
    fn test_enroll_unknown_agent() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let agent = registry.resolve_or_enroll("unknown-agent-123").unwrap();
        assert_eq!(agent.trust_level, TrustLevel::Restricted);
        // PERMISSIVE_AUTO_ENROLL=true grants Write to unknown agents
        assert_eq!(
            agent.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
    }

    #[test]
    fn test_enrolled_agent_has_write_when_permissive() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("new-agent").unwrap();
        // PERMISSIVE_AUTO_ENROLL=true grants Write
        assert!(agent.capabilities.contains(&Capability::Write));
    }

    #[test]
    fn test_enrolled_agent_lacks_admin() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("new-agent").unwrap();
        assert!(!agent.capabilities.contains(&Capability::Admin));
    }

    #[test]
    fn test_resolve_existing_agent() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let human = registry.resolve_or_enroll("human").unwrap();
        assert_eq!(human.trust_level, TrustLevel::Privileged);
        assert_eq!(
            human.capabilities,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::Admin
            ]
        );
    }

    #[test]
    fn test_has_capability_true() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        assert!(registry.has_capability("human", Capability::Read).unwrap());
        assert!(registry.has_capability("human", Capability::Write).unwrap());
    }

    #[test]
    fn test_has_capability_false() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        registry.resolve_or_enroll("agent-x").unwrap();
        // PERMISSIVE_AUTO_ENROLL=true grants Write, but Admin is never auto-granted
        assert!(
            !registry
                .has_capability("agent-x", Capability::Admin)
                .unwrap()
        );
    }

    #[test]
    fn test_require_capability_ok() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        assert!(
            registry
                .require_capability("human", Capability::Write)
                .is_ok()
        );
    }

    #[test]
    fn test_require_capability_denied() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        registry.resolve_or_enroll("agent-x").unwrap();
        // Admin is never auto-granted, so this should be denied
        let result = registry.require_capability("agent-x", Capability::Admin);
        assert!(matches!(result, Err(ServerError::CapabilityDenied { .. })));
    }

    #[test]
    fn test_update_last_seen() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let before = registry.resolve_or_enroll("human").unwrap();
        // Sleep briefly so timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));
        registry.update_last_seen("human").unwrap();
        let after = registry.resolve_or_enroll("human").unwrap();

        assert!(after.last_seen_at >= before.last_seen_at);
        // Capabilities should not change
        assert_eq!(before.capabilities, after.capabilities);
    }

    #[test]
    fn test_enroll_anonymous() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("anonymous").unwrap();
        assert_eq!(agent.trust_level, TrustLevel::Restricted);
        // PERMISSIVE_AUTO_ENROLL=true grants Write to anonymous too
        assert_eq!(
            agent.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
    }

    #[test]
    fn test_permissive_auto_enroll_grants_read_write_search() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        // When PERMISSIVE_AUTO_ENROLL is true, unknown agents get [Read, Write, Search]
        let agent = registry.resolve_or_enroll("brand-new-agent").unwrap();
        assert_eq!(agent.trust_level, TrustLevel::Restricted);
        assert!(agent.capabilities.contains(&Capability::Read));
        assert!(agent.capabilities.contains(&Capability::Write));
        assert!(agent.capabilities.contains(&Capability::Search));
        assert!(!agent.capabilities.contains(&Capability::Admin));
    }

    // -- alc-002: enroll_agent --

    #[test]
    fn test_enroll_new_agent_created() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let result = registry
            .enroll_agent(
                "human",
                "new-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Search],
            )
            .unwrap();

        assert!(result.created);
        assert_eq!(result.agent.trust_level, TrustLevel::Internal);
        assert_eq!(
            result.agent.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
        assert!(result.agent.active);
    }

    #[test]
    fn test_enroll_new_agent_enrolled_at_set() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let result = registry
            .enroll_agent(
                "human",
                "new-agent",
                TrustLevel::Internal,
                vec![Capability::Read],
            )
            .unwrap();

        assert!(result.agent.enrolled_at > 0);
    }

    #[test]
    fn test_enroll_update_existing_agent() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // Auto-enroll as Restricted
        let original = registry.resolve_or_enroll("worker").unwrap();
        assert_eq!(original.trust_level, TrustLevel::Restricted);

        // Update via enrollment
        let result = registry
            .enroll_agent(
                "human",
                "worker",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Search],
            )
            .unwrap();

        assert!(!result.created);
        assert_eq!(result.agent.trust_level, TrustLevel::Internal);
        assert_eq!(
            result.agent.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
    }

    #[test]
    fn test_enroll_update_preserves_enrolled_at() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let original = registry.resolve_or_enroll("worker").unwrap();
        let original_enrolled_at = original.enrolled_at;

        // Brief pause to ensure timestamps would differ
        std::thread::sleep(std::time::Duration::from_millis(10));

        let result = registry
            .enroll_agent(
                "human",
                "worker",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write],
            )
            .unwrap();

        assert_eq!(result.agent.enrolled_at, original_enrolled_at);
    }

    #[test]
    fn test_enroll_update_preserves_active() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let original = registry.resolve_or_enroll("worker").unwrap();
        assert!(original.active);

        let result = registry
            .enroll_agent(
                "human",
                "worker",
                TrustLevel::Internal,
                vec![Capability::Read],
            )
            .unwrap();

        assert!(result.agent.active);
    }

    #[test]
    fn test_enroll_rejects_system() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let result = registry.enroll_agent(
            "human",
            "system",
            TrustLevel::Internal,
            vec![Capability::Read],
        );
        assert!(matches!(result, Err(ServerError::ProtectedAgent { .. })));
    }

    #[test]
    fn test_enroll_rejects_human() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // Pre-enroll an admin agent
        registry
            .enroll_agent(
                "human",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Admin],
            )
            .unwrap();

        let result = registry.enroll_agent(
            "admin-agent",
            "human",
            TrustLevel::Internal,
            vec![Capability::Read],
        );
        assert!(matches!(result, Err(ServerError::ProtectedAgent { .. })));
    }

    #[test]
    fn test_enroll_allows_case_different_system() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // "SYSTEM" (uppercase) is NOT "system" -- case-sensitive IDs
        let result = registry.enroll_agent(
            "human",
            "SYSTEM",
            TrustLevel::Internal,
            vec![Capability::Read],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_enroll_protected_no_state_change() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let before = registry.resolve_or_enroll("system").unwrap();

        // Attempt to modify protected agent -- should fail
        let _ = registry.enroll_agent(
            "human",
            "system",
            TrustLevel::Restricted,
            vec![Capability::Read],
        );

        let after = registry.resolve_or_enroll("system").unwrap();
        assert_eq!(before.trust_level, after.trust_level);
        assert_eq!(before.capabilities, after.capabilities);
    }

    #[test]
    fn test_enroll_self_without_admin_rejected() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // Pre-enroll admin-agent with Admin
        registry
            .enroll_agent(
                "human",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Admin],
            )
            .unwrap();

        // Self-enrollment without Admin -> SelfLockout
        let result = registry.enroll_agent(
            "admin-agent",
            "admin-agent",
            TrustLevel::Internal,
            vec![Capability::Read, Capability::Write, Capability::Search],
        );
        assert!(matches!(result, Err(ServerError::SelfLockout)));
    }

    #[test]
    fn test_enroll_self_with_admin_allowed() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // Pre-enroll admin-agent
        registry
            .enroll_agent(
                "human",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Admin],
            )
            .unwrap();

        // Self-enrollment retaining Admin -> OK
        let result = registry
            .enroll_agent(
                "admin-agent",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Admin],
            )
            .unwrap();

        assert!(!result.created);
        assert!(result.agent.capabilities.contains(&Capability::Admin));
    }

    #[test]
    fn test_enroll_sequential_updates() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // First enrollment
        registry
            .enroll_agent(
                "human",
                "agent-x",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write],
            )
            .unwrap();

        // Second enrollment with different trust level
        let result = registry
            .enroll_agent(
                "human",
                "agent-x",
                TrustLevel::Restricted,
                vec![Capability::Read],
            )
            .unwrap();

        assert_eq!(result.agent.trust_level, TrustLevel::Restricted);
        assert_eq!(result.agent.capabilities, vec![Capability::Read]);
    }

    #[test]
    fn test_enroll_then_resolve() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        // Enroll with Write capability
        registry
            .enroll_agent(
                "human",
                "new-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Search],
            )
            .unwrap();

        // Resolve should return the enrolled record, not re-enroll as Restricted
        let resolved = registry.resolve_or_enroll("new-agent").unwrap();
        assert_eq!(resolved.trust_level, TrustLevel::Internal);
        assert!(resolved.capabilities.contains(&Capability::Write));
    }
}
