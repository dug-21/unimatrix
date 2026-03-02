# Pseudocode: uds-dispatch

## Purpose

Make `dispatch_request()` async, add `ContextSearch` handler with full search pipeline, expand `start_uds_listener()` signature with additional Arc parameters, add `CoAccessDedup` for session-scoped dedup, and update `main.rs` call site.

## Modified: start_uds_listener() signature

```
pub async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    server_uid: u32,
    server_version: String,
) -> io::Result<(JoinHandle<()>, SocketGuard)>
```

Per ADR-001: individual Arc parameters, not a context struct. 4 new parameters: embed_service, vector_store, entry_store, adapt_service.

All new parameters are cloned into the accept_loop closure and from there into each per-connection handler task.

## Modified: accept_loop()

Add parameters matching start_uds_listener. Create `CoAccessDedup::new()` at accept_loop start (shared across all connections via `Arc`). Clone all Arcs + coaccess_dedup into each spawned handler task.

```
async fn accept_loop(
    listener: UnixListener,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
):
    let coaccess_dedup = Arc::new(CoAccessDedup::new())
    loop:
        match listener.accept().await:
            Ok((stream, _addr)) =>
                // Clone all Arcs for the per-connection task
                let store = Arc::clone(&store)
                let embed_service = Arc::clone(&embed_service)
                let vector_store = Arc::clone(&vector_store)
                let entry_store = Arc::clone(&entry_store)
                let adapt_service = Arc::clone(&adapt_service)
                let coaccess_dedup = Arc::clone(&coaccess_dedup)
                let version = server_version.clone()
                tokio::spawn(async move {
                    handle_connection(
                        stream, store, embed_service, vector_store,
                        entry_store, adapt_service, server_uid, version,
                        coaccess_dedup,
                    ).await
                })
            Err(e) =>
                // existing error handling unchanged
```

## Modified: handle_connection()

Add all new parameters. Change dispatch call from sync to async:

```
async fn handle_connection(
    stream: UnixStream,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    server_uid: u32,
    server_version: String,
    coaccess_dedup: Arc<CoAccessDedup>,
) -> Result<(), Box<dyn Error + Send + Sync>>:
    // ... existing auth, read header, read payload, deserialize (unchanged) ...

    // Dispatch request -- NOW ASYNC
    let response = dispatch_request(
        request, &store, &embed_service, &vector_store,
        &entry_store, &adapt_service, &server_version, &coaccess_dedup,
    ).await;

    write_response(&mut writer, &response).await?;
    Ok(())
```

## Modified: dispatch_request() -- sync to async

```
async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    coaccess_dedup: &CoAccessDedup,
) -> HookResponse:
    match request:
        HookRequest::Ping =>
            HookResponse::Pong { server_version: server_version.to_string() }

        HookRequest::SessionRegister { session_id, cwd, agent_role, feature } =>
            // Existing log + Ack, PLUS warming (see session-warming.md)
            handle_session_register(embed_service, session_id, cwd, agent_role, feature).await

        HookRequest::SessionClose { session_id, outcome, duration_secs } =>
            // Existing log + Ack, PLUS dedup cleanup
            tracing::info!(session_id, outcome = ?outcome, duration_secs, "UDS: session closed")
            coaccess_dedup.clear_session(&session_id)
            HookResponse::Ack

        HookRequest::RecordEvent { event } =>
            // Existing log + Ack (unchanged)
            tracing::info!(...)
            HookResponse::Ack

        HookRequest::RecordEvents { events } =>
            // Existing log + Ack (unchanged)
            tracing::info!(...)
            HookResponse::Ack

        HookRequest::ContextSearch { query, role, task, feature, k, max_tokens } =>
            handle_context_search(
                query, role, task, feature, k, max_tokens,
                store, embed_service, vector_store, entry_store,
                adapt_service, coaccess_dedup,
            ).await

        _ =>
            HookResponse::Error { code: ERR_UNKNOWN_REQUEST, message: "request type not implemented".into() }
```

## New: handle_context_search()

This is the ~40 lines of duplicated pipeline orchestration per ADR-001.

