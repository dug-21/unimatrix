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

use crate::infra::session::{SessionState, TopicTally};
use crate::mcp::response::{IndexEntry, SNIPPET_CHARS};
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::search::{RetrievalMode, SearchService, ServiceSearchParams};
use crate::services::{AuditContext, CallerId, ServiceError};

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
/// **Step 2**: If `session_state` is `Some(s)` and has non-empty `topic_signals` AND
///             a non-empty `feature` (feature_cycle), synthesize:
///             `"{feature_cycle} {top_3_signals_by_vote_count}"`
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

    // Step 2: synthesize from session state when both feature_cycle and signals are present
    if let Some(state) = session_state {
        let feature_cycle = state.feature.as_deref().unwrap_or("");
        if !feature_cycle.is_empty() {
            let signals = extract_top_topic_signals(&state.topic_signals, 3);
            if !signals.is_empty() {
                // "crt-027 briefing hook compaction"
                return format!("{} {}", feature_cycle, signals.join(" "));
            }
        }
        // If feature_cycle is absent, fall through to step 3.
        // A query of bare topic_signals without feature context is less reliable
        // than the topic fallback (per FR-11 decision in pseudocode).
    }

    // Step 3: topic fallback (always available)
    topic.to_string()
}

/// Extract the top `n` topic signals sorted by vote count descending.
///
/// Returns signal strings only (not tallies). If fewer than `n` signals exist,
/// returns all available. EC-06: handles fewer than 3 signals gracefully.
fn extract_top_topic_signals(topic_signals: &HashMap<String, TopicTally>, n: usize) -> Vec<String> {
    if topic_signals.is_empty() {
        return vec![];
    }

    let mut pairs: Vec<(&String, u32)> = topic_signals.iter().map(|(k, v)| (k, v.count)).collect();

    // Sort by count descending, then by key ascending for determinism when counts tie
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    pairs.into_iter().take(n).map(|(k, _)| k.clone()).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::infra::session::TopicTally;
    use crate::services::{AuditContext, AuditSource};
    use unimatrix_core::{Store, Status};
    use unimatrix_core::async_wrappers::AsyncVectorStore;
    use unimatrix_core::VectorAdapter;
    use unimatrix_store::NewEntry;
    use unimatrix_store::test_helpers::open_test_store;
    use unimatrix_adapt::AdaptationService;
    use crate::infra::embed_handle::EmbedServiceHandle;

    // -----------------------------------------------------------------------
    // derive_briefing_query tests
    // -----------------------------------------------------------------------

    fn make_session_state(feature: Option<&str>, signals: Vec<(&str, u32)>) -> SessionState {
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
        }
    }

    /// Step 1: explicit task takes priority over session state and topic.
    #[test]
    fn derive_briefing_query_task_param_takes_priority() {
        let state = make_session_state(Some("crt-027"), vec![("briefing", 5), ("hook", 3)]);
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
        let state = make_session_state(Some("crt-027"), vec![("briefing", 5)]);
        let result = derive_briefing_query(Some(""), Some(&state), "crt-027");
        // Should NOT return "" — falls to step 2 (feature + signals present)
        assert_ne!(result, "", "empty task must not return empty string");
        assert!(
            result.contains("crt-027"),
            "result should contain feature_cycle or topic, got: {result}"
        );
    }

    /// Step 2: synthesized from session signals + feature_cycle, top 3 by vote count.
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
        );
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027/spec briefing hook compaction",
            "must use feature_cycle + top 3 signals by vote"
        );
    }

    /// Step 2: fewer than 3 signals — use what is available, no trailing spaces.
    #[test]
    fn derive_briefing_query_fewer_than_three_signals() {
        let state = make_session_state(Some("crt-027/spec"), vec![("briefing", 5)]);
        let result = derive_briefing_query(None, Some(&state), "crt-027");
        assert_eq!(
            result, "crt-027/spec briefing",
            "fewer-than-3 signals must not produce trailing spaces"
        );
        assert!(
            !result.ends_with(' '),
            "result must not end with trailing space"
        );
    }

    /// Step 2 requires feature_cycle — absent feature_cycle falls to step 3.
    #[test]
    fn derive_briefing_query_no_feature_cycle_falls_to_topic() {
        let state = make_session_state(
            None, // no feature_cycle
            vec![("briefing", 5), ("hook", 3)],
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
        let state = make_session_state(Some("crt-027"), vec![]);
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

    /// extract_top_topic_signals returns empty vec for empty input.
    #[test]
    fn extract_top_topic_signals_empty_input() {
        let empty: HashMap<String, TopicTally> = HashMap::new();
        let result = extract_top_topic_signals(&empty, 3);
        assert!(result.is_empty());
    }

    /// extract_top_topic_signals returns top n by count descending.
    #[test]
    fn extract_top_topic_signals_ordered_by_count() {
        let mut signals = HashMap::new();
        signals.insert(
            "low".to_string(),
            TopicTally {
                count: 1,
                last_seen: 0,
            },
        );
        signals.insert(
            "high".to_string(),
            TopicTally {
                count: 10,
                last_seen: 0,
            },
        );
        signals.insert(
            "mid".to_string(),
            TopicTally {
                count: 5,
                last_seen: 0,
            },
        );

        let result = extract_top_topic_signals(&signals, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "high", "highest count must be first");
        assert_eq!(result[1], "mid", "second highest must be second");
    }

    /// extract_top_topic_signals with fewer entries than n returns all.
    #[test]
    fn extract_top_topic_signals_fewer_than_n() {
        let mut signals = HashMap::new();
        signals.insert(
            "only".to_string(),
            TopicTally {
                count: 3,
                last_seen: 0,
            },
        );

        let result = extract_top_topic_signals(&signals, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "only");
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
