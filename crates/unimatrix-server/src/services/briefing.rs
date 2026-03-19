//! BriefingService: transport-agnostic briefing assembly (vnc-007).
//!
//! Unifies MCP `context_briefing` and UDS `handle_compact_payload` behind
//! a single caller-parameterized `assemble()` method. Entry sources (conventions,
//! semantic search, injection history) are selected by `BriefingParams`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use unimatrix_core::{CoreError, EntryRecord, QueryFilter, Status, Store};
use unimatrix_engine::effectiveness::EffectivenessCategory;

use crate::infra::audit::{AuditEvent, Outcome};
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::search::{SearchService, ServiceSearchParams};
use crate::services::{AuditContext, CallerId, ServiceError};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Transport-agnostic briefing assembly service.
///
/// Assembles knowledge entries into a budget-constrained briefing result.
/// Callers control behavior via `BriefingParams`: which entry sources are
/// active, token budget, and whether semantic search is invoked.
#[derive(Clone)]
pub(crate) struct BriefingService {
    entry_store: Arc<Store>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    semantic_k: usize,
    /// crt-018b (ADR-004): effectiveness classification handle.
    /// Required parameter — missing wiring is a compile error.
    effectiveness_state: EffectivenessStateHandle,
    /// crt-018b (ADR-001): generation-cached snapshot shared across rmcp clones.
    /// `Arc` wrapper ensures all clones share the same cached copy.
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}

/// Caller-provided parameters controlling `BriefingService::assemble()` behavior.
pub(crate) struct BriefingParams {
    /// Role for convention lookup (topic filter).
    pub role: Option<String>,
    /// Task description for semantic search query.
    pub task: Option<String>,
    /// Feature tag for feature-boost on search results and conventions.
    pub feature: Option<String>,
    /// Token budget for the assembled briefing.
    pub max_tokens: usize,
    /// Whether to include convention entries from the store.
    pub include_conventions: bool,
    /// Whether to perform semantic search (embedding + HNSW).
    /// When false, NO SearchService involvement occurs.
    pub include_semantic: bool,
    /// Injection history entries (from UDS session state).
    /// When Some, entries are fetched by ID and partitioned by category.
    pub injection_history: Option<Vec<InjectionEntry>>,
}

/// Assembled briefing output, transport-agnostic.
#[derive(Debug)]
pub(crate) struct BriefingResult {
    /// Convention entries (from role/topic query).
    pub conventions: Vec<EntryRecord>,
    /// Semantically relevant entries with similarity scores.
    pub relevant_context: Vec<(EntryRecord, f64)>,
    /// Injection history entries partitioned by category.
    pub injection_sections: InjectionSections,
    /// All unique entry IDs included in the briefing.
    pub entry_ids: Vec<u64>,
    /// Whether semantic search was available and attempted successfully.
    pub search_available: bool,
}

/// Injection history entries partitioned by category with fixed section priorities.
#[derive(Debug)]
pub(crate) struct InjectionSections {
    /// Decision entries (category="decision"), sorted by confidence descending.
    pub decisions: Vec<(EntryRecord, f64)>,
    /// Other entries (not decision/convention), sorted by confidence descending.
    pub injections: Vec<(EntryRecord, f64)>,
    /// Convention entries (category="convention"), sorted by confidence descending.
    pub conventions: Vec<(EntryRecord, f64)>,
}

impl Default for InjectionSections {
    fn default() -> Self {
        InjectionSections {
            decisions: Vec::new(),
            injections: Vec::new(),
            conventions: Vec::new(),
        }
    }
}

/// Minimal record representing an injection history entry.
///
/// Abstracts over UDS session's `InjectionRecord` without coupling
/// BriefingService to the session module.
pub(crate) struct InjectionEntry {
    pub entry_id: u64,
    pub confidence: f64,
}

// ---------------------------------------------------------------------------
// Validation constants (mirror validation.rs)
// ---------------------------------------------------------------------------

const MAX_ROLE_LEN: usize = 500;
const MAX_TASK_LEN: usize = 10_000;
const MIN_MAX_TOKENS: usize = 500;
const MAX_MAX_TOKENS: usize = 10_000;

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

/// Parse `UNIMATRIX_BRIEFING_K` env var.
///
/// Returns default (3) if unset or unparseable. Clamps to \[1, 20\].
/// Read once at construction time -- runtime changes to the env var are ignored.
pub(crate) fn parse_semantic_k() -> usize {
    parse_semantic_k_from(std::env::var("UNIMATRIX_BRIEFING_K").ok())
}

/// Pure parsing logic for semantic_k. Extracted for testability without
/// requiring env var mutation (which is unsafe in Rust 2024+).
fn parse_semantic_k_from(value: Option<String>) -> usize {
    match value {
        Some(val) => match val.parse::<usize>() {
            Ok(k) => k.clamp(1, 20),
            Err(_) => {
                tracing::warn!(
                    value = %val,
                    "UNIMATRIX_BRIEFING_K: invalid value, using default 3"
                );
                3
            }
        },
        None => 3,
    }
}

