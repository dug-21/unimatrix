# Pseudocode: SearchService (services/search.rs)

## Types

```
struct SearchService {
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
}

struct ServiceSearchParams {
    query: String,
    k: usize,
    filters: Option<QueryFilter>,
    similarity_floor: Option<f64>,
    confidence_floor: Option<f64>,
    feature_tag: Option<String>,      // reserved for future feature boost
    co_access_anchors: Option<Vec<u64>>,  // reserved for explicit anchor override
    caller_agent_id: Option<String>,  // for provenance boost
}

struct SearchResults {
    entries: Vec<ScoredEntry>,
    query_embedding: Vec<f32>,
}

struct ScoredEntry {
    entry: EntryRecord,
    final_score: f64,
    similarity: f64,
    confidence: f64,
}
```

## Constants

```
const EF_SEARCH: usize = 32;
```

## Constructor

```
fn new(
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
) -> Self:
    SearchService { store, vector_store, entry_store, embed_service, adapt_service, gateway }
```

## search()

```
async fn search(&self, params: ServiceSearchParams, audit_ctx: &AuditContext)
    -> Result<SearchResults, ServiceError>:

    // Step 1: S1 + S3 validation via gateway
    let _scan_warning = self.gateway.validate_search_query(
        &params.query, params.k, audit_ctx
    )?;

    // Step 2: Get embedding adapter
    let adapter = self.embed_service.get_adapter().await
        .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

    // Step 3: Embed query via spawn_blocking
    let query = params.query.clone()
    let raw_embedding = tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter)
        move || adapter.embed_entry("", &query)
    }).await
        .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
        .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

    // Step 4: Adapt embedding (MicroLoRA)
    let adapted = self.adapt_service.adapt_embedding(&raw_embedding, None, None)
    let embedding = unimatrix_embed::l2_normalized(&adapted)

    // Step 5: HNSW search (filtered or unfiltered)
    let search_results = if let Some(ref filter) = params.filters:
        // Pre-filter by metadata
        let entries = self.entry_store.query(filter.clone()).await
            .map_err(|e| ServiceError::Core(e))?
        let allowed_ids: Vec<u64> = entries.iter().map(|e| e.id).collect()
        if allowed_ids.is_empty():
            vec![]
        else:
            self.vector_store.search_filtered(embedding.clone(), params.k, EF_SEARCH, allowed_ids).await
                .map_err(|e| ServiceError::Core(e))?
    else:
        self.vector_store.search(embedding.clone(), params.k, EF_SEARCH).await
            .map_err(|e| ServiceError::Core(e))?

    // Step 6: Fetch entries, exclude quarantined (S4)
    let mut results_with_scores = Vec::new()
    for sr in &search_results:
        match self.entry_store.get(sr.entry_id).await:
            Ok(entry) =>
                if SecurityGateway::is_quarantined(&entry.status):
                    continue
                results_with_scores.push((entry, sr.similarity))
            Err(_) => continue  // silently skip deleted entries

    // Step 7: Re-rank: 0.85*sim + 0.15*conf + provenance
    results_with_scores.sort_by(|(a, sim_a), (b, sim_b)|:
        let prov_a = if a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
        let prov_b = if b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
        let score_a = rerank_score(*sim_a, a.confidence) + prov_a
        let score_b = rerank_score(*sim_b, b.confidence) + prov_b
        score_b.partial_cmp(&score_a).unwrap_or(Equal)
    )

    // Step 8: Co-access boost
    if results_with_scores.len() > 1:
        let now = current_timestamp_secs()
        let staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS)
        let anchor_count = results_with_scores.len().min(3)
        let anchor_ids = results_with_scores[..anchor_count].map(|(e,_)| e.id)
        let result_ids = results_with_scores.map(|(e,_)| e.id)

        let store = Arc::clone(&self.store)
        let boost_map = tokio::task::spawn_blocking(move ||
            compute_search_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
        ).await.unwrap_or_else(|e| {
            tracing::warn!("co-access boost task failed: {e}")
            HashMap::new()
        })

        if !boost_map.is_empty():
            results_with_scores.sort_by(|(a, sim_a), (b, sim_b)|:
                let base_a = rerank_score(*sim_a, a.confidence)
                let base_b = rerank_score(*sim_b, b.confidence)
                let boost_a = boost_map.get(&a.id).copied().unwrap_or(0.0)
                let boost_b = boost_map.get(&b.id).copied().unwrap_or(0.0)
                let prov_a = if a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
                let prov_b = if b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 }
                let final_a = base_a + boost_a + prov_a
                let final_b = base_b + boost_b + prov_b
                final_b.partial_cmp(&final_a).unwrap_or(Equal)
            )

    // Step 9: Apply floors (if set)
    if let Some(sim_floor) = params.similarity_floor:
        results_with_scores.retain(|(_, sim)| *sim >= sim_floor)
    if let Some(conf_floor) = params.confidence_floor:
        results_with_scores.retain(|(entry, _)| entry.confidence >= conf_floor)

    // Step 10: Truncate to k
    results_with_scores.truncate(params.k)

    // Step 11: Build ScoredEntry results
    let entries: Vec<ScoredEntry> = results_with_scores.iter().map(|(entry, sim)|:
        ScoredEntry {
            entry: entry.clone(),
            final_score: rerank_score(*sim, entry.confidence),  // approximate; actual may include boosts
            similarity: *sim,
            confidence: entry.confidence,
        }
    ).collect()

    // Step 12: S5 audit
    let target_ids: Vec<u64> = entries.iter().map(|e| e.entry.id).collect()
    self.gateway.emit_audit(AuditEvent {
        operation: "search_service",
        target_ids,
        detail: format!("returned {} results", entries.len()),
        ...from audit_ctx
    })

    Ok(SearchResults {
        entries,
        query_embedding: embedding,
    })
```

## Notes

- The search pipeline is an exact extraction of the logic from tools.rs lines 280-431 and uds_listener.rs lines 586-780.
- The provenance boost currently uses "lesson-learned" category check rather than caller_agent_id ownership. This matches the EXISTING behavior in both tools.rs and uds_listener.rs. The architecture mentions caller_agent_id-based provenance but the current code uses category-based. We preserve the current behavior (like-for-like).
- Feature boost (feature_tag) is reserved in the params struct but not implemented -- no current code uses it. The field exists for future vnc-007 use.
- co_access_anchors override is reserved but not used -- anchors are computed from top results (same as current behavior).
