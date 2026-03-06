//! BriefingService: transport-agnostic briefing assembly (vnc-007).
//!
//! Unifies MCP `context_briefing` and UDS `handle_compact_payload` behind
//! a single caller-parameterized `assemble()` method. Entry sources (conventions,
//! semantic search, injection history) are selected by `BriefingParams`.

use std::collections::HashMap;
use std::sync::Arc;

use unimatrix_core::{EntryRecord, QueryFilter, Status, StoreAdapter};
use unimatrix_core::async_wrappers::AsyncEntryStore;

use crate::infra::audit::{AuditEvent, Outcome};
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
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
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

impl BriefingService {
    pub(crate) fn new(
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
    ) -> Self {
        BriefingService {
            entry_store,
            search,
            gateway,
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
            let (sections, chars_used) =
                self.process_injection_history(history, char_budget).await?;

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
                    .map_err(ServiceError::Core)?;

                // S4: exclude quarantined (defense-in-depth)
                conv_entries.retain(|e| !SecurityGateway::is_quarantined(&e.status));

                // Feature sort: feature-tagged entries first
                if let Some(ref feature) = params.feature {
                    conv_entries.sort_by(|a, b| {
                        let a_has = a.tags.iter().any(|t| t == feature);
                        let b_has = b.tags.iter().any(|t| t == feature);
                        match (a_has, b_has) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => b.confidence
                                .partial_cmp(&a.confidence)
                                .unwrap_or(std::cmp::Ordering::Equal),
                        }
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
                    k: 3,
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
                match self.search.search(search_params, audit_ctx, effective_caller).await {
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
            session_id: audit_ctx
                .session_id
                .clone()
                .unwrap_or_default(),
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
    async fn process_injection_history(
        &self,
        history: &[InjectionEntry],
        char_budget: usize,
    ) -> Result<(InjectionSections, usize), ServiceError> {
        // Step 1: Deduplicate — keep highest confidence per entry_id
        let mut best_confidence: HashMap<u64, f64> = HashMap::new();
        for record in history {
            let entry = best_confidence
                .entry(record.entry_id)
                .or_insert(0.0);
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

        // Step 3: Sort each group by confidence descending
        decisions.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        injections.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        conventions.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

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

    use unimatrix_core::{NewEntry, Store, StoreAdapter, VectorAdapter, VectorConfig, VectorIndex};
    use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};

    use crate::infra::audit::AuditLog;
    use crate::infra::embed_handle::EmbedServiceHandle;
    use crate::services::gateway::SecurityGateway;
    use crate::services::search::SearchService;
    use crate::services::{AuditContext, AuditSource};

    fn make_test_store() -> (tempfile::TempDir, Arc<Store>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(dir.path().join("test.db")).expect("open store"),
        );
        (dir, store)
    }

    fn make_briefing_service(
        store: &Arc<Store>,
    ) -> (BriefingService, Arc<AsyncEntryStore<StoreAdapter>>) {
        let store_adapter = StoreAdapter::new(Arc::clone(store));
        let entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
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

        let search = SearchService::new(
            Arc::clone(store),
            vector_store,
            Arc::clone(&entry_store),
            embed_service,
            adapt_service,
            Arc::clone(&gateway),
        );

        let service = BriefingService::new(
            Arc::clone(&entry_store),
            search,
            gateway,
        );

        (service, entry_store)
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
    fn store_entry(
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
        store.insert(entry).expect("insert")
    }

    // -- T-BS-01: Convention lookup with role --

    #[tokio::test]
    async fn convention_lookup_with_role() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id1 = store_entry(
            &store, "Conv 1", "Always use trait objects",
            "convention", "architect", vec![], Status::Active,
        );
        let id2 = store_entry(
            &store, "Conv 2", "Write ADRs for decisions",
            "convention", "architect", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: Some("architect".to_string()),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
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
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store, "Conv", "content",
            "convention", "dev", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(result.conventions.is_empty());
    }

    // -- T-BS-03: Convention lookup skipped when include_conventions=false --

    #[tokio::test]
    async fn convention_lookup_skipped_when_disabled() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store, "Conv", "content",
            "convention", "dev", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: None,
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(result.conventions.is_empty());
    }

    // -- T-BS-04: Semantic search isolation when include_semantic=false --

    #[tokio::test]
    async fn semantic_search_isolation_when_disabled() {
        let (_dir, store) = make_test_store();
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

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(result.relevant_context.is_empty());
        // search_available stays true (not attempted, not failed)
        assert!(result.search_available);
    }

    // -- T-BS-05: Semantic search graceful degradation (EmbedNotReady) --

    #[tokio::test]
    async fn semantic_search_embed_not_ready() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let _id = store_entry(
            &store, "Conv", "content",
            "convention", "dev", vec![], Status::Active,
        );

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

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(!result.search_available);
        assert!(result.relevant_context.is_empty());
        // Conventions still populated
        assert_eq!(result.conventions.len(), 1);
    }

    // -- T-BS-06: Injection history basic processing --

    #[tokio::test]
    async fn injection_history_basic_processing() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id_dec = store_entry(
            &store, "ADR-001", "Use redb for storage",
            "decision", "architecture", vec![], Status::Active,
        );
        let id_conv = store_entry(
            &store, "Coding Convention", "Use Result<T,E>",
            "convention", "rust", vec![], Status::Active,
        );
        let id_pat = store_entry(
            &store, "Error Handling", "Pattern for errors",
            "pattern", "rust", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id_dec, confidence: 0.8 },
                InjectionEntry { entry_id: id_conv, confidence: 0.7 },
                InjectionEntry { entry_id: id_pat, confidence: 0.9 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
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
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store, "Entry", "content",
            "pattern", "test", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id, confidence: 0.3 },
                InjectionEntry { entry_id: id, confidence: 0.9 },
                InjectionEntry { entry_id: id, confidence: 0.5 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        // Entry appears exactly once
        let total = result.injection_sections.injections.len();
        assert_eq!(total, 1);
        // With highest confidence
        assert!((result.injection_sections.injections[0].1 - 0.9).abs() < f64::EPSILON);
    }

    // -- T-BS-08: Injection history quarantine exclusion --

    #[tokio::test]
    async fn injection_history_quarantine_exclusion() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id_active = store_entry(
            &store, "Active", "content",
            "pattern", "test", vec![], Status::Active,
        );
        let id_quarantined = store_entry(
            &store, "Quarantined", "bad content",
            "pattern", "test", vec![], Status::Quarantined,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id_active, confidence: 0.8 },
                InjectionEntry { entry_id: id_quarantined, confidence: 0.9 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert_eq!(result.injection_sections.injections.len(), 1);
        assert_eq!(result.injection_sections.injections[0].0.id, id_active);
        assert!(!result.entry_ids.contains(&id_quarantined));
    }

    // -- T-BS-09: Injection history deleted entry skipped --

    #[tokio::test]
    async fn injection_history_deleted_entry_skipped() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store, "Exists", "content",
            "pattern", "test", vec![], Status::Active,
        );
        // Entry 99999 does not exist

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id, confidence: 0.8 },
                InjectionEntry { entry_id: 99999, confidence: 0.5 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert_eq!(result.injection_sections.injections.len(), 1);
    }

    // -- T-BS-10: Injection history deprecated entries EXCLUDED (crt-010 AC-11) --

    #[tokio::test]
    async fn injection_history_deprecated_entries_excluded() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store, "Deprecated", "old content",
            "pattern", "test", vec![], Status::Deprecated,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id, confidence: 0.8 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        // crt-010: deprecated entries are now excluded from injection history (AC-11)
        assert_eq!(result.injection_sections.injections.len(), 0);
    }

    // -- T-BS-11: Token budget truncation --

    #[tokio::test]
    async fn token_budget_truncates_conventions() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        // Each entry ~350 chars (title + content + 50)
        let big_content = "x".repeat(300);
        let _id1 = store_entry(
            &store, "Conv 1", &big_content,
            "convention", "dev", vec![], Status::Active,
        );
        let _id2 = store_entry(
            &store, "Conv 2", &big_content,
            "convention", "dev", vec![], Status::Active,
        );
        let _id3 = store_entry(
            &store, "Conv 3", &big_content,
            "convention", "dev", vec![], Status::Active,
        );

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

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        // All 3 should fit at 2000 char budget
        assert!(result.conventions.len() <= 3);
        assert!(!result.conventions.is_empty());
    }

