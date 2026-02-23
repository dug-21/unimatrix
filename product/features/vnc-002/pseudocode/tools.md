# Pseudocode: tools (C5)

## File: `crates/unimatrix-server/src/tools.rs` (REWRITTEN)

### Updated Param Structs

All 4 param structs get a `format` field:

```
struct SearchParams:
    // ... existing fields ...
    pub format: Option<String>   // NEW

struct LookupParams:
    // ... existing fields ...
    pub format: Option<String>   // NEW

struct StoreParams:
    // ... existing fields ...
    pub format: Option<String>   // NEW

struct GetParams:
    // ... existing fields ...
    pub format: Option<String>   // NEW
```

### Tool Implementations

All tools change from sync `fn` to async `async fn` (rmcp supports this).

```
const EF_SEARCH: usize = 32  // HNSW search expansion factor

async fn context_search(&self, params: SearchParams) -> Result<CallToolResult, ErrorData>:
    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check
    self.registry.require_capability(&identity.agent_id, Capability::Search)?

    // 3. Validation
    validate_search_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Parse k
    let k = validated_k(params.k)?

    // 6. Get embedding adapter
    let adapter = self.embed_service.get_adapter().await?

    // 7. Embed query
    let embedding = tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter)
        let query = params.query.clone()
        move || adapter.embed_entry("", &query)
    }).await??

    // 8. Search
    let search_results = if params.topic.is_some() || params.category.is_some() || params.tags.is_some():
        // Build QueryFilter for metadata pre-filtering
        let filter = QueryFilter {
            topic: params.topic.clone(),
            category: params.category.clone(),
            tags: params.tags.clone(),
            status: Some(Status::Active),
            time_range: None,
        }
        let entries = self.entry_store.query(filter).await?
        let allowed_ids: Vec<u64> = entries.iter().map(|e| e.id).collect()
        if allowed_ids.is_empty():
            vec![]
        else:
            self.vector_store.search_filtered(embedding, k, EF_SEARCH, allowed_ids).await?
    else:
        self.vector_store.search(embedding, k, EF_SEARCH).await?

    // 9. Fetch full entries for results
    let mut results_with_scores: Vec<(EntryRecord, f32)> = Vec::new()
    for sr in search_results:
        match self.entry_store.get(sr.entry_id).await:
            Ok(entry) => results_with_scores.push((entry, sr.similarity))
            Err(_) => continue  // silently skip deleted entries (FR-01g)

    // 10. Format response
    let result = format_search_results(&results_with_scores, format)

    // 11. Audit (standalone)
    let target_ids = results_with_scores.iter().map(|(e, _)| e.id).collect()
    let _ = self.audit.log_event(AuditEvent {
        event_id: 0, timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_search".to_string(),
        target_ids,
        outcome: Outcome::Success,
        detail: format!("returned {} results", results_with_scores.len()),
    })

    Ok(result)


async fn context_lookup(&self, params: LookupParams) -> Result<CallToolResult, ErrorData>:
    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check
    self.registry.require_capability(&identity.agent_id, Capability::Read)?

    // 3. Validation
    validate_lookup_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Parse limit
    let limit = validated_limit(params.limit)?

    // 6. Branch: ID-based vs filter-based
    let result = if let Some(id) = params.id:
        let id = validated_id(id)?
        let entry = self.entry_store.get(id).await?
        format_single_entry(&entry, format)
    else:
        // Build filter
        let status = match &params.status:
            Some(s) => Some(parse_status(s)?)
            None => Some(Status::Active)  // default to Active (FR-02e)

        let filter = QueryFilter {
            topic: params.topic.clone(),
            category: params.category.clone(),
            tags: params.tags.clone(),
            status,
            time_range: None,
        }
        let mut entries = self.entry_store.query(filter).await?
        entries.truncate(limit)
        format_lookup_results(&entries, format)

    // 7. Audit (standalone)
    let _ = self.audit.log_event(AuditEvent {
        event_id: 0, timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_lookup".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: "lookup completed".to_string(),
    })

    Ok(result)


async fn context_store(&self, params: StoreParams) -> Result<CallToolResult, ErrorData>:
    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (Write required)
    self.registry.require_capability(&identity.agent_id, Capability::Write)?

    // 3. Validation
    validate_store_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Category validation
    self.categories.validate(&params.category)?

    // 6. Content scanning
    if let Err(scan_result) = ContentScanner::global().scan(&params.content):
        return Err(ServerError::ContentScanRejected {
            category: scan_result.category.to_string(),
            description: scan_result.description.to_string(),
        }.into())
    if let Some(title) = &params.title:
        if let Err(scan_result) = ContentScanner::global().scan_title(title):
            return Err(ServerError::ContentScanRejected {
                category: scan_result.category.to_string(),
                description: scan_result.description.to_string(),
            }.into())

    // 7. Embed title+content
    let title = params.title.unwrap_or_else(|| format!("{}: {}", params.topic, params.category))
    let adapter = self.embed_service.get_adapter().await?
    let embedding = tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter)
        let t = title.clone()
        let c = params.content.clone()
        move || adapter.embed_entry(&t, &c)
    }).await??

    // 8. Near-duplicate detection
    let dup_results = self.vector_store.search(embedding.clone(), 1, EF_SEARCH).await?
    if let Some(top) = dup_results.first():
        if top.similarity >= 0.92:
            let existing = self.entry_store.get(top.entry_id).await?
            // Audit duplicate detection
            let _ = self.audit.log_event(AuditEvent {
                event_id: 0, timestamp: 0,
                session_id: String::new(),
                agent_id: identity.agent_id,
                operation: "context_store".to_string(),
                target_ids: vec![existing.id],
                outcome: Outcome::Success,
                detail: format!("near-duplicate detected: entry #{} at {:.2} similarity", existing.id, top.similarity),
            })
            return Ok(format_duplicate_found(&existing, top.similarity, format))

    // 9. Build NewEntry
    let new_entry = NewEntry {
        title: title.clone(),
        content: params.content,
        topic: params.topic,
        category: params.category,
        tags: params.tags.unwrap_or_default(),
        source: params.source.unwrap_or_default(),
        status: Status::Active,
        created_by: identity.agent_id.clone(),
        feature_cycle: String::new(),
        trust_source: "agent".to_string(),
    }

    // 10. Combined transaction: insert + audit
    let audit_event = AuditEvent {
        event_id: 0, timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_store".to_string(),
        target_ids: vec![],  // will be filled by insert_with_audit
        outcome: Outcome::Success,
        detail: format!("stored entry: {}", title),
    }
    let (entry_id, record) = self.insert_with_audit(new_entry, embedding, audit_event).await?

    // 11. Format response
    Ok(format_store_success(&record, format))


async fn context_get(&self, params: GetParams) -> Result<CallToolResult, ErrorData>:
    // 1. Identity
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check
    self.registry.require_capability(&identity.agent_id, Capability::Read)?

    // 3. Validation
    validate_get_params(&params)?

    // 4. Parse format
    let format = parse_format(&params.format)?

    // 5. Get entry
    let id = validated_id(params.id)?
    let entry = self.entry_store.get(id).await?

    // 6. Format response
    let result = format_single_entry(&entry, format)

    // 7. Audit (standalone)
    let _ = self.audit.log_event(AuditEvent {
        event_id: 0, timestamp: 0,
        session_id: String::new(),
        agent_id: identity.agent_id,
        operation: "context_get".to_string(),
        target_ids: vec![id],
        outcome: Outcome::Success,
        detail: format!("retrieved entry #{id}"),
    })

    Ok(result)
```

### Key Constraints
- Tools are async (rmcp tool_router supports async tool methods)
- Execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit
- Capability before validation (cheap before expensive)
- EmbedServiceHandle.get_adapter() returns Loading/Failed errors
- Near-duplicate check at 0.92 similarity threshold
- Combined transaction only for context_store inserts
- Read tools use standalone audit (existing log_event)
- Deleted entries silently skipped in search results
- Default status filter for lookup is Active
- context_get returns full content in all formats (FR-11h)
