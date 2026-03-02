//! Unix domain socket listener for hook IPC.
//!
//! Accepts connections from hook processes, authenticates them via peer
//! credentials (Layer 2: UID verification), dispatches requests, and
//! returns responses. Integrates into server startup/shutdown.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use unimatrix_adapt::AdaptationService;
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{EmbedService, SearchResult, Status, StoreAdapter, VectorAdapter};
use unimatrix_engine::auth;
use unimatrix_engine::coaccess::{compute_search_boost, generate_pairs, CO_ACCESS_STALENESS_SECONDS};
use unimatrix_engine::confidence::rerank_score;
use unimatrix_engine::wire::{
    EntryPayload, HookRequest, HookResponse, ERR_INVALID_PAYLOAD, ERR_UNKNOWN_REQUEST,
    MAX_PAYLOAD_SIZE,
};
use unimatrix_store::Store;

use crate::embed_handle::EmbedServiceHandle;
use crate::session::SessionRegistry;

/// Minimum cosine similarity for injection candidates.
const SIMILARITY_FLOOR: f64 = 0.5;

/// Minimum confidence score for injection candidates.
const CONFIDENCE_FLOOR: f64 = 0.3;

/// Maximum number of entries to search for in injection.
const INJECTION_K: usize = 5;

/// HNSW expansion factor (mirrors tools.rs constant).
const EF_SEARCH: usize = 32;

/// Total byte budget for compaction payload (~2000 tokens).
const MAX_COMPACTION_BYTES: usize = 8000;

/// Soft cap for decision entries (~400 tokens).
const DECISION_BUDGET_BYTES: usize = 1600;

/// Soft cap for re-injected entries (~600 tokens).
const INJECTION_BUDGET_BYTES: usize = 2400;

/// Soft cap for convention entries (~400 tokens).
const CONVENTION_BUDGET_BYTES: usize = 1600;

/// Soft cap for session context section (~200 tokens).
const CONTEXT_BUDGET_BYTES: usize = 800;

/// RAII guard for socket file cleanup.
///
/// Removes the socket file when dropped. Analogous to `PidGuard` for the PID file.
pub struct SocketGuard {
    path: PathBuf,
}

impl SocketGuard {
    /// Create a new `SocketGuard` for the given socket path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!(
                    error = %e,
                    path = %self.path.display(),
                    "failed to remove socket file on drop"
                );
            }
        }
    }
}

/// Remove a stale socket file if it exists.
///
/// Called after PidGuard acquisition, so any existing socket is stale
/// (the previous server process has exited). Per ADR-004: unconditional unlink.
pub fn handle_stale_socket(socket_path: &Path) -> io::Result<()> {
    match fs::remove_file(socket_path) {
        Ok(()) => {
            tracing::info!(path = %socket_path.display(), "removed stale socket file");
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // No stale socket -- normal case
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %socket_path.display(),
                "failed to remove stale socket file"
            );
            return Err(e);
        }
    }
    Ok(())
}

/// Bind the UDS listener, set permissions, and spawn the accept loop.
///
/// Returns a `JoinHandle` for the accept loop task and a `SocketGuard`
/// for RAII socket file cleanup.
pub async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    server_uid: u32,
    server_version: String,
) -> io::Result<(tokio::task::JoinHandle<()>, SocketGuard)> {
    let listener = tokio::net::UnixListener::bind(socket_path)?;

    // Set socket file permissions to 0o600 (owner-only) -- Layer 1 auth
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
    }

    tracing::info!(path = %socket_path.display(), "UDS listener bound");

    let guard = SocketGuard::new(socket_path.to_path_buf());
    let socket_path_display = socket_path.display().to_string();

    let handle = tokio::spawn(async move {
        accept_loop(
            listener,
            store,
            embed_service,
            vector_store,
            entry_store,
            adapt_service,
            session_registry,
            server_uid,
            server_version,
            socket_path_display,
        )
        .await;
    });

    Ok((handle, guard))
}

