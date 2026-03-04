//! UsageService: unified usage recording for MCP and UDS transports (vnc-009).
//!
//! Single `record_access` entry point with `AccessSource` enum routing to
//! variant-specific internal methods. All recording is fire-and-forget via
//! `spawn_blocking`.

use std::sync::Arc;

use unimatrix_store::Store;

use crate::infra::registry::TrustLevel;
use crate::infra::usage_dedup::{UsageDedup, VoteAction};

/// Unified usage recording service for both transports.
///
/// Replaces `UnimatrixBackend::record_usage_for_entries()` (MCP path)
/// and inline injection/co-access recording in `uds/listener.rs`.
#[derive(Clone)]
pub(crate) struct UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
}

/// Discriminates the origin of a usage event.
#[allow(dead_code)]
pub(crate) enum AccessSource {
    /// MCP tool call retrieval (search, lookup, get).
    McpTool,
    /// UDS hook injection (co-access + feature entries).
    HookInjection,
    /// Briefing assembly from either transport.
    Briefing,
}

/// Contextual data for usage recording.
#[allow(dead_code)]
pub(crate) struct UsageContext {
    /// Session ID (prefixed with transport, e.g. "mcp::abc").
    pub session_id: Option<String>,
    /// Agent identity for dedup keying.
    pub agent_id: Option<String>,
    /// Helpful/unhelpful vote (MCP only).
    pub helpful: Option<bool>,
    /// Feature cycle for FEATURE_ENTRIES writes.
    pub feature_cycle: Option<String>,
    /// Trust level for feature entry gating.
    pub trust_level: Option<TrustLevel>,
}

impl UsageService {
    /// Create a new UsageService with shared store and dedup state.
    pub(crate) fn new(store: Arc<Store>, usage_dedup: Arc<UsageDedup>) -> Self {
        UsageService { store, usage_dedup }
    }

    /// Record access for a set of entry IDs.
    ///
    /// Fire-and-forget: spawns blocking tasks and returns immediately.
    /// Errors in spawned tasks are logged, never propagated.
    pub(crate) fn record_access(
        &self,
        entry_ids: &[u64],
        source: AccessSource,
        ctx: UsageContext,
    ) {
        if entry_ids.is_empty() {
            return;
        }

        match source {
            AccessSource::McpTool => self.record_mcp_usage(entry_ids, ctx),
            AccessSource::HookInjection => self.record_hook_injection(entry_ids, ctx),
            AccessSource::Briefing => self.record_briefing_usage(entry_ids, ctx),
        }
    }

    /// MCP tool usage: dedup access, vote processing, confidence recomputation.
    ///
    /// Direct port of `UnimatrixBackend::record_usage_for_entries()` from server.rs.
    fn record_mcp_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
        let agent_id = ctx.agent_id.clone().unwrap_or_default();