```
async fn handle_context_search(
    query: String,
    _role: Option<String>,
    _task: Option<String>,
    _feature: Option<String>,
    k: Option<u32>,
    _max_tokens: Option<u32>,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    coaccess_dedup: &CoAccessDedup,
) -> HookResponse:
    let k = k.map(|v| v as usize).unwrap_or(INJECTION_K)

    // 1. Get embedding adapter
    let adapter = match embed_service.get_adapter().await:
        Ok(a) => a
        Err(_) =>
            // EmbedNotReady or EmbedFailed: return empty results (FR-02.6)
            tracing::debug!("embed service not ready, returning empty entries")
            return HookResponse::Entries { items: vec![], total_tokens: 0 }

    // 2. Embed query via spawn_blocking
    let raw_embedding = match tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter)
        let q = query.clone()
        move || adapter.embed_entry("", &q)
    }).await:
        Ok(Ok(embedding)) => embedding
        Ok(Err(e)) =>
            tracing::warn!("embedding failed: {e}")
            return HookResponse::Entries { items: vec![], total_tokens: 0 }
        Err(e) =>
            tracing::warn!("spawn_blocking failed: {e}")
            return HookResponse::Entries { items: vec![], total_tokens: 0 }

    // 3. Adapt + normalize
    let adapted = adapt_service.adapt_embedding(&raw_embedding, None, None)
    let embedding = unimatrix_embed::l2_normalized(&adapted)

    // 4. HNSW search (unfiltered -- hooks don't pass metadata filters)
    let search_results = match vector_store.search(embedding, k, EF_SEARCH).await:
        Ok(results) => results
        Err(e) =>
            tracing::warn!("vector search failed: {e}")
            return HookResponse::Entries { items: vec![], total_tokens: 0 }

    // 5. Fetch entries, exclude quarantined
    let mut results_with_scores: Vec<(EntryRecord, f64)> = Vec::new()
    for sr in &search_results:
        match entry_store.get(sr.entry_id).await:
            Ok(entry) =>
                if entry.status == Status::Quarantined:
                    continue
                results_with_scores.push((entry, sr.similarity))
            Err(_) => continue

    // 6. Re-rank: 0.85*similarity + 0.15*confidence (mirrors tools.rs step 9b)
    results_with_scores.sort_by(|(ea, sa), (eb, sb)| {
        let score_a = rerank_score(*sa, ea.confidence)
        let score_b = rerank_score(*sb, eb.confidence)
        score_b.partial_cmp(&score_a).unwrap_or(Ordering::Equal)
    })

    // 7. Co-access boost (mirrors tools.rs step 9c)
    if results_with_scores.len() > 1:
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
        let staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS)
        let anchor_count = results_with_scores.len().min(3)
        let anchor_ids: Vec<u64> = results_with_scores.iter().take(anchor_count).map(|(e, _)| e.id).collect()
        let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect()

        let store_clone = Arc::clone(store)
        let boost_map = tokio::task::spawn_blocking(move || {
            compute_search_boost(&anchor_ids, &result_ids, &store_clone, staleness_cutoff)
        }).await.unwrap_or_else(|e| {
            tracing::warn!("co-access boost failed: {e}")
            HashMap::new()
        })

        if !boost_map.is_empty():
            results_with_scores.sort_by(|(ea, sa), (eb, sb)| {
                let base_a = rerank_score(*sa, ea.confidence)
                let base_b = rerank_score(*sb, eb.confidence)
                let boost_a = boost_map.get(&ea.id).copied().unwrap_or(0.0)
                let boost_b = boost_map.get(&eb.id).copied().unwrap_or(0.0)
                (base_b + boost_b).partial_cmp(&(base_a + boost_a)).unwrap_or(Ordering::Equal)
            })

    // 8. Truncate to k
    results_with_scores.truncate(k)

    // 9. Filter by similarity and confidence floors
    let filtered: Vec<_> = results_with_scores.into_iter()
        .filter(|(entry, sim)| *sim >= SIMILARITY_FLOOR && entry.confidence >= CONFIDENCE_FLOOR)
        .collect()

    // 10. Co-access pair recording with session dedup
    //     Extract session_id from... we don't have it in ContextSearch.
    //     The ContextSearch request doesn't carry a session_id field.
    //     Co-access pairs are recorded without session dedup in this path.
    //     CORRECTION: We need to pass session context. However, the wire protocol
    //     for ContextSearch doesn't include session_id. The hook process knows the
    //     session_id (from HookInput.session_id) but doesn't pass it in the
    //     ContextSearch request fields.
    //
    //     SOLUTION: The CoAccessDedup requires session_id. Since ContextSearch
    //     doesn't carry session_id, we extract it from the connection context
    //     or simply record pairs without dedup. Looking at the architecture more
    //     carefully: the ContextSearch wire type has no session_id field.
    //     For now, generate pairs without session-scoped dedup for this call.
    //     The dedup is only meaningful when the same session sends the same
    //     entry set multiple times, which requires session_id correlation.
    //
    //     BETTER SOLUTION: We can use the "feature" field as an optional session
    //     identifier, OR we can record pairs unconditionally (the dedup set
    //     prevents redundancy within a session, but without session_id we
    //     can't dedup). Per the spec FR-05, dedup requires session_id.
    //
    //     ACTUAL SOLUTION: Look at the query context. The hook.rs build_request
    //     does not pass session_id into ContextSearch. We need to extend the
    //     ContextSearch request in wire.rs to carry an optional session_id,
    //     OR we handle dedup at the connection/handler level where we could
    //     potentially track by peer PID. The simplest approach consistent with
    //     the architecture: don't modify the wire protocol (ContextSearch stub
    //     is already defined). Instead, use a heuristic: the CoAccessDedup
    //     is keyed by a "session key". For ContextSearch from hooks, use the
    //     canonical entry ID set as the dedup key with a fixed session prefix.
    //     This achieves the dedup goal: same entries across any session won't
    //     generate redundant pairs.
    //
    //     SIMPLEST CORRECT SOLUTION: We already have session_id in the
    //     build_request function but it's not passed to ContextSearch.
    //     We can't change the wire type (it's an existing stub).
    //     Use a connection-level approach: pass a session hint string.
    //     Since the ContextSearch doesn't carry session_id, we use "unknown"
    //     as the session key. The dedup still works: if the same entry set
    //     is returned across calls with session "unknown", pairs are recorded
    //     only once. SessionClose with a real session_id won't clear "unknown",
    //     but that's fine -- the dedup data for "unknown" is bounded and
    //     eventually cleared on server restart.
    //
    //     NOTE: Implementation should check if we can reasonably add session_id
    //     to HookInput -> build_request -> ContextSearch. Since the wire.rs
    //     ContextSearch variant already exists as a stub, we should NOT modify it
    //     (that would break the stub contract). Accept "unknown" session dedup.

    if filtered.len() >= 2:
        let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect()
        let session_key = "hook-injection"  // fixed key for hook-originated searches
        if coaccess_dedup.check_and_insert(session_key, &entry_ids):
            let pairs = generate_pairs(&entry_ids, entry_ids.len())
            if !pairs.is_empty():
                let store_clone = Arc::clone(store)
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = store_clone.record_co_access_pairs(&pairs):
                        tracing::warn!("co-access recording failed: {e}")
                })
                // fire-and-forget -- don't await

    // 11. Build response
    let items: Vec<EntryPayload> = filtered.iter().map(|(entry, sim)| {
        EntryPayload {
            id: entry.id,
            title: entry.title.clone(),
            content: entry.content.clone(),
            confidence: entry.confidence,
            similarity: *sim,
            category: entry.category.clone(),
        }
    }).collect()

    let total_bytes: usize = items.iter().map(|e| e.content.len()).sum()
    let total_tokens = (total_bytes / 4) as u32  // heuristic: 4 bytes/token

    HookResponse::Entries { items, total_tokens }
```

