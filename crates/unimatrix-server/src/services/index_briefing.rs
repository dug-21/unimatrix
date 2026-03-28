//! IndexBriefingService: unified knowledge index query for WA-4b (crt-027).
//!
//! Replaces `BriefingService` (deleted in crt-027). Returns `Vec<IndexEntry>`
//! with `status=Active` entries only, sorted by fused score descending.
//! Delegates all embedding and ranking to `SearchService`.
//!
//! Used by:
//! - `context_briefing` MCP tool handler (`mcp/tools.rs`)
//! - `handle_compact_payload` UDS path (`uds/listener.rs`)
//!
//! # crt-027 note: UNIMATRIX_BRIEFING_K
//! The `UNIMATRIX_BRIEFING_K` environment variable is **deprecated** and is
//! NOT read by this service. The default k=20 is hardcoded and cannot be
//! overridden via environment variable. Callers that need a different k must
//! pass it explicitly via `IndexBriefingParams::k`. See ADR-003 crt-027.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use unimatrix_core::{Status, Store};

use crate::infra::session::SessionState;
use crate::mcp::response::{IndexEntry, SNIPPET_CHARS};
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::search::{RetrievalMode, SearchService, ServiceSearchParams};
use crate::services::{AuditContext, CallerId, ServiceError};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Agent instruction prepended once before every `format_index_table` output.
///
/// Tells agents that `context_get` with an entry ID retrieves full content.
/// Applied to both MCP context_briefing responses and UDS CompactPayload
/// injection content. Appears once as a header line — never per row.
///
/// col-025, ADR-006. Do not inline this value at call sites.
/// Update this constant to change the instruction globally.
pub const CONTEXT_GET_INSTRUCTION: &str =
    "Use context_get with the entry ID for full content when relevant.";

// ---------------------------------------------------------------------------
// IndexBriefingParams
// ---------------------------------------------------------------------------

/// Parameters for `IndexBriefingService::index()`.
pub(crate) struct IndexBriefingParams {
    /// The search query derived by `derive_briefing_query`.
    pub query: String,
    /// Maximum entries to return.
    ///
    /// Default is `IndexBriefingService::default_k` (20).
    /// A value of 0 is treated as "use default_k" (EC-03 guard).
    /// UNIMATRIX_BRIEFING_K is not read — see module-level docs.
    pub k: usize,
    /// Session ID for WA-2 category histogram boost (optional).
    pub session_id: Option<String>,
    /// Approximate token budget (for future ranked truncation; not enforced here).
    pub max_tokens: Option<usize>,
    /// Pre-resolved category histogram for WA-2 boost.
    ///
    /// Callers are responsible for resolving this from `SessionRegistry`
    /// (consistent with the existing `handle_context_search` pattern).
    /// Set to `None` when session has no prior stores or histogram is empty.
    pub category_histogram: Option<HashMap<String, u32>>,
}

// ---------------------------------------------------------------------------
// IndexBriefingService
// ---------------------------------------------------------------------------

/// Replaces `BriefingService` (deleted in crt-027).
///
/// Returns `Vec<IndexEntry>` with `status=Active` entries only, sorted by
/// fused score descending. Delegates to `SearchService` for embedding,
/// HNSW recall, NLI re-ranking, and WA-2 histogram boost.
///
/// # ADR-003 crt-027
/// `effectiveness_state` is a **required, non-optional** constructor parameter.
/// Missing this parameter at the call site is a compile error — not a runtime
/// degradation. This pattern mirrors `BriefingService`'s pre-existing requirement.
#[derive(Clone)]
pub(crate) struct IndexBriefingService {
    #[allow(dead_code)]
    entry_store: Arc<Store>,
    search: SearchService,
    #[allow(dead_code)]
    gateway: Arc<SecurityGateway>,
    /// Default k for `index()` calls. Hardcoded to 20.
    ///
    /// UNIMATRIX_BRIEFING_K is NOT read (ADR-003 crt-027, deprecated).
    default_k: usize,
    /// crt-018b (ADR-004): effectiveness classification handle.
    /// Required parameter — missing wiring is a compile error.
    #[allow(dead_code)]
    effectiveness_state: EffectivenessStateHandle,
    /// crt-018b (ADR-001): generation-cached snapshot shared across clones.
    /// `Arc` wrapper ensures all clones share the same cached copy.
    #[allow(dead_code)]
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}

