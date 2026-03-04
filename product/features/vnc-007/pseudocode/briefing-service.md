# Pseudocode: BriefingService (services/briefing.rs)

## Module Structure

```rust
// services/briefing.rs
// Transport-agnostic briefing assembly service.

use std::collections::HashMap;
use std::sync::Arc;

use unimatrix_core::{EntryRecord, QueryFilter, Status, StoreAdapter};
use unimatrix_core::async_wrappers::AsyncEntryStore;

use crate::services::{AuditContext, ServiceError};
use crate::services::gateway::SecurityGateway;
use crate::services::search::SearchService;
use crate::audit::{AuditEvent, Outcome};
```

## Types

```rust
pub(crate) struct BriefingService {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
}

pub(crate) struct BriefingParams {
    pub role: Option<String>,
    pub task: Option<String>,
    pub feature: Option<String>,
    pub max_tokens: usize,
    pub include_conventions: bool,
    pub include_semantic: bool,
    pub injection_history: Option<Vec<InjectionEntry>>,
}

pub(crate) struct BriefingResult {
    pub conventions: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub injection_sections: InjectionSections,
    pub entry_ids: Vec<u64>,
    pub search_available: bool,
}

pub(crate) struct InjectionSections {
    pub decisions: Vec<(EntryRecord, f64)>,
    pub injections: Vec<(EntryRecord, f64)>,
    pub conventions: Vec<(EntryRecord, f64)>,
}

// Default impl for InjectionSections (all empty vecs)
impl Default for InjectionSections { ... }

pub(crate) struct InjectionEntry {
    pub entry_id: u64,
    pub confidence: f64,
}
```

## BriefingService::new

```
fn new(entry_store, search, gateway) -> Self:
    Store refs in struct fields
    Return Self { entry_store, search, gateway }
```

## BriefingService::assemble (core pipeline)

```
async fn assemble(&self, params: BriefingParams, audit_ctx: &AuditContext) -> Result<BriefingResult, ServiceError>:

    // Step 1: S3 input validation
    validate_briefing_inputs(&self.gateway, &params)?
      - if role.is_some(): check length <= 500, no control chars
      - if task.is_some(): check length <= 10000, no control chars (allow newlines)
      - max_tokens must be in [500, 10000] range (reuse validation.rs constants)

    // Step 2: Initialize budget tracker
    let char_budget = params.max_tokens * 4  // ~4 chars per token
    let mut budget_remaining = char_budget

    // Step 3: Initialize result accumulators
    let mut conventions = Vec::new()
    let mut relevant_context = Vec::new()
    let mut injection_sections = InjectionSections::default()
    let mut all_entry_ids = Vec::new()
    let mut search_available = true

    // Step 4: Injection history path (if provided)
    if let Some(ref history) = params.injection_history:
        injection_sections = self.process_injection_history(history, char_budget).await?
        // Collect entry IDs from injection sections
        for (entry, _) in &injection_sections.decisions:
            all_entry_ids.push(entry.id)
        for (entry, _) in &injection_sections.injections:
            all_entry_ids.push(entry.id)
        for (entry, _) in &injection_sections.conventions:
            all_entry_ids.push(entry.id)
        // Deduct injection budget from remaining
        budget_remaining = budget_remaining.saturating_sub(injection_char_usage)

    // Step 5: Convention lookup path
    if params.include_conventions && params.role.is_some():
        // Only do standalone convention lookup if injection didn't already provide conventions
        // OR if injection has no conventions
        let role = params.role.as_ref().unwrap()
        let conv_entries = self.entry_store.query(QueryFilter {
            topic: Some(role.clone()),
            category: Some("convention".to_string()),
            status: Some(Status::Active),
            tags: None,
            time_range: None,
        }).await.map_err(ServiceError::Core)?

        // S4: exclude quarantined (defense-in-depth, Active query should exclude)
        let conv_entries: Vec<_> = conv_entries.into_iter()
            .filter(|e| !SecurityGateway::is_quarantined(&e.status))
            .collect()

        // Feature sort: feature-tagged entries first if feature provided
        if let Some(ref feature) = params.feature:
            sort feature-tagged entries first, then by confidence desc

        // Budget allocation for conventions (linear fill)
        for entry in conv_entries:
            let entry_chars = entry.title.len() + entry.content.len() + 50
            if budget_remaining >= entry_chars:
                conventions.push(entry)
                all_entry_ids.push(entry.id)
                budget_remaining -= entry_chars
            else:
                break

    // Step 6: Semantic search path
    if params.include_semantic && params.task.is_some():
        let task = params.task.as_ref().unwrap()
        match self.search.search(ServiceSearchParams {
            query: task.clone(),
            k: 3,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: params.feature.clone(),
            co_access_anchors: if all_entry_ids.is_empty() { None } else { Some(all_entry_ids.clone()) },
            caller_agent_id: None,
        }, audit_ctx).await:
            Ok(results) =>
                // Budget allocation for relevant_context (linear fill)
                for scored_entry in results.entries:
                    let entry = scored_entry.entry
                    let sim = scored_entry.similarity
                    let entry_chars = entry.title.len() + entry.content.len() + 50
                    if budget_remaining >= entry_chars:
                        all_entry_ids.push(entry.id)
                        relevant_context.push((entry, sim))
                        budget_remaining -= entry_chars
                    else:
                        break
            Err(ServiceError::EmbeddingFailed(_)) =>
                // EmbedNotReady -- graceful degradation
                search_available = false
            Err(e) =>
                // Propagate other errors
                return Err(e)

    // Step 7: Deduplicate entry IDs
    all_entry_ids.sort_unstable()
    all_entry_ids.dedup()

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
    })

    Ok(BriefingResult {
        conventions,
        relevant_context,
        injection_sections,
        entry_ids: all_entry_ids,
        search_available,
    })
```

