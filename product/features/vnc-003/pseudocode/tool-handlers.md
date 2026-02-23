# Pseudocode: C1 Tool Handler Implementations

## File: `crates/unimatrix-server/src/tools.rs`

### New Param Structs (4)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CorrectParams {
    pub original_id: i64,
    pub content: String,
    pub reason: Option<String>,
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub title: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeprecateParams {
    pub id: i64,
    pub reason: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BriefingParams {
    pub role: String,
    pub task: String,
    pub feature: Option<String>,
    pub max_tokens: Option<i64>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}
```

### New Imports Required

```rust
use crate::response::{
    format_correct_success, format_deprecate_success,
    format_status_report, format_briefing,
    StatusReport, Briefing,
};
use crate::validation::{
    validate_correct_params, validate_deprecate_params,
    validate_status_params, validate_briefing_params,
    validated_max_tokens,
};
use unimatrix_store::{
    ENTRIES, CATEGORY_INDEX, TOPIC_INDEX, COUNTERS, STATUS_INDEX,
    deserialize_entry,
};
```

### New Tool: `context_correct`

```
#[tool(
    name = "context_correct",
    description = "Correct an existing knowledge entry. Deprecates the original and creates a new corrected entry with a chain link. Use when an entry contains wrong information."
)]
async fn context_correct(
    &self,
    Parameters(params): Parameters<CorrectParams>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (Write required)
    self.registry.require_capability(&identity.agent_id, Capability::Write)?

    // 3. Validation
    validate_correct_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Validate original_id
    let original_id = validated_id(params.original_id)?

    // 6. Get original entry
    let original = self.entry_store.get(original_id).await?

    // 7. Verify original is not deprecated
    if original.status == Status::Deprecated:
        return Err(InvalidInput { field: "original_id", reason: "cannot correct a deprecated entry" })

    // 8. Category validation: only if explicit new category provided
    if let Some(category) = &params.category:
        self.categories.validate(category)?
    // else: inherited category from original, skip validation (AC-05)

    // 9. Content scanning on new content
    ContentScanner::global().scan(&params.content)?
    if let Some(title) = &params.title:
        ContentScanner::global().scan_title(title)?

    // 10. Get embedding adapter (fails with EmbedNotReady if not ready)
    let adapter = self.embed_service.get_adapter().await?

    // 11. Build title for embedding
    let title = params.title.unwrap_or_else(|| original.title.clone())

    // 12. Embed title+content
    let embedding = spawn_blocking({
        let adapter = Arc::clone(&adapter)
        let t = title.clone()
        let c = params.content.clone()
        move || adapter.embed_entry(&t, &c)
    }).await??

    // 13. Build NewEntry with inheritance
    let new_entry = NewEntry {
        title: title,
        content: params.content,
        topic: params.topic.unwrap_or_else(|| original.topic.clone()),
        category: params.category.unwrap_or_else(|| original.category.clone()),
        tags: params.tags.unwrap_or_else(|| original.tags.clone()),
        source: original.source.clone(),
        status: Status::Active,
        created_by: identity.agent_id.clone(),
        feature_cycle: original.feature_cycle.clone(),
        trust_source: "agent".to_string(),
    }

    // 14. Combined transaction
    let audit_event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_correct".to_string(),
        target_ids: vec![],  // filled by correct_with_audit
        outcome: Outcome::Success,
        detail: format!("corrected entry #{original_id}: {}", params.reason.as_deref().unwrap_or("no reason")),
    }
    let (deprecated_original, new_correction) = self.correct_with_audit(
        original_id, new_entry, embedding, params.reason, audit_event
    ).await?

    // 15. Format response
    Ok(format_correct_success(&deprecated_original, &new_correction, format))
```

### New Tool: `context_deprecate`

```
#[tool(
    name = "context_deprecate",
    description = "Deprecate a knowledge entry. The entry remains accessible but is excluded from default lookups. Use when knowledge is outdated or no longer relevant."
)]
async fn context_deprecate(
    &self,
    Parameters(params): Parameters<DeprecateParams>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (Write required)
    self.registry.require_capability(&identity.agent_id, Capability::Write)?

    // 3. Validation
    validate_deprecate_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Validate ID
    let entry_id = validated_id(params.id)?

    // 6. Get entry (verify exists)
    let entry = self.entry_store.get(entry_id).await?

    // 7. Idempotency: if already deprecated, return success immediately
    if entry.status == Status::Deprecated:
        return Ok(format_deprecate_success(&entry, params.reason.as_deref(), format))

    // 8. Deprecate with audit
    let audit_event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_deprecate".to_string(),
        target_ids: vec![],  // filled by deprecate_with_audit
        outcome: Outcome::Success,
        detail: String::new(),  // filled by deprecate_with_audit
    }
    let deprecated = self.deprecate_with_audit(
        entry_id, params.reason.clone(), audit_event
    ).await?

    // 9. Format response
    Ok(format_deprecate_success(&deprecated, params.reason.as_deref(), format))
