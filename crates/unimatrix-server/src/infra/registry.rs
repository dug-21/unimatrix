//! Agent registry: identity, trust levels, and capabilities.
//!
//! Rewritten for nxs-011: all database access via async SqlxStore methods.
//! Delegates to unimatrix_store::SqlxStore agent_* methods.

use std::sync::Arc;

use unimatrix_store::SqlxStore;

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
    store: Arc<SqlxStore>,
}

impl AgentRegistry {
    /// Create a new registry backed by the given store.
    pub fn new(store: Arc<SqlxStore>) -> Result<Self, ServerError> {
        Ok(AgentRegistry { store })
    }

    /// Bootstrap default agents if they don't already exist.
    ///
    /// Creates "system" (System trust), "human" (Privileged trust), and
    /// "cortical-implant" (Internal trust) agents on first run.
    /// Idempotent -- safe to call on every startup.
    pub fn bootstrap_defaults(&self) -> Result<(), ServerError> {
        block_sync(self.store.agent_bootstrap_defaults())
            .map_err(|e| ServerError::Registry(e.to_string()))
    }

    /// Look up an agent by ID, auto-enrolling as Restricted if unknown.
    ///
    /// Uses a read-first optimization to avoid write transactions for known agents.
    pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError> {
        block_sync(
            self.store
                .agent_resolve_or_enroll(agent_id, PERMISSIVE_AUTO_ENROLL),
        )
        .map_err(|e| ServerError::Registry(e.to_string()))
    }

    /// Check if an agent has a specific capability.
    pub fn has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool, ServerError> {
        let record = block_sync(self.store.agent_get(agent_id))
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
        block_sync(self.store.agent_update_last_seen(agent_id))
            .map_err(|e| ServerError::Registry(e.to_string()))
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

        let (created, agent) = block_sync(self.store.agent_enroll(
            target_id,
            trust_level,
            capabilities,
        ))
        .map_err(|e| ServerError::Registry(e.to_string()))?;

        Ok(EnrollResult { created, agent })
    }
}

/// Bridge an async future to sync context.
///
/// When called from within a multi-thread tokio runtime, uses `block_in_place`
/// to avoid nesting runtimes. When called from a sync context (no runtime),
/// creates a temporary current-thread runtime.
fn block_sync<F, T, E>(fut: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(fut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_store::pool_config::PoolConfig;

    async fn make_store() -> Arc<SqlxStore> {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::default())
            .await
            .expect("open store");
        std::mem::forget(dir);
        Arc::new(store)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bootstrap_creates_system_and_human() {
        let store = make_store().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bootstrap_idempotent() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let first = registry.resolve_or_enroll("system").unwrap();
        registry.bootstrap_defaults().unwrap();
        let second = registry.resolve_or_enroll("system").unwrap();

        assert_eq!(first.enrolled_at, second.enrolled_at);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enroll_unknown_agent() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let agent = registry.resolve_or_enroll("unknown-agent-123").unwrap();
        assert_eq!(agent.trust_level, TrustLevel::Restricted);
        assert_eq!(
            agent.capabilities,
            vec![Capability::Read, Capability::Write, Capability::Search]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enrolled_agent_has_write_when_permissive() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("new-agent").unwrap();
        assert!(agent.capabilities.contains(&Capability::Write));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enrolled_agent_lacks_admin() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("new-agent").unwrap();
        assert!(!agent.capabilities.contains(&Capability::Admin));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_resolve_existing_agent() {
        let store = make_store().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_has_capability_true() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        assert!(registry.has_capability("human", Capability::Read).unwrap());
        assert!(registry.has_capability("human", Capability::Write).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_has_capability_false() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();

        registry.resolve_or_enroll("agent-x").unwrap();
        assert!(
            !registry
                .has_capability("agent-x", Capability::Admin)
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_require_capability_ok() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        assert!(
            registry
                .require_capability("human", Capability::Write)
                .is_ok()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_require_capability_denied() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();

        registry.resolve_or_enroll("agent-x").unwrap();
        let result = registry.require_capability("agent-x", Capability::Admin);
        assert!(matches!(result, Err(ServerError::CapabilityDenied { .. })));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_update_last_seen() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let before = registry.resolve_or_enroll("human").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        registry.update_last_seen("human").unwrap();
        let after = registry.resolve_or_enroll("human").unwrap();

        assert!(after.last_seen_at >= before.last_seen_at);
        assert_eq!(before.capabilities, after.capabilities);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enroll_rejects_system() {
        let store = make_store().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enroll_self_without_admin_rejected() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        registry
            .enroll_agent(
                "human",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Write, Capability::Admin],
            )
            .unwrap();

        let result = registry.enroll_agent(
            "admin-agent",
            "admin-agent",
            TrustLevel::Internal,
            vec![Capability::Read, Capability::Write, Capability::Search],
        );
        assert!(matches!(result, Err(ServerError::SelfLockout)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enroll_self_with_admin_allowed() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        registry
            .enroll_agent(
                "human",
                "admin-agent",
                TrustLevel::Internal,
                vec![Capability::Read, Capability::Admin],
            )
            .unwrap();

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_status_read_cap_non_admin_agent_passes() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();
        registry.bootstrap_defaults().unwrap();

        let _agent = registry.resolve_or_enroll("restricted-reader").unwrap();

        let result = registry.require_capability("restricted-reader", Capability::Read);
        assert!(
            result.is_ok(),
            "Restricted agent with Read capability must pass the Read gate"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_status_anonymous_fresh_install_passes_read_gate() {
        let store = make_store().await;
        let registry = AgentRegistry::new(store).unwrap();

        let agent = registry.resolve_or_enroll("brand-new-fresh-agent").unwrap();
        assert!(
            agent.capabilities.contains(&Capability::Read),
            "Auto-enrolled agent must have Read capability"
        );

        let result = registry.require_capability("brand-new-fresh-agent", Capability::Read);
        assert!(
            result.is_ok(),
            "Fresh-install (anonymous) agent must pass Read gate"
        );
    }
}