## process_injection_history (private helper)

```
async fn process_injection_history(
    &self,
    history: &[InjectionEntry],
    char_budget: usize,
) -> Result<InjectionSections, ServiceError>:

    // Step 1: Deduplicate — keep highest confidence per entry_id
    let mut best_confidence: HashMap<u64, f64> = HashMap::new()
    for record in history:
        let entry = best_confidence.entry(record.entry_id).or_insert(0.0)
        if record.confidence > *entry:
            *entry = record.confidence

    // Step 2: Fetch entries, exclude quarantined, partition by category
    let mut decisions = Vec::new()
    let mut injections = Vec::new()
    let mut conventions = Vec::new()

    for (&entry_id, &confidence) in &best_confidence:
        match self.entry_store.get(entry_id).await:
            Ok(entry) =>
                if SecurityGateway::is_quarantined(&entry.status):
                    continue
                match entry.category.as_str():
                    "decision" => decisions.push((entry, confidence))
                    "convention" => conventions.push((entry, confidence))
                    _ => injections.push((entry, confidence))
            Err(_) => continue  // entry deleted, skip

    // Step 3: Sort each group by confidence descending
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal))
    injections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal))
    conventions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal))

    // Step 4: Proportional budget allocation (per ADR-003)
    // Header: 5%, Decisions: 40%, Injections: 30%, Conventions: 20%, Buffer: 5%
    let decision_budget = char_budget * 40 / 100
    let injection_budget = char_budget * 30 / 100
    let convention_budget = char_budget * 20 / 100

    decisions = truncate_to_budget(decisions, decision_budget)
    injections = truncate_to_budget(injections, injection_budget)
    conventions = truncate_to_budget(conventions, convention_budget)

    Ok(InjectionSections { decisions, injections, conventions })
```

## truncate_to_budget (private helper)

```
fn truncate_to_budget(
    entries: Vec<(EntryRecord, f64)>,
    char_budget: usize,
) -> Vec<(EntryRecord, f64)>:
    let mut result = Vec::new()
    let mut used = 0
    for (entry, confidence) in entries:
        let entry_chars = entry.title.len() + entry.content.len() + 50
        if used + entry_chars <= char_budget:
            result.push((entry, confidence))
            used += entry_chars
        else:
            break
    result
```

## validate_briefing_inputs (private helper)

```
fn validate_briefing_inputs(
    gateway: &SecurityGateway,
    params: &BriefingParams,
) -> Result<(), ServiceError>:
    // S3: role validation
    if let Some(ref role) = params.role:
        if role.len() > 500:
            return Err(ServiceError::ValidationFailed("role exceeds 500 characters"))
        check_control_chars(role)?

    // S3: task validation
    if let Some(ref task) = params.task:
        if task.len() > 10_000:
            return Err(ServiceError::ValidationFailed("task exceeds 10000 characters"))
        // Allow \n, \t in task (same as search query)
        for ch in task.chars():
            if ch.is_control() && ch != '\n' && ch != '\t':
                return Err(ServiceError::ValidationFailed("task contains control characters"))

    // S3: max_tokens range
    if params.max_tokens < 500 || params.max_tokens > 10_000:
        return Err(ServiceError::ValidationFailed("max_tokens must be between 500 and 10000"))

    Ok(())
```

## Patterns & Deviations

**Patterns followed:**
- ServiceLayer injection (same as SearchService, StoreService, ConfidenceService)
- SecurityGateway S3/S4/S5 at pipeline points (same as SearchService)
- Fire-and-forget audit emission
- Token budget estimation: `(title.len() + content.len() + 50) / 4`

**Deviations:**
- BriefingService does NOT hold Store, VectorIndex, EmbedServiceHandle directly (delegates to SearchService)
- Injection history processing reuses the same deduplicate-partition-sort logic from uds_listener.rs primary_path, moved into BriefingService
- Budget allocation uses char_budget internally but accepts max_tokens at API boundary (conversion: max_tokens * 4)

## Open Questions

None. All design decisions are resolved in ADRs 001-004.