```

### New Tool: `context_status`

```
#[tool(
    name = "context_status",
    description = "Get the health status of the knowledge base. Shows entry counts, category/topic distributions, correction chains, and security metrics. Requires Admin capability."
)]
async fn context_status(
    &self,
    Parameters(params): Parameters<StatusParams>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (Admin required)
    self.registry.require_capability(&identity.agent_id, Capability::Admin)?

    // 3. Validation
    validate_status_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Build report in a single read transaction (consistent snapshot)
    let store = Arc::clone(&self.store)
    let topic_filter = params.topic.clone()
    let category_filter = params.category.clone()

    let report = spawn_blocking(move || {
        let read_txn = store.begin_read()?

        // 5a. Read status counters
        let counters = read_txn.open_table(COUNTERS)?
        let total_active = counters.get("total_active")?.map(|g| g.value()).unwrap_or(0)
        let total_deprecated = counters.get("total_deprecated")?.map(|g| g.value()).unwrap_or(0)
        let total_proposed = counters.get("total_proposed")?.map(|g| g.value()).unwrap_or(0)

        // 5b. Category distribution from CATEGORY_INDEX
        let cat_table = read_txn.open_table(CATEGORY_INDEX)?
        let mut category_distribution: BTreeMap<String, u64> = BTreeMap::new()
        if let Some(filter_cat) = &category_filter:
            // Count only that category's entries
            let range = cat_table.range((filter_cat.as_str(), 0u64)..=(filter_cat.as_str(), u64::MAX))?
            let count = range.count() as u64
            if count > 0:
                category_distribution.insert(filter_cat.clone(), count)
        else:
            // Scan all categories
            for item in cat_table.iter()?:
                let (key, _) = item?
                let (cat_str, _id) = key.value()
                *category_distribution.entry(cat_str.to_string()).or_insert(0) += 1

        // 5c. Topic distribution from TOPIC_INDEX
        let topic_table = read_txn.open_table(TOPIC_INDEX)?
        let mut topic_distribution: BTreeMap<String, u64> = BTreeMap::new()
        if let Some(filter_topic) = &topic_filter:
            let range = topic_table.range((filter_topic.as_str(), 0u64)..=(filter_topic.as_str(), u64::MAX))?
            let count = range.count() as u64
            if count > 0:
                topic_distribution.insert(filter_topic.clone(), count)
        else:
            for item in topic_table.iter()?:
                let (key, _) = item?
                let (topic_str, _id) = key.value()
                *topic_distribution.entry(topic_str.to_string()).or_insert(0) += 1

        // 5d. Correction chain metrics + security metrics from ENTRIES scan
        let entries_table = read_txn.open_table(ENTRIES)?
        let mut entries_with_supersedes = 0u64
        let mut entries_with_superseded_by = 0u64
        let mut total_correction_count = 0u64
        let mut trust_source_dist: BTreeMap<String, u64> = BTreeMap::new()
        let mut entries_without_attribution = 0u64

        for item in entries_table.iter()?:
            let (_key, value) = item?
            let record = deserialize_entry(value.value())?
            if record.supersedes.is_some():
                entries_with_supersedes += 1
            if record.superseded_by.is_some():
                entries_with_superseded_by += 1
            total_correction_count += record.correction_count as u64
            let ts = if record.trust_source.is_empty() { "(none)" } else { &record.trust_source }
            *trust_source_dist.entry(ts.to_string()).or_insert(0) += 1
            if record.created_by.is_empty():
                entries_without_attribution += 1

        // 5e. Build StatusReport
        let report = StatusReport {
            total_active,
            total_deprecated,
            total_proposed,
            category_distribution: category_distribution.into_iter().collect(),
            topic_distribution: topic_distribution.into_iter().collect(),
            entries_with_supersedes,
            entries_with_superseded_by,
            total_correction_count,
            trust_source_distribution: trust_source_dist.into_iter().collect(),
            entries_without_attribution,
        }

        Ok::<StatusReport, ServerError>(report)
    }).await??

    // 6. Audit (standalone, best-effort)
    let _ = self.audit.log_event(AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_status".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: "status report generated".to_string(),
    })

    // 7. Format response
    Ok(format_status_report(&report, format))