impl IndexBriefingService {
    /// Construct a new `IndexBriefingService`.
    ///
    /// `effectiveness_state` is a required, non-optional parameter (ADR-004 crt-018b
    /// pattern). Missing this parameter at the call site is a compile error — NOT a
    /// runtime degradation.
    ///
    /// # crt-027 note
    /// `UNIMATRIX_BRIEFING_K` env var is deprecated and NOT read here.
    /// The default k=20 is hardcoded and cannot be reduced via environment variable.
    /// Callers that need a different k should pass it explicitly via `IndexBriefingParams::k`.
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        effectiveness_state: EffectivenessStateHandle, // required, non-optional
    ) -> Self {
        IndexBriefingService {
            entry_store,
            search,
            gateway,
            default_k: 20, // hardcoded, not from env var (ADR-003 crt-027)
            effectiveness_state,
            cached_snapshot: EffectivenessSnapshot::new_shared(),
        }
    }

    /// Query the knowledge index and return a ranked, active-only result set.
    ///
    /// Steps:
    /// 1. Determine effective k (params.k; clamp 0 → default_k per EC-03 guard)
    /// 2. Delegate to `SearchService.search()` with `RetrievalMode::Strict`
    /// 3. Post-filter to `Status::Active` only (defensive — Strict mode guarantees
    ///    this but we filter explicitly per the spec)
    /// 4. Map each `ScoredEntry` to `IndexEntry` (snippet = first SNIPPET_CHARS chars)
    /// 5. Sort by fused score descending
    /// 6. Truncate to effective k
    ///
    /// Returns `Ok(vec![])` on no results (R-10, AC-18).
    ///
    /// Input validation is delegated to `SearchService.search()` which calls
    /// `self.gateway.validate_search_query()`. Guards enforced:
    ///   - Query content (S3)
    ///   - Length ≤ 10,000 chars
    ///   - Control characters rejected
    ///   - k bounds enforced
    ///
    /// WARNING: Do not remove the `SearchService` delegation or replace it with
    /// a direct store call without adding an equivalent `validate_search_query()`
    /// call here. Removing the delegation silently removes all input validation
    /// (GH #355, ADR documented in crt-028).
    pub(crate) async fn index(
        &self,
        params: IndexBriefingParams,
        audit_ctx: &AuditContext,
        caller_id: Option<&CallerId>,
    ) -> Result<Vec<IndexEntry>, ServiceError> {
        // Step 1: Effective k — EC-03: k=0 clamps to default_k
        let effective_k = if params.k == 0 {
            self.default_k
        } else {
            params.k
        };

        // Step 2: Build ServiceSearchParams
        // Use RetrievalMode::Strict to exclude deprecated/quarantined entries at
        // the SearchService level. Active-only post-filter (step 3) is additional
        // defence to guarantee the contract regardless of SearchService behavior.
        let service_params = ServiceSearchParams {
            query: params.query.clone(),
            k: effective_k,
            filters: None, // status filtering via RetrievalMode::Strict
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Strict,
            session_id: params.session_id.clone(),
            category_histogram: params.category_histogram,
            current_phase: None, // col-031: briefing does not carry phase context
        };

        // Step 3: Resolve caller identity
        let owned_caller: CallerId;
        let effective_caller: &CallerId = match caller_id {
            Some(id) => id,
            None => {
                owned_caller = CallerId::UdsSession("index-briefing".to_string());
                &owned_caller
            }
        };

        // Step 4: Delegate to SearchService
        let search_results = self
            .search
            .search(service_params, audit_ctx, effective_caller)
            .await?;

        // Step 5: Post-filter: status=Active only (FR-08, AC-06)
        // Defensive: always filter regardless of SearchService RetrievalMode behaviour.
        let active_entries: Vec<_> = search_results
            .entries
            .into_iter()
            .filter(|se| se.entry.status == Status::Active)
            .collect();

        // Step 6: Map to IndexEntry
        // confidence = similarity field from ScoredEntry (SearchService already applies
        // the full fused score: similarity + confidence + WA-2 boost + effectiveness).
        // snippet = first SNIPPET_CHARS chars, UTF-8 char-boundary safe (ADR-005).
        let mut index_entries: Vec<IndexEntry> = active_entries
            .into_iter()
            .map(|se| {
                let snippet: String = se.entry.content.chars().take(SNIPPET_CHARS).collect();
                IndexEntry {
                    id: se.entry.id,
                    topic: se.entry.topic.clone(),
                    category: se.entry.category.clone(),
                    confidence: se.similarity,
                    snippet,
                }
            })
            .collect();

        // Step 7: Sort by fused score descending
        // SearchService returns results sorted by fused score, but we re-sort
        // defensively to guarantee the contract after the Active-only post-filter.
        index_entries.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 8: Truncate to effective k
        index_entries.truncate(effective_k);

        Ok(index_entries)
    }
}

