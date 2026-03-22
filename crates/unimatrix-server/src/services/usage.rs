//! UsageService: unified usage recording for MCP and UDS transports (vnc-009).
//!
//! Single `record_access` entry point with `AccessSource` enum routing to
//! variant-specific internal methods. All recording is fire-and-forget via
//! `spawn_blocking`.

use std::sync::Arc;

use unimatrix_core::Store;

use crate::infra::registry::TrustLevel;
use crate::infra::usage_dedup::{UsageDedup, VoteAction};
use crate::services::confidence::ConfidenceStateHandle;

/// Unified usage recording service for both transports.
///
/// Replaces `UnimatrixBackend::record_usage_for_entries()` (MCP path)
/// and inline injection/co-access recording in `uds/listener.rs`.
#[derive(Clone)]
pub(crate) struct UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
    /// crt-019 (ADR-001): empirical prior parameters for confidence recomputation.
    ///
    /// Snapshot of `(alpha0, beta0)` taken before each `spawn_blocking` call so
    /// the capturing closure uses the latest tick values, not cold-start defaults.
    /// All lock acquisitions use `unwrap_or_else(|e| e.into_inner())` (FM-03).
    confidence_state: ConfidenceStateHandle,
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
    /// Access weight multiplier: 1 = normal, 2 = deliberate retrieval (context_lookup).
    ///
    /// Default MUST be 1. A value of 0 silently drops the access increment (EC-04).
    /// UsageDedup fires BEFORE this multiplier is applied (C-05).
    pub access_weight: u32,
    /// Workflow phase active at the moment `context_store` was called (ADR-001 crt-025).
    ///
    /// Snapshotted from `SessionState.current_phase` at call time — never re-read from
    /// live state during drain or spawn. `None` for all non-store operations (search,
    /// lookup, get, correct, deprecate, etc.) and for store calls with no active phase.
    pub current_phase: Option<String>,
}

impl UsageService {
    /// Create a new UsageService with shared store, dedup state, and confidence handle.
    pub(crate) fn new(
        store: Arc<Store>,
        usage_dedup: Arc<UsageDedup>,
        confidence_state: ConfidenceStateHandle,
    ) -> Self {
        UsageService {
            store,
            usage_dedup,
            confidence_state,
        }
    }

    /// Record access for a set of entry IDs.
    ///
    /// Fire-and-forget: spawns blocking tasks and returns immediately.
    /// Errors in spawned tasks are logged, never propagated.
    pub(crate) fn record_access(&self, entry_ids: &[u64], source: AccessSource, ctx: UsageContext) {
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

        // Step 1: Dedup access counts FIRST (C-05: dedup before multiply).
        // UsageDedup filters entries already seen by this agent this session.
        let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids);

        // Step 2: Determine vote actions
        let mut helpful_ids = Vec::new();
        let mut unhelpful_ids = Vec::new();
        let mut decrement_helpful_ids = Vec::new();
        let mut decrement_unhelpful_ids = Vec::new();

