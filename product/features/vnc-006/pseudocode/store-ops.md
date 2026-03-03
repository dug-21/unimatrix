# Pseudocode: StoreService (services/store_ops.rs)

## Types

```
struct StoreService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
}

struct InsertResult {
    entry: EntryRecord,
    duplicate_of: Option<u64>,
}

struct CorrectResult {
    corrected_entry: EntryRecord,
    deprecated_original: EntryRecord,
}
```

## Constants

```
const DUPLICATE_THRESHOLD: f64 = 0.92;
const EF_SEARCH: usize = 32;
```

## Constructor

```
fn new(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
) -> Self:
    StoreService { store, vector_index, vector_store, entry_store, embed_service, adapt_service, gateway }
```

## insert()

```
async fn insert(&self, entry: NewEntry, embedding: Option<Vec<f32>>,
    audit_ctx: &AuditContext) -> Result<InsertResult, ServiceError>:

    // Step 1: S1 + S3 validation via gateway
    self.gateway.validate_write(
        &entry.title, &entry.content, &entry.category, &entry.tags, audit_ctx
    )?

    // Step 2: Generate embedding if not pre-computed
    let embedding = match embedding:
        Some(e) => e,
        None =>
            let adapter = self.embed_service.get_adapter().await
                .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
            let title = entry.title.clone()
            let content = entry.content.clone()
            let raw = tokio::task::spawn_blocking({
                let adapter = Arc::clone(&adapter)
                move || adapter.embed_entry(&title, &content)
            }).await
                .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
                .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?

            let adapted = self.adapt_service.adapt_embedding(
                &raw,
                Some(&entry.category),
                Some(&entry.topic),
            )
            unimatrix_embed::l2_normalized(&adapted)

    // Step 3: Near-duplicate detection
    let dup_results = self.vector_store.search(embedding.clone(), 1, EF_SEARCH).await
        .map_err(|e| ServiceError::Core(e))?

    if let Some(top) = dup_results.first():
        if top.similarity >= DUPLICATE_THRESHOLD:
            match self.entry_store.get(top.entry_id).await:
                Ok(existing) =>
                    // Audit duplicate detection
                    self.gateway.emit_audit(AuditEvent {
                        operation: "store_service_duplicate",
                        target_ids: vec![existing.id],
                        detail: format!("near-duplicate at {:.2} similarity", top.similarity),
                        ...from audit_ctx
                    })
                    return Ok(InsertResult {
                        entry: existing,
                        duplicate_of: Some(top.entry_id),
                    })
                Err(_) => { /* entry deleted since search, proceed */ }

    // Step 4: Atomic insert with audit via insert_with_audit
    // Reuse existing UnimatrixServer::insert_with_audit pattern
    // This delegates to the server's insert_with_audit which already handles
    // entry + indexes + VECTOR_MAP + audit in one transaction + HNSW insert after.
    //
    // NOTE: StoreService does not directly call Store::insert_in_txn.
    // Instead, it uses the existing insert_with_audit on UnimatrixServer which
    // already provides atomic entry+audit. The insert_in_txn refactoring
    // will be applied to insert_with_audit internally in a later step.
    //
    // For vnc-006 scope: StoreService holds references to store + vector_index
    // and calls the same atomic pattern that insert_with_audit uses.
    let audit_event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: audit_ctx.session_id.clone().unwrap_or_default(),
        agent_id: audit_ctx.caller_id.clone(),
        operation: "context_store",
        target_ids: vec![],  // filled by insert logic
        outcome: Outcome::Success,
        detail: format!("stored entry: {}", entry.title),
    }

    // Allocate HNSW data_id
    let data_id = self.vector_index.allocate_data_id()
    let embedding_dim = embedding.len() as u16

    let store = Arc::clone(&self.store)
    let audit_log = Arc::clone(&self.gateway.audit)

    let (entry_id, record) = tokio::task::spawn_blocking(move || {
        let txn = store.begin_write()?
        let id = next_entry_id(&txn)?
        let content_hash = compute_content_hash(&entry.title, &entry.content)
        let now = current_timestamp_secs()

        // Build EntryRecord (same as server.rs insert_with_audit)
        let record = EntryRecord { id, ...entry fields, embedding_dim, content_hash, ... }

        // Write ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX,
        // STATUS_INDEX, VECTOR_MAP, OUTCOME_INDEX (if outcome), status counter
        // ... same index writes as server.rs insert_with_audit ...

        // Write audit in same transaction
        let audit_with_target = AuditEvent { target_ids: vec![id], ..audit_event }
        audit_log.write_in_txn(&txn, audit_with_target)?

        txn.commit()?
        Ok((id, record))
    }).await??

    // HNSW insert (after transaction commits)
    if !embedding.is_empty():
        self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)?

    Ok(InsertResult {
        entry: record,
        duplicate_of: None,
    })
```

## correct()

```
async fn correct(&self, original_id: u64, corrected: NewEntry, reason: Option<String>,
    audit_ctx: &AuditContext) -> Result<CorrectResult, ServiceError>:

    // Step 1: S1 + S3 validation on corrected content
    self.gateway.validate_write(
        &corrected.title, &corrected.content, &corrected.category, &corrected.tags, audit_ctx
    )?

    // Step 2: Generate embedding for corrected entry
    let adapter = self.embed_service.get_adapter().await
        .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    let title = corrected.title.clone()
    let content = corrected.content.clone()
    let raw = tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter)
        move || adapter.embed_entry(&title, &content)
    }).await??

    let adapted = self.adapt_service.adapt_embedding(&raw, Some(&corrected.category), Some(&corrected.topic))
    let embedding = unimatrix_embed::l2_normalized(&adapted)

    // Step 3: Atomic correct with audit
    // Reuse existing correct_with_audit pattern from server.rs
    let audit_event = AuditEvent {
        operation: "context_correct",
        agent_id: audit_ctx.caller_id.clone(),
        session_id: audit_ctx.session_id.clone().unwrap_or_default(),
        detail: format!("corrected entry #{}", original_id),
        ...
    }

    let data_id = self.vector_index.allocate_data_id()
    let embedding_dim = embedding.len() as u16
    let store = Arc::clone(&self.store)
    let audit_log = Arc::clone(&self.gateway.audit)

    let (deprecated_original, new_correction) = tokio::task::spawn_blocking(move || {
        let txn = store.begin_write()?

        // 1. Read and validate original
        // 2. Verify not already deprecated/quarantined
        // 3. Deprecate original (update status, superseded_by, STATUS_INDEX)
        // 4. Create new correction entry (like insert: all indexes)
        // 5. Set supersedes link on new entry
        // 6. Write audit
        // 7. Commit atomically

        // ... same logic as server.rs correct_with_audit ...

        Ok((deprecated_original, new_correction))
    }).await??

    // HNSW insert for new correction
    if !embedding.is_empty():
        self.vector_index.insert_hnsw_only(new_correction.id, data_id, &embedding)?

    Ok(CorrectResult {
        corrected_entry: new_correction,
        deprecated_original,
    })
```

## Notes

- StoreService essentially encapsulates the logic from `UnimatrixServer::insert_with_audit` (server.rs lines 186-348) and `UnimatrixServer::correct_with_audit` (server.rs lines 350-520).
- The gateway's validate_write is called BEFORE embedding to fail fast on bad content.
- The duplicate detection uses the same threshold (0.92) and approach as the current tools.rs context_store handler.
- For vnc-006, the store field references allow the service to access `store.begin_write()` directly, matching the existing pattern. The `insert_in_txn` method in unimatrix-store is used to extract the index-writing logic for reuse.