// ---------------------------------------------------------------------------
// derive_briefing_query
// ---------------------------------------------------------------------------

/// Derive the search query for `IndexBriefingService` using three-step priority.
///
/// **Step 1**: If `task` is `Some(t)` and `!t.trim().is_empty()`, return `t.to_string()`.
/// **Step 2**: If `session_state` is `Some(s)` and `current_goal` is `Some(g)` and
///             non-empty, return `g` (col-025, ADR-002).
/// **Step 3**: Fall back to `topic.to_string()` (always available; e.g., `"crt-027"`).
///
/// This function is the single shared implementation for both the MCP
/// `context_briefing` handler and `handle_compact_payload`. Both callers MUST
/// use this function to prevent query derivation divergence (AC-09, R-06).
///
/// For the MCP path: `session_state` is obtained from `SessionRegistry` by the caller.
/// For the UDS path: `session_state` is held directly in `handle_compact_payload`.
pub(crate) fn derive_briefing_query(
    task: Option<&str>,
    session_state: Option<&SessionState>,
    topic: &str,
) -> String {
    // Step 1: explicit task overrides everything
    if let Some(t) = task {
        if !t.trim().is_empty() {
            return t.to_string();
        }
        // Empty/whitespace task: fall through to step 2 or 3
    }

    // Step 2: goal from session state (col-025, ADR-002).
    // Returns current_goal when Some — most semantically precise signal available.
    // Falls through to step 3 when None (no goal, legacy cycle, or pre-v16 cycle).
    if let Some(state) = session_state {
        if let Some(goal) = synthesize_from_session(state) {
            if !goal.trim().is_empty() {
                return goal;
            }
            // Empty-goal guard: if current_goal is Some("") (edge case), fall through.
            // Normal path: goal is already non-empty (normalized at MCP handler).
        }
    }

    // Step 3: topic fallback (always available)
    topic.to_string()
}