        if let Some(helpful_value) = ctx.helpful {
            let vote_actions = self
                .usage_dedup
                .check_votes(&agent_id, entry_ids, helpful_value);
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

        // Steps 3-5: Batch all DB writes into a single spawn_blocking (vnc-010).
        //
        // Previously each write (usage+confidence, feature_entries, co_access) was
        // a separate spawn_blocking, each independently acquiring the Store mutex.
        // This caused blocking pool saturation under concurrent MCP requests.

        // ADR-001 crt-025: Capture phase snapshot BEFORE the spawn so the closure
        // uses the call-time value, not live SessionState at flush time.
        let phase_snapshot = ctx.current_phase.clone();

        let store = Arc::clone(&self.store);

        // R-11 VERIFIED: The store loops over `all_ids` (outer loop) and uses
        // `access_ids` only for set-membership checks. Passing [id, id] in both
        // `all_ids` and `access_ids` produces access_count += 2 (not deduplicated).
        // This means the flat_map repeat approach IS viable for access_weight > 1.
        //
        // C-05: Dedup (filter_access) fires in Step 1 above. Only entries not yet
        // seen by this agent appear in `access_ids`. The multiplier applies only to
        // those fresh entries — a repeated lookup by the same agent produces 0.
        let multiplied_all_ids: Vec<u64> = if ctx.access_weight <= 1 {
            entry_ids.to_vec()
        } else {
            // Each entry appears access_weight times in all_ids for correct increment
            entry_ids
                .iter()
                .flat_map(|&id| std::iter::repeat(id).take(ctx.access_weight as usize))
                .collect()
        };

        let multiplied_access_ids: Vec<u64> = if ctx.access_weight <= 1 {
            access_ids
        } else {
            // access_ids (post-dedup) gets multiplied; deduped entries remain absent
            access_ids
                .iter()
                .flat_map(|&id| std::iter::repeat(id).take(ctx.access_weight as usize))
                .collect()
        };

        // Pre-compute co-access pairs (in-memory, no lock needed)
        let co_access_pairs = if entry_ids.len() >= 2 {
            let pairs =
                crate::coaccess::generate_pairs(entry_ids, crate::coaccess::MAX_CO_ACCESS_ENTRIES);
            let new_pairs = self.usage_dedup.filter_co_access_pairs(&pairs);
            if new_pairs.is_empty() {
                None
            } else {
                Some(new_pairs)
            }
        } else {
            None
        };

        // Pre-compute feature recording eligibility
        let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
            let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
            if matches!(
                trust,
                TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal
            ) {
                Some((feature_str, entry_ids.to_vec()))
            } else {
                None
            }
        });

        let confidence_fn: Box<dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send + Sync> =
            Box::new(move |entry, now| {
                crate::confidence::compute_confidence(
                    entry,
                    now,
                    &unimatrix_engine::confidence::ConfidenceParams::default(),
                )
            });

        let _ = tokio::spawn(async move {
            // Async usage recording (nxs-011: record_usage_with_confidence is now async)
            if let Err(e) = store
                .record_usage_with_confidence(
                    &multiplied_all_ids,
                    &multiplied_access_ids,
                    &helpful_ids,
                    &unhelpful_ids,
                    &decrement_helpful_ids,
                    &decrement_unhelpful_ids,
                    Some(confidence_fn),
                )
                .await
            {
                tracing::warn!("usage recording failed: {e}");
            }

            if let Some((feature_str, ids)) = feature_recording {
                // ADR-001 crt-025: use call-time phase snapshot, not live SessionState.
                if let Err(e) = store
                    .record_feature_entries(&feature_str, &ids, phase_snapshot.as_deref())
                    .await
                {
                    tracing::warn!("failed to record feature entries: {e}");
                }
            }

            if let Some(pairs) = co_access_pairs {
                store.record_co_access_pairs(&pairs);
            }
        });
    }

    /// Hook injection usage: co-access pairs and feature entries.
    ///
    /// Injection log writes remain in listener.rs (need per-entry confidence).
    /// Fire-and-forget via sync channel (record_co_access_pairs/record_feature_entries
    /// are infallible, nxs-011).
    fn record_hook_injection(&self, entry_ids: &[u64], ctx: UsageContext) {
        // Pre-compute co-access pairs (in-memory)
        let co_access_pairs = if entry_ids.len() >= 2 {
            let pairs = crate::coaccess::generate_pairs(entry_ids, entry_ids.len());
            if pairs.is_empty() { None } else { Some(pairs) }
        } else {
            None
        };

        // Pre-compute feature recording eligibility
        let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
            let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
            if matches!(
                trust,
                TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal
            ) {
                Some((feature_str, entry_ids.to_vec()))
            } else {
                None
            }
        });

        // Nothing to write
        if co_access_pairs.is_none() && feature_recording.is_none() {
            return;
        }

        // ADR-001 crt-025: Capture phase snapshot BEFORE any spawn so the closure
        // uses the call-time value, not live SessionState at flush time.
        let phase_snapshot = ctx.current_phase.clone();

        let store = Arc::clone(&self.store);
        // co_access: fire-and-forget via analytics drain (eventual consistency acceptable)
        if let Some(pairs) = co_access_pairs {
            let s = Arc::clone(&store);
            tokio::task::spawn_blocking(move || {
                s.record_co_access_pairs(&pairs);
            });
        }
        // feature_entries: direct async write (immediate visibility required)
        if let Some((feature_str, ids)) = feature_recording {
            let s = Arc::clone(&store);
            tokio::spawn(async move {
                // ADR-001 crt-025: use call-time phase snapshot, not live SessionState.
                if let Err(e) = s
                    .record_feature_entries(&feature_str, &ids, phase_snapshot.as_deref())
                    .await
                {
                    tracing::warn!("failed to record feature entries: {e}");
                }
            });
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
        let _ = tokio::spawn(async move {
            // Async usage recording (nxs-011)
            if let Err(e) = store
                .record_usage_with_confidence(
                    &access_ids,
                    &access_ids,
                    &[],
                    &[],
                    &[],
                    &[],
                    Some(
                        Box::new(move |entry: &unimatrix_store::EntryRecord, now: u64| {
                            crate::confidence::compute_confidence(
                                entry,
                                now,
                                &unimatrix_engine::confidence::ConfidenceParams::default(),
                            )
                        })
                            as Box<dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send + Sync>,
                    ),
                )
                .await
            {
                tracing::warn!("briefing usage recording failed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use sqlx::Row;
    use unimatrix_store::test_helpers::open_test_store;

    async fn make_usage_service() -> (UsageService, Arc<Store>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(open_test_store(&dir).await);
        let usage_dedup = Arc::new(UsageDedup::new());
        let confidence_state = crate::services::confidence::ConfidenceState::new_handle();
        let service = UsageService::new(Arc::clone(&store), usage_dedup, confidence_state);
        (service, store, dir)
    }

    async fn insert_test_entry(store: &Store) -> u64 {
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
        store.insert(entry).await.expect("insert")
    }

    #[tokio::test]
    async fn test_record_access_empty_ids() {
        let (service, _store, _dir) = make_usage_service().await;
        // Should return immediately without panic
        service.record_access(
            &[],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("test".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: None,
                access_weight: 1,
                current_phase: None,
            },
        );
    }

    #[tokio::test]
    async fn test_record_access_mcp_increments_access() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );

        // Wait for spawn_blocking to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert!(
            entry.access_count >= 1,
            "access_count should be >= 1, got {}",
            entry.access_count
        );
    }

    #[tokio::test]
    async fn test_record_access_mcp_helpful_vote() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(true),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(entry.helpful_count, 1);
    }

    #[tokio::test]
    async fn test_record_access_mcp_unhelpful_vote() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(false),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(entry.unhelpful_count, 1);
    }

    #[tokio::test]
    async fn test_record_access_mcp_vote_correction() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // Vote unhelpful first
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(false),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Correct to helpful
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(true),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(entry.helpful_count, 1);
        assert_eq!(entry.unhelpful_count, 0);
    }

    #[tokio::test]
    async fn test_record_access_mcp_duplicate_vote_noop() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // Vote helpful twice with same agent
        for _ in 0..2 {
            service.record_access(
                &[id],
                AccessSource::McpTool,
                UsageContext {
                    session_id: None,
                    agent_id: Some("agent-1".to_string()),
                    helpful: Some(true),
                    feature_cycle: None,
                    trust_level: Some(TrustLevel::Internal),
                    access_weight: 1,
                    current_phase: None,
                },
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let entry = store.get(id).await.expect("get");
        assert_eq!(entry.helpful_count, 1, "duplicate vote should be noop");
    }

    #[tokio::test]
    async fn test_record_access_mcp_access_dedup() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // Two calls with same agent
        for _ in 0..2 {
            service.record_access(
                &[id],
                AccessSource::McpTool,
                UsageContext {
                    session_id: None,
                    agent_id: Some("agent-1".to_string()),
                    helpful: None,
                    feature_cycle: None,
                    trust_level: Some(TrustLevel::Internal),
                    access_weight: 1,
                    current_phase: None,
                },
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let entry = store.get(id).await.expect("get");
        assert_eq!(
            entry.access_count, 1,
            "dedup should prevent double increment"
        );
    }

    #[tokio::test]
    async fn test_record_access_briefing_no_votes() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::Briefing,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert!(entry.access_count >= 1);
        assert_eq!(entry.helpful_count, 0, "briefing should not record votes");
    }

    #[tokio::test]
    async fn test_record_access_fire_and_forget_returns_quickly() {
        let (service, _store, _dir) = make_usage_service().await;
        let start = std::time::Instant::now();
        service.record_access(
            &[1, 2, 3],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(true),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "record_access should return quickly, took {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    async fn test_record_access_mcp_feature_recording() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: Some("vnc-009".to_string()),
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let rows = sqlx::query("SELECT entry_id FROM feature_entries WHERE feature_id = ?1")
            .bind("vnc-009")
            .fetch_all(store.write_pool_server())
            .await
            .unwrap();
        let found: Vec<u64> = rows
            .into_iter()
            .map(|row| {
                let v: i64 = row.get::<i64, _>(0);
                v as u64
            })
            .collect();
        assert!(found.contains(&id), "feature entry should be recorded");
    }

    #[tokio::test]
    async fn test_record_access_mcp_feature_restricted_ignored() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("restricted-agent".to_string()),
                helpful: None,
                feature_cycle: Some("vnc-009".to_string()),
                trust_level: Some(TrustLevel::Restricted),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?1",
        )
        .bind("vnc-009")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap_or(0);
        assert_eq!(
            count, 0,
            "restricted agent feature entry should not be recorded"
        );
    }

    /// T-INT-01: Verify confidence is recomputed after recording MCP usage.
    /// Exercises the full UsageService -> Store -> confidence recomputation path.
    #[tokio::test]
    async fn test_mcp_usage_confidence_recomputed() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // Before: confidence is 0.0
        assert_eq!(store.get(id).await.unwrap().confidence, 0.0);

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: Some(true),
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let entry = store.get(id).await.expect("get");
        assert!(
            entry.confidence > 0.0,
            "confidence should be recomputed after usage recording"
        );
        assert_eq!(entry.access_count, 1);
        assert_eq!(entry.helpful_count, 1);
    }

    /// T-INT-02: Verify UsageDedup prevents double access_count via UsageService.
    #[tokio::test]
    async fn test_mcp_usage_dedup_prevents_double_access() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // First call: access_count becomes 1
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(store.get(id).await.unwrap().access_count, 1);

        // Second call: same agent+entry -> deduped
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-1".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            store.get(id).await.unwrap().access_count,
            1,
            "dedup should prevent double increment"
        );
    }

    /// AC-08a: context_get implicit helpful vote — helpful: Some(true) increments helpful_count.
    /// This is what the context_get handler now passes via params.helpful.or(Some(true)).
    #[tokio::test]
    async fn test_context_get_implicit_helpful_vote_increments_helpful_count() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-get-1".to_string()),
                helpful: Some(true), // what context_get handler passes when params.helpful.is_none()
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(
            entry.helpful_count, 1,
            "implicit helpful vote must increment helpful_count"
        );
        assert_eq!(entry.access_count, 1, "access_count must also increment");
    }

    /// AC-08a: context_get with explicit helpful=false does NOT increment helpful_count.
    #[tokio::test]
    async fn test_context_get_explicit_false_does_not_increment_helpful() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-get-2".to_string()),
                helpful: Some(false), // explicit unhelpful — must not increment helpful_count
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(
            entry.helpful_count, 0,
            "explicit false must not increment helpful_count"
        );
        assert_eq!(
            entry.unhelpful_count, 1,
            "explicit false must increment unhelpful_count"
        );
    }

    /// AC-08b / R-11 fallback: context_lookup access_weight=2 increments access_count by 2.
    /// New agent, new entry — dedup passes, access_weight multiplier applied.
    #[tokio::test]
    async fn test_context_lookup_access_weight_2_increments_by_2() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-lookup-1".to_string()),
                helpful: None, // no implicit vote for lookup
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 2, // context_lookup sets this
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let entry = store.get(id).await.expect("get");
        assert_eq!(
            entry.access_count, 2,
            "lookup with access_weight=2 must produce access_count += 2"
        );
        assert_eq!(
            entry.helpful_count, 0,
            "lookup must not inject helpful vote"
        );
    }

    /// C-05: dedup fires BEFORE access_weight multiplier.
    /// Same agent calling context_lookup twice: second call deduped, access_count stays 2.
    #[tokio::test]
    async fn test_context_lookup_dedup_before_multiply_second_call_zero() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        // First lookup: access_weight=2, fresh agent -> access_count becomes 2
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-lookup-dedup".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 2,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            store.get(id).await.unwrap().access_count,
            2,
            "first lookup: access_count must be 2"
        );

        // Second lookup: same agent -> UsageDedup filters the entry -> empty access_ids
        // -> multiplier applied to empty set -> 0 increments (C-05)
        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-lookup-dedup".to_string()),
                helpful: None,
                feature_cycle: None,
                trust_level: Some(TrustLevel::Internal),
                access_weight: 2,
                current_phase: None,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            store.get(id).await.unwrap().access_count,
            2,
            "second lookup same agent: access_count must remain 2 (dedup before multiply)"
        );
    }

    // ---------------------------------------------------------------------------
    // crt-025 Component 8: Context Store Phase Capture — UsageContext unit tests
    // (R-14, FR-06.1)
    // ---------------------------------------------------------------------------

    /// test_usage_context_has_current_phase_field (R-14)
    ///
    /// Structural compile-time test: `UsageContext` has `current_phase: Option<String>`.
    /// This test compiles only if the field exists on the struct.
    #[test]
    fn test_usage_context_has_current_phase_field() {
        // Construct with explicit current_phase — fails to compile if field is absent.
        let ctx = UsageContext {
            session_id: None,
            agent_id: None,
            helpful: None,
            feature_cycle: None,
            trust_level: None,
            access_weight: 1,
            current_phase: Some("scope".to_string()),
        };
        assert_eq!(ctx.current_phase.as_deref(), Some("scope"));

        let ctx_none = UsageContext {
            session_id: None,
            agent_id: None,
            helpful: None,
            feature_cycle: None,
            trust_level: None,
            access_weight: 1,
            current_phase: None,
        };
        assert!(ctx_none.current_phase.is_none());
    }

    /// test_usage_context_current_phase_propagates_to_feature_entry (R-14, FR-06.1)
    ///
    /// Verifies that when `UsageContext.current_phase = Some("scope")` is provided
    /// along with a valid feature_cycle, the recorded feature_entries row has phase
    /// = "scope" (not NULL). Exercises the full MCP usage recording path.
    #[tokio::test]
    async fn test_usage_context_current_phase_propagates_to_feature_entry() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-phase-1".to_string()),
                helpful: None,
                feature_cycle: Some("crt-025".to_string()),
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: Some("scope".to_string()),
            },
        );

        // Allow spawn to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let row = sqlx::query(
            "SELECT phase FROM feature_entries WHERE feature_id = 'crt-025' AND entry_id = ?1",
        )
        .bind(id as i64)
        .fetch_one(store.write_pool_server())
        .await
        .expect("feature_entries row must exist");

        let phase: Option<String> = row.get(0);
        assert_eq!(
            phase.as_deref(),
            Some("scope"),
            "phase must be 'scope' (not NULL) when current_phase=Some('scope') (R-14, FR-06.1)"
        );
    }

    /// Verify that `current_phase = None` produces a NULL phase in feature_entries.
    #[tokio::test]
    async fn test_usage_context_phase_none_produces_null_phase() {
        let (service, store, _dir) = make_usage_service().await;
        let id = insert_test_entry(&store).await;

        service.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: None,
                agent_id: Some("agent-phase-2".to_string()),
                helpful: None,
                feature_cycle: Some("crt-025-null".to_string()),
                trust_level: Some(TrustLevel::Internal),
                access_weight: 1,
                current_phase: None,
            },
        );

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let row = sqlx::query(
            "SELECT phase FROM feature_entries WHERE feature_id = 'crt-025-null' AND entry_id = ?1",
        )
        .bind(id as i64)
        .fetch_one(store.write_pool_server())
        .await
        .expect("feature_entries row must exist");

        let phase: Option<String> = row.get(0);
        assert!(
            phase.is_none(),
            "phase must be NULL when current_phase=None"
        );
    }
}
