//! Unix domain socket listener for hook IPC.
//!
//! Accepts connections from hook processes, authenticates them via peer
//! credentials (Layer 2: UID verification), dispatches requests, and
//! returns responses. Integrates into server startup/shutdown.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use unimatrix_adapt::AdaptationService;
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{EmbedService, NewEntry, Status, StoreAdapter, VectorAdapter};
use unimatrix_engine::auth;
use unimatrix_engine::coaccess::generate_pairs;
use unimatrix_engine::confidence::rerank_score;
use unimatrix_engine::wire::{
    EntryPayload, HookRequest, HookResponse, ERR_INVALID_PAYLOAD, MAX_PAYLOAD_SIZE,
};
use unimatrix_store::Store;
use unimatrix_store::{InjectionLogRecord, SessionLifecycleStatus, SessionRecord, SignalRecord, SignalType, SignalSource};

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::registry::Capability;
use crate::server::PendingEntriesAnalysis;
use crate::infra::session::{ReworkEvent, SessionOutcome, SessionRegistry, SignalOutput};
use crate::uds::uds_has_capability;

// -- col-010 helpers --

/// Current unix timestamp in seconds.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Validate session_id format: [a-zA-Z0-9-_], max 128 chars. (FR-04, SEC-01)
fn sanitize_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("session_id must not be empty".to_string());
    }
    if session_id.len() > 128 {
        return Err("session_id too long (max 128 chars)".to_string());
    }
    for ch in session_id.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
            return Err(format!("session_id contains invalid character: {:?}", ch));
        }
    }
    Ok(())
}

/// Sanitize a metadata field: strip non-printable ASCII, truncate to 128 chars. (SEC-02)
fn sanitize_metadata_field(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .take(128)
        .collect()
}

/// Fire-and-forget `spawn_blocking`. The returned JoinHandle is dropped.
fn spawn_blocking_fire_and_forget<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let _ = tokio::task::spawn_blocking(f);
}

/// Minimum cosine similarity for injection candidates.
const SIMILARITY_FLOOR: f64 = 0.5;

/// Minimum confidence score for injection candidates.
const CONFIDENCE_FLOOR: f64 = 0.3;