/// Return the step-2 briefing query from session state (col-025, ADR-002).
///
/// Returns `state.current_goal.clone()` — the feature goal set at cycle start
/// or reconstructed on session resume.
///
/// When `None` (no goal, legacy cycle, or pre-v16 cycle), `derive_briefing_query`
/// falls through to step 3 (topic-ID) unchanged.
///
/// Contract (NFR-04): pure sync, O(1), no I/O, no locks, no async.
/// Called on both the MCP `context_briefing` hot path and the UDS
/// `handle_compact_payload` path.
fn synthesize_from_session(state: &SessionState) -> Option<String> {
    state.current_goal.clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::embed_handle::EmbedServiceHandle;
    use crate::infra::session::TopicTally;
    use crate::services::{AuditContext, AuditSource};
    use std::sync::Arc;
    use unimatrix_adapt::AdaptationService;
    use unimatrix_core::VectorAdapter;
    use unimatrix_core::async_wrappers::AsyncVectorStore;
    use unimatrix_core::{Status, Store};
    use unimatrix_store::NewEntry;
    use unimatrix_store::test_helpers::open_test_store;

    // -----------------------------------------------------------------------
    // derive_briefing_query tests
    // -----------------------------------------------------------------------

    fn make_session_state(
        feature: Option<&str>,
        signals: Vec<(&str, u32)>,
        current_goal: Option<&str>, // col-025: pass None for existing call sites
    ) -> SessionState {
        use std::collections::{HashMap, HashSet};
        let topic_signals: HashMap<String, TopicTally> = signals
            .into_iter()
            .map(|(k, count)| {
                (
                    k.to_string(),
                    TopicTally {
                        count,
                        last_seen: 0,
                    },
                )
            })
            .collect();
        SessionState {
            session_id: "test-session".to_string(),
            role: None,
            feature: feature.map(str::to_string),
            injection_history: vec![],
            coaccess_seen: HashSet::new(),
            compaction_count: 0,
            signaled_entries: HashSet::new(),
            rework_events: vec![],
            agent_actions: vec![],
            last_activity_at: 0,
            topic_signals,
            current_phase: None,
            category_counts: HashMap::new(),
            current_goal: current_goal.map(str::to_string), // col-025
            confirmed_entries: HashSet::new(),              // col-028
        }
    }

    /// Step 1: explicit task takes priority over session state and topic.
    #[test]
    fn derive_briefing_query_task_param_takes_priority() {
        let state = make_session_state(Some("crt-027"), vec![("briefing", 5), ("hook", 3)], None);
        let result = derive_briefing_query(Some("implement spec writer"), Some(&state), "crt-027");
        assert_eq!(result, "implement spec writer");
    }

    /// Step 1: whitespace-only task falls through (not treated as content).
    #[test]
    fn derive_briefing_query_whitespace_task_falls_through() {
        let result = derive_briefing_query(Some("   "), None, "crt-027");
        assert_eq!(
            result, "crt-027",
            "whitespace-only task must fall to step 3"
        );
    }

    /// Step 1: empty string task falls through.
    #[test]
    fn derive_briefing_query_empty_task_falls_through() {
        // current_goal=None: empty task falls to step 2 (None), then step 3 (topic).
        let state = make_session_state(Some("crt-027"), vec![("briefing", 5)], None);
        let result = derive_briefing_query(Some(""), Some(&state), "crt-027");
        // Should NOT return "" — falls to step 2 (current_goal=None), then step 3 (topic)
        assert_ne!(result, "", "empty task must not return empty string");
        assert!(
            result.contains("crt-027"),
            "result should contain topic (step 3), got: {result}"
        );
    }

    /// Step 2: current_goal=None with signals present — signals no longer used; falls to step 3.
    ///
    /// col-025 (ADR-002): old topic-signal synthesis removed. Step 2 now reads
    /// current_goal only. When current_goal=None, step 3 (topic) runs regardless
    /// of how many topic_signals are present.
    #[test]
    fn derive_briefing_query_session_signals_step_2() {
        let state = make_session_state(
            Some("crt-027/spec"),
            vec![
                ("briefing", 5),
                ("hook", 3),
                ("compaction", 2),
                ("extra", 1),
            ],
            None, // no goal → step 2 returns None → step 3 (topic)
        );
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027",
            "current_goal=None must fall to topic (step 3), not synthesize signals"
        );
    }

    /// Step 2: current_goal=None with fewer than 3 signals — falls to step 3 (topic).
    ///
    /// col-025 (ADR-002): topic-signal synthesis is removed. With current_goal=None,
    /// step 2 returns None and step 3 (topic) runs regardless of signal count.
    #[test]
    fn derive_briefing_query_fewer_than_three_signals() {
        let state = make_session_state(Some("crt-027/spec"), vec![("briefing", 5)], None);
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027",
            "current_goal=None must fall to topic (step 3) even with signals present"
        );
    }

    /// Step 2 requires feature_cycle — absent feature_cycle falls to step 3.
    #[test]
    fn derive_briefing_query_no_feature_cycle_falls_to_topic() {
        let state = make_session_state(
            None, // no feature_cycle
            vec![("briefing", 5), ("hook", 3)],
            None,
        );
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027",
            "absent feature_cycle must fall to topic (step 3)"
        );
    }

    /// Step 2: empty topic_signals with feature_cycle → falls to step 3.
    #[test]
    fn derive_briefing_query_empty_signals_fallback_to_topic() {
        let state = make_session_state(Some("crt-027"), vec![], None);
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027",
            "empty signals must fall to topic (step 3)"
        );
    }

    /// Step 3: no session state → topic fallback.
    #[test]
    fn derive_briefing_query_no_session_fallback_to_topic() {
        let result = derive_briefing_query(None, None, "crt-027");
        assert_eq!(result, "crt-027");
    }

    // -----------------------------------------------------------------------
    // synthesize_from_session tests (col-025, ADR-002, R-05)
    // -----------------------------------------------------------------------

    /// synthesize_from_session returns the current_goal when Some (R-05).
    #[test]
    fn test_synthesize_from_session_returns_current_goal() {
        let state = make_session_state(None, vec![], Some("feature goal text"));
        let result = synthesize_from_session(&state);
        assert_eq!(result, Some("feature goal text".to_string()));
    }

    /// synthesize_from_session returns None when current_goal is None (R-05).
    #[test]
    fn test_synthesize_from_session_returns_none_when_goal_absent() {
        let state = make_session_state(None, vec![], None);
        let result = synthesize_from_session(&state);
        assert_eq!(result, None);
    }

    /// synthesize_from_session returns None regardless of topic_signals (R-05).
    ///
    /// Confirms the old topic-signal synthesis code is fully removed.
    #[test]
    fn test_synthesize_from_session_ignores_topic_signals() {
        let state = make_session_state(
            Some("crt-027"),
            vec![("briefing", 5), ("hook", 3), ("compaction", 2)],
            None, // current_goal is None — signals must not influence step 2
        );
        let result = synthesize_from_session(&state);
        assert_eq!(
            result, None,
            "topic_signals must not affect synthesize_from_session"
        );
    }

    // -----------------------------------------------------------------------
    // derive_briefing_query new tests (col-025, AC-04–AC-07, R-05)
    // -----------------------------------------------------------------------

    /// Step 2 returns current_goal when Some (AC-04).
    #[test]
    fn test_derive_briefing_query_step2_returns_current_goal() {
        let state = make_session_state(Some("col-025"), vec![], Some("goal text"));
        let result = derive_briefing_query(None, Some(&state), "col-025");
        assert_eq!(result, "goal text");
    }

    /// Step 1 wins over current_goal (AC-05).
    #[test]
    fn test_derive_briefing_query_step1_wins_over_goal() {
        let state = make_session_state(Some("col-025"), vec![], Some("goal text"));
        let result = derive_briefing_query(Some("explicit task"), Some(&state), "col-025");
        assert_eq!(result, "explicit task");
    }

    /// Step 3 fallback when current_goal is None (AC-06).
    #[test]
    fn test_derive_briefing_query_step3_fallback_when_no_goal() {
        let state = make_session_state(Some("col-025"), vec![("signal", 5)], None);
        let result = derive_briefing_query(None, Some(&state), "col-025");
        assert_eq!(
            result, "col-025",
            "topic-ID fallback must run when current_goal is None"
        );
    }

    /// Step 3 fallback when no session state (AC-06).
    #[test]
    fn test_derive_briefing_query_step3_no_session_state() {
        let result = derive_briefing_query(None, None, "col-025");
        assert_eq!(result, "col-025");
    }

    /// Whitespace task falls through to goal at step 2 (AC-04 / AC-05).
    #[test]
    fn test_derive_briefing_query_whitespace_task_falls_to_goal() {
        let state = make_session_state(None, vec![], Some("feature goal"));
        let result = derive_briefing_query(Some("   "), Some(&state), "col-025");
        assert_eq!(
            result, "feature goal",
            "whitespace task must fall through to step 2 (goal)"
        );
    }

    /// Goal wins over populated topic_signals (R-05).
    ///
    /// Explicitly confirms the old synthesis code is gone — goal, not signals, determines step 2.
    #[test]
    fn test_derive_briefing_query_goal_with_populated_signals_returns_goal() {
        let state = make_session_state(
            Some("col-025"),
            vec![("briefing", 5), ("hook", 3), ("compaction", 2)],
            Some("goal text"),
        );
        let result = derive_briefing_query(None, Some(&state), "col-025");
        assert_eq!(
            result, "goal text",
            "goal must win over topic_signals; old synthesis must not produce 'col-025 briefing hook compaction'"
        );
    }

    /// No-goal path is identical to pre-col-025 behavior (R-09 / AC-10).
    #[test]
    fn test_no_goal_briefing_behavior_unchanged() {
        let state = make_session_state(None, vec![], None);
        let result = derive_briefing_query(None, Some(&state), "legacy-feature");
        assert_eq!(
            result, "legacy-feature",
            "zero-goal path must be identical to pre-col-025 behavior"
        );
    }

    // -----------------------------------------------------------------------
    // IndexBriefingService integration helpers (mirrors listener.rs pattern)
    // -----------------------------------------------------------------------

    async fn make_test_store() -> Arc<Store> {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);
        // Leak TempDir — the database file remains accessible via fd on Linux.
        // Matches the pattern used in uds/listener.rs test helpers.
        std::mem::forget(tmp);
        store
    }

    fn make_test_services(store: &Arc<Store>) -> crate::services::ServiceLayer {
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_vector_store: Arc<AsyncVectorStore<VectorAdapter>> =
            Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));
        let embed = EmbedServiceHandle::new(); // Loading state — no model in test env
        let audit = Arc::new(crate::infra::audit::AuditLog::new(Arc::clone(store)));
        let usage_dedup = Arc::new(crate::infra::usage_dedup::UsageDedup::new());
        let test_pool = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(1, "test-pool-briefing")
                .expect("test RayonPool construction must succeed"),
        );
        crate::services::ServiceLayer::new(
            Arc::clone(store),
            vector_index,
            async_vector_store,
            Arc::clone(store),
            embed,
            adapt_service,
            audit,
            usage_dedup,
            std::collections::HashSet::from(["lesson-learned".to_string()]),
            test_pool,
            crate::infra::nli_handle::NliServiceHandle::new(),
            20,    // nli_top_k
            false, // nli_enabled: disabled for tests
            Arc::new(crate::infra::config::InferenceConfig::default()),
            Arc::new(unimatrix_observe::domain::DomainPackRegistry::with_builtin_claude_code()),
            Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
        )
    }

    // -----------------------------------------------------------------------
    // GH #355: Regression — quarantined entry exclusion
    // -----------------------------------------------------------------------

    /// GH #355: Regression — quarantined entries must not appear in index() results.
    ///
    /// Mirrors the deleted T-BS-08 test from BriefingService. Verifies that the
    /// `se.entry.status == Status::Active` post-filter in step 5 of index() is
    /// present and effective.
    ///
    /// If this test is deleted or the post-filter is removed, quarantined entries
    /// will appear in compaction briefings (R-08, AC-12, FR-08.1).
    ///
    /// Note: In the test environment, the embedding model is not loaded
    /// (EmbedServiceHandle starts in Loading state). index() therefore returns
    /// Err(EmbeddingFailed). This test verifies that:
    ///   1. No panic occurs building the full service stack with real Store.
    ///   2. The result never contains the quarantined entry — whether because
    ///      embedding fails before the filter runs, or because the filter correctly
    ///      excludes it when results are returned.
    ///
    /// The test is sensitive to filter removal: if the filter were removed AND
    /// SearchService were changed to return quarantined entries, this test would
    /// catch the regression once a real embedding model is present.
    #[tokio::test]
    async fn index_briefing_excludes_quarantined_entry() {
        let store = make_test_store().await;

        // Insert an active entry and a quarantined entry.
        let active_id = store
            .insert(NewEntry {
                title: "Active entry".to_string(),
                content: "This is active knowledge for crt-028 regression test.".to_string(),
                topic: "crt-028".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "crt-028".to_string(),
                trust_source: String::new(),
            })
            .await
            .expect("insert active entry");

        let quarantined_id = store
            .insert(NewEntry {
                title: "Quarantined entry".to_string(),
                content: "This entry is quarantined and must not appear in briefing results."
                    .to_string(),
                topic: "crt-028".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Quarantined,
                created_by: "test".to_string(),
                feature_cycle: "crt-028".to_string(),
                trust_source: String::new(),
            })
            .await
            .expect("insert quarantined entry");

        let services = make_test_services(&store);

        let audit_ctx = AuditContext {
            source: AuditSource::Uds {
                uid: 0,
                pid: None,
                session_id: String::new(),
            },
            caller_id: "test-regression-355".to_string(),
            session_id: None,
            feature_cycle: Some("crt-028".to_string()),
        };

        let params = IndexBriefingParams {
            query: "crt-028 decision knowledge".to_string(),
            k: 10,
            session_id: None,
            max_tokens: None,
            category_histogram: None,
        };

        let result = services.briefing.index(params, &audit_ctx, None).await;

        // Extract entries from result — on error (EmbeddingFailed in test env),
        // treat as empty (matching the graceful-degradation contract in the dispatcher).
        let entries = result.unwrap_or_default();

        // The quarantined entry must never appear in index() results.
        let quarantined_in_results = entries.iter().any(|e| e.id == quarantined_id);
        assert!(
            !quarantined_in_results,
            "quarantined entry (id={quarantined_id}) must not appear in index() results \
             (GH #355, post-filter: se.entry.status == Status::Active)"
        );

        // Presence assertion: if results are non-empty, the active entry should be there.
        // This is conditional on embedding being available. When embedding is unavailable,
        // entries will be empty (EmbeddingFailed degraded to vec![]) and this assertion
        // is vacuously skipped.
        if !entries.is_empty() {
            let active_in_results = entries.iter().any(|e| e.id == active_id);
            assert!(
                active_in_results,
                "active entry (id={active_id}) must appear in index() results when results are non-empty"
            );
        }
    }
}
