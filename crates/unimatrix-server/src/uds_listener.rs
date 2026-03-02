//! Unix domain socket listener for hook IPC.
//!
//! Accepts connections from hook processes, authenticates them via peer
//! credentials (Layer 2: UID verification), dispatches requests, and
//! returns responses. Integrates into server startup/shutdown.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// Minimum cosine similarity for injection candidates.
const SIMILARITY_FLOOR: f64 = 0.5;

/// Minimum confidence score for injection candidates.
const CONFIDENCE_FLOOR: f64 = 0.3;

/// Maximum number of entries to search for in injection.
const INJECTION_K: usize = 5;

/// HNSW expansion factor (mirrors tools.rs constant).
const EF_SEARCH: usize = 32;

/// Session-scoped co-access dedup to prevent redundant pair writes.
///
/// Tracks which entry-set combinations have already had co-access pairs
/// recorded for each session. Per ADR-003: in-memory only, cleared on
/// SessionClose, bounded by session count x unique entry sets.
pub(crate) struct CoAccessDedup {
    sessions: Mutex<HashMap<String, HashSet<Vec<u64>>>>,
}

impl CoAccessDedup {
    pub fn new() -> Self {
        CoAccessDedup {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if the entry set is NEW (not previously seen for this session).
    /// Side effect: inserts the set if new.
    pub fn check_and_insert(&self, session_id: &str, entry_ids: &[u64]) -> bool {
        let mut canonical = entry_ids.to_vec();
        canonical.sort_unstable();

        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let set = sessions.entry(session_id.to_string()).or_default();
        set.insert(canonical) // returns true if the value was not present
    }

    /// Remove all dedup state for a session.
    pub fn clear_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.remove(session_id);
    }
}

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
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
) {
    let coaccess_dedup = Arc::new(CoAccessDedup::new());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let store = Arc::clone(&store);
                let embed_service = Arc::clone(&embed_service);
                let vector_store = Arc::clone(&vector_store);
                let entry_store = Arc::clone(&entry_store);
                let adapt_service = Arc::clone(&adapt_service);
                let coaccess_dedup = Arc::clone(&coaccess_dedup);
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
                        server_uid,
                        version,
                        coaccess_dedup,
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
    server_uid: u32,
    server_version: String,
    coaccess_dedup: Arc<CoAccessDedup>,
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
        &coaccess_dedup,
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
    coaccess_dedup: &CoAccessDedup,
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

            // Clear co-access dedup state for this session (ADR-003)
            coaccess_dedup.clear_session(&session_id);

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
            role: _,
            task: _,
            feature: _,
            k,
            max_tokens: _,
        } => {
            handle_context_search(
                query,
                k,
                store,
                embed_service,
                vector_store,
                entry_store,
                adapt_service,
                coaccess_dedup,
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
    k: Option<u32>,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    coaccess_dedup: &CoAccessDedup,
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

    // 10. Co-access pair recording with dedup
    if filtered.len() >= 2 {
        let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect();
        // Use fixed session key since ContextSearch doesn't carry session_id
        let session_key = "hook-injection";
        if coaccess_dedup.check_and_insert(session_key, &entry_ids) {
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

    // 11. Build response
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

    // -- Helper to create a minimal set of dispatch args --
    fn make_store() -> Arc<Store> {
        Arc::new(Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap())
    }

    fn make_embed_service() -> Arc<EmbedServiceHandle> {
        EmbedServiceHandle::new()
    }

    fn make_dedup() -> CoAccessDedup {
        CoAccessDedup::new()
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
        // Should not panic
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

    // -- CoAccessDedup tests --

    #[test]
    fn coaccess_dedup_new_set_returns_true() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_dedup_duplicate_returns_false() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
        assert!(!dedup.check_and_insert("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_dedup_different_set_returns_true() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
        assert!(dedup.check_and_insert("s1", &[1, 2, 4]));
    }

    #[test]
    fn coaccess_dedup_different_session_returns_true() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
        assert!(dedup.check_and_insert("s2", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_dedup_clear_session() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
        dedup.clear_session("s1");
        // After clear, same set is considered new again
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_dedup_canonical_ordering() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[3, 1, 2]));
        // Same set in different order should be a duplicate
        assert!(!dedup.check_and_insert("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_dedup_clear_only_affects_target_session() {
        let dedup = make_dedup();
        assert!(dedup.check_and_insert("s1", &[1, 2]));
        assert!(dedup.check_and_insert("s2", &[1, 2]));
        dedup.clear_session("s1");
        // s1 cleared, s2 should still be a duplicate
        assert!(dedup.check_and_insert("s1", &[1, 2])); // new for s1
        assert!(!dedup.check_and_insert("s2", &[1, 2])); // still dup for s2
    }

    // -- Dispatch tests (async per ADR-002) --

    // Note: These tests cannot create full embed/vector/entry stores for
    // ContextSearch testing (that requires a full server setup). They verify
    // that the async migration doesn't break existing handlers.

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = make_dedup();
        // We need dummy vector/entry stores -- use the store adapter pattern
        // For dispatch tests of non-ContextSearch handlers, these are never accessed
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let response = dispatch_request(
            HookRequest::Ping,
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        match response {
            HookResponse::Pong { server_version } => {
                assert_eq!(server_version, "0.1.0");
            }
            _ => panic!("expected Pong"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_register_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = make_dedup();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let response = dispatch_request(
            HookRequest::SessionRegister {
                session_id: "s1".to_string(),
                cwd: "/work".to_string(),
                agent_role: None,
                feature: None,
            },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));
    }

    #[tokio::test]
    async fn dispatch_session_close_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = make_dedup();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let response = dispatch_request(
            HookRequest::SessionClose {
                session_id: "s1".to_string(),
                outcome: Some("success".to_string()),
                duration_secs: 60,
            },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));
    }

    #[tokio::test]
    async fn dispatch_record_event_returns_ack() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = make_dedup();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let event = ImplantEvent {
            event_type: "test".to_string(),
            session_id: "s1".to_string(),
            timestamp: 0,
            payload: serde_json::json!({}),
        };
        let response = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));
    }

    #[tokio::test]
    async fn dispatch_unknown_returns_error() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = make_dedup();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let response = dispatch_request(
            HookRequest::Briefing {
                role: "dev".to_string(),
                task: "test".to_string(),
                feature: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        match response {
            HookResponse::Error { code, .. } => {
                assert_eq!(code, ERR_UNKNOWN_REQUEST);
            }
            _ => panic!("expected Error"),
        }
    }

    #[tokio::test]
    async fn dispatch_context_search_embed_not_ready() {
        let store = make_store();
        let embed = make_embed_service(); // Not started -- will return EmbedNotReady
        let dedup = make_dedup();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        let response = dispatch_request(
            HookRequest::ContextSearch {
                query: "test".to_string(),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;
        match response {
            HookResponse::Entries { items, total_tokens } => {
                assert!(items.is_empty(), "expected empty items for EmbedNotReady");
                assert_eq!(total_tokens, 0);
            }
            _ => panic!("expected Entries, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_close_clears_dedup() {
        let store = make_store();
        let embed = make_embed_service();
        let dedup = CoAccessDedup::new();
        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(&store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let adapt_service = Arc::new(AdaptationService::new(unimatrix_adapt::AdaptConfig::default()));

        // Insert some dedup state
        dedup.check_and_insert("s1", &[1, 2, 3]);

        // Dispatch SessionClose
        let _ = dispatch_request(
            HookRequest::SessionClose {
                session_id: "s1".to_string(),
                outcome: None,
                duration_secs: 0,
            },
            &store,
            &embed,
            &async_vector_store,
            &async_entry_store,
            &adapt_service,
            "0.1.0",
            &dedup,
        )
        .await;

        // After clear, same set should be considered new
        assert!(dedup.check_and_insert("s1", &[1, 2, 3]));
    }
}