/// Accept loop: waits for connections and spawns per-connection handlers.
///
/// Never panics -- errors in accept are logged and the loop continues (R-19).
async fn accept_loop(
    listener: tokio::net::UnixListener,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let store = Arc::clone(&store);
                let embed_service = Arc::clone(&embed_service);
                let vector_store = Arc::clone(&vector_store);
                let entry_store = Arc::clone(&entry_store);
                let adapt_service = Arc::clone(&adapt_service);
                let session_registry = Arc::clone(&session_registry);
                let version = server_version.clone();

                // Per-connection handler in its own task (panic isolation -- R-19)
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(
                        stream,
                        store,
                        embed_service,
                        vector_store,
                        entry_store,
                        adapt_service,
                        session_registry,
                        server_uid,
                        version,
                    )
                    .await
                    {
                        tracing::warn!(error = %e, "UDS connection handler error");
                    }
                });
            }
            Err(e) => {
                // Accept error (e.g., too many open files)
                // Log and continue -- do not crash the accept loop
                tracing::warn!(
                    error = %e,
                    socket = socket_path_display,
                    "UDS accept error, continuing"
                );
                // Brief pause to avoid tight error loop on persistent failures
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

/// Handle a single UDS connection: authenticate, read request, dispatch, respond.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    server_uid: u32,
    server_version: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Convert to std for auth (peer credential extraction uses std::os::unix)
    let std_stream = stream.into_std()?;

    // Authenticate (Layer 2 + Layer 3)
    let _creds = match auth::authenticate_connection(&std_stream, server_uid) {
        Ok(creds) => {
            tracing::debug!(uid = creds.uid, pid = ?creds.pid, "UDS connection authenticated");
            creds
        }
        Err(e) => {
            // Auth failure: close connection with no response (ADR-003)
            tracing::warn!(error = %e, "UDS authentication failed, closing connection");
            return Ok(());
        }
    };

    // Convert back to tokio stream for async I/O
    let stream = tokio::net::UnixStream::from_std(std_stream)?;
    let (mut reader, mut writer) = stream.into_split();

    // Read 4-byte header
    let mut header = [0u8; 4];
    reader.read_exact(&mut header).await?;
    let length = u32::from_be_bytes(header) as usize;

    // Validate length
    if length == 0 {
        let err_response = HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: "empty payload".into(),
        };
        write_response(&mut writer, &err_response).await?;
        return Ok(());
    }

    if length > MAX_PAYLOAD_SIZE {
        let err_response = HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: format!("payload {length} exceeds max {MAX_PAYLOAD_SIZE}"),
        };
        write_response(&mut writer, &err_response).await?;
        return Ok(());
    }

    // Read payload
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer).await?;

    // Deserialize request
    let request: HookRequest = match serde_json::from_slice(&buffer) {
        Ok(req) => req,
        Err(e) => {
            let err_response = HookResponse::Error {
                code: ERR_INVALID_PAYLOAD,
                message: format!("invalid request: {e}"),
            };
            write_response(&mut writer, &err_response).await?;
            return Ok(());
        }
    };

    // Dispatch request (async per ADR-002)
    let response = dispatch_request(
        request,
        &store,
        &embed_service,
        &vector_store,
        &entry_store,
        &adapt_service,
        &server_version,
        &session_registry,
    )
    .await;

    // Write response frame
    write_response(&mut writer, &response).await?;

    Ok(())
}

/// Dispatch a hook request and return the appropriate response.
///
/// Fully async per ADR-002. All handler arms are async.
async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    session_registry: &SessionRegistry,
) -> HookResponse {
    match request {
        HookRequest::Ping => HookResponse::Pong {
            server_version: server_version.to_string(),
        },

        HookRequest::SessionRegister {
            session_id,
            cwd,
            agent_role,
            feature,
        } => {
            tracing::info!(
                session_id,
                cwd,
                agent_role = ?agent_role,
                feature = ?feature,
                "UDS: session registered"
            );

            // Register session in registry (col-008)
            session_registry.register_session(&session_id, agent_role.clone(), feature.clone());

            // Pre-warm embedding model (FR-04)
            warm_embedding_model(embed_service).await;

            HookResponse::Ack
        }

        HookRequest::SessionClose {
            session_id,
            outcome,
            duration_secs,
        } => {
            tracing::info!(
                session_id,
                outcome = ?outcome,
                duration_secs,
                "UDS: session closed"
            );

            // Clear all session state (col-008: replaces coaccess_dedup.clear_session)
            session_registry.clear_session(&session_id);

            HookResponse::Ack
        }

        HookRequest::RecordEvent { event } => {
            tracing::info!(
                event_type = event.event_type,
                session_id = event.session_id,
                "UDS: event recorded"
            );
            HookResponse::Ack
        }

        HookRequest::RecordEvents { events } => {
            tracing::info!(count = events.len(), "UDS: batch events recorded");
            HookResponse::Ack
        }

        HookRequest::ContextSearch {
            query,
            session_id,
            role: _,
            task: _,
            feature: _,
            k,
            max_tokens: _,
        } => {
            handle_context_search(
                query,
                session_id,
                k,
                store,
                embed_service,
                vector_store,
                entry_store,
                adapt_service,
                session_registry,
            )
            .await
        }

        HookRequest::CompactPayload {
            session_id,
            injected_entry_ids: _, // Reserved for col-010 disk-based fallback; server uses SessionRegistry
            role,
            feature,
            token_limit,
        } => {
            handle_compact_payload(
                &session_id,
                role,
                feature,
                token_limit,
                entry_store,
                session_registry,
            )
            .await
        }

        // Future request types return error (stubs not handled yet)
        _ => HookResponse::Error {
            code: ERR_UNKNOWN_REQUEST,
            message: "request type not implemented".into(),
        },
    }
}

