# Pseudocode: audit-optimization (C6)

## File: `crates/unimatrix-server/src/audit.rs` (EXTENDED)

### New Method on AuditLog

```
impl AuditLog:
    // Existing log_event() unchanged

    fn write_in_txn(&self, txn: &WriteTransaction, event: AuditEvent) -> Result<u64, ServerError>:
        // Get and increment the audit ID counter in the CALLER's transaction
        let mut counters = txn.open_table(COUNTERS)
            .map_err(|e| ServerError::Audit(e.to_string()))?
        let current_id = match counters.get("next_audit_id"):
            Some(guard) => guard.value()
            None => 1  // first event ever
        counters.insert("next_audit_id", current_id + 1)?

        // Build final event with assigned ID and timestamp
        let final_event = AuditEvent {
            event_id: current_id,
            timestamp: current_unix_seconds(),
            ..event
        }

        // Serialize and insert into AUDIT_LOG table (still in caller's txn)
        let mut audit_table = txn.open_table(AUDIT_LOG)?
        let bytes = serialize_audit_event(&final_event)?
        audit_table.insert(current_id, bytes.as_slice())?

        // DO NOT commit -- caller owns the transaction
        Ok(current_id)
```

## File: `crates/unimatrix-server/src/server.rs` (EXTENDED)

### New Fields on UnimatrixServer

```
struct UnimatrixServer:
    // ... existing 7 fields ...
    pub(crate) categories: Arc<CategoryAllowlist>   // NEW
    pub(crate) store: Arc<Store>                    // NEW: raw store for combined txn
```

### Updated Constructor

```
fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>,   // NEW
    store: Arc<Store>,                    // NEW
) -> Self:
    // ... build server_info as before ...
    UnimatrixServer {
        entry_store, vector_store, embed_service, registry, audit,
        categories,   // NEW
        store,        // NEW
        tool_router: Self::tool_router(),
        server_info,
    }
```

### New Method: insert_with_audit

```
impl UnimatrixServer:
    async fn insert_with_audit(
        &self,
        entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(u64, EntryRecord), ServerError>:
        let store = Arc::clone(&self.store)
        let audit_log = Arc::clone(&self.audit)

        // Step 1: Combined write transaction (spawn_blocking for redb)
        let (entry_id, record) = tokio::task::spawn_blocking(move || -> Result<(u64, EntryRecord), ServerError> {
            // This is essentially Store::insert() logic + audit write + vector mapping
            // all in a single transaction

            let txn = store.begin_write().map_err(|e| ServerError::Core(CoreError::Store(e.into())))?

            // Generate entry ID
            let id = {
                let mut counters = txn.open_table(COUNTERS)?
                let current = match counters.get("next_entry_id"):
                    Some(g) => g.value()
                    None => 1
                counters.insert("next_entry_id", current + 1)?
                current
            }

            // Compute content hash
            let content_hash = compute_content_hash(&entry.title, &entry.content)

            // Build EntryRecord
            let now = current_unix_seconds()
            let record = EntryRecord {
                id, title: entry.title, content: entry.content,
                topic: entry.topic, category: entry.category,
                tags: entry.tags, source: entry.source,
                status: entry.status, confidence: 0.0,
                created_at: now, updated_at: now,
                last_accessed_at: 0, access_count: 0,
                supersedes: None, superseded_by: None,
                correction_count: 0, embedding_dim: 0,
                created_by: entry.created_by.clone(),
                modified_by: entry.created_by,
                content_hash, previous_hash: String::new(),
                version: 1,
                feature_cycle: entry.feature_cycle,
                trust_source: entry.trust_source,
            }

            // Write ENTRIES
            let bytes = serialize_entry(&record)?
            { let mut t = txn.open_table(ENTRIES)?; t.insert(id, bytes.as_slice())?; }

            // Write all 5 secondary indexes
            { let mut t = txn.open_table(TOPIC_INDEX)?; t.insert((record.topic.as_str(), id), ())?; }
            { let mut t = txn.open_table(CATEGORY_INDEX)?; t.insert((record.category.as_str(), id), ())?; }
            { let mut t = txn.open_multimap_table(TAG_INDEX)?;
              for tag in &record.tags { t.insert(tag.as_str(), id)?; } }
            { let mut t = txn.open_table(TIME_INDEX)?; t.insert((record.created_at, id), ())?; }
            { let mut t = txn.open_table(STATUS_INDEX)?; t.insert((record.status as u8, id), ())?; }

            // Increment status counter
            increment_counter(&txn, status_counter_key(record.status), 1)?

            // Write audit event in same transaction
            let audit_event_with_target = AuditEvent {
                target_ids: vec![id],
                ..audit_event
            }
            audit_log.write_in_txn(&txn, audit_event_with_target)?

            // Commit everything atomically
            txn.commit()?
            Ok((id, record))
        }).await.map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??

        // Step 2: Insert embedding into HNSW index (separate from redb)
        self.vector_store.insert(entry_id, embedding).await
            .map_err(ServerError::Core)?

        // Step 3: Write vector mapping (entry_id -> hnsw_data_id)
        // The HNSW insert returns a data_id implicitly through the VectorAdapter
        // Actually: the VectorAdapter.insert() handles both HNSW insert and vector mapping
        // So step 2 already handles this.

        Ok((entry_id, record))
```

### Key Constraints
- write_in_txn does NOT commit -- caller owns the transaction
- write_in_txn uses same COUNTERS["next_audit_id"] key as log_event
- insert_with_audit uses spawn_blocking for the redb write transaction
- HNSW insert happens AFTER the redb commit (separate data structure)
- The VectorAdapter.insert() handles both HNSW insertion and vector mapping
- All 5 secondary indexes + status counter updated in same transaction
- Entry serialization uses the same serialize_entry() from schema module