/// Maximum number of entries to search for in injection.
const INJECTION_K: usize = 5;

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
    pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    server_uid: u32,
    server_version: String,
    services: crate::services::ServiceLayer,
    audit_log: Arc<AuditLog>,
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
            pending_entries_analysis,
            server_uid,
            server_version,
            socket_path_display,
            services,
            audit_log,
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
    pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
    services: crate::services::ServiceLayer,
    audit_log: Arc<AuditLog>,
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
                let pending_entries_analysis = Arc::clone(&pending_entries_analysis);
                let version = server_version.clone();
                let services = services.clone();
                let audit_log = Arc::clone(&audit_log);

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
                        pending_entries_analysis,
                        server_uid,
                        version,
                        services,
                        audit_log,
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
    pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    server_uid: u32,
    server_version: String,
    services: crate::services::ServiceLayer,
    audit_log: Arc<AuditLog>,
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

            // F-23: Audit auth failure (fire-and-forget)
            let audit_log = Arc::clone(&audit_log);
            let error_msg = format!("{e}");
            let _ = tokio::task::spawn_blocking(move || {
                let event = AuditEvent {
                    event_id: 0,
                    timestamp: 0,
                    session_id: String::new(),
                    agent_id: "uds-auth".to_string(),
                    operation: "uds_auth_failure".to_string(),
                    target_ids: vec![],
                    outcome: Outcome::Error,
                    detail: error_msg,
                };
                if let Err(write_err) = audit_log.log_event(event) {
                    tracing::warn!(error = %write_err, "failed to write auth failure audit");
                }
            });
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
        &pending_entries_analysis,
        &services,
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
    _vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    _adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    session_registry: &SessionRegistry,
    pending_entries_analysis: &Arc<Mutex<PendingEntriesAnalysis>>,
    services: &crate::services::ServiceLayer,
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
            // vnc-008: UDS capability enforcement
            if !uds_has_capability(Capability::SessionWrite) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: SessionWrite required".to_string(),
                };
            }
            // col-010: Validate session_id before any writes (SEC-01)
            if let Err(e) = sanitize_session_id(&session_id) {
                tracing::warn!(session_id, error = %e, "UDS: SessionRegister rejected: invalid session_id");
                return HookResponse::Error {
                    code: ERR_INVALID_PAYLOAD,
                    message: e,
                };
            }

            // col-010: Sanitize metadata fields (SEC-02)
            let clean_role: Option<String> = agent_role.as_deref().map(sanitize_metadata_field);
            let clean_feature: Option<String> = feature.as_deref().map(sanitize_metadata_field);

            tracing::info!(
                session_id,
                cwd,
                agent_role = ?clean_role,
                feature = ?clean_feature,
                "UDS: session registered"
            );

            // Register session in registry (col-008)
            session_registry.register_session(&session_id, clean_role.clone(), clean_feature.clone());

            // col-010: Persist SessionRecord to SESSIONS table (fire-and-forget)
            {
                let record = SessionRecord {
                    session_id: session_id.clone(),
                    feature_cycle: clean_feature,
                    agent_role: clean_role,
                    started_at: unix_now_secs(),
                    ended_at: None,
                    status: SessionLifecycleStatus::Active,
                    compaction_count: 0,
                    outcome: None,
                    total_injections: 0,
                };
                let store_clone = Arc::clone(store);
                spawn_blocking_fire_and_forget(move || {
                    if let Err(e) = store_clone.insert_session(&record) {
                        tracing::warn!(
                            session_id = %record.session_id,
                            error = %e,
                            "UDS: SESSIONS insert failed"
                        );
                    }
                });
            }

            // Pre-warm embedding model (FR-04)
            warm_embedding_model(embed_service).await;

            HookResponse::Ack
        }

        HookRequest::SessionClose {
            session_id,
            outcome,
            duration_secs,
        } => {
            if !uds_has_capability(Capability::SessionWrite) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: SessionWrite required".to_string(),
                };
            }
            if let Err(e) = sanitize_session_id(&session_id) {
                tracing::warn!(session_id, error = %e, "UDS: SessionClose rejected: invalid session_id");
                return HookResponse::Error {
                    code: ERR_INVALID_PAYLOAD,
                    message: e,
                };
            }

            tracing::info!(
                session_id,
                outcome = ?outcome,
                duration_secs,
                "UDS: session closed"
            );

            let hook_outcome = outcome.as_deref().unwrap_or("");
            process_session_close(
                &session_id,
                hook_outcome,
                store,
                session_registry,
                entry_store,
                pending_entries_analysis,
            )
            .await
        }

        // col-009: Rework candidate events from PostToolUse hook
        HookRequest::RecordEvent { ref event }
            if event.event_type == "post_tool_use_rework_candidate" =>
        {
            if !uds_has_capability(Capability::SessionWrite) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: SessionWrite required".to_string(),
                };
            }
            let tool_name = event
                .payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let file_path = event
                .payload
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let had_failure = event
                .payload
                .get("had_failure")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let rework_event = ReworkEvent {
                tool_name,
                file_path,
                had_failure,
                timestamp: event.timestamp,
            };

            session_registry.record_rework_event(&event.session_id, rework_event);
            HookResponse::Ack
        }

        HookRequest::RecordEvent { event } => {
            if !uds_has_capability(Capability::SessionWrite) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: SessionWrite required".to_string(),
                };
            }
            tracing::info!(
                event_type = event.event_type,
                session_id = event.session_id,
                "UDS: event recorded"
            );
            HookResponse::Ack
        }

        HookRequest::RecordEvents { events } => {
            if !uds_has_capability(Capability::SessionWrite) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: SessionWrite required".to_string(),
                };
            }
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
            if !uds_has_capability(Capability::Search) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: Search required".to_string(),
                };
            }
            if let Some(ref sid) = session_id {
                if let Err(e) = sanitize_session_id(sid) {
                    tracing::warn!(session_id = sid, error = %e, "UDS: ContextSearch rejected: invalid session_id");
                    return HookResponse::Error {
                        code: ERR_INVALID_PAYLOAD,
                        message: e,
                    };
                }
            }

            handle_context_search(
                query,
                session_id,
                k,
                store,
                session_registry,
                services,
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
            if !uds_has_capability(Capability::Search) || !uds_has_capability(Capability::Read) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: Search + Read required".to_string(),
                };
            }
            handle_compact_payload(
                &session_id,
                role,
                feature,
                token_limit,
                session_registry,
                services,
            )
            .await
        }

        HookRequest::Briefing {
            role,
            task,
            feature,
            max_tokens,
        } => {
            if !uds_has_capability(Capability::Search) || !uds_has_capability(Capability::Read) {
                return HookResponse::Error {
                    code: -32003,
                    message: "insufficient capability: Search + Read required".to_string(),
                };
            }
            let audit_ctx = crate::services::AuditContext {
                source: crate::services::AuditSource::Uds {
                    uid: 0,
                    pid: None,
                    session_id: String::new(),
                },
                caller_id: "uds-briefing".to_string(),
                session_id: None,
                feature_cycle: None,
            };

            let effective_max_tokens = max_tokens
                .map(|v| v as usize)
                .unwrap_or(3000);

            let briefing_params = crate::services::briefing::BriefingParams {
                role: Some(role),
                task: Some(task),
                feature,
                max_tokens: effective_max_tokens,
                include_conventions: true,
                include_semantic: true,
                injection_history: None,
            };

            match services.briefing.assemble(briefing_params, &audit_ctx, None).await {
                Ok(result) => {
                    let mut content = String::new();
                    if !result.conventions.is_empty() {
                        content.push_str("## Conventions\n");
                        for entry in &result.conventions {
                            content.push_str(&format!("- {}: {}\n", entry.title, entry.content));
                        }
                        content.push('\n');
                    }
                    if !result.relevant_context.is_empty() {
                        content.push_str("## Relevant Context\n");
                        for (entry, score) in &result.relevant_context {
                            content.push_str(&format!(
                                "- {} ({:.2}): {}\n",
                                entry.title, score, entry.content
                            ));
                        }
                    }
                    let token_count = (content.len() / 4) as u32;
                    HookResponse::BriefingContent { content, token_count }
                }
                Err(e) => HookResponse::Error {
                    code: ERR_INVALID_PAYLOAD,
                    message: format!("briefing failed: {e}"),
                },
            }
        }
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
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
    let k = k.map(|v| v as usize).unwrap_or(INJECTION_K);

    // 1. Build AuditContext (UDS transport)
    let audit_ctx = crate::services::AuditContext {
        source: crate::services::AuditSource::Uds {
            uid: 0,
            pid: None,
            session_id: session_id.clone().unwrap_or_default(),
        },
        caller_id: "uds".to_string(),
        session_id: session_id.clone(),
        feature_cycle: None,
    };

    // 2. Build ServiceSearchParams with UDS-specific floors
    let service_params = crate::services::ServiceSearchParams {
        query: query.clone(),
        k,
        filters: None, // UDS doesn't pass metadata filters
        similarity_floor: Some(SIMILARITY_FLOOR),
        confidence_floor: Some(CONFIDENCE_FLOOR),
        feature_tag: None,
        co_access_anchors: None,
        caller_agent_id: None,
    };

    // 3. Delegate to SearchService (UDS sessions are rate-exempt via CallerId::UdsSession)
    let uds_caller = crate::services::CallerId::UdsSession(
        session_id.clone().unwrap_or_else(|| "uds-anon".to_string()),
    );
    let search_results = match services.search.search(service_params, &audit_ctx, &uds_caller).await {
        Ok(results) => results,
        Err(e) => {
            tracing::warn!("search service error: {e}");
            return HookResponse::Entries {
                items: vec![],
                total_tokens: 0,
            };
        }
    };

    // 4. Convert SearchResults to filtered (entry, similarity) pairs
    let filtered: Vec<_> = search_results
        .entries
        .iter()
        .map(|se| (se.entry.clone(), se.similarity))
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

    // 10b. col-010: Persist injection log batch to INJECTION_LOG (fire-and-forget, ADR-003)
    if let Some(ref sid) = session_id {
        if !sid.is_empty() && !filtered.is_empty() {
            let now = unix_now_secs();
            let records: Vec<InjectionLogRecord> = filtered
                .iter()
                .map(|(entry, sim)| InjectionLogRecord {
                    log_id: 0, // allocated by insert_injection_log_batch
                    session_id: sid.clone(),
                    entry_id: entry.id,
                    confidence: rerank_score(*sim, entry.confidence),
                    timestamp: now,
                })
                .collect();
            let store_clone = Arc::clone(store);
            let sid_clone = sid.clone();
            spawn_blocking_fire_and_forget(move || {
                if let Err(e) = store_clone.insert_injection_log_batch(&records) {
                    tracing::warn!(
                        session_id = %sid_clone,
                        count = records.len(),
                        error = %e,
                        "INJECTION_LOG batch write failed"
                    );
                }
            });
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
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
    // 1. Byte/token budget (transport concern)
    let max_bytes = match token_limit {
        Some(limit) => ((limit as usize) * 4).min(MAX_COMPACTION_BYTES),
        None => MAX_COMPACTION_BYTES,
    };
    let max_tokens = max_bytes / 4;

    // 2. Session state resolution (transport concern)
    let session_state = session_registry.get_state(session_id);
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

    // 3. Determine path
    let has_injection_history = session_state
        .as_ref()
        .is_some_and(|s| !s.injection_history.is_empty());

    // 4. Build AuditContext (transport-specific)
    let audit_ctx = crate::services::AuditContext {
        source: crate::services::AuditSource::Uds {
            uid: 0,
            pid: None,
            session_id: session_id.to_string(),
        },
        caller_id: "uds-compact".to_string(),
        session_id: Some(session_id.to_string()),
        feature_cycle: None,
    };

    // 5. Build injection history from session state
    let injection_history = if has_injection_history {
        let session = session_state.as_ref().unwrap();
        Some(
            session
                .injection_history
                .iter()
                .map(|r| crate::services::briefing::InjectionEntry {
                    entry_id: r.entry_id,
                    confidence: r.confidence,
                })
                .collect(),
        )
    } else {
        None
    };

    // 6. Build BriefingParams — include_semantic=false (no embedding on compact path)
    let briefing_params = crate::services::briefing::BriefingParams {
        role: effective_role.clone(),
        task: None,
        feature: effective_feature.clone(),
        max_tokens,
        include_conventions: !has_injection_history, // fallback includes conventions
        include_semantic: false, // CRITICAL: no embedding, no vector search
        injection_history,
    };

    // 7. Delegate to BriefingService
    let result = match services.briefing.assemble(briefing_params, &audit_ctx, None).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("compact payload assembly failed: {e}");
            return HookResponse::BriefingContent {
                content: String::new(),
                token_count: 0,
            };
        }
    };

    // 8. Convert BriefingResult to CompactionCategories for formatting
    let categories = CompactionCategories {
        decisions: result.injection_sections.decisions,
        injections: result.injection_sections.injections,
        conventions: if has_injection_history {
            result.injection_sections.conventions
        } else {
            // Fallback path: conventions from BriefingResult.conventions
            result
                .conventions
                .into_iter()
                .map(|e| {
                    let c = e.confidence;
                    (e, c)
                })
                .collect()
        },
    };

    // 9. Format payload (transport-specific formatting)
    let content = format_compaction_payload(
        &categories,
        effective_role.as_deref(),
        effective_feature.as_deref(),
        compaction_count,
        max_bytes,
    );

    // 10. Increment compaction count (transport concern)
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