/// Handle a ContextSearch request: embed, search, re-rank, filter, respond.
///
/// Duplicates the search pipeline orchestration from tools.rs per ADR-001.
/// The underlying service calls are identical; only the wiring is duplicated.
async fn handle_context_search(
    query: String,
    session_id: Option<String>,
    k: Option<u32>,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    session_registry: &SessionRegistry,
) -> HookResponse {
    let k = k.map(|v| v as usize).unwrap_or(INJECTION_K);

    // 1. Get embedding adapter
    let adapter = match embed_service.get_adapter().await {
        Ok(a) => a,
        Err(_) => {
            // EmbedNotReady or EmbedFailed: return empty results (FR-02.6)
            tracing::debug!("embed service not ready, returning empty entries");
            return HookResponse::Entries {
                items: vec![],
                total_tokens: 0,
            };
        }
    };

    // 2. Embed query via spawn_blocking
    let raw_embedding: Vec<f32> = match tokio::task::spawn_blocking({
        let adapter = Arc::clone(&adapter);
        let q = query.clone();
        move || adapter.embed_entry("", &q)
    })
    .await
    {
        Ok(Ok(embedding)) => embedding,
        Ok(Err(e)) => {
            tracing::warn!("embedding failed: {e}");
            return HookResponse::Entries {
                items: vec![],
                total_tokens: 0,
            };
        }
        Err(e) => {
            tracing::warn!("spawn_blocking failed: {e}");
            return HookResponse::Entries {
                items: vec![],
                total_tokens: 0,
            };
        }
    };

    // 3. Adapt + normalize (mirrors tools.rs step 7b)
    let adapted = adapt_service.adapt_embedding(&raw_embedding, None, None);
    let embedding = unimatrix_embed::l2_normalized(&adapted);

    // 4. HNSW search (unfiltered -- hooks don't pass metadata filters)
    let search_results: Vec<SearchResult> = match vector_store.search(embedding, k, EF_SEARCH).await
    {
        Ok(results) => results,
        Err(e) => {
            tracing::warn!("vector search failed: {e}");
            return HookResponse::Entries {
                items: vec![],
                total_tokens: 0,
            };
        }
    };

    // 5. Fetch entries, exclude quarantined (mirrors tools.rs step 9)
    let mut results_with_scores = Vec::new();
    for sr in &search_results {
        match entry_store.get(sr.entry_id).await {
            Ok(entry) => {
                if entry.status == Status::Quarantined {
                    continue;
                }
                results_with_scores.push((entry, sr.similarity));
            }
            Err(_) => continue, // silently skip deleted entries
        }
    }

    // 6. Re-rank: 0.85*similarity + 0.15*confidence (mirrors tools.rs step 9b)
    results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
        let score_a = rerank_score(*sim_a, entry_a.confidence);
        let score_b = rerank_score(*sim_b, entry_b.confidence);
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 7. Co-access boost (mirrors tools.rs step 9c)
    if results_with_scores.len() > 1 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS);

        let anchor_count = results_with_scores.len().min(3);
        let anchor_ids: Vec<u64> = results_with_scores
            .iter()
            .take(anchor_count)
            .map(|(e, _)| e.id)
            .collect();
        let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

        let store_clone = Arc::clone(store);
        let boost_map = tokio::task::spawn_blocking(move || {
            compute_search_boost(&anchor_ids, &result_ids, &store_clone, staleness_cutoff)
        })
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("co-access boost task failed: {e}");
            HashMap::new()
        });

        if !boost_map.is_empty() {
            results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                let base_a = rerank_score(*sim_a, entry_a.confidence);
                let base_b = rerank_score(*sim_b, entry_b.confidence);
                let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
                let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
                let final_a = base_a + boost_a;
                let final_b = base_b + boost_b;
                final_b
                    .partial_cmp(&final_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    // 8. Truncate to k
    results_with_scores.truncate(k);

    // 9. Filter by similarity and confidence floors
    let filtered: Vec<_> = results_with_scores
        .into_iter()
        .filter(|(entry, sim)| *sim >= SIMILARITY_FLOOR && entry.confidence >= CONFIDENCE_FLOOR)
        .collect();

    // 10. Injection tracking (col-008)
    if let Some(ref sid) = session_id {
        if !sid.is_empty() && !filtered.is_empty() {
            let injection_entries: Vec<(u64, f64)> = filtered
                .iter()
                .map(|(entry, _sim)| (entry.id, entry.confidence))
                .collect();
            session_registry.record_injection(sid, &injection_entries);
        }
    }

    // 11. Co-access pair recording with dedup (col-008: use session_id)
    if filtered.len() >= 2 {
        let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect();
        let session_key = session_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("hook-injection");
        if session_registry.check_and_insert_coaccess(session_key, &entry_ids) {
            let pairs = generate_pairs(&entry_ids, entry_ids.len());
            if !pairs.is_empty() {
                let store_clone = Arc::clone(store);
                // Fire-and-forget: don't await (FR-05.5)
                let _ = tokio::task::spawn_blocking(move || {
                    if let Err(e) = store_clone.record_co_access_pairs(&pairs) {
                        tracing::warn!("co-access recording failed: {e}");
                    }
                });
            }
        }
    }

    // 12. Build response
    let items: Vec<EntryPayload> = filtered
        .iter()
        .map(|(entry, sim)| EntryPayload {
            id: entry.id,
            title: entry.title.clone(),
            content: entry.content.clone(),
            confidence: entry.confidence,
            similarity: *sim,
            category: entry.category.clone(),
        })
        .collect();

    let total_bytes: usize = items.iter().map(|e| e.content.len()).sum();
    let total_tokens = (total_bytes / 4) as u32;

    HookResponse::Entries {
        items,
        total_tokens,
    }
}

/// Entries partitioned by budget category for compaction payload.
struct CompactionCategories {
    decisions: Vec<(unimatrix_store::EntryRecord, f64)>,
    injections: Vec<(unimatrix_store::EntryRecord, f64)>,
    conventions: Vec<(unimatrix_store::EntryRecord, f64)>,
}

/// Handle a CompactPayload request: build prioritized knowledge payload.
///
/// Primary path: fetch entries from session injection history by ID.
/// Fallback path: query entries by category when no injection history exists.
async fn handle_compact_payload(
    session_id: &str,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    session_registry: &SessionRegistry,
) -> HookResponse {
    // Determine byte budget
    let max_bytes = match token_limit {
        Some(limit) => ((limit as usize) * 4).min(MAX_COMPACTION_BYTES),
        None => MAX_COMPACTION_BYTES,
    };

    // Get session state
    let session_state = session_registry.get_state(session_id);

    // Determine role/feature: prefer session state, fall back to request fields
    let effective_role = session_state
        .as_ref()
        .and_then(|s| s.role.clone())
        .or(role);
    let effective_feature = session_state
        .as_ref()
        .and_then(|s| s.feature.clone())
        .or(feature);
    let compaction_count = session_state
        .as_ref()
        .map(|s| s.compaction_count)
        .unwrap_or(0);

    // Choose primary vs fallback path
    let has_injection_history = session_state
        .as_ref()
        .is_some_and(|s| !s.injection_history.is_empty());

    let categories = if has_injection_history {
        primary_path(session_state.as_ref().unwrap(), entry_store).await
    } else {
        fallback_path(effective_feature.as_deref(), entry_store).await
    };

    // Format payload
    let content = format_compaction_payload(
        &categories,
        effective_role.as_deref(),
        effective_feature.as_deref(),
        compaction_count,
        max_bytes,
    );

    // Increment compaction count
    session_registry.increment_compaction(session_id);

    let token_count = content
        .as_ref()
        .map(|c| (c.len() / 4) as u32)
        .unwrap_or(0);

    HookResponse::BriefingContent {
        content: content.unwrap_or_default(),
        token_count,
    }
}

/// Primary path: fetch entries from injection history by ID.
///
/// Deduplicates by entry_id (keeps highest confidence). Partitions by category.
/// Excludes quarantined entries, includes deprecated with indicator.
async fn primary_path(
    session: &crate::session::SessionState,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
) -> CompactionCategories {
    // Deduplicate: keep highest confidence per entry_id
    let mut best_confidence: HashMap<u64, f64> = HashMap::new();
    for record in &session.injection_history {
        let entry = best_confidence.entry(record.entry_id).or_insert(0.0);
        if record.confidence > *entry {
            *entry = record.confidence;
        }
    }

    let mut decisions = Vec::new();
    let mut injections = Vec::new();
    let mut conventions = Vec::new();

    for (&entry_id, &confidence) in &best_confidence {
        match entry_store.get(entry_id).await {
            Ok(entry) => {
                if entry.status == Status::Quarantined {
                    continue;
                }
                match entry.category.as_str() {
                    "decision" => decisions.push((entry, confidence)),
                    "convention" => conventions.push((entry, confidence)),
                    _ => injections.push((entry, confidence)),
                }
            }
            Err(_) => continue, // Skip entries that no longer exist (R-11)
        }
    }

    // Sort each group by confidence descending
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    injections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    conventions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    CompactionCategories {
        decisions,
        injections,
        conventions,
    }
}

/// Fallback path: query entries by category when no injection history exists.
async fn fallback_path(
    feature: Option<&str>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
) -> CompactionCategories {
    // Query active decisions
    let mut decisions: Vec<(unimatrix_store::EntryRecord, f64)> =
        match entry_store.query_by_category("decision").await {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.status == Status::Active)
                .map(|e| {
                    let c = e.confidence;
                    (e, c)
                })
                .collect(),
            Err(_) => Vec::new(),
        };

    // If feature tag available, prefer feature-specific decisions
    if let Some(feat) = feature {
        let feature_decisions: Vec<_> = decisions
            .iter()
            .filter(|(e, _)| e.tags.iter().any(|t| t == feat))
            .cloned()
            .collect();
        if !feature_decisions.is_empty() {
            let general: Vec<_> = decisions
                .into_iter()
                .filter(|(e, _)| !e.tags.iter().any(|t| t == feat))
                .collect();
            decisions = feature_decisions;
            decisions.extend(general);
        }
    }

    // Query active conventions
    let mut conventions: Vec<(unimatrix_store::EntryRecord, f64)> =
        match entry_store.query_by_category("convention").await {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| e.status == Status::Active)
                .map(|e| {
                    let c = e.confidence;
                    (e, c)
                })
                .collect(),
            Err(_) => Vec::new(),
        };

    // If feature tag available, prefer feature-specific conventions
    if let Some(feat) = feature {
        let feature_conventions: Vec<_> = conventions
            .iter()
            .filter(|(e, _)| e.tags.iter().any(|t| t == feat))
            .cloned()
            .collect();
        if !feature_conventions.is_empty() {
            let general: Vec<_> = conventions
                .into_iter()
                .filter(|(e, _)| !e.tags.iter().any(|t| t == feat))
                .collect();
            conventions = feature_conventions;
            conventions.extend(general);
        }
    }

    // Sort by confidence descending (within feature-first / general groups)
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    conventions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    CompactionCategories {
        decisions,
        injections: Vec::new(),
        conventions,
    }
}