```

### New Tool: `context_briefing`

```
#[tool(
    name = "context_briefing",
    description = "Get an orientation briefing for a role and task. Includes role conventions, duties, and task-relevant context from the knowledge base. Use at the start of any task."
)]
async fn context_briefing(
    &self,
    Parameters(params): Parameters<BriefingParams>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (Read required)
    self.registry.require_capability(&identity.agent_id, Capability::Read)?

    // 3. Validation
    validate_briefing_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Validate max_tokens
    let max_tokens = validated_max_tokens(params.max_tokens)?
    let char_budget = max_tokens * 4  // ~4 chars per token

    // 6. Lookup conventions: topic=role, category="convention", status=Active
    let conventions = self.entry_store.query(QueryFilter {
        topic: Some(params.role.clone()),
        category: Some("convention".to_string()),
        status: Some(Status::Active),
        tags: None,
        time_range: None,
    }).await?

    // 7. Lookup duties: topic=role, category="duties", status=Active
    let duties = self.entry_store.query(QueryFilter {
        topic: Some(params.role.clone()),
        category: Some("duties".to_string()),
        status: Some(Status::Active),
        tags: None,
        time_range: None,
    }).await?

    // 8. Semantic search (if embed ready)
    let (relevant_context, search_available) = match self.embed_service.get_adapter().await {
        Ok(adapter) => {
            // Embed task description
            let task = params.task.clone()
            let embedding = spawn_blocking({
                let adapter = Arc::clone(&adapter)
                move || adapter.embed_entry("", &task)
            }).await??

            // Search with k=3
            let search_results = self.vector_store
                .search(embedding, 3, EF_SEARCH).await?

            // Fetch full entries
            let mut results = Vec::new()
            for sr in &search_results:
                if let Ok(entry) = self.entry_store.get(sr.entry_id).await:
                    results.push((entry, sr.similarity))

            // Feature boost: if feature param provided, boost entries tagged with it
            if let Some(feature) = &params.feature:
                results.sort_by(|a, b| {
                    let a_has_feature = a.0.tags.iter().any(|t| t == feature)
                    let b_has_feature = b.0.tags.iter().any(|t| t == feature)
                    match (a_has_feature, b_has_feature):
                        (true, false) => Ordering::Less   // a first
                        (false, true) => Ordering::Greater // b first
                        _ => b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal)  // by similarity
                })

            (results, true)
        }
        Err(_) => {
            // Embed not ready -- graceful degradation (AC-28)
            (vec![], false)
        }
    }

    // 9. Apply token budget
    // Priority order: conventions > duties > relevant_context
    let mut used_chars = 0usize
    let mut budget_conventions = Vec::new()
    for entry in &conventions:
        let entry_chars = entry.title.len() + entry.content.len() + 50  // overhead
        if used_chars + entry_chars <= char_budget:
            budget_conventions.push(entry.clone())
            used_chars += entry_chars

    let mut budget_duties = Vec::new()
    for entry in &duties:
        let entry_chars = entry.title.len() + entry.content.len() + 50
        if used_chars + entry_chars <= char_budget:
            budget_duties.push(entry.clone())
            used_chars += entry_chars

    let mut budget_context = Vec::new()
    for (entry, score) in &relevant_context:
        let entry_chars = entry.title.len() + entry.content.len() + 50
        if used_chars + entry_chars <= char_budget:
            budget_context.push((entry.clone(), *score))
            used_chars += entry_chars

    // 10. Build briefing
    let briefing = Briefing {
        role: params.role.clone(),
        task: params.task.clone(),
        conventions: budget_conventions,
        duties: budget_duties,
        relevant_context: budget_context,
        search_available,
    }

    // 11. Audit (standalone, best-effort)
    let _ = self.audit.log_event(AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_briefing".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: format!("briefing for role={}, task={}", params.role, params.task),
    })

    // 12. Format response
    Ok(format_briefing(&briefing, format))
```