// -- col-009: Signal dispatch helpers --

/// Process session close: sweep stale sessions, generate signals, run consumers.
///
/// Never panics. Always returns HookResponse::Ack.
async fn process_session_close(
    session_id: &str,
    hook_outcome: &str,
    store: &Arc<Store>,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> HookResponse {
    // col-010: capture session metadata before drain (state is removed by drain)
    let (feature_cycle, agent_role, injection_count, compaction_count) = {
        if let Some(state) = session_registry.get_state(session_id) {
            (
                state.feature.clone(),
                state.role.clone(),
                state.injection_history.len() as u32,
                state.compaction_count,
            )
        } else {
            (None, None, 0u32, 0u32)
        }
    };

    // Step 1: Sweep stale sessions first (FR-09.1)
    let stale_outputs = session_registry.sweep_stale_sessions();
    for (stale_session_id, stale_output) in stale_outputs {
        tracing::info!(session_id = %stale_session_id, "UDS: sweeping stale session");
        write_signals_to_queue(&stale_output, store).await;
    }

    // Step 2: Generate signals for the closing session (atomic — ADR-003)
    let maybe_output = session_registry.drain_and_signal_session(session_id, hook_outcome);

    if let Some(ref output) = maybe_output {
        // col-010: resolve final status and outcome string
        let (final_status, outcome_str) = match output.final_outcome {
            SessionOutcome::Success  => (SessionLifecycleStatus::Completed, "success"),
            SessionOutcome::Rework   => (SessionLifecycleStatus::Completed, "rework"),
            SessionOutcome::Abandoned => (SessionLifecycleStatus::Abandoned, "abandoned"),
        };
        let is_abandoned = final_status == SessionLifecycleStatus::Abandoned;

        // col-010: update SESSIONS record (fire-and-forget)
        {
            let sid = session_id.to_string();
            let store_clone = Arc::clone(store);
            let status_clone = final_status.clone();
            let outcome_owned = outcome_str.to_string();
            spawn_blocking_fire_and_forget(move || {
                let result = store_clone.update_session(&sid, |r| {
                    r.status = status_clone;
                    r.ended_at = Some(unix_now_secs());
                    r.outcome = Some(outcome_owned.clone());
                    r.total_injections = injection_count;
                    r.compaction_count = compaction_count;
                });
                if let Err(e) = result {
                    tracing::warn!(
                        session_id = %sid,
                        error = %e,
                        "UDS: SESSIONS update failed"
                    );
                }
            });
        }

        // col-010: write auto-outcome entry if session had injections and was not abandoned
        if !is_abandoned && injection_count > 0 {
            write_auto_outcome_entry(
                store,
                session_id,
                outcome_str,
                injection_count,
                feature_cycle.as_deref(),
                agent_role.as_deref(),
            );
        }

        // Step 3: Write signals to SIGNAL_QUEUE
        write_signals_to_queue(output, store).await;

        // Step 4: Run consumers (after queue is written)
        run_confidence_consumer(store, entry_store, pending).await;
        run_retrospective_consumer(store, pending, entry_store).await;
    }
    // If session absent (already cleared): no-op (idempotent — AC-03)

    HookResponse::Ack
}

/// Write an auto-generated outcome entry for a session that completed with injections.
///
/// Called from process_session_close when `final_status != Abandoned && injection_count > 0`.
/// Fire-and-forget: spawns a blocking task; never awaits the result.
fn write_auto_outcome_entry(
    store: &Arc<Store>,
    session_id: &str,
    outcome_str: &str,   // "success" | "rework"
    injection_count: u32,
    feature_cycle: Option<&str>,
    agent_role: Option<&str>,
) {
    let content = format!(
        "Session {} completed with outcome: {}. Injected {} entries.",
        session_id, outcome_str, injection_count
    );
    let result_tag = if outcome_str == "success" {
        "result:pass"
    } else {
        "result:rework"
    };
    let tags = vec!["type:session".to_string(), result_tag.to_string()];

    let _ = agent_role; // metadata available for future enrichment; not used in content
    let entry = NewEntry {
        title: format!("Session outcome: {}", session_id),
        content,
        topic: format!("session/{}", session_id),
        category: "outcome".to_string(),
        tags,
        source: "hook".to_string(),
        status: Status::Active,
        created_by: "cortical-implant".to_string(),
        feature_cycle: feature_cycle.unwrap_or("").to_string(),
        trust_source: "system".to_string(),
    };

    let store_clone = Arc::clone(store);
    let sid = session_id.to_string();
    spawn_blocking_fire_and_forget(move || {
        match store_clone.insert(entry) {
            Ok(entry_id) => {
                tracing::debug!(
                    session_id = %sid,
                    entry_id = %entry_id,
                    "Auto-outcome entry written"
                );
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %sid,
                    error = %e,
                    "Auto-outcome write failed"
                );
            }
        }
    });
}