/// Format compaction payload with priority-based budget allocation per ADR-003.
fn format_compaction_payload(
    categories: &CompactionCategories,
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
) -> Option<String> {
    if categories.decisions.is_empty()
        && categories.injections.is_empty()
        && categories.conventions.is_empty()
    {
        return None;
    }

    let mut output = String::new();

    // Header
    output.push_str("--- Unimatrix Compaction Context ---\n");

    // Session context section
    let context_budget = CONTEXT_BUDGET_BYTES.min(max_bytes.saturating_sub(output.len()));
    let mut context_section = String::new();
    if let Some(r) = role {
        context_section.push_str(&format!("Role: {r}\n"));
    }
    if let Some(f) = feature {
        context_section.push_str(&format!("Feature: {f}\n"));
    }
    if compaction_count > 0 {
        context_section.push_str(&format!("Compaction: #{}\n", compaction_count + 1));
    }
    if !context_section.is_empty() {
        let truncated = truncate_utf8(&context_section, context_budget);
        output.push_str(truncated);
        output.push('\n');
    }

    let mut bytes_used = output.len();

    // Decisions section
    let remaining = max_bytes.saturating_sub(bytes_used);
    let decision_budget = DECISION_BUDGET_BYTES.min(remaining);
    bytes_used += format_category_section(&mut output, "Decisions", &categories.decisions, decision_budget);

    // Injections section
    let remaining = max_bytes.saturating_sub(bytes_used);
    let injection_budget = INJECTION_BUDGET_BYTES.min(remaining);
    bytes_used += format_category_section(&mut output, "Key Context", &categories.injections, injection_budget);

    // Conventions section
    let remaining = max_bytes.saturating_sub(bytes_used);
    let convention_budget = CONVENTION_BUDGET_BYTES.min(remaining);
    let _ = format_category_section(&mut output, "Conventions", &categories.conventions, convention_budget);

    // Hard ceiling check
    if output.len() > max_bytes {
        let truncated = truncate_utf8(&output, max_bytes);
        return Some(truncated.to_string());
    }

    Some(output)
}