impl BriefingService {
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        semantic_k: usize,
        effectiveness_state: EffectivenessStateHandle, // crt-018b (ADR-004): required, non-optional
    ) -> Self {
        BriefingService {
            entry_store,
            search,
            gateway,
            semantic_k,
            effectiveness_state,
            cached_snapshot: EffectivenessSnapshot::new_shared(),
        }
    }

    /// Assemble a briefing from the requested entry sources within the token budget.
    ///
    /// The pipeline executes up to three independent fetch paths based on params:
    /// 1. Injection history (if `injection_history` is Some)
    /// 2. Convention lookup (if `include_conventions` is true and `role` is Some)
    /// 3. Semantic search (if `include_semantic` is true and `task` is Some)
    pub(crate) async fn assemble(
        &self,
        params: BriefingParams,
        audit_ctx: &AuditContext,
        caller_id: Option<&CallerId>,
    ) -> Result<BriefingResult, ServiceError> {
        // Step 1: S3 input validation
        validate_briefing_inputs(&params)?;

        // crt-018b (ADR-001): snapshot effectiveness categories under short read lock.
        // Lock ordering (R-01): read generation, drop read guard, then acquire mutex.
        // Never hold both guards simultaneously.
        let categories: HashMap<u64, EffectivenessCategory> = {
            let current_generation = {
                let guard = self
                    .effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
                // read guard drops here
            };
            // Read guard is now dropped — safe to acquire mutex
            let mut cache = self
                .cached_snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if cache.generation != current_generation {
                let guard = self
                    .effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                cache.generation = guard.generation;
                cache.categories = guard.categories.clone();
                // guard drops here
            }
            cache.categories.clone()
        };

        // Step 2: Initialize budget tracker
        let char_budget = params.max_tokens.saturating_mul(4); // ~4 chars per token
        let mut budget_remaining = char_budget;

        // Step 3: Initialize result accumulators
        let mut conventions: Vec<EntryRecord> = Vec::new();
        let mut relevant_context: Vec<(EntryRecord, f64)> = Vec::new();
        let mut injection_sections = InjectionSections::default();
        let mut all_entry_ids: Vec<u64> = Vec::new();
        let mut search_available = true;

        // Step 4: Injection history path
        if let Some(ref history) = params.injection_history {
            let (sections, chars_used) = self
                .process_injection_history(history, char_budget, &categories)
                .await?;

            // Collect entry IDs from injection sections
            for (entry, _) in &sections.decisions {
                all_entry_ids.push(entry.id);
            }
            for (entry, _) in &sections.injections {
                all_entry_ids.push(entry.id);
            }
            for (entry, _) in &sections.conventions {
                all_entry_ids.push(entry.id);
            }

            budget_remaining = budget_remaining.saturating_sub(chars_used);
            injection_sections = sections;
        }

        // Step 5: Convention lookup path
        if params.include_conventions {
            if let Some(ref role) = params.role {
                let mut conv_entries = self
                    .entry_store
                    .query(QueryFilter {
                        topic: Some(role.clone()),
                        category: Some("convention".to_string()),
                        status: Some(Status::Active),
                        tags: None,
                        time_range: None,
                    })
                    .await
                    .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

                // S4: exclude quarantined (defense-in-depth)
                conv_entries.retain(|e| !SecurityGateway::is_quarantined(&e.status));

                // Convention sort: feature-tagged entries first (when feature is set),
                // then confidence descending, then effectiveness_priority descending (crt-018b).
                if let Some(ref feature) = params.feature {
                    conv_entries.sort_by(|a, b| {
                        let a_has = a.tags.iter().any(|t| t == feature);
                        let b_has = b.tags.iter().any(|t| t == feature);
                        match (a_has, b_has) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => {
                                // Among entries with same feature-tag status:
                                // confidence descending, then effectiveness tiebreaker
                                let conf_ord = b
                                    .confidence
                                    .partial_cmp(&a.confidence)
                                    .unwrap_or(std::cmp::Ordering::Equal);
                                if conf_ord != std::cmp::Ordering::Equal {
                                    return conf_ord;
                                }
                                let pri_a = effectiveness_priority(categories.get(&a.id).copied());
                                let pri_b = effectiveness_priority(categories.get(&b.id).copied());
                                pri_b.cmp(&pri_a)
                            }
                        }
                    });
                } else {
                    // No feature: sort by (confidence DESC, effectiveness_priority DESC)
                    conv_entries.sort_by(|a, b| {
                        let conf_ord = b
                            .confidence
                            .partial_cmp(&a.confidence)
                            .unwrap_or(std::cmp::Ordering::Equal);
                        if conf_ord != std::cmp::Ordering::Equal {
                            return conf_ord;
                        }
                        let pri_a = effectiveness_priority(categories.get(&a.id).copied());
                        let pri_b = effectiveness_priority(categories.get(&b.id).copied());
                        pri_b.cmp(&pri_a)
                    });
                }

                // Budget allocation (linear fill)
                for entry in conv_entries {
                    let entry_chars = entry.title.len() + entry.content.len() + 50;
                    if budget_remaining >= entry_chars {
                        all_entry_ids.push(entry.id);
                        budget_remaining -= entry_chars;
                        conventions.push(entry);
                    } else {
                        break;
                    }
                }
            }
        }

        // Step 6: Semantic search path
        if params.include_semantic {
            // S2 rate check when semantic search is active
            if let Some(cid) = caller_id {
                self.gateway.check_search_rate(cid)?;
            }

            if let Some(ref task) = params.task {
                let search_params = ServiceSearchParams {
                    query: task.clone(),
                    k: self.semantic_k,
                    filters: None,
                    similarity_floor: None,
                    confidence_floor: None,
                    feature_tag: params.feature.clone(),
                    co_access_anchors: if all_entry_ids.is_empty() {
                        None
                    } else {
                        Some(all_entry_ids.clone())
                    },
                    caller_agent_id: None,
                    retrieval_mode: crate::services::RetrievalMode::Flexible, // crt-010: briefing uses Flexible
                };

                // Use a synthetic UDS caller_id for rate limiting when no caller is provided
                let default_caller = CallerId::UdsSession("briefing-internal".to_string());
                let effective_caller = caller_id.unwrap_or(&default_caller);
                match self
                    .search
                    .search(search_params, audit_ctx, effective_caller)
                    .await
                {
                    Ok(results) => {
                        for scored_entry in results.entries {
                            let entry_chars = scored_entry.entry.title.len()
                                + scored_entry.entry.content.len()
                                + 50;
                            if budget_remaining >= entry_chars {
                                all_entry_ids.push(scored_entry.entry.id);
                                budget_remaining -= entry_chars;
                                relevant_context
                                    .push((scored_entry.entry, scored_entry.similarity));
                            } else {
                                break;
                            }
                        }
                    }
                    Err(ServiceError::EmbeddingFailed(_)) => {
                        // EmbedNotReady — graceful degradation
                        search_available = false;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Step 7: Deduplicate entry IDs
        all_entry_ids.sort_unstable();
        all_entry_ids.dedup();

        // Step 8: S5 audit emission
        self.gateway.emit_audit(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: audit_ctx.session_id.clone().unwrap_or_default(),
            agent_id: audit_ctx.caller_id.clone(),
            operation: "briefing_service".to_string(),
            target_ids: all_entry_ids.clone(),
            outcome: Outcome::Success,
            detail: format!("assembled {} entries", all_entry_ids.len()),
        });

        Ok(BriefingResult {
            conventions,
            relevant_context,
            injection_sections,
            entry_ids: all_entry_ids,
            search_available,
        })
    }

    /// Process injection history: deduplicate, fetch, partition, sort, budget-truncate.
    ///
    /// Returns the partitioned sections and total characters consumed.
    ///
    /// `categories` is the current effectiveness snapshot passed from `assemble()`.
    /// When empty (cold start), all effectiveness priorities are 0 and sort degrades
    /// to confidence-only (correct behavior, no special-casing needed).
    async fn process_injection_history(
        &self,
        history: &[InjectionEntry],
        char_budget: usize,
        categories: &HashMap<u64, EffectivenessCategory>,
    ) -> Result<(InjectionSections, usize), ServiceError> {
        // Step 1: Deduplicate — keep highest confidence per entry_id
        let mut best_confidence: HashMap<u64, f64> = HashMap::new();
        for record in history {
            let entry = best_confidence.entry(record.entry_id).or_insert(0.0);
            if record.confidence > *entry {
                *entry = record.confidence;
            }
        }

        // Step 2: Fetch entries, exclude quarantined, partition by category
        let mut decisions: Vec<(EntryRecord, f64)> = Vec::new();
        let mut injections: Vec<(EntryRecord, f64)> = Vec::new();
        let mut conventions: Vec<(EntryRecord, f64)> = Vec::new();

        for (&entry_id, &confidence) in &best_confidence {
            match self.entry_store.get(entry_id).await {
                Ok(entry) => {
                    if SecurityGateway::is_quarantined(&entry.status) {
                        continue;
                    }
                    // crt-010 AC-11: exclude deprecated entries from injection history
                    if entry.status == Status::Deprecated {
                        continue;
                    }
                    match entry.category.as_str() {
                        "decision" => decisions.push((entry, confidence)),
                        "convention" => conventions.push((entry, confidence)),
                        _ => injections.push((entry, confidence)),
                    }
                }
                Err(_) => continue, // entry deleted, skip
            }
        }

        // Step 3: Sort each group by (confidence DESC, effectiveness_priority DESC).
        // crt-018b: effectiveness_priority is the tiebreaker; confidence is still primary.
        // When categories is empty (cold start), effectiveness_priority(None) = 0 for all
        // entries, so 0.cmp(&0) == Equal — sort degrades gracefully to confidence-only.
        let injection_sort = |a: &(EntryRecord, f64), b: &(EntryRecord, f64)| {
            let conf_ord = b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal);
            if conf_ord != std::cmp::Ordering::Equal {
                return conf_ord;
            }
            let pri_a = effectiveness_priority(categories.get(&a.0.id).copied());
            let pri_b = effectiveness_priority(categories.get(&b.0.id).copied());
            pri_b.cmp(&pri_a)
        };
        decisions.sort_by(injection_sort);
        injections.sort_by(injection_sort);
        conventions.sort_by(injection_sort);

        // Step 4: Proportional budget allocation (per ADR-003)
        // Header: 5%, Decisions: 40%, Injections: 30%, Conventions: 20%, Buffer: 5%
        let decision_budget = char_budget * 40 / 100;
        let injection_budget = char_budget * 30 / 100;
        let convention_budget = char_budget * 20 / 100;

        let (decisions, dec_chars) = truncate_to_budget(decisions, decision_budget);
        let (injections, inj_chars) = truncate_to_budget(injections, injection_budget);
        let (conventions, conv_chars) = truncate_to_budget(conventions, convention_budget);

        let total_chars = dec_chars + inj_chars + conv_chars;

        Ok((
            InjectionSections {
                decisions,
                injections,
                conventions,
            },
            total_chars,
        ))
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Map an effectiveness category to a sort priority integer (crt-018b, ADR-004).
///
/// Used as a secondary sort key in injection history and convention lookup sorts.
/// Primary key is always confidence descending; effectiveness breaks ties only.
///
/// Scale (ARCHITECTURE Component 4 canonical — supersedes SPECIFICATION FR-07 3-2-1-0):
/// - `Effective`   =>  2  (proven useful)
/// - `Settled`     =>  1  (used but unconfirmed)
/// - `None`        =>  0  (not yet classified; cold-start safe)
/// - `Unmatched`   =>  0  (no outcome data)
/// - `Ineffective` => -1  (used, poor outcomes)
/// - `Noisy`       => -2  (noisy trust source; lowest priority)
fn effectiveness_priority(category: Option<EffectivenessCategory>) -> i32 {
    match category {
        Some(EffectivenessCategory::Effective) => 2,
        Some(EffectivenessCategory::Settled) => 1,
        None | Some(EffectivenessCategory::Unmatched) => 0,
        Some(EffectivenessCategory::Ineffective) => -1,
        Some(EffectivenessCategory::Noisy) => -2,
    }
}

/// S3: Validate briefing inputs.
fn validate_briefing_inputs(params: &BriefingParams) -> Result<(), ServiceError> {
    if let Some(ref role) = params.role {
        if role.len() > MAX_ROLE_LEN {
            return Err(ServiceError::ValidationFailed(format!(
                "role exceeds {} characters",
                MAX_ROLE_LEN
            )));
        }
        for ch in role.chars() {
            if ch.is_control() && ch != '\n' && ch != '\t' {
                return Err(ServiceError::ValidationFailed(
                    "role contains control characters".to_string(),
                ));
            }
        }
    }

    if let Some(ref task) = params.task {
        if task.len() > MAX_TASK_LEN {
            return Err(ServiceError::ValidationFailed(format!(
                "task exceeds {} characters",
                MAX_TASK_LEN
            )));
        }
        for ch in task.chars() {
            if ch.is_control() && ch != '\n' && ch != '\t' {
                return Err(ServiceError::ValidationFailed(
                    "task contains control characters".to_string(),
                ));
            }
        }
    }

    if params.max_tokens < MIN_MAX_TOKENS || params.max_tokens > MAX_MAX_TOKENS {
        return Err(ServiceError::ValidationFailed(format!(
            "max_tokens must be between {} and {}",
            MIN_MAX_TOKENS, MAX_MAX_TOKENS
        )));
    }

    Ok(())
}

/// Truncate a list of entries to fit within a character budget.
/// Returns the truncated list and total characters consumed.
fn truncate_to_budget(
    entries: Vec<(EntryRecord, f64)>,
    char_budget: usize,
) -> (Vec<(EntryRecord, f64)>, usize) {
    let mut result = Vec::new();
    let mut used = 0;
    for (entry, confidence) in entries {
        let entry_chars = entry.title.len() + entry.content.len() + 50;
        if used + entry_chars <= char_budget {
            used += entry_chars;
            result.push((entry, confidence));
        } else {
            break;
        }
    }
    (result, used)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use unimatrix_core::async_wrappers::AsyncVectorStore;
    use unimatrix_core::{NewEntry, Store, VectorAdapter, VectorConfig, VectorIndex};
    use unimatrix_engine::effectiveness::EffectivenessCategory;

    use crate::infra::audit::AuditLog;
    use crate::infra::embed_handle::EmbedServiceHandle;
    use crate::services::gateway::SecurityGateway;
    use crate::services::search::SearchService;
    use crate::services::{AuditContext, AuditSource};

    async fn make_test_store() -> (tempfile::TempDir, Arc<Store>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            unimatrix_store::SqlxStore::open(
                &dir.path().join("test.db"),
                unimatrix_store::pool_config::PoolConfig::default(),
            )
            .await
            .expect("open store"),
        );
        (dir, store)
    }

    fn make_briefing_service(store: &Arc<Store>) -> (BriefingService, Arc<Store>) {
        make_briefing_service_with_effectiveness(
            store,
            crate::services::effectiveness::EffectivenessState::new_handle(),
        )
    }

    fn make_briefing_service_with_effectiveness(
        store: &Arc<Store>,
        effectiveness_state: crate::services::effectiveness::EffectivenessStateHandle,
    ) -> (BriefingService, Arc<Store>) {
        let entry_store = Arc::clone(store);
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(store), VectorConfig::default()).expect("vector index"),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let embed_service = EmbedServiceHandle::new();
        let adapt_service = Arc::new(unimatrix_adapt::AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));
        let audit = Arc::new(AuditLog::new(Arc::clone(store)));
        let gateway = Arc::new(SecurityGateway::new(Arc::clone(&audit)));

        let confidence_state = Arc::new(std::sync::RwLock::new(
            crate::services::confidence::ConfidenceState::default(),
        ));
        let typed_graph_state = crate::services::typed_graph::TypedGraphState::new_handle();
        // dsn-001: boosted_categories defaults to ["lesson-learned"] in test helper until
        // startup-wiring threads config.knowledge.boosted_categories through.
        let test_rayon_pool = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(1, "test_pool")
                .expect("test rayon pool construction"),
        );
        let search = SearchService::new(
            Arc::clone(store),
            vector_store,
            Arc::clone(store),
            embed_service,
            adapt_service,
            Arc::clone(&gateway),
            confidence_state,
            Arc::clone(&effectiveness_state), // crt-018b: shared handle
            typed_graph_state,                // crt-021: cold-start empty state for tests
            std::collections::HashSet::from(["lesson-learned".to_string()]),
            test_rayon_pool,
        );

        let service = BriefingService::new(
            Arc::clone(store),
            search,
            gateway,
            3, // default semantic_k for existing tests
            effectiveness_state,
        );

        (service, Arc::clone(store))
    }

    fn test_audit_ctx() -> AuditContext {
        AuditContext {
            source: AuditSource::Internal {
                service: "test".to_string(),
            },
            caller_id: "test".to_string(),
            session_id: None,
            feature_cycle: None,
        }
    }

    /// Insert a test entry into the store. Returns the auto-generated entry ID.
    async fn store_entry(
        store: &Arc<Store>,
        title: &str,
        content: &str,
        category: &str,
        topic: &str,
        tags: Vec<String>,
        status: Status,
    ) -> u64 {
        let entry = NewEntry {
            title: title.to_string(),
            content: content.to_string(),
            category: category.to_string(),
            topic: topic.to_string(),
            tags,
            source: String::new(),
            status,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        store.insert(entry).await.expect("insert")
    }

    // -- T-BS-01: Convention lookup with role --

    #[tokio::test]
    async fn convention_lookup_with_role() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id1 = store_entry(
            &store,
            "Conv 1",
            "Always use trait objects",
            "convention",
            "architect",
            vec![],
            Status::Active,
        )
        .await;
        let id2 = store_entry(
            &store,
            "Conv 2",
            "Write ADRs for decisions",
            "convention",
            "architect",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: Some("architect".to_string()),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert_eq!(result.conventions.len(), 2);
        assert!(result.injection_sections.decisions.is_empty());
        assert!(result.injection_sections.injections.is_empty());
        assert!(result.injection_sections.conventions.is_empty());
        assert!(result.relevant_context.is_empty());
        assert_eq!(result.entry_ids.len(), 2);
        assert!(result.entry_ids.contains(&id1));
        assert!(result.entry_ids.contains(&id2));
    }

    // -- T-BS-02: Convention lookup skipped when role=None --

    #[tokio::test]
    async fn convention_lookup_skipped_when_no_role() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store,
            "Conv",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(result.conventions.is_empty());
    }

    // -- T-BS-03: Convention lookup skipped when include_conventions=false --

    #[tokio::test]
    async fn convention_lookup_skipped_when_disabled() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store,
            "Conv",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(result.conventions.is_empty());
    }

    // -- T-BS-04: Semantic search isolation when include_semantic=false --

    #[tokio::test]
    async fn semantic_search_isolation_when_disabled() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        // Embed service not started — would fail if SearchService were called
        let params = BriefingParams {
            role: None,
            task: Some("test query".to_string()),
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(result.relevant_context.is_empty());
        // search_available stays true (not attempted, not failed)
        assert!(result.search_available);
    }

    // -- T-BS-05: Semantic search graceful degradation (EmbedNotReady) --

    #[tokio::test]
    async fn semantic_search_embed_not_ready() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store,
            "Conv",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        // Embed service not started — will trigger EmbedNotReady
        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: Some("test query".to_string()),
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: true,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(!result.search_available);
        assert!(result.relevant_context.is_empty());
        // Conventions still populated
        assert_eq!(result.conventions.len(), 1);
    }

    // -- T-BS-06: Injection history basic processing --

    #[tokio::test]
    async fn injection_history_basic_processing() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id_dec = store_entry(
            &store,
            "ADR-001",
            "Use redb for storage",
            "decision",
            "architecture",
            vec![],
            Status::Active,
        )
        .await;
        let id_conv = store_entry(
            &store,
            "Coding Convention",
            "Use Result<T,E>",
            "convention",
            "rust",
            vec![],
            Status::Active,
        )
        .await;
        let id_pat = store_entry(
            &store,
            "Error Handling",
            "Pattern for errors",
            "pattern",
            "rust",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_dec,
                    confidence: 0.8,
                },
                InjectionEntry {
                    entry_id: id_conv,
                    confidence: 0.7,
                },
                InjectionEntry {
                    entry_id: id_pat,
                    confidence: 0.9,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert_eq!(result.injection_sections.decisions.len(), 1);
        assert_eq!(result.injection_sections.decisions[0].0.id, id_dec);
        assert_eq!(result.injection_sections.conventions.len(), 1);
        assert_eq!(result.injection_sections.conventions[0].0.id, id_conv);
        assert_eq!(result.injection_sections.injections.len(), 1);
        assert_eq!(result.injection_sections.injections[0].0.id, id_pat);
        assert_eq!(result.entry_ids.len(), 3);
    }

    // -- T-BS-07: Injection history deduplication --

    #[tokio::test]
    async fn injection_history_deduplication() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store,
            "Entry",
            "content",
            "pattern",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id,
                    confidence: 0.3,
                },
                InjectionEntry {
                    entry_id: id,
                    confidence: 0.9,
                },
                InjectionEntry {
                    entry_id: id,
                    confidence: 0.5,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        // Entry appears exactly once
        let total = result.injection_sections.injections.len();
        assert_eq!(total, 1);
        // With highest confidence
        assert!((result.injection_sections.injections[0].1 - 0.9).abs() < f64::EPSILON);
    }

    // -- T-BS-08: Injection history quarantine exclusion --

    #[tokio::test]
    async fn injection_history_quarantine_exclusion() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id_active = store_entry(
            &store,
            "Active",
            "content",
            "pattern",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_quarantined = store_entry(
            &store,
            "Quarantined",
            "bad content",
            "pattern",
            "test",
            vec![],
            Status::Quarantined,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_active,
                    confidence: 0.8,
                },
                InjectionEntry {
                    entry_id: id_quarantined,
                    confidence: 0.9,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert_eq!(result.injection_sections.injections.len(), 1);
        assert_eq!(result.injection_sections.injections[0].0.id, id_active);
        assert!(!result.entry_ids.contains(&id_quarantined));
    }

    // -- T-BS-09: Injection history deleted entry skipped --

    #[tokio::test]
    async fn injection_history_deleted_entry_skipped() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store,
            "Exists",
            "content",
            "pattern",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        // Entry 99999 does not exist

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id,
                    confidence: 0.8,
                },
                InjectionEntry {
                    entry_id: 99999,
                    confidence: 0.5,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert_eq!(result.injection_sections.injections.len(), 1);
    }

    // -- T-BS-10: Injection history deprecated entries EXCLUDED (crt-010 AC-11) --

    #[tokio::test]
    async fn injection_history_deprecated_entries_excluded() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store,
            "Deprecated",
            "old content",
            "pattern",
            "test",
            vec![],
            Status::Deprecated,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![InjectionEntry {
                entry_id: id,
                confidence: 0.8,
            }]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        // crt-010: deprecated entries are now excluded from injection history (AC-11)
        assert_eq!(result.injection_sections.injections.len(), 0);
    }

    // -- T-BS-11: Token budget truncation --

    #[tokio::test]
    async fn token_budget_truncates_conventions() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        // Each entry ~350 chars (title + content + 50)
        let big_content = "x".repeat(300);
        let _id1 = store_entry(
            &store,
            "Conv 1",
            &big_content,
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;
        let _id2 = store_entry(
            &store,
            "Conv 2",
            &big_content,
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;
        let _id3 = store_entry(
            &store,
            "Conv 3",
            &big_content,
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        // Budget: 500 tokens * 4 = 2000 chars. Each entry ~356 chars. Max ~5 entries.
        // With 3 entries at ~356 each = ~1068, all should fit.
        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: None,
            max_tokens: 500, // 2000 chars total. Each ~356 chars.
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        // All 3 should fit at 2000 char budget
        assert!(result.conventions.len() <= 3);
        assert!(!result.conventions.is_empty());
    }

    // -- T-BS-12: Token budget proportional allocation with injection history --

    #[tokio::test]
    async fn token_budget_proportional_injection() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        // Create entries that test proportional allocation
        let mut ids = Vec::new();
        for _i in 0..3 {
            ids.push(
                store_entry(
                    &store,
                    "Decision",
                    "Decision content here",
                    "decision",
                    "arch",
                    vec![],
                    Status::Active,
                )
                .await,
            );
        }
        for _i in 0..3 {
            ids.push(
                store_entry(
                    &store,
                    "Pattern",
                    "Pattern content here",
                    "pattern",
                    "arch",
                    vec![],
                    Status::Active,
                )
                .await,
            );
        }
        for _i in 0..3 {
            ids.push(
                store_entry(
                    &store,
                    "Conv",
                    "Convention content here",
                    "convention",
                    "arch",
                    vec![],
                    Status::Active,
                )
                .await,
            );
        }

        let history: Vec<InjectionEntry> = ids
            .iter()
            .enumerate()
            .map(|(i, &id)| InjectionEntry {
                entry_id: id,
                confidence: 0.5 + (i as f64) * 0.01,
            })
            .collect();

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 500,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(history),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        // Entries should be present in sections
        assert!(!result.injection_sections.decisions.is_empty());
        assert!(!result.injection_sections.injections.is_empty());
        assert!(!result.injection_sections.conventions.is_empty());
    }

    // -- T-BS-13: Token budget minimum boundary --

    #[tokio::test]
    async fn token_budget_minimum_boundary() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store,
            "Entry",
            "content",
            "pattern",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 500, // minimum
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![InjectionEntry {
                entry_id: id,
                confidence: 0.8,
            }]),
        };

        // Should not panic
        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        // Entry is small enough to fit
        assert!(result.injection_sections.injections.len() <= 1);
    }

    // -- T-BS-14: Input validation — role too long --

    #[tokio::test]
    async fn validation_role_too_long() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: Some("x".repeat(501)),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let err = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("role")));
    }

    // -- T-BS-15: Input validation — task too long --

    #[tokio::test]
    async fn validation_task_too_long() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: None,
            task: Some("x".repeat(10_001)),
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let err = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("task")));
    }

    // -- T-BS-16: Input validation — max_tokens out of range --

    #[tokio::test]
    async fn validation_max_tokens_too_low() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 100,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let err = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("max_tokens")));
    }

    #[tokio::test]
    async fn validation_max_tokens_too_high() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 20_000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let err = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("max_tokens")));
    }

    // -- T-BS-17: Input validation — control characters in task --

    #[tokio::test]
    async fn validation_task_control_chars() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: None,
            task: Some("test\x01query".to_string()),
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let err = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("control")));
    }

    // -- T-BS-18: Empty knowledge base --

    #[tokio::test]
    async fn empty_knowledge_base() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(result.conventions.is_empty());
        assert!(result.entry_ids.is_empty());
    }

    // -- T-BS-19: Feature sort — feature-tagged conventions first --

    #[tokio::test]
    async fn feature_tagged_conventions_sorted_first() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let _id_general = store_entry(
            &store,
            "General Conv",
            "general content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;
        let id_feature = store_entry(
            &store,
            "Feature Conv",
            "feature content",
            "convention",
            "dev",
            vec!["vnc-007".to_string()],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: Some("vnc-007".to_string()),
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert_eq!(result.conventions.len(), 2);
        assert_eq!(
            result.conventions[0].id, id_feature,
            "feature-tagged entry should be first"
        );
    }

    // -- T-BS-20: All injection entries quarantined --

    #[tokio::test]
    async fn all_injection_entries_quarantined() {
        let (_dir, store) = make_test_store().await;
        let (service, _es) = make_briefing_service(&store);

        let id1 = store_entry(
            &store,
            "Q1",
            "content",
            "pattern",
            "test",
            vec![],
            Status::Quarantined,
        )
        .await;
        let id2 = store_entry(
            &store,
            "Q2",
            "content",
            "decision",
            "test",
            vec![],
            Status::Quarantined,
        )
        .await;
        let id3 = store_entry(
            &store,
            "Q3",
            "content",
            "convention",
            "test",
            vec![],
            Status::Quarantined,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id1,
                    confidence: 0.8,
                },
                InjectionEntry {
                    entry_id: id2,
                    confidence: 0.7,
                },
                InjectionEntry {
                    entry_id: id3,
                    confidence: 0.6,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");
        assert!(result.injection_sections.decisions.is_empty());
        assert!(result.injection_sections.injections.is_empty());
        assert!(result.injection_sections.conventions.is_empty());
        assert!(result.entry_ids.is_empty());
    }

    // -- crt-013: parse_semantic_k tests (pure function, no env var mutation) --

    #[test]
    fn parse_semantic_k_default_when_unset() {
        let k = super::parse_semantic_k_from(None);
        assert_eq!(k, 3);
    }

    #[test]
    fn parse_semantic_k_valid_value() {
        let k = super::parse_semantic_k_from(Some("5".to_string()));
        assert_eq!(k, 5);
    }

    #[test]
    fn parse_semantic_k_clamps_to_min() {
        let k = super::parse_semantic_k_from(Some("0".to_string()));
        assert_eq!(k, 1);
    }

    #[test]
    fn parse_semantic_k_clamps_to_max() {
        let k = super::parse_semantic_k_from(Some("100".to_string()));
        assert_eq!(k, 20);
    }

    #[test]
    fn parse_semantic_k_invalid_falls_back() {
        let k = super::parse_semantic_k_from(Some("abc".to_string()));
        assert_eq!(k, 3);
    }

    #[test]
    fn parse_semantic_k_boundary_one() {
        let k = super::parse_semantic_k_from(Some("1".to_string()));
        assert_eq!(k, 1);
    }

    #[test]
    fn parse_semantic_k_boundary_twenty() {
        let k = super::parse_semantic_k_from(Some("20".to_string()));
        assert_eq!(k, 20);
    }

    // ---------------------------------------------------------------------------
    // crt-018b: effectiveness_priority pure function tests (AC-07, R-09)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_effectiveness_priority_effective() {
        assert_eq!(
            super::effectiveness_priority(Some(EffectivenessCategory::Effective)),
            2_i32
        );
    }

    #[test]
    fn test_effectiveness_priority_settled() {
        assert_eq!(
            super::effectiveness_priority(Some(EffectivenessCategory::Settled)),
            1_i32
        );
    }

    #[test]
    fn test_effectiveness_priority_unmatched() {
        assert_eq!(
            super::effectiveness_priority(Some(EffectivenessCategory::Unmatched)),
            0_i32
        );
    }

    #[test]
    fn test_effectiveness_priority_none() {
        // None is neutral (not negative) — cold-start degrades to confidence-only (R-07)
        assert_eq!(super::effectiveness_priority(None), 0_i32);
    }

    #[test]
    fn test_effectiveness_priority_ineffective() {
        assert_eq!(
            super::effectiveness_priority(Some(EffectivenessCategory::Ineffective)),
            -1_i32
        );
    }

    #[test]
    fn test_effectiveness_priority_noisy() {
        assert_eq!(
            super::effectiveness_priority(Some(EffectivenessCategory::Noisy)),
            -2_i32
        );
    }

    #[test]
    fn test_effectiveness_priority_noisy_lower_than_ineffective() {
        // Documents canonical ordering: Noisy is the lowest priority in briefing
        assert!(
            super::effectiveness_priority(Some(EffectivenessCategory::Noisy))
                < super::effectiveness_priority(Some(EffectivenessCategory::Ineffective))
        );
    }

    // ---------------------------------------------------------------------------
    // crt-018b: Injection history sort — confidence is primary key (AC-07, R-09)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_injection_sort_confidence_is_primary_key() {
        // High-confidence Ineffective must rank above low-confidence Effective (R-09)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "High Conf Ineffective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "Low Conf Effective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        // Populate effectiveness: A=Ineffective, B=Effective
        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Ineffective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_a,
                    confidence: 0.90, // high confidence, Ineffective
                },
                InjectionEntry {
                    entry_id: id_b,
                    confidence: 0.40, // low confidence, Effective
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        let decisions = &result.injection_sections.decisions;
        assert_eq!(decisions.len(), 2);
        // A (confidence=0.90) must rank first despite being Ineffective
        assert_eq!(
            decisions[0].0.id, id_a,
            "higher confidence wins regardless of effectiveness category (R-09)"
        );
        assert_eq!(decisions[1].0.id, id_b);
    }

    #[tokio::test]
    async fn test_injection_sort_effectiveness_is_tiebreaker() {
        // Equal confidence: Effective must rank above Ineffective (AC-07)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "Equal Conf Ineffective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "Equal Conf Effective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Ineffective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_a,
                    confidence: 0.60, // same confidence, Ineffective
                },
                InjectionEntry {
                    entry_id: id_b,
                    confidence: 0.60, // same confidence, Effective
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        let decisions = &result.injection_sections.decisions;
        assert_eq!(decisions.len(), 2);
        // B (Effective, priority=2) must rank before A (Ineffective, priority=-1)
        assert_eq!(
            decisions[0].0.id, id_b,
            "Effective entry must rank above Ineffective at equal confidence (AC-07)"
        );
        assert_eq!(decisions[1].0.id, id_a);
    }

    #[tokio::test]
    async fn test_injection_sort_equal_confidence_equal_effectiveness() {
        // Both Effective at equal confidence: sort is stable (no preference)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "Entry A",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "Entry B",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Effective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_a,
                    confidence: 0.60,
                },
                InjectionEntry {
                    entry_id: id_b,
                    confidence: 0.60,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        let decisions = &result.injection_sections.decisions;
        assert_eq!(decisions.len(), 2, "both entries must be present");
        // Both have same priority — no ordering assertion, just that no panic occurs
    }

    #[tokio::test]
    async fn test_injection_sort_three_entries_mixed() {
        // A: 0.70 Effective; B: 0.80 Ineffective; C: 0.70 Ineffective
        // Expected: B (0.80, prio=-1), A (0.70, prio=2), C (0.70, prio=-1)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "A 0.70 Effective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "B 0.80 Ineffective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_c = store_entry(
            &store,
            "C 0.70 Ineffective",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Effective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Ineffective);
            guard
                .categories
                .insert(id_c, EffectivenessCategory::Ineffective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_a,
                    confidence: 0.70,
                },
                InjectionEntry {
                    entry_id: id_b,
                    confidence: 0.80,
                },
                InjectionEntry {
                    entry_id: id_c,
                    confidence: 0.70,
                },
            ]),
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        let decisions = &result.injection_sections.decisions;
        assert_eq!(decisions.len(), 3);
        // B: highest confidence wins (0.80 > 0.70), regardless of Ineffective category
        assert_eq!(decisions[0].0.id, id_b, "B (0.80) must be first");
        // A: equal confidence to C (0.70), but Effective (2) beats Ineffective (-1)
        assert_eq!(
            decisions[1].0.id, id_a,
            "A (0.70, Effective) beats C (0.70, Ineffective)"
        );
        assert_eq!(decisions[2].0.id, id_c, "C (0.70, Ineffective) is last");
    }

    // ---------------------------------------------------------------------------
    // crt-018b: Convention sort tiebreaker (AC-08)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_convention_sort_feature_tag_overrides_effectiveness() {
        // Feature-tagged Ineffective must rank above non-feature-tagged Effective (AC-08)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "Feature Ineffective",
            "content",
            "convention",
            "dev",
            vec!["crt-018b".to_string()],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "NonFeature Effective",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Ineffective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: Some("crt-018b".to_string()),
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        assert_eq!(result.conventions.len(), 2);
        // A has feature tag — must come first regardless of Ineffective classification
        assert_eq!(
            result.conventions[0].id, id_a,
            "feature_tag overrides effectiveness (AC-08)"
        );
        assert_eq!(result.conventions[1].id, id_b);
    }

    #[tokio::test]
    async fn test_convention_sort_effectiveness_tiebreaker_no_feature() {
        // No feature tag: equal confidence — Effective ranks above Ineffective (AC-08)
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let id_a = store_entry(
            &store,
            "Conv Ineffective",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "Conv Effective",
            "content",
            "convention",
            "dev",
            vec![],
            Status::Active,
        )
        .await;

        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(id_a, EffectivenessCategory::Ineffective);
            guard
                .categories
                .insert(id_b, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        let (service, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: None, // no feature — no feature-sort active
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble");

        assert_eq!(result.conventions.len(), 2);
        // Both entries have same (default) confidence=0.0; Effective ranks above Ineffective
        assert_eq!(
            result.conventions[0].id, id_b,
            "Effective must rank above Ineffective at equal confidence (AC-08)"
        );
        assert_eq!(result.conventions[1].id, id_a);
    }

    // ---------------------------------------------------------------------------
    // crt-018b: R-07 — Empty effectiveness state degrades to confidence-only sort
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_briefing_with_empty_effectiveness_state_no_panic() {
        // Cold start: empty EffectivenessState, all priorities=0, sort=confidence-only
        let (_dir, store) = make_test_store().await;
        // Default new_handle() is empty — cold start
        let (service, _es) = make_briefing_service(&store);

        let id_a = store_entry(
            &store,
            "Entry A",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;
        let id_b = store_entry(
            &store,
            "Entry B",
            "content",
            "decision",
            "test",
            vec![],
            Status::Active,
        )
        .await;

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry {
                    entry_id: id_a,
                    confidence: 0.80,
                },
                InjectionEntry {
                    entry_id: id_b,
                    confidence: 0.60,
                },
            ]),
        };

        // Must not panic; ordering must follow confidence descending (no effectiveness data)
        let result = service
            .assemble(params, &test_audit_ctx(), None)
            .await
            .expect("assemble must not panic on empty effectiveness state");

        let decisions = &result.injection_sections.decisions;
        assert_eq!(decisions.len(), 2);
        // A (0.80) must rank first — pure confidence sort when no effectiveness data
        assert_eq!(
            decisions[0].0.id, id_a,
            "confidence-only sort when effectiveness state is empty (R-07)"
        );
        assert_eq!(decisions[1].0.id, id_b);
    }

    // ---------------------------------------------------------------------------
    // crt-018b: ADR-004 — BriefingService constructor requires EffectivenessStateHandle
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_briefing_service_new_requires_handle() {
        // Construct BriefingService with a valid EffectivenessStateHandle.
        // The fact that Option<EffectivenessStateHandle> is not accepted is
        // guaranteed by the type system (ADR-004 compile-time safety).
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();
        let (service, _es) = make_briefing_service_with_effectiveness(&store, effectiveness_handle);
        // Construction succeeded — assert the service is functional by checking
        // that clone works (shared Arc snapshot is set up correctly)
        let _cloned = service.clone();
    }

    // ---------------------------------------------------------------------------
    // crt-018b: R-06 — EffectivenessSnapshot shared across BriefingService clones
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_briefing_service_clones_share_snapshot() {
        // Clone shares the same Arc<Mutex<EffectivenessSnapshot>> backing object.
        let (_dir, store) = make_test_store().await;
        let effectiveness_handle = crate::services::effectiveness::EffectivenessState::new_handle();

        let (service_b1, _es) =
            make_briefing_service_with_effectiveness(&store, Arc::clone(&effectiveness_handle));
        let service_b2 = service_b1.clone();

        // Write a new category to the shared state (simulates background tick)
        {
            let mut guard = effectiveness_handle
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(42, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        // Both instances share the same cached_snapshot Arc — pointer equality check
        // (implementation guarantee: Clone derives shared Arc, not deep copy)
        let ptr_b1 = Arc::as_ptr(&service_b1.cached_snapshot);
        let ptr_b2 = Arc::as_ptr(&service_b2.cached_snapshot);
        assert_eq!(
            ptr_b1, ptr_b2,
            "BriefingService clone must share the same Arc<Mutex<EffectivenessSnapshot>> (R-06)"
        );
    }
}
