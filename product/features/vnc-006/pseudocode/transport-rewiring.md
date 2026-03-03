# Pseudocode: Transport Rewiring

## Overview

This component covers the changes to tools.rs and uds_listener.rs to delegate business logic to the service layer, plus the changes to server.rs to add the ServiceLayer field.

## server.rs Changes

### UnimatrixServer struct

```
pub struct UnimatrixServer {
    // ... existing fields ...
    pub(crate) services: ServiceLayer,  // NEW
}
```

### UnimatrixServer::new()

```
pub fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    adapt_service: Arc<AdaptationService>,
) -> Self:
    // ... existing construction ...

    // NEW: Construct service layer
    let services = ServiceLayer::new(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&vector_store),
        Arc::clone(&entry_store),
        Arc::clone(&embed_service),
        Arc::clone(&adapt_service),
        Arc::clone(&audit),
    )

    UnimatrixServer {
        // ... existing fields ...
        services,  // NEW
        // ...
    }
```

## tools.rs Changes

### context_search (replace inline search pipeline)

```
async fn context_search(&self, params: SearchParams) -> Result<CallToolResult, ErrorData>:
    // 1. Identity (KEEP -- transport-specific)
    let identity = self.resolve_agent(&params.agent_id)?

    // 2. Capability check (KEEP -- transport-specific)
    self.registry.require_capability(&identity.agent_id, Capability::Search)?

    // 3-5. Validation, format, k (KEEP -- transport-specific param parsing)
    validate_search_params(&params)?
    validate_feature(&params.feature)?
    validate_helpful(&params.helpful)?
    let format = parse_format(&params.format)?
    let k = validated_k(params.k)?

    // NEW: Build AuditContext
    let audit_ctx = AuditContext {
        source: AuditSource::Mcp {
            agent_id: identity.agent_id.clone(),
            trust_level: identity.trust_level,
        },
        caller_id: identity.agent_id.clone(),
        session_id: None,
        feature_cycle: None,
    }

    // NEW: Build ServiceSearchParams
    let service_params = ServiceSearchParams {
        query: params.query.clone(),
        k,
        filters: if params.topic.is_some() || params.category.is_some() || params.tags.is_some() {
            Some(QueryFilter {
                topic: params.topic.clone(),
                category: params.category.clone(),
                tags: params.tags.clone(),
                status: Some(Status::Active),
                time_range: None,
            })
        } else {
            None
        },
        similarity_floor: None,
        confidence_floor: None,
        feature_tag: params.feature.clone(),
        co_access_anchors: None,
        caller_agent_id: Some(identity.agent_id.clone()),
    }

    // NEW: Delegate to SearchService
    let search_results = self.services.search.search(service_params, &audit_ctx).await
        .map_err(rmcp::ErrorData::from)?

    // KEEP: Format response (transport-specific)
    let results_with_scores: Vec<(EntryRecord, f64)> = search_results.entries
        .iter()
        .map(|se| (se.entry.clone(), se.similarity))
        .collect()
    let result = format_search_results(&results_with_scores, format)

    // KEEP: Usage recording (transport-specific)
    let target_ids: Vec<u64> = search_results.entries.iter().map(|se| se.entry.id).collect()
    self.record_usage_for_entries(
        &identity.agent_id,
        identity.trust_level,
        &target_ids,
        params.helpful,
        params.feature.as_deref(),
    ).await

    Ok(result)
```

### context_store (replace inline write + confidence)

```
async fn context_store(&self, params: StoreParams) -> Result<CallToolResult, ErrorData>:
    // 1-5a. Identity, capability, validation, format, category (KEEP)
    // ...

    // 5b. Content scanning -- REMOVED (moved to gateway via StoreService)
    // The gateway.validate_write inside StoreService handles S1 scanning.

    // 6-7b. Embedding -- REMOVED (moved to StoreService)

    // 8. Near-duplicate detection -- REMOVED (moved to StoreService)

    // NEW: Build AuditContext
    let audit_ctx = AuditContext {
        source: AuditSource::Mcp {
            agent_id: identity.agent_id.clone(),
            trust_level: identity.trust_level,
        },
        caller_id: identity.agent_id.clone(),
        session_id: None,
        feature_cycle: None,
    }

    // Build NewEntry (KEEP -- transport-specific param assembly)
    let new_entry = NewEntry { ... }

    // NEW: Delegate to StoreService
    let insert_result = self.services.store_ops.insert(new_entry, None, &audit_ctx).await
        .map_err(rmcp::ErrorData::from)?

    // Handle duplicate (KEEP -- transport-specific formatting)
    if let Some(dup_id) = insert_result.duplicate_of:
        return Ok(format_duplicate_found(&insert_result.entry, ...))

    // NEW: Confidence recompute via service
    self.services.confidence.recompute(&[insert_result.entry.id])

    // Format response (KEEP)
    Ok(format_store_success(&insert_result.entry, format))
```