/// Write a SignalRecord to SIGNAL_QUEUE for the given SignalOutput.
///
/// Only writes if there are entry_ids to signal (FR-04.6).
pub(crate) async fn write_signals_to_queue(output: &SignalOutput, store: &Arc<Store>) {
    let (entry_ids, signal_type, signal_source) = match output.final_outcome {
        SessionOutcome::Success if !output.helpful_entry_ids.is_empty() => (
            output.helpful_entry_ids.clone(),
            SignalType::Helpful,
            SignalSource::ImplicitOutcome,
        ),
        SessionOutcome::Rework if !output.flagged_entry_ids.is_empty() => (
            output.flagged_entry_ids.clone(),
            SignalType::Flagged,
            SignalSource::ImplicitRework,
        ),
        _ => return, // No entries to signal (abandoned or empty)
    };

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let record = SignalRecord {
        signal_id: 0, // Allocated by insert_signal
        session_id: output.session_id.clone(),
        created_at,
        entry_ids,
        signal_type,
        signal_source,
    };

    if let Err(e) = store.insert_signal(&record) {
        tracing::warn!(
            session_id = %output.session_id,
            error = %e,
            "write_signals_to_queue: failed to insert signal"
        );
    }
}

/// Drain Helpful signals from SIGNAL_QUEUE and apply helpful_count increments.
///
/// Also updates success_session_count in PendingEntriesAnalysis (FR-06.2b).
pub(crate) async fn run_confidence_consumer(
    store: &Arc<Store>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
) {
    // Step 1: Drain all Helpful signals in one transaction
    let signals = match store.drain_signals(SignalType::Helpful) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "run_confidence_consumer: drain_signals failed");
            return;
        }
    };

    if signals.is_empty() {
        return;
    }

    // Step 2: Deduplicate entry_ids across all drained signals
    let mut all_entry_ids: HashSet<u64> = HashSet::new();
    for signal in &signals {
        for &entry_id in &signal.entry_ids {
            all_entry_ids.insert(entry_id);
        }
    }

    // Step 3: Increment helpful_count for all unique entries (via crt-002 path)
    // Uses spawn_blocking since record_usage_with_confidence is synchronous.
    let entry_ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect();
    let store_clone = Arc::clone(store);
    if let Err(e) = tokio::task::spawn_blocking(move || {
        store_clone.record_usage_with_confidence(
            &entry_ids_vec,
            &[],            // access_ids: no access count bump for implicit signals
            &entry_ids_vec, // helpful_ids: all signal entries
            &[],
            &[],
            &[],
            None,
        )
    })
    .await
    {
        // spawn_blocking join error — warn and continue
        tracing::warn!(error = %e, "run_confidence_consumer: spawn_blocking failed");
    }
    // Note: per-entry failures (entry deleted) are handled inside record_usage_with_confidence
    // by skipping entries that no longer exist.

    // Step 4: Update success_session_count in PendingEntriesAnalysis (FR-06.2b)
    // First pass: update existing entries under lock
    let entries_needing_fetch: Vec<u64> = {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        let mut needing_fetch = Vec::new();
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if let Some(existing) = pending_guard.entries.get_mut(&entry_id) {
                    existing.success_session_count += 1;
                } else {
                    needing_fetch.push(entry_id);
                }
            }
        }
        needing_fetch
    };

    // Second pass: fetch metadata for new entries (outside lock — async I/O)
    let mut fetched: std::collections::HashMap<u64, (String, String)> = std::collections::HashMap::new();
    for entry_id in &entries_needing_fetch {
        let (title, category) = match entry_store.get(*entry_id).await {
            Ok(record) => (record.title.clone(), record.category.clone()),
            Err(_) => (String::new(), String::new()),
        };
        fetched.insert(*entry_id, (title, category));
    }

    // Third pass: insert new entries (back under lock)
    if !fetched.is_empty() {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for (entry_id, (title, category)) in fetched {
            if let Some(existing) = pending_guard.entries.get_mut(&entry_id) {
                // Added between our first pass and now
                existing.success_session_count += 1;
            } else {
                let analysis = unimatrix_observe::EntryAnalysis {
                    entry_id,
                    title,
                    category,
                    rework_flag_count: 0,
                    injection_count: 0,
                    success_session_count: 1,
                    rework_session_count: 0,
                };
                pending_guard.upsert(analysis);
            }
        }
    }
}