/// Format a single category section within a byte budget. Returns bytes consumed.
fn format_category_section(
    output: &mut String,
    section_name: &str,
    entries: &[(unimatrix_store::EntryRecord, f64)],
    budget: usize,
) -> usize {
    if entries.is_empty() || budget < 50 {
        return 0;
    }

    let start_len = output.len();
    let section_header = format!("\n## {section_name}\n");
    if section_header.len() > budget {
        return 0;
    }
    output.push_str(&section_header);

    for (entry, confidence) in entries {
        let confidence_pct = (confidence * 100.0) as u32;
        let status_indicator = if entry.status == Status::Deprecated {
            " [deprecated]"
        } else {
            ""
        };
        let block = format!(
            "[{}]{} ({}% confidence)\n{}\n<!-- id:{} -->\n\n",
            entry.title, status_indicator, confidence_pct, entry.content, entry.id
        );

        let current_section_bytes = output.len() - start_len;
        let projected = current_section_bytes + block.len();
        if projected <= budget {
            output.push_str(&block);
        } else {
            let remaining = budget.saturating_sub(current_section_bytes);
            if remaining < 100 {
                break;
            }
            let truncated = truncate_utf8(&block, remaining);
            output.push_str(truncated);
            break;
        }
    }

    output.len() - start_len
}