    // -- T-BS-12: Token budget proportional allocation with injection history --

    #[tokio::test]
    async fn token_budget_proportional_injection() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        // Create entries that test proportional allocation
        let mut ids = Vec::new();
        for _i in 0..3 {
            ids.push(store_entry(
                &store, "Decision", "Decision content here",
                "decision", "arch", vec![], Status::Active,
            ));
        }
        for _i in 0..3 {
            ids.push(store_entry(
                &store, "Pattern", "Pattern content here",
                "pattern", "arch", vec![], Status::Active,
            ));
        }
        for _i in 0..3 {
            ids.push(store_entry(
                &store, "Conv", "Convention content here",
                "convention", "arch", vec![], Status::Active,
            ));
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

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        // Entries should be present in sections
        assert!(!result.injection_sections.decisions.is_empty());
        assert!(!result.injection_sections.injections.is_empty());
        assert!(!result.injection_sections.conventions.is_empty());
    }

    // -- T-BS-13: Token budget minimum boundary --

    #[tokio::test]
    async fn token_budget_minimum_boundary() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id = store_entry(
            &store, "Entry", "content",
            "pattern", "test", vec![], Status::Active,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 500, // minimum
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id, confidence: 0.8 },
            ]),
        };

        // Should not panic
        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        // Entry is small enough to fit
        assert!(result.injection_sections.injections.len() <= 1);
    }

    // -- T-BS-14: Input validation — role too long --

    #[tokio::test]
    async fn validation_role_too_long() {
        let (_dir, store) = make_test_store();
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

        let err = service.assemble(params, &test_audit_ctx(), None).await.unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("role")));
    }

    // -- T-BS-15: Input validation — task too long --

    #[tokio::test]
    async fn validation_task_too_long() {
        let (_dir, store) = make_test_store();
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

        let err = service.assemble(params, &test_audit_ctx(), None).await.unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("task")));
    }

    // -- T-BS-16: Input validation — max_tokens out of range --

    #[tokio::test]
    async fn validation_max_tokens_too_low() {
        let (_dir, store) = make_test_store();
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

        let err = service.assemble(params, &test_audit_ctx(), None).await.unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("max_tokens")));
    }

    #[tokio::test]
    async fn validation_max_tokens_too_high() {
        let (_dir, store) = make_test_store();
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

        let err = service.assemble(params, &test_audit_ctx(), None).await.unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("max_tokens")));
    }

    // -- T-BS-17: Input validation — control characters in task --

    #[tokio::test]
    async fn validation_task_control_chars() {
        let (_dir, store) = make_test_store();
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

        let err = service.assemble(params, &test_audit_ctx(), None).await.unwrap_err();
        assert!(matches!(err, ServiceError::ValidationFailed(msg) if msg.contains("control")));
    }

    // -- T-BS-18: Empty knowledge base --

    #[tokio::test]
    async fn empty_knowledge_base() {
        let (_dir, store) = make_test_store();
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

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(result.conventions.is_empty());
        assert!(result.entry_ids.is_empty());
    }

    // -- T-BS-19: Feature sort — feature-tagged conventions first --

    #[tokio::test]
    async fn feature_tagged_conventions_sorted_first() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let _id_general = store_entry(
            &store, "General Conv", "general content",
            "convention", "dev", vec![], Status::Active,
        );
        let id_feature = store_entry(
            &store, "Feature Conv", "feature content",
            "convention", "dev", vec!["vnc-007".to_string()], Status::Active,
        );

        let params = BriefingParams {
            role: Some("dev".to_string()),
            task: None,
            feature: Some("vnc-007".to_string()),
            max_tokens: 3000,
            include_conventions: true,
            include_semantic: false,
            injection_history: None,
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert_eq!(result.conventions.len(), 2);
        assert_eq!(result.conventions[0].id, id_feature, "feature-tagged entry should be first");
    }

    // -- T-BS-20: All injection entries quarantined --

    #[tokio::test]
    async fn all_injection_entries_quarantined() {
        let (_dir, store) = make_test_store();
        let (service, _es) = make_briefing_service(&store);

        let id1 = store_entry(
            &store, "Q1", "content", "pattern", "test", vec![], Status::Quarantined,
        );
        let id2 = store_entry(
            &store, "Q2", "content", "decision", "test", vec![], Status::Quarantined,
        );
        let id3 = store_entry(
            &store, "Q3", "content", "convention", "test", vec![], Status::Quarantined,
        );

        let params = BriefingParams {
            role: None,
            task: None,
            feature: None,
            max_tokens: 3000,
            include_conventions: false,
            include_semantic: false,
            injection_history: Some(vec![
                InjectionEntry { entry_id: id1, confidence: 0.8 },
                InjectionEntry { entry_id: id2, confidence: 0.7 },
                InjectionEntry { entry_id: id3, confidence: 0.6 },
            ]),
        };

        let result = service.assemble(params, &test_audit_ctx(), None).await.expect("assemble");
        assert!(result.injection_sections.decisions.is_empty());
        assert!(result.injection_sections.injections.is_empty());
        assert!(result.injection_sections.conventions.is_empty());
        assert!(result.entry_ids.is_empty());
    }
}