### context_correct (replace inline correct + confidence)

```
async fn context_correct(&self, params: CorrectParams) -> Result<CallToolResult, ErrorData>:
    // 1-6. Identity, capability, validation, format, scanning -- simplified
    // Scanning REMOVED (moved to gateway via StoreService)

    // NEW: Build AuditContext
    let audit_ctx = AuditContext { ... }

    // Build correction NewEntry (KEEP)
    let corrected_entry = NewEntry { ... }

    // NEW: Delegate to StoreService
    let correct_result = self.services.store_ops.correct(
        original_id, corrected_entry, reason, &audit_ctx
    ).await.map_err(rmcp::ErrorData::from)?

    // NEW: Confidence recompute for both entries
    self.services.confidence.recompute(&[
        correct_result.corrected_entry.id,
        correct_result.deprecated_original.id,
    ])

    // Format response (KEEP)
    Ok(format_correct_success(...))
```

### Confidence block replacements in tools.rs

Replace each inline `spawn_blocking { compute_confidence + update_confidence }` block with:

```
self.services.confidence.recompute(&[entry_id])
```

Specific locations:
- context_store: line 682-701 -> `self.services.confidence.recompute(&[entry_id])`
- context_correct: lines 929-940 -> `self.services.confidence.recompute(&[new_id, deprecated_id])`
- context_deprecate: line 1028 -> `self.services.confidence.recompute(&[entry_id])`

## uds_listener.rs Changes

### handle_context_search (replace inline search pipeline)

```
async fn handle_context_search(
    query: String,
    session_id: Option<String>,
    k: Option<u32>,
    services: &ServiceLayer,  // NEW param replaces individual store/embed/vector refs
    session_registry: &SessionRegistry,
    store: &Arc<Store>,  // still needed for injection log + co-access
) -> HookResponse:

    let k = k.map(|v| v as usize).unwrap_or(INJECTION_K)

    // NEW: Build AuditContext
    let audit_ctx = AuditContext {
        source: AuditSource::Uds {
            uid: 0,  // filled from peer creds at call site
            pid: None,
            session_id: session_id.clone().unwrap_or_default(),
        },
        caller_id: "uds".to_string(),
        session_id: session_id.clone(),
        feature_cycle: None,
    }

    // NEW: Build ServiceSearchParams
    let service_params = ServiceSearchParams {
        query: query.clone(),
        k,
        filters: None,  // UDS doesn't pass metadata filters
        similarity_floor: Some(SIMILARITY_FLOOR),
        confidence_floor: Some(CONFIDENCE_FLOOR),
        feature_tag: None,
        co_access_anchors: None,
        caller_agent_id: None,
    }

    // NEW: Delegate to SearchService
    let search_results = match services.search.search(service_params, &audit_ctx).await:
        Ok(results) => results,
        Err(e) =>
            tracing::warn!("search service error: {e}")
            return HookResponse::Entries { items: vec![], total_tokens: 0 }

    // KEEP: Injection tracking, injection log, co-access (transport-specific)
    // Convert SearchResults to the format expected by remaining UDS logic
    let filtered: Vec<(EntryRecord, f64)> = search_results.entries
        .iter()
        .map(|se| (se.entry.clone(), se.similarity))
        .collect()

    // ... rest of injection tracking, injection log, co-access, formatting ...
    // These remain UDS-specific and are NOT moved to the service layer
```

## lib.rs Changes

```
// Add to lib.rs module declarations:
pub(crate) mod services;
```

Wait -- lib.rs currently uses `pub mod` for everything. Since services is pub(crate), we need to check if that's compatible with the existing test structure. If tests are in separate files that import from the crate, we may need `pub mod services` or keep it `pub(crate)` and adjust test imports.

Looking at existing lib.rs: all modules are `pub mod`. For consistency and to allow integration tests to access service types if needed, use `pub mod services` but keep all types inside as `pub(crate)`.

```
pub mod services;  // types inside are pub(crate)
```

## Notes

- Transport-specific concerns that stay in the transport: identity resolution, capability checks, format parsing, usage recording, injection logging, co-access recording, response formatting.
- Service concerns that move: embedding, search pipeline, content scanning, input validation, audit emission, confidence recompute, atomic write+audit.
- The UDS path's handle_context_search function signature changes to accept `&ServiceLayer` instead of individual component references.
- tools.rs context_search drops ~150 lines (steps 6-10 replaced by service call).
- tools.rs context_store drops ~100 lines (steps 6-11 replaced by service call).
- tools.rs context_correct drops ~80 lines (scanning + write logic replaced).