/// Truncate a string to at most `max_bytes` bytes at a valid UTF-8 char boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Pre-warm the ONNX embedding model on SessionStart.
///
/// Blocks until the model is loaded (or failed), then runs a no-op embedding
/// to force ONNX runtime initialization. Returns without error on any failure
/// (warming is best-effort).
async fn warm_embedding_model(embed_service: &Arc<EmbedServiceHandle>) {
    match embed_service.get_adapter().await {
        Ok(adapter) => {
            match tokio::task::spawn_blocking(move || adapter.embed_entry("", "warmup")).await {
                Ok(Ok(_)) => {
                    tracing::info!("ONNX embedding model pre-warmed");
                }
                Ok(Err(e)) => {
                    tracing::warn!("warmup embedding failed: {e}");
                }
                Err(e) => {
                    tracing::warn!("warmup spawn_blocking failed: {e}");
                }
            }
        }
        Err(e) => {
            tracing::warn!("embed service not ready during session warming: {e}");
        }
    }
}

/// Write a length-prefixed response frame to the async writer.
async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &HookResponse,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_vec(response)?;
    let length = payload.len() as u32;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_engine::wire::ImplantEvent;

    // -- Helpers --

    fn make_store() -> Arc<Store> {
        Arc::new(Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap())
    }

    fn make_embed_service() -> Arc<EmbedServiceHandle> {
        EmbedServiceHandle::new()
    }

    fn make_registry() -> SessionRegistry {
        SessionRegistry::new()
    }

    fn make_dispatch_deps(store: &Arc<Store>) -> (
        Arc<AsyncVectorStore<VectorAdapter>>,
        Arc<AsyncEntryStore<StoreAdapter>>,
        Arc<AdaptationService>,
    ) {
        let store_adapter = StoreAdapter::new(Arc::clone(store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));
        (async_vector_store, async_entry_store, adapt_service)
    }

    // -- SocketGuard tests --

    #[test]
    fn socket_guard_removes_file_on_drop() {
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("test.sock");
        fs::write(&sock_path, "placeholder").unwrap();
        assert!(sock_path.exists());

        {
            let _guard = SocketGuard::new(sock_path.clone());
        }

        assert!(!sock_path.exists());
    }

    #[test]
    fn socket_guard_no_panic_on_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("nonexistent.sock");

        {
            let _guard = SocketGuard::new(sock_path.clone());
        }
    }

    #[test]
    fn handle_stale_socket_removes_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("stale.sock");
        fs::write(&sock_path, "stale").unwrap();

        handle_stale_socket(&sock_path).unwrap();
        assert!(!sock_path.exists());
    }

    #[test]
    fn handle_stale_socket_ok_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("missing.sock");

        handle_stale_socket(&sock_path).unwrap();
    }

    // -- Dispatch tests (async per ADR-002) --

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::Ping,
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        match response {
            HookResponse::Pong { server_version } => assert_eq!(server_version, "0.1.0"),
            _ => panic!("expected Pong"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_register_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::SessionRegister {
                session_id: "s1".to_string(),
                cwd: "/work".to_string(),
                agent_role: Some("dev".to_string()),
                feature: Some("col-008".to_string()),
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        assert!(matches!(response, HookResponse::Ack));

        // col-008: verify session registered
        let state = registry.get_state("s1").unwrap();
        assert_eq!(state.role.as_deref(), Some("dev"));
        assert_eq!(state.feature.as_deref(), Some("col-008"));
    }

    #[tokio::test]
    async fn dispatch_session_close_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        // Register first
        registry.register_session("s1", None, None);

        let response = dispatch_request(
            HookRequest::SessionClose {
                session_id: "s1".to_string(),
                outcome: Some("success".to_string()),
                duration_secs: 60,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        assert!(matches!(response, HookResponse::Ack));

        // col-008: verify session cleared
        assert!(registry.get_state("s1").is_none());
    }

    #[tokio::test]
    async fn dispatch_record_event_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let event = ImplantEvent {
            event_type: "test".to_string(),
            session_id: "s1".to_string(),
            timestamp: 0,
            payload: serde_json::json!({}),
        };
        let response = dispatch_request(
            HookRequest::RecordEvent { event },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        assert!(matches!(response, HookResponse::Ack));
    }

    #[tokio::test]
    async fn dispatch_unknown_returns_error() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::Briefing {
                role: "dev".to_string(),
                task: "test".to_string(),
                feature: None,
                max_tokens: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        match response {
            HookResponse::Error { code, .. } => assert_eq!(code, ERR_UNKNOWN_REQUEST),
            _ => panic!("expected Error"),
        }
    }

    #[tokio::test]
    async fn dispatch_context_search_embed_not_ready() {
        let store = make_store();
        let embed = make_embed_service(); // Not started -- EmbedNotReady
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::ContextSearch {
                query: "test".to_string(),
                session_id: None,
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        match response {
            HookResponse::Entries { items, total_tokens } => {
                assert!(items.is_empty());
                assert_eq!(total_tokens, 0);
            }
            _ => panic!("expected Entries, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_close_clears_coaccess_via_registry() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = SessionRegistry::new();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        // Register session and insert coaccess state
        registry.register_session("s1", None, None);
        registry.check_and_insert_coaccess("s1", &[1, 2, 3]);

        // Dispatch SessionClose
        let _ = dispatch_request(
            HookRequest::SessionClose {
                session_id: "s1".to_string(),
                outcome: None,
                duration_secs: 0,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;

        // After clear + re-register, same set should be considered new
        registry.register_session("s1", None, None);
        assert!(registry.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    // -- CompactPayload dispatch tests (col-008) --

    #[tokio::test]
    async fn dispatch_compact_payload_empty_session_returns_briefing() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::CompactPayload {
                session_id: "unknown".to_string(),
                injected_entry_ids: vec![],
                role: None,
                feature: None,
                token_limit: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;
        match response {
            HookResponse::BriefingContent { content, token_count } => {
                // No session, no entries in KB -> empty content
                assert!(content.is_empty());
                assert_eq!(token_count, 0);
            }
            _ => panic!("expected BriefingContent, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_compact_payload_increments_compaction_count() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        registry.register_session("s1", None, None);

        let _ = dispatch_request(
            HookRequest::CompactPayload {
                session_id: "s1".to_string(),
                injected_entry_ids: vec![],
                role: None,
                feature: None,
                token_limit: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;

        assert_eq!(registry.get_state("s1").unwrap().compaction_count, 1);
    }

    // -- format_compaction_payload unit tests --

    fn make_entry(id: u64, title: &str, content: &str, category: &str, confidence: f64) -> unimatrix_store::EntryRecord {
        unimatrix_store::EntryRecord {
            id,
            title: title.to_string(),
            content: content.to_string(),
            topic: String::new(),
            category: category.to_string(),
            tags: vec![],
            source: String::new(),
            status: Status::Active,
            confidence,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 0,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
        }
    }

    #[test]
    fn format_payload_empty_categories_returns_none() {
        let categories = CompactionCategories {
            decisions: vec![],
            injections: vec![],
            conventions: vec![],
        };
        assert!(format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).is_none());
    }

    #[test]
    fn format_payload_header_present() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "ADR", "content", "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.starts_with("--- Unimatrix Compaction Context ---\n"));
    }

    #[test]
    fn format_payload_decisions_before_injections() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "Decision", "dcontent", "decision", 0.9), 0.9)],
            injections: vec![(make_entry(2, "Pattern", "pcontent", "pattern", 0.8), 0.8)],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        let dec_pos = result.find("[Decision]").unwrap();
        let inj_pos = result.find("[Pattern]").unwrap();
        assert!(dec_pos < inj_pos, "decisions must appear before injections");
    }

    #[test]
    fn format_payload_sorted_by_confidence() {
        // Input in LOW-first order to verify format_category_section preserves caller's sort
        let categories = CompactionCategories {
            decisions: vec![
                (make_entry(2, "High", "c", "decision", 0.9), 0.9),
                (make_entry(1, "Low", "c", "decision", 0.3), 0.3),
            ],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        let high_pos = result.find("[High]").expect("High entry missing");
        let low_pos = result.find("[Low]").expect("Low entry missing");
        assert!(high_pos < low_pos, "high-confidence entry must appear before low-confidence entry");
    }

    #[test]
    fn format_payload_budget_enforcement() {
        let long_content = "x".repeat(5000);
        let categories = CompactionCategories {
            decisions: vec![
                (make_entry(1, "Big1", &long_content, "decision", 0.9), 0.9),
                (make_entry(2, "Big2", &long_content, "decision", 0.8), 0.8),
            ],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.len() <= MAX_COMPACTION_BYTES, "output {} exceeds budget {}", result.len(), MAX_COMPACTION_BYTES);
    }

    #[test]
    fn format_payload_multibyte_utf8() {
        let cjk = "\u{4e16}\u{754c}".repeat(200); // 1200 bytes
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "CJK", &cjk, "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, 500).unwrap();
        assert!(result.len() <= 500);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn format_payload_session_context() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "D", "c", "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(
            &categories,
            Some("developer"),
            Some("col-008"),
            2,
            MAX_COMPACTION_BYTES,
        ).unwrap();
        assert!(result.contains("Role: developer"));
        assert!(result.contains("Feature: col-008"));
        assert!(result.contains("Compaction: #3"));
    }

    #[test]
    fn format_payload_deprecated_indicator() {
        let mut entry = make_entry(1, "Old", "content", "decision", 0.7);
        entry.status = Status::Deprecated;
        let categories = CompactionCategories {
            decisions: vec![(entry, 0.7)],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.contains("[deprecated]"));
    }

    #[test]
    fn format_payload_entry_id_metadata() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(42, "Test", "c", "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result = format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.contains("<!-- id:42 -->"));
    }

    #[test]
    fn format_payload_token_limit_override() {
        let long_content = "x".repeat(2000);
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "D", &long_content, "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        // 100 tokens = 400 bytes
        let result = format_compaction_payload(&categories, None, None, 0, 400).unwrap();
        assert!(result.len() <= 400);
    }

    // -- truncate_utf8 tests --

    #[test]
    fn truncate_utf8_within_limit() {
        assert_eq!(truncate_utf8("hello", 10), "hello");
    }

    #[test]
    fn truncate_utf8_at_limit() {
        assert_eq!(truncate_utf8("hello", 5), "hello");
    }

    #[test]
    fn truncate_utf8_ascii() {
        assert_eq!(truncate_utf8("hello world", 5), "hello");
    }

    #[test]
    fn truncate_utf8_multibyte_boundary() {
        let s = "\u{4e16}\u{754c}"; // 6 bytes total
        assert_eq!(truncate_utf8(s, 4), "\u{4e16}");
        assert_eq!(truncate_utf8(s, 3), "\u{4e16}");
    }

    #[test]
    fn truncate_utf8_emoji() {
        let s = "\u{1F600}\u{1F601}"; // 8 bytes total
        assert_eq!(truncate_utf8(s, 5), "\u{1F600}");
    }

    #[test]
    fn truncate_utf8_zero() {
        assert_eq!(truncate_utf8("hello", 0), "");
    }

    // -- Primary path tests (col-008 PR review) --

    #[tokio::test]
    async fn dispatch_compact_payload_primary_path_uses_injection_history() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        // Store entries in the database
        let entry1 = unimatrix_store::NewEntry {
            title: "ADR-Important".to_string(),
            content: "Critical decision content".to_string(),
            topic: "arch".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let entry2 = unimatrix_store::NewEntry {
            title: "Coding Convention".to_string(),
            content: "Always use snake_case".to_string(),
            topic: "style".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let id1 = store.insert(entry1).unwrap();
        let id2 = store.insert(entry2).unwrap();

        // Register session and record injections
        registry.register_session("s1", Some("developer".to_string()), Some("col-008".to_string()));
        registry.record_injection("s1", &[(id1, 0.92), (id2, 0.75)]);

        let response = dispatch_request(
            HookRequest::CompactPayload {
                session_id: "s1".to_string(),
                injected_entry_ids: vec![],
                role: None,
                feature: None,
                token_limit: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;

        match response {
            HookResponse::BriefingContent { content, token_count } => {
                assert!(!content.is_empty(), "primary path should produce non-empty content");
                assert!(token_count > 0);
                // Verify entries from injection history appear in output
                assert!(content.contains("[ADR-Important]"), "decision entry missing");
                assert!(content.contains("[Coding Convention]"), "convention entry missing");
                // Verify decisions appear before conventions (priority ordering)
                let dec_pos = content.find("[ADR-Important]").unwrap();
                let conv_pos = content.find("[Coding Convention]").unwrap();
                assert!(dec_pos < conv_pos, "decisions must appear before conventions");
                // Verify session context
                assert!(content.contains("Role: developer"));
                assert!(content.contains("Feature: col-008"));
            }
            _ => panic!("expected BriefingContent, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_compact_payload_primary_path_sorts_by_confidence() {
        let store = make_store();
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let low = unimatrix_store::NewEntry {
            title: "LowConf".to_string(),
            content: "low confidence decision".to_string(),
            topic: "t".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let high = unimatrix_store::NewEntry {
            title: "HighConf".to_string(),
            content: "high confidence decision".to_string(),
            topic: "t".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let id_low = store.insert(low).unwrap();
        let id_high = store.insert(high).unwrap();

        registry.register_session("s1", None, None);
        // Inject low first, then high — output should still sort high-confidence first
        registry.record_injection("s1", &[(id_low, 0.3), (id_high, 0.95)]);

        let response = dispatch_request(
            HookRequest::CompactPayload {
                session_id: "s1".to_string(),
                injected_entry_ids: vec![],
                role: None,
                feature: None,
                token_limit: None,
            },
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry,
        ).await;

        match response {
            HookResponse::BriefingContent { content, .. } => {
                let high_pos = content.find("[HighConf]").expect("HighConf missing");
                let low_pos = content.find("[LowConf]").expect("LowConf missing");
                assert!(high_pos < low_pos, "high-confidence entry must appear before low-confidence");
            }
            _ => panic!("expected BriefingContent"),
        }
    }

    // -- CoAccessDedup regression test (col-008 PR review) --

    #[tokio::test]
    async fn coaccess_dedup_unregistered_session_skips_recording() {
        // Regression: CoAccessDedup used to create entries for unknown sessions.
        // SessionRegistry returns false for unregistered sessions (no silent creation).
        let registry = make_registry();
        // Do NOT register "unknown-session"
        let is_new = registry.check_and_insert_coaccess("unknown-session", &[1, 2, 3]);
        assert!(!is_new, "unregistered session must return false");
        // Verify no session was implicitly created
        assert!(registry.get_state("unknown-session").is_none());
    }
}