/// Drain Flagged signals from SIGNAL_QUEUE and update PendingEntriesAnalysis.
pub(crate) async fn run_retrospective_consumer(
    store: &Arc<Store>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
) {
    // Step 1: Drain all Flagged signals
    let signals = match store.drain_signals(SignalType::Flagged) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "run_retrospective_consumer: drain_signals failed");
            return;
        }
    };

    if signals.is_empty() {
        return;
    }

    // Step 2: Collect entry_ids not yet in PendingEntriesAnalysis (outside lock)
    let entries_needing_fetch: Vec<u64> = {
        let pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        signals
            .iter()
            .flat_map(|s| s.entry_ids.iter().copied())
            .filter(|id| !pending_guard.entries.contains_key(id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    };

    // Step 3: Fetch metadata for new entries (outside lock — async I/O)
    let mut fetched: std::collections::HashMap<u64, (String, String)> = std::collections::HashMap::new();
    for entry_id in entries_needing_fetch {
        let (title, category) = match entry_store.get(entry_id).await {
            Ok(record) => (record.title.clone(), record.category.clone()),
            Err(_) => (String::new(), String::new()),
        };
        fetched.insert(entry_id, (title, category));
    }

    // Step 4: Apply updates to PendingEntriesAnalysis (under lock)
    {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if let Some(existing) = pending_guard.entries.get_mut(&entry_id) {
                    existing.rework_flag_count += 1;
                    existing.rework_session_count += 1;
                } else {
                    let (title, category) = fetched
                        .get(&entry_id)
                        .cloned()
                        .unwrap_or_default();
                    let analysis = unimatrix_observe::EntryAnalysis {
                        entry_id,
                        title,
                        category,
                        rework_flag_count: 1,
                        injection_count: 0,
                        success_session_count: 0,
                        rework_session_count: 1,
                    };
                    pending_guard.upsert(analysis);
                }
            }
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

    fn make_pending() -> Arc<Mutex<PendingEntriesAnalysis>> {
        Arc::new(Mutex::new(PendingEntriesAnalysis::new()))
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

    fn make_services(
        store: &Arc<Store>,
        embed: &Arc<EmbedServiceHandle>,
        vs: &Arc<AsyncVectorStore<VectorAdapter>>,
        es: &Arc<AsyncEntryStore<StoreAdapter>>,
        adapt: &Arc<AdaptationService>,
    ) -> crate::services::ServiceLayer {
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(
                Arc::clone(store),
                unimatrix_core::VectorConfig::default(),
            )
            .unwrap(),
        );
        let audit = Arc::new(crate::infra::audit::AuditLog::new(Arc::clone(store)));
        let usage_dedup = Arc::new(crate::infra::usage_dedup::UsageDedup::new());
        crate::services::ServiceLayer::new(
            Arc::clone(store),
            vector_index,
            Arc::clone(vs),
            Arc::clone(es),
            Arc::clone(embed),
            Arc::clone(adapt),
            audit,
            usage_dedup,
        )
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
        ).await;
        assert!(matches!(response, HookResponse::Ack));
    }

    // vnc-007: HookRequest::Briefing is now handled (returns BriefingContent).
    // The previous test sent Briefing and expected ERR_UNKNOWN_REQUEST. Now all
    // variants are handled, so this test verifies Briefing returns BriefingContent.
    #[tokio::test]
    async fn dispatch_briefing_returns_content() {
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
        ).await;
        match response {
            HookResponse::BriefingContent { .. } => {}
            other => panic!("expected BriefingContent, got {other:?}"),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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
            &store, &embed, &vs, &es, &adapt, "0.1.0", &registry, &make_pending(), &make_services(&store, &embed, &vs, &es, &adapt),
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

    // -- col-010: sanitize_session_id tests (R-11, SEC-01) --

    #[test]
    fn sanitize_session_id_valid_alphanumeric() {
        assert!(sanitize_session_id("session123").is_ok());
    }

    #[test]
    fn sanitize_session_id_valid_with_dash_underscore() {
        assert!(sanitize_session_id("sess-abc_XYZ-01").is_ok());
    }

    #[test]
    fn sanitize_session_id_valid_128_chars() {
        let id = "a".repeat(128);
        assert!(sanitize_session_id(&id).is_ok());
    }

    #[test]
    fn sanitize_session_id_rejects_too_long() {
        let id = "a".repeat(129);
        let err = sanitize_session_id(&id).unwrap_err();
        assert!(err.contains("too long"), "expected 'too long', got: {err}");
    }

    #[test]
    fn sanitize_session_id_rejects_exclamation() {
        let err = sanitize_session_id("abc!def").unwrap_err();
        assert!(err.contains("invalid character"), "expected 'invalid character', got: {err}");
    }

    #[test]
    fn sanitize_session_id_rejects_space() {
        assert!(sanitize_session_id("hello world").is_err());
    }

    #[test]
    fn sanitize_session_id_rejects_slash() {
        assert!(sanitize_session_id("path/to/session").is_err());
    }

    #[test]
    fn sanitize_session_id_rejects_dot() {
        assert!(sanitize_session_id("sess.ion").is_err());
    }

    #[test]
    fn sanitize_session_id_rejects_empty() {
        let err = sanitize_session_id("").unwrap_err();
        assert!(err.contains("must not be empty"), "expected 'must not be empty', got: {err}");
    }

    // -- col-010: sanitize_metadata_field tests (SEC-02) --

    #[test]
    fn sanitize_metadata_field_passes_printable_ascii() {
        assert_eq!(sanitize_metadata_field("uni-rust-dev"), "uni-rust-dev");
    }

    #[test]
    fn sanitize_metadata_field_strips_control_chars() {
        let input = "abc\x00\x01\x1Fdef";
        assert_eq!(sanitize_metadata_field(input), "abcdef");
    }

    #[test]
    fn sanitize_metadata_field_truncates_at_128() {
        let input = "a".repeat(200);
        assert_eq!(sanitize_metadata_field(&input).len(), 128);
    }

    #[test]
    fn sanitize_metadata_field_strips_newline() {
        assert_eq!(sanitize_metadata_field("line1\nline2"), "line1line2");
    }
}