        // Step 1: Dedup access counts
        let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids);

        // Step 2: Determine vote actions
        let mut helpful_ids = Vec::new();
        let mut unhelpful_ids = Vec::new();
        let mut decrement_helpful_ids = Vec::new();
        let mut decrement_unhelpful_ids = Vec::new();

        if let Some(helpful_value) = ctx.helpful {
            let vote_actions = self.usage_dedup.check_votes(&agent_id, entry_ids, helpful_value);
            for (id, action) in vote_actions {
                match action {
                    VoteAction::NewVote => {
                        if helpful_value {
                            helpful_ids.push(id);
                        } else {
                            unhelpful_ids.push(id);
                        }
                    }
                    VoteAction::CorrectedVote => {
                        if helpful_value {
                            helpful_ids.push(id);
                            decrement_unhelpful_ids.push(id);
                        } else {
                            unhelpful_ids.push(id);
                            decrement_helpful_ids.push(id);
                        }
                    }
                    VoteAction::NoOp => {}
                }
            }
        }

        // Step 3: Record usage with confidence (spawn_blocking, fire-and-forget)
        let store = Arc::clone(&self.store);
        let all_ids = entry_ids.to_vec();

        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = store.record_usage_with_confidence(
                &all_ids,
                &access_ids,
                &helpful_ids,
                &unhelpful_ids,
                &decrement_helpful_ids,
                &decrement_unhelpful_ids,
                Some(&crate::confidence::compute_confidence),
            ) {
                tracing::warn!("usage recording failed: {e}");
            }
        });

        // Step 4: Record feature entries if applicable (trust gating)
        if let Some(feature_str) = ctx.feature_cycle {
            let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
            if matches!(trust, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal) {
                let store = Arc::clone(&self.store);
                let ids = entry_ids.to_vec();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Err(e) = store.record_feature_entries(&feature_str, &ids) {
                        tracing::warn!("feature entry recording failed: {e}");
                    }
                });
            }
        }

        // Step 5: Co-access recording (fire-and-forget, crt-004)
        if entry_ids.len() >= 2 {
            let pairs = crate::coaccess::generate_pairs(
                entry_ids,
                crate::coaccess::MAX_CO_ACCESS_ENTRIES,
            );
            let new_pairs = self.usage_dedup.filter_co_access_pairs(&pairs);

            if !new_pairs.is_empty() {
                let store = Arc::clone(&self.store);
                let _ = tokio::task::spawn_blocking(move || {
                    if let Err(e) = store.record_co_access_pairs(&new_pairs) {
                        tracing::warn!("co-access recording failed: {e}");
                    }
                });
            }
        }
    }

    /// Hook injection usage: co-access pairs and feature entries.
    ///
    /// Injection log writes remain in listener.rs (need per-entry confidence).
    fn record_hook_injection(&self, entry_ids: &[u64], ctx: UsageContext) {
        // Co-access pairs
        if entry_ids.len() >= 2 {
            let pairs = crate::coaccess::generate_pairs(entry_ids, entry_ids.len());
            if !pairs.is_empty() {
                let store = Arc::clone(&self.store);
                let _ = tokio::task::spawn_blocking(move || {
                    if let Err(e) = store.record_co_access_pairs(&pairs) {
                        tracing::warn!("co-access recording failed: {e}");
                    }
                });
            }
        }

        // Feature entries
        if let Some(feature_str) = ctx.feature_cycle {
            let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
            if matches!(trust, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal) {
                let store = Arc::clone(&self.store);
                let ids = entry_ids.to_vec();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Err(e) = store.record_feature_entries(&feature_str, &ids) {
                        tracing::warn!("feature entry recording failed: {e}");
                    }
                });
            }
        }
    }

    /// Briefing usage: access count only (no votes, no injection log).
    fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
        let agent_id = ctx.agent_id.clone().unwrap_or_default();

        // Dedup access count only
        let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids);

        if access_ids.is_empty() {
            return;
        }

        let store = Arc::clone(&self.store);
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = store.record_usage_with_confidence(
                &access_ids,
                &access_ids,
                &[],
                &[],
                &[],
                &[],
                Some(&crate::confidence::compute_confidence),
            ) {
                tracing::warn!("briefing usage recording failed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use redb::ReadableMultimapTable;

    fn make_usage_service() -> (UsageService, Arc<Store>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(dir.path().join("test.redb")).expect("store"));
        let usage_dedup = Arc::new(UsageDedup::new());
        let service = UsageService::new(Arc::clone(&store), usage_dedup);
        (service, store, dir)
    }

    fn insert_test_entry(store: &Store) -> u64 {
        let entry = unimatrix_core::NewEntry {
            title: "test".to_string(),
            content: "test content".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: String::new(),
            status: unimatrix_core::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
        };
        store.insert(entry).expect("insert")
    }

    #[tokio::test]
    async fn test_record_access_empty_ids() {
        let (service, _store, _dir) = make_usage_service();
        // Should return immediately without panic
        service.record_access(&[], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("test".to_string()),
            helpful: None,
            feature_cycle: None,
            trust_level: None,
        });
    }

    #[tokio::test]
    async fn test_record_access_mcp_increments_access() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: None,
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });

        // Wait for spawn_blocking to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).expect("get");
        assert!(entry.access_count >= 1, "access_count should be >= 1, got {}", entry.access_count);
    }

    #[tokio::test]
    async fn test_record_access_mcp_helpful_vote() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: Some(true),
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).expect("get");
        assert_eq!(entry.helpful_count, 1);
    }

    #[tokio::test]
    async fn test_record_access_mcp_unhelpful_vote() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: Some(false),
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).expect("get");
        assert_eq!(entry.unhelpful_count, 1);
    }

    #[tokio::test]
    async fn test_record_access_mcp_vote_correction() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        // Vote unhelpful first
        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: Some(false),
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Correct to helpful
        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: Some(true),
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).expect("get");
        assert_eq!(entry.helpful_count, 1);
        assert_eq!(entry.unhelpful_count, 0);
    }

    #[tokio::test]
    async fn test_record_access_mcp_duplicate_vote_noop() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        // Vote helpful twice with same agent
        for _ in 0..2 {
            service.record_access(&[id], AccessSource::McpTool, UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(true),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
            });
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let entry = store.get(id).expect("get");
        assert_eq!(entry.helpful_count, 1, "duplicate vote should be noop");
    }

    #[tokio::test]
    async fn test_record_access_mcp_access_dedup() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        // Two calls with same agent
        for _ in 0..2 {
            service.record_access(&[id], AccessSource::McpTool, UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
            });
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let entry = store.get(id).expect("get");
        assert_eq!(entry.access_count, 1, "dedup should prevent double increment");
    }

    #[tokio::test]
    async fn test_record_access_briefing_no_votes() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::Briefing, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: None,
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).expect("get");
        assert!(entry.access_count >= 1);
        assert_eq!(entry.helpful_count, 0, "briefing should not record votes");
    }

    #[tokio::test]
    async fn test_record_access_fire_and_forget_returns_quickly() {
        let (service, _store, _dir) = make_usage_service();
        let start = std::time::Instant::now();
        service.record_access(&[1, 2, 3], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: Some(true),
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
        });
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 50, "record_access should return quickly, took {}ms", elapsed.as_millis());
    }

    #[tokio::test]
    async fn test_record_access_mcp_feature_recording() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: None,
            feature_cycle: Some("vnc-009".to_string()),
            trust_level: Some(TrustLevel::Internal),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let txn = store.begin_read().expect("begin_read");
        let table = txn.open_multimap_table(unimatrix_store::FEATURE_ENTRIES).expect("open table");
        let found: Vec<u64> = table.get("vnc-009").expect("get").map(|r| r.unwrap().value()).collect();
        assert!(found.contains(&id), "feature entry should be recorded");
    }

    #[tokio::test]
    async fn test_record_access_mcp_feature_restricted_ignored() {
        let (service, store, _dir) = make_usage_service();
        let id = insert_test_entry(&store);

        service.record_access(&[id], AccessSource::McpTool, UsageContext {
            session_id: None,
            agent_id: Some("restricted-agent".to_string()),
            helpful: None,
            feature_cycle: Some("vnc-009".to_string()),
            trust_level: Some(TrustLevel::Restricted),
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let txn = store.begin_read().expect("begin_read");
        let table = txn.open_multimap_table(unimatrix_store::FEATURE_ENTRIES).expect("open table");
        let count = table.get("vnc-009").expect("get").count();
        assert_eq!(count, 0, "restricted agent feature entry should not be recorded");
    }
}