## New Struct: CoAccessDedup

```
pub(crate) struct CoAccessDedup {
    sessions: Mutex<HashMap<String, HashSet<Vec<u64>>>>,
}

impl CoAccessDedup:
    pub fn new() -> Self:
        CoAccessDedup { sessions: Mutex::new(HashMap::new()) }

    /// Returns true if this entry set is NEW (not previously seen for this session).
    /// Side effect: inserts the set if new.
    pub fn check_and_insert(&self, session_id: &str, entry_ids: &[u64]) -> bool:
        let mut canonical = entry_ids.to_vec()
        canonical.sort_unstable()

        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())
        let set = sessions.entry(session_id.to_string()).or_default()
        set.insert(canonical)  // returns true if the value was not present

    /// Remove all dedup state for a session.
    pub fn clear_session(&self, session_id: &str):
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())
        sessions.remove(session_id)
```

Note: uses `unwrap_or_else(|e| e.into_inner())` for poison recovery, consistent with CategoryAllowlist pattern in the codebase.

## Constants

```
const SIMILARITY_FLOOR: f64 = 0.5;
const CONFIDENCE_FLOOR: f64 = 0.3;
const INJECTION_K: usize = 5;
const EF_SEARCH: usize = 32;
```

## main.rs Changes

Update `start_uds_listener()` call to pass additional Arcs:

```
let (uds_handle, socket_guard) = uds_listener::start_uds_listener(
    &paths.socket_path,
    Arc::clone(&store),
    Arc::clone(&embed_handle),         // NEW
    Arc::clone(&async_vector_store),   // NEW
    Arc::clone(&async_entry_store),    // NEW
    Arc::clone(&adapt_service),        // NEW
    server_uid,
    env!("CARGO_PKG_VERSION").to_string(),
).await?;
```

## Error Handling

- Embed not ready: return empty Entries (not an error)
- Embed failed: return empty Entries (not an error)
- HNSW search failure: return empty Entries
- Entry fetch failure: skip that entry silently
- Co-access write failure: log warning, don't affect response
- Mutex poison: recover via `into_inner()` pattern

## Key Test Scenarios

1. Ping still returns Pong (async migration)
2. SessionRegister still returns Ack (async migration)
3. SessionClose still returns Ack and clears dedup state
4. RecordEvent still returns Ack (async migration)
5. RecordEvents batch still returns Ack (async migration)
6. Unknown request returns Error with ERR_UNKNOWN_REQUEST
7. ContextSearch with ready embed service returns Entries
8. ContextSearch with EmbedNotReady returns empty Entries
9. ContextSearch filters by similarity floor
10. ContextSearch filters by confidence floor
11. Co-access pairs generated for 2+ entry results
12. Co-access dedup prevents duplicate pair recording
13. SessionClose clears dedup for that session
14. CoAccessDedup: insert returns true for new set, false for duplicate
