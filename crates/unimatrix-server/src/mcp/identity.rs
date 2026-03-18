//! Agent identity extraction and resolution.
//!
//! Extracts agent_id from tool parameters and resolves it against the registry.

use crate::error::ServerError;
use crate::infra::registry::{AgentRegistry, Capability, TrustLevel};

/// Resolved agent identity for downstream capability checks and audit logging.
#[derive(Debug, Clone)]
pub struct ResolvedIdentity {
    /// The agent's identifier.
    pub agent_id: String,
    /// The agent's trust level.
    pub trust_level: TrustLevel,
    /// The agent's capabilities.
    pub capabilities: Vec<Capability>,
}

/// Extract agent_id from tool parameter, defaulting to "anonymous".
///
/// Trims whitespace. Empty strings after trimming default to "anonymous".
pub fn extract_agent_id(agent_id: &Option<String>) -> String {
    match agent_id {
        Some(id) => {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                "anonymous".to_string()
            } else {
                trimmed.to_string()
            }
        }
        None => "anonymous".to_string(),
    }
}

/// Resolve an agent identity against the registry.
///
/// Looks up the agent, auto-enrolling if unknown, and updates last_seen.
/// Returns a `ResolvedIdentity` for downstream use.
pub fn resolve_identity(
    registry: &AgentRegistry,
    agent_id: &str,
) -> Result<ResolvedIdentity, ServerError> {
    let record = registry.resolve_or_enroll(agent_id)?;
    registry.update_last_seen(agent_id)?;

    Ok(ResolvedIdentity {
        agent_id: record.agent_id,
        trust_level: record.trust_level,
        capabilities: record.capabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_registry() -> AgentRegistry {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(
            rt.block_on(unimatrix_store::SqlxStore::open(
                &path,
                unimatrix_store::pool_config::PoolConfig::default(),
            ))
            .unwrap(),
        );
        std::mem::forget(dir);
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();
        registry
    }

    #[test]
    fn test_extract_some_value() {
        assert_eq!(
            extract_agent_id(&Some("test-agent".to_string())),
            "test-agent"
        );
    }

    #[test]
    fn test_extract_none() {
        assert_eq!(extract_agent_id(&None), "anonymous");
    }

    #[test]
    fn test_extract_empty_string() {
        assert_eq!(extract_agent_id(&Some("".to_string())), "anonymous");
    }

    #[test]
    fn test_extract_whitespace_only() {
        assert_eq!(extract_agent_id(&Some("   ".to_string())), "anonymous");
    }

    #[test]
    fn test_extract_trims() {
        assert_eq!(extract_agent_id(&Some("  test  ".to_string())), "test");
    }

    #[test]
    fn test_extract_special_characters() {
        assert_eq!(
            extract_agent_id(&Some("uni-architect-v2".to_string())),
            "uni-architect-v2"
        );
    }

    #[test]
    fn test_resolve_known_agent() {
        let registry = make_registry();
        let identity = resolve_identity(&registry, "human").unwrap();
        assert_eq!(identity.agent_id, "human");
        assert_eq!(identity.trust_level, TrustLevel::Privileged);
        assert_eq!(
            identity.capabilities,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::Admin
            ]
        );
    }

    #[test]
    fn test_resolve_unknown_agent() {
        let registry = make_registry();
        let identity = resolve_identity(&registry, "new-agent").unwrap();
        assert_eq!(identity.trust_level, TrustLevel::Restricted);
        // PERMISSIVE_AUTO_ENROLL=true grants Write to unknown agents
        assert_eq!(
            identity.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
    }

    #[test]
    fn test_resolve_anonymous() {
        let registry = make_registry();
        let identity = resolve_identity(&registry, "anonymous").unwrap();
        assert_eq!(identity.trust_level, TrustLevel::Restricted);
    }
}
