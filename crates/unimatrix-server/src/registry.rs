//! Agent registry: identity, trust levels, and capabilities.
//!
//! Uses the AGENT_REGISTRY redb table for persistence.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use unimatrix_store::{AGENT_REGISTRY, Store};

use crate::error::ServerError;

/// An enrolled agent's identity and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRecord {
    /// Unique agent identifier.
    pub agent_id: String,
    /// Agent's position in the trust hierarchy.
    pub trust_level: TrustLevel,
    /// Permissions granted to this agent.
    pub capabilities: Vec<Capability>,
    /// Optional topic restrictions (None = all topics allowed).
    pub allowed_topics: Option<Vec<String>>,
    /// Optional category restrictions (None = all categories allowed).
    pub allowed_categories: Option<Vec<String>>,
    /// Unix timestamp of enrollment.
    pub enrolled_at: u64,
    /// Unix timestamp of last interaction.
    pub last_seen_at: u64,
    /// Whether the agent is active.
    pub active: bool,
}

/// Agent trust hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Unimatrix internal operations.
    System,
    /// Human user via MCP client.
    Privileged,
    /// Orchestrator agents (scrum-master, etc).
    Internal,
    /// Unknown/worker agents (default for auto-enrollment).
    Restricted,
}

/// Atomic permission unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Read context entries.
    Read,
    /// Write (store) context entries.
    Write,
    /// Search context entries.
    Search,
    /// Administrative operations.
    Admin,
}

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
    /// Creates "system" (System trust) and "human" (Privileged trust)
    /// agents on first run. Idempotent -- safe to call on every startup.
    pub fn bootstrap_defaults(&self) -> Result<(), ServerError> {
        let now = current_unix_seconds();
        let txn = self
            .store
            .begin_write()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        {
            let mut table = txn
                .open_table(AGENT_REGISTRY)
                .map_err(|e| ServerError::Registry(e.to_string()))?;

            // Bootstrap "system" if not present
            if table
                .get("system")
                .map_err(|e| ServerError::Registry(e.to_string()))?
                .is_none()
            {
                let record = AgentRecord {
                    agent_id: "system".to_string(),
                    trust_level: TrustLevel::System,
                    capabilities: vec![
                        Capability::Read,
                        Capability::Write,
                        Capability::Search,
                        Capability::Admin,
                    ],
                    allowed_topics: None,
                    allowed_categories: None,
                    enrolled_at: now,
                    last_seen_at: now,
                    active: true,
                };
                let bytes = serialize_agent(&record)?;
                table
                    .insert("system", bytes.as_slice())
                    .map_err(|e| ServerError::Registry(e.to_string()))?;
            }

            // Bootstrap "human" if not present
            if table
                .get("human")
                .map_err(|e| ServerError::Registry(e.to_string()))?
                .is_none()
            {
                let record = AgentRecord {
                    agent_id: "human".to_string(),
                    trust_level: TrustLevel::Privileged,
                    capabilities: vec![
                        Capability::Read,
                        Capability::Write,
                        Capability::Search,
                        Capability::Admin,
                    ],
                    allowed_topics: None,
                    allowed_categories: None,
                    enrolled_at: now,
                    last_seen_at: now,
                    active: true,
                };
                let bytes = serialize_agent(&record)?;
                table
                    .insert("human", bytes.as_slice())
                    .map_err(|e| ServerError::Registry(e.to_string()))?;
            }
        }
        txn.commit()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        Ok(())
    }

    /// Look up an agent by ID, auto-enrolling as Restricted if unknown.
    ///
    /// Uses a read-first optimization to avoid write transactions for known agents.
    pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError> {
        // Read-first: check if agent exists
        {
            let read_txn = self
                .store
                .begin_read()
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            let table = read_txn
                .open_table(AGENT_REGISTRY)
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            if let Some(guard) = table
                .get(agent_id)
                .map_err(|e| ServerError::Registry(e.to_string()))?
            {
                return deserialize_agent(guard.value());
            }
        }

        // Not found: auto-enroll as Restricted
        let now = current_unix_seconds();
        let new_agent = AgentRecord {
            agent_id: agent_id.to_string(),
            trust_level: TrustLevel::Restricted,
            capabilities: vec![Capability::Read, Capability::Search],
            allowed_topics: None,
            allowed_categories: None,
            enrolled_at: now,
            last_seen_at: now,
            active: true,
        };

        let txn = self
            .store
            .begin_write()
            .map_err(|e| ServerError::Registry(e.to_string()))?;

        // Double-check: another thread may have enrolled between read and write
        let already_exists = {
            let table = txn
                .open_table(AGENT_REGISTRY)
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            match table
                .get(agent_id)
                .map_err(|e| ServerError::Registry(e.to_string()))?
            {
                Some(guard) => Some(deserialize_agent(guard.value())?),
                None => None,
            }
        };

        if let Some(record) = already_exists {
            txn.commit()
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            return Ok(record);
        }

        {
            let mut table = txn
                .open_table(AGENT_REGISTRY)
                .map_err(|e| ServerError::Registry(e.to_string()))?;
            let bytes = serialize_agent(&new_agent)?;
            table
                .insert(agent_id, bytes.as_slice())
                .map_err(|e| ServerError::Registry(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        Ok(new_agent)
    }

    /// Check if an agent has a specific capability.
    pub fn has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool, ServerError> {
        let read_txn = self
            .store
            .begin_read()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        let table = read_txn
            .open_table(AGENT_REGISTRY)
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        let guard = table
            .get(agent_id)
            .map_err(|e| ServerError::Registry(e.to_string()))?
            .ok_or_else(|| ServerError::Registry(format!("agent '{agent_id}' not found")))?;
        let record = deserialize_agent(guard.value())?;
        Ok(record.capabilities.contains(&cap))
    }

    /// Require an agent to have a specific capability.
    ///
    /// Returns `Ok(())` if the agent has the capability, or
    /// `Err(ServerError::CapabilityDenied)` if not.
    pub fn require_capability(
        &self,
        agent_id: &str,
        cap: Capability,
    ) -> Result<(), ServerError> {
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
        let txn = self
            .store
            .begin_write()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        {
            let mut table = txn
                .open_table(AGENT_REGISTRY)
                .map_err(|e| ServerError::Registry(e.to_string()))?;

            // Read existing record, extracting bytes before releasing borrow
            let existing = {
                let guard = table
                    .get(agent_id)
                    .map_err(|e| ServerError::Registry(e.to_string()))?;
                match guard {
                    Some(g) => Some(deserialize_agent(g.value())?),
                    None => None,
                }
            };

            if let Some(mut record) = existing {
                record.last_seen_at = current_unix_seconds();
                let bytes = serialize_agent(&record)?;
                table
                    .insert(agent_id, bytes.as_slice())
                    .map_err(|e| ServerError::Registry(e.to_string()))?;
            }
        }
        txn.commit()
            .map_err(|e| ServerError::Registry(e.to_string()))?;
        Ok(())
    }
}

/// Get the current time as unix seconds.
fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Serialize an AgentRecord to bincode bytes using the serde-compatible path.
fn serialize_agent(record: &AgentRecord) -> Result<Vec<u8>, ServerError> {
    bincode::serde::encode_to_vec(record, bincode::config::standard())
        .map_err(|e| ServerError::Registry(format!("serialization failed: {e}")))
}

/// Deserialize an AgentRecord from bincode bytes using the serde-compatible path.
fn deserialize_agent(bytes: &[u8]) -> Result<AgentRecord, ServerError> {
    let (record, _) = bincode::serde::decode_from_slice::<AgentRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| ServerError::Registry(format!("deserialization failed: {e}")))?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Arc<Store> {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
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
        assert_eq!(agent.capabilities, vec![Capability::Read, Capability::Search]);
    }

    #[test]
    fn test_enrolled_agent_lacks_write() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("new-agent").unwrap();
        assert!(!agent.capabilities.contains(&Capability::Write));
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
        assert!(!registry.has_capability("agent-x", Capability::Write).unwrap());
    }

    #[test]
    fn test_require_capability_ok() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        assert!(registry.require_capability("human", Capability::Write).is_ok());
    }

    #[test]
    fn test_require_capability_denied() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        registry.resolve_or_enroll("agent-x").unwrap();
        let result = registry.require_capability("agent-x", Capability::Write);
        assert!(matches!(
            result,
            Err(ServerError::CapabilityDenied { .. })
        ));
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
    fn test_agent_record_roundtrip() {
        let record = AgentRecord {
            agent_id: "test".to_string(),
            trust_level: TrustLevel::Internal,
            capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
            allowed_topics: Some(vec!["auth".to_string()]),
            allowed_categories: None,
            enrolled_at: 1000,
            last_seen_at: 2000,
            active: true,
        };
        let bytes = serialize_agent(&record).unwrap();
        let deserialized = deserialize_agent(&bytes).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_enroll_anonymous() {
        let store = make_store();
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("anonymous").unwrap();
        assert_eq!(agent.trust_level, TrustLevel::Restricted);
        assert_eq!(agent.capabilities, vec![Capability::Read, Capability::Search]);
    }
}
