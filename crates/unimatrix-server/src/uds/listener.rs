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
use unimatrix_core::Store;
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{EmbedService, NewEntry, Status, StoreAdapter, VectorAdapter};
use unimatrix_engine::auth;
use unimatrix_engine::coaccess::generate_pairs;
use unimatrix_engine::confidence::rerank_score;
use unimatrix_engine::wire::{
    ERR_INVALID_PAYLOAD, EntryPayload, HookRequest, HookResponse, MAX_PAYLOAD_SIZE,
};
use unimatrix_store::{
    InjectionLogRecord, QueryLogRecord, SessionLifecycleStatus, SessionRecord, SignalRecord,
    SignalSource, SignalType,
};

// sqlx is used in insert_observation / insert_observations_batch for raw queries.
use sqlx;

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::registry::Capability;
use crate::infra::session::{
    ReworkEvent, SessionOutcome, SessionRegistry, SetFeatureResult, SignalOutput,
};
use crate::infra::validation::{CYCLE_START_EVENT, CYCLE_STOP_EVENT};
use crate::server::PendingEntriesAnalysis;
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
            session_registry.register_session(
                &session_id,
                clean_role.clone(),
                clean_feature.clone(),
            );

            // col-010: Persist SessionRecord to SESSIONS table
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
                    keywords: None,
                };
                // insert_session writes directly to the write pool for immediate visibility.
                if let Err(e) = store.insert_session(&record).await {
                    tracing::warn!("failed to persist session record: {e}");
                }
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

            // col-017: Accumulate topic signal from rework candidate events
            if let Some(ref signal) = event.topic_signal {
                session_registry.record_topic_signal(
                    &event.session_id,
                    signal.clone(),
                    event.timestamp,
                );
            }

            // col-019: Persist rework PostToolUse as observation (fire-and-forget)
            let store_for_obs = Arc::clone(store);
            let obs = extract_observation_fields(&event);
            tokio::task::spawn_blocking(move || {
                if let Err(e) = insert_observation(&store_for_obs, &obs) {
                    tracing::error!(error = %e, "rework observation write failed");
                }
            });

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

            // col-022: cycle_start gets force-set attribution + keywords persistence.
            // Must run BEFORE the generic #198 path so set_feature_if_absent becomes a no-op.
            // cycle_stop has no special handling -- falls through to generic observation persistence.
            if event.event_type == CYCLE_START_EVENT {
                handle_cycle_start(&event, session_registry, store);
            }

            // #198 Part 1: Extract explicit feature_cycle from event payload
            if let Some(fc) = event.payload.get("feature_cycle").and_then(|v| v.as_str()) {
                let fc_clean = sanitize_metadata_field(fc);
                if !fc_clean.is_empty()
                    && session_registry.set_feature_if_absent(&event.session_id, &fc_clean)
                {
                    tracing::info!(
                        session_id = %event.session_id,
                        feature_cycle = %fc_clean,
                        "#198: feature_cycle set from event payload"
                    );
                    let store_fc = Arc::clone(store);
                    let sid = event.session_id.clone();
                    let fc_owned = fc_clean;
                    let _ = tokio::spawn(async move {
                        if let Err(e) =
                            update_session_feature_cycle(&store_fc, &sid, &fc_owned).await
                        {
                            tracing::warn!(error = %e, "#198: feature_cycle persist failed");
                        }
                    });
                }
            }

            // col-017: Accumulate topic signal in session state
            if let Some(ref signal) = event.topic_signal {
                session_registry.record_topic_signal(
                    &event.session_id,
                    signal.clone(),
                    event.timestamp,
                );

                // #198 Part 2: Check eager attribution after signal accumulation
                if let Some(winner) = session_registry.check_eager_attribution(&event.session_id) {
                    if session_registry.set_feature_if_absent(&event.session_id, &winner) {
                        tracing::info!(
                            session_id = %event.session_id,
                            feature_cycle = %winner,
                            "#198: feature_cycle set via eager attribution"
                        );
                        let store_eager = Arc::clone(store);
                        let sid = event.session_id.clone();
                        let _ = tokio::spawn(async move {
                            if let Err(e) =
                                update_session_feature_cycle(&store_eager, &sid, &winner).await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "#198: eager attribution persist failed"
                                );
                            }
                        });
                    }
                }
            }

            // col-012: Persist observation to SQLite (fire-and-forget)
            let store_for_obs = Arc::clone(store);
            let obs = extract_observation_fields(&event);
            spawn_blocking_fire_and_forget(move || {
                if let Err(e) = insert_observation(&store_for_obs, &obs) {
                    tracing::error!(error = %e, "observation write failed");
                }
            });

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

            // #198 Part 1: Extract explicit feature_cycle from batch event payloads
            // Track which sessions got eager attribution to avoid redundant checks
            let mut eager_resolved: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for event in &events {
                if let Some(fc) = event.payload.get("feature_cycle").and_then(|v| v.as_str()) {
                    let fc_clean = sanitize_metadata_field(fc);
                    if !fc_clean.is_empty()
                        && session_registry.set_feature_if_absent(&event.session_id, &fc_clean)
                    {
                        tracing::info!(
                            session_id = %event.session_id,
                            feature_cycle = %fc_clean,
                            "#198: feature_cycle set from batch event payload"
                        );
                        let store_fc = Arc::clone(store);
                        let sid = event.session_id.clone();
                        let fc_owned = fc_clean;
                        let _ = tokio::spawn(async move {
                            if let Err(e) =
                                update_session_feature_cycle(&store_fc, &sid, &fc_owned).await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "#198: feature_cycle persist failed"
                                );
                            }
                        });
                        eager_resolved.insert(event.session_id.clone());
                    }
                }
            }

            // col-017: Accumulate topic signals for all events in batch
            for event in &events {
                if let Some(ref signal) = event.topic_signal {
                    session_registry.record_topic_signal(
                        &event.session_id,
                        signal.clone(),
                        event.timestamp,
                    );
                }
            }

            // #198 Part 2: Check eager attribution for sessions that accumulated signals
            // Collect unique session IDs that had topic signals
            let signal_sessions: std::collections::HashSet<&str> = events
                .iter()
                .filter(|e| e.topic_signal.is_some())
                .map(|e| e.session_id.as_str())
                .collect();
            for sid in signal_sessions {
                if eager_resolved.contains(sid) {
                    continue;
                }
                if let Some(winner) = session_registry.check_eager_attribution(sid) {
                    if session_registry.set_feature_if_absent(sid, &winner) {
                        tracing::info!(
                            session_id = %sid,
                            feature_cycle = %winner,
                            "#198: feature_cycle set via eager attribution (batch)"
                        );
                        let store_eager = Arc::clone(store);
                        let sid_owned = sid.to_string();
                        let _ = tokio::spawn(async move {
                            if let Err(e) =
                                update_session_feature_cycle(&store_eager, &sid_owned, &winner)
                                    .await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "#198: eager attribution persist failed"
                                );
                            }
                        });
                    }
                }
            }

            // col-012: Batch persist observations in single transaction (fire-and-forget)
            let store_for_obs = Arc::clone(store);
            let obs_batch: Vec<ObservationRow> =
                events.iter().map(extract_observation_fields).collect();
            spawn_blocking_fire_and_forget(move || {
                if let Err(e) = insert_observations_batch(&store_for_obs, &obs_batch) {
                    tracing::error!(error = %e, "batch observation write failed");
                }
            });

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

            // col-018: Record UserPromptSubmit observation as a side effect.
            // The hook maps non-empty UserPromptSubmit to ContextSearch; the
            // prompt is the richest topic signal but was previously unrecorded.
            let topic_signal = unimatrix_observe::extract_topic_signal(&query);

            if let Some(ref sid) = session_id {
                if !query.is_empty() {
                    if let Some(ref signal) = topic_signal {
                        session_registry.record_topic_signal(sid, signal.clone(), unix_now_secs());
                    }

                    let truncated_input: String = query.chars().take(4096).collect();
                    let obs = ObservationRow {
                        session_id: sid.clone(),
                        ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
                        hook: "UserPromptSubmit".to_string(),
                        tool: None,
                        input: Some(truncated_input),
                        response_size: None,
                        response_snippet: None,
                        topic_signal: topic_signal.clone(),
                    };

                    let store_for_obs = Arc::clone(store);
                    spawn_blocking_fire_and_forget(move || {
                        if let Err(e) = insert_observation(&store_for_obs, &obs) {
                            tracing::error!(error = %e, "col-018: UserPromptSubmit observation write failed");
                        }
                    });
                }
            }

            handle_context_search(query, session_id, k, store, session_registry, services).await
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

            let effective_max_tokens = max_tokens.map(|v| v as usize).unwrap_or(3000);

            let briefing_params = crate::services::briefing::BriefingParams {
                role: Some(role),
                task: Some(task),
                feature,
                max_tokens: effective_max_tokens,
                include_conventions: true,
                include_semantic: true,
                injection_history: None,
            };

            match services
                .briefing
                .assemble(briefing_params, &audit_ctx, None)
                .await
            {
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
                    HookResponse::BriefingContent {
                        content,
                        token_count,
                    }
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
        retrieval_mode: crate::services::RetrievalMode::Strict, // crt-010: UDS uses strict mode
    };

    // 3. Delegate to SearchService (UDS sessions are rate-exempt via CallerId::UdsSession)
    let uds_caller = crate::services::CallerId::UdsSession(
        session_id.clone().unwrap_or_else(|| "uds-anon".to_string()),
    );
    let search_results = match services
        .search
        .search(service_params, &audit_ctx, &uds_caller)
        .await
    {
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
            // crt-019: use adaptive confidence_weight from shared ConfidenceState (#258)
            let confidence_weight = {
                let handle = services.confidence_state_handle();
                let guard = handle.read().unwrap_or_else(|e| e.into_inner());
                guard.confidence_weight
            };
            let records: Vec<InjectionLogRecord> = filtered
                .iter()
                .map(|(entry, sim)| InjectionLogRecord {
                    log_id: 0, // allocated by insert_injection_log_batch
                    session_id: sid.clone(),
                    entry_id: entry.id,
                    confidence: rerank_score(*sim, entry.confidence, confidence_weight),
                    timestamp: now,
                })
                .collect();
            let store_clone = Arc::clone(store);
            spawn_blocking_fire_and_forget(move || {
                store_clone.insert_injection_log_batch(&records);
            });
        }
    }

    // 10c. nxs-010: Persist query_log row (fire-and-forget, ADR-002)
    if let Some(ref sid) = session_id {
        if !sid.is_empty() {
            let entry_ids: Vec<u64> = filtered.iter().map(|(e, _)| e.id).collect();
            let scores: Vec<f64> = filtered.iter().map(|(_, sim)| *sim).collect();

            let record = QueryLogRecord::new(
                sid.clone(),
                query.clone(),
                &entry_ids,
                &scores,
                "strict",
                "uds",
            );

            let store_clone = Arc::clone(store);
            spawn_blocking_fire_and_forget(move || {
                store_clone.insert_query_log(&record);
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
                spawn_blocking_fire_and_forget(move || {
                    store_clone.record_co_access_pairs(&pairs);
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
    let effective_role = session_state.as_ref().and_then(|s| s.role.clone()).or(role);
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
        include_semantic: false,                     // CRITICAL: no embedding, no vector search
        injection_history,
    };

    // 7. Delegate to BriefingService
    let result = match services
        .briefing
        .assemble(briefing_params, &audit_ctx, None)
        .await
    {
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

    let token_count = content.as_ref().map(|c| (c.len() / 4) as u32).unwrap_or(0);

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
    bytes_used += format_category_section(
        &mut output,
        "Decisions",
        &categories.decisions,
        decision_budget,
    );

    // Injections section
    let remaining = max_bytes.saturating_sub(bytes_used);
    let injection_budget = INJECTION_BUDGET_BYTES.min(remaining);
    bytes_used += format_category_section(
        &mut output,
        "Key Context",
        &categories.injections,
        injection_budget,
    );

    // Conventions section
    let remaining = max_bytes.saturating_sub(bytes_used);
    let convention_budget = CONVENTION_BUDGET_BYTES.min(remaining);
    let _ = format_category_section(
        &mut output,
        "Conventions",
        &categories.conventions,
        convention_budget,
    );

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
    // col-017: also capture topic_signals for majority vote resolution
    let (feature_cycle, agent_role, injection_count, compaction_count, topic_signals) = {
        if let Some(state) = session_registry.get_state(session_id) {
            (
                state.feature.clone(),
                state.role.clone(),
                state.injection_history.len() as u32,
                state.compaction_count,
                state.topic_signals.clone(),
            )
        } else {
            (None, None, 0u32, 0u32, std::collections::HashMap::new())
        }
    };

    // col-017: Resolve topic via majority vote before drain (FR-06.1)
    let resolved_topic = majority_vote(&topic_signals);

    // Step 1: Sweep stale sessions first (FR-09.1)
    // #198 Part 3: Sweep now resolves feature_cycle via majority vote before eviction
    let stale_outputs = session_registry.sweep_stale_sessions();
    for sweep_result in &stale_outputs {
        tracing::info!(session_id = %sweep_result.session_id, "UDS: sweeping stale session");
        // #198: Persist resolved feature_cycle for stale session
        if let Some(ref fc) = sweep_result.resolved_feature {
            let store_fc = Arc::clone(store);
            let sid = sweep_result.session_id.clone();
            let fc_owned = fc.clone();
            let _ = tokio::spawn(async move {
                if let Err(e) = update_session_feature_cycle(&store_fc, &sid, &fc_owned).await {
                    tracing::warn!(error = %e, "#198: stale session feature_cycle persist failed");
                }
            });
        }
        write_signals_to_queue(&sweep_result.output, store).await;
    }

    // Step 2: Generate signals for the closing session (atomic — ADR-003)
    let maybe_output = session_registry.drain_and_signal_session(session_id, hook_outcome);

    if let Some(ref output) = maybe_output {
        // col-010: resolve final status and outcome string
        let (final_status, outcome_str) = match output.final_outcome {
            SessionOutcome::Success => (SessionLifecycleStatus::Completed, "success"),
            SessionOutcome::Rework => (SessionLifecycleStatus::Completed, "rework"),
            SessionOutcome::Abandoned => (SessionLifecycleStatus::Abandoned, "abandoned"),
        };
        let is_abandoned = final_status == SessionLifecycleStatus::Abandoned;

        // col-017: Determine final feature_cycle — majority vote wins, else fallback to
        // content-based attribution, else use the registered feature from SessionStart.
        let final_feature_cycle = if let Some(ref topic) = resolved_topic {
            // Majority vote produced a result — use it
            tracing::info!(
                session_id,
                topic = %topic,
                "col-017: topic resolved via majority vote"
            );
            Some(topic.clone())
        } else {
            // No hook-side signals — fallback to content-based attribution (FR-06.2)
            let store_clone = Arc::clone(store);
            let sid = session_id.to_string();
            let fallback = tokio::task::spawn_blocking(move || {
                content_based_attribution_fallback(&store_clone, &sid)
            })
            .await
            .unwrap_or(None);

            if fallback.is_some() {
                tracing::info!(
                    session_id,
                    topic = ?fallback,
                    "col-017: topic resolved via content-based fallback"
                );
            }
            fallback.or(feature_cycle.clone())
        };

        // col-010: update SESSIONS record (fire-and-forget)
        // col-017: include resolved feature_cycle
        {
            let sid = session_id.to_string();
            let store_clone = Arc::clone(store);
            let status_clone = final_status.clone();
            let outcome_owned = outcome_str.to_string();
            let fc = final_feature_cycle.clone();
            let _ = tokio::spawn(async move {
                let result = store_clone
                    .update_session(&sid, |r| {
                        r.status = status_clone;
                        r.ended_at = Some(unix_now_secs());
                        r.outcome = Some(outcome_owned.clone());
                        r.total_injections = injection_count;
                        r.compaction_count = compaction_count;
                        if let Some(ref topic) = fc {
                            r.feature_cycle = Some(topic.clone());
                        }
                    })
                    .await;
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
                final_feature_cycle.as_deref().or(feature_cycle.as_deref()),
                agent_role.as_deref(),
            );
        }

        // Step 3: Write signals to SIGNAL_QUEUE
        write_signals_to_queue(output, store).await;

        // Step 4: Run consumers (after queue is written)
        // Pass the resolved feature_cycle so entries accumulate in the correct bucket.
        // An empty string is used for sessions without feature cycle attribution.
        let fc_key = final_feature_cycle.as_deref().unwrap_or("");
        run_confidence_consumer(store, entry_store, pending, fc_key).await;
        run_retrospective_consumer(store, pending, entry_store, fc_key).await;
    }
    // If session absent (already cleared): no-op (idempotent — AC-03)

    HookResponse::Ack
}

/// Content-based attribution fallback for SessionClose when no hook-side signals exist (col-017).
///
/// Loads observations for the session, runs `attribute_sessions` for each unique
/// candidate feature, returns the feature with the most attributed observations.
fn content_based_attribution_fallback(store: &Store, session_id: &str) -> Option<String> {
    use sqlx::Row as _;
    use unimatrix_observe::types::{HookType, ObservationRecord, ParsedSession};

    // Use block_in_place to bridge async sqlx into this sync context.
    // block_in_place works in both spawn_blocking and multi_thread test contexts.
    let pool = store.write_pool_server();
    let rows = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            sqlx::query(
                "SELECT session_id, ts_millis, hook, tool, input FROM observations \
                 WHERE session_id = ?1 ORDER BY ts_millis ASC",
            )
            .bind(session_id)
            .fetch_all(pool),
        )
    })
    .ok()?;

    let records: Vec<ObservationRecord> = rows
        .into_iter()
        .map(|row| {
            let session_id: String = row.get::<String, _>(0);
            let ts_millis: i64 = row.get::<i64, _>(1);
            let hook_str: String = row.get::<String, _>(2);
            let tool: Option<String> = row.get::<Option<String>, _>(3);
            let input_str: Option<String> = row.get::<Option<String>, _>(4);
            ObservationRecord {
                ts: (ts_millis / 1000) as u64,
                hook: match hook_str.as_str() {
                    "PreToolUse" => HookType::PreToolUse,
                    "PostToolUse" => HookType::PostToolUse,
                    "SubagentStart" => HookType::SubagentStart,
                    "SubagentStop" => HookType::SubagentStop,
                    _ => HookType::PreToolUse,
                },
                session_id,
                tool,
                input: input_str.map(serde_json::Value::String),
                response_size: None,
                response_snippet: None,
            }
        })
        .collect();

    if records.is_empty() {
        return None;
    }

    // Extract all unique candidate features from the records (no DB access needed)
    let mut candidates: std::collections::HashSet<String> = std::collections::HashSet::new();
    for record in &records {
        if let Some(input) = &record.input {
            let input_str = match input {
                serde_json::Value::String(s) => s.clone(),
                _ => serde_json::to_string(&input).unwrap_or_default(),
            };
            if let Some(id) = unimatrix_observe::extract_topic_signal(&input_str) {
                candidates.insert(id);
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Find the candidate with the most attributed observations
    let session = ParsedSession {
        session_id: session_id.to_string(),
        records,
    };
    let sessions = vec![session];

    let mut best: Option<(String, usize)> = None;
    for candidate in &candidates {
        let attributed = unimatrix_observe::attribute_sessions(&sessions, candidate);
        let count = attributed.len();
        if count > 0 {
            if best.is_none() || count > best.as_ref().map(|(_, c)| *c).unwrap_or(0) {
                best = Some((candidate.clone(), count));
            }
        }
    }

    best.map(|(feature, _)| feature)
}

/// Write an auto-generated outcome entry for a session that completed with injections.
///
/// Called from process_session_close when `final_status != Abandoned && injection_count > 0`.
/// Fire-and-forget: spawns a blocking task; never awaits the result.
fn write_auto_outcome_entry(
    store: &Arc<Store>,
    session_id: &str,
    outcome_str: &str, // "success" | "rework"
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
    let _ = tokio::spawn(async move {
        match store_clone.insert(entry).await {
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

    if let Err(e) = store.insert_signal(&record).await {
        tracing::error!("Failed to insert signal record: {e}");
    }
}

/// Drain Helpful signals from SIGNAL_QUEUE and apply helpful_count increments.
///
/// Also updates success_session_count in PendingEntriesAnalysis (FR-06.2b).
/// `feature_cycle`: the feature cycle key for the bucket to write into.
/// Pass an empty string for sessions with no feature cycle attribution.
pub(crate) async fn run_confidence_consumer(
    store: &Arc<Store>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
    feature_cycle: &str,
) {
    // Step 1: Drain all Helpful signals in one transaction.
    let signals = match store.drain_signals(SignalType::Helpful).await {
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
    let entry_ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect();
    if let Err(e) = store
        .record_usage_with_confidence(
            &entry_ids_vec,
            &[],            // access_ids: no access count bump for implicit signals
            &entry_ids_vec, // helpful_ids: all signal entries
            &[],
            &[],
            &[],
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "run_confidence_consumer: record_usage_with_confidence failed");
    }
    // Note: per-entry failures (entry deleted) are handled inside record_usage_with_confidence
    // by skipping entries that no longer exist.

    // Step 4: Update success_session_count in PendingEntriesAnalysis (FR-06.2b)
    //
    // Dedup: each unique (session_id, entry_id) pair increments success_session_count
    // at most once per drain cycle. Different sessions correctly count separately.
    // The HashSet persists across all three passes (ADR-001, crt-011).
    let mut session_counted: HashSet<(String, u64)> = HashSet::new();

    // First pass: update existing entries under lock
    let entries_needing_fetch: Vec<u64> = {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        let mut needing_fetch = Vec::new();
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                // Access the bucket directly for in-place mutation (success_session_count).
                // Using the bucket's entries map rather than upsert() to avoid a full
                // read-modify-write cycle for the common case (entry already exists).
                let in_bucket = pending_guard
                    .buckets
                    .get_mut(feature_cycle)
                    .and_then(|b| b.entries.get_mut(&entry_id))
                    .is_some();
                if in_bucket {
                    if session_counted.insert((signal.session_id.clone(), entry_id)) {
                        if let Some(b) = pending_guard.buckets.get_mut(feature_cycle) {
                            if let Some(existing) = b.entries.get_mut(&entry_id) {
                                existing.success_session_count += 1;
                            }
                        }
                    }
                } else {
                    needing_fetch.push(entry_id);
                }
            }
        }
        needing_fetch
    };

    // Second pass: fetch metadata for new entries (outside lock — async I/O)
    let mut fetched: std::collections::HashMap<u64, (String, String)> =
        std::collections::HashMap::new();
    for entry_id in &entries_needing_fetch {
        let (title, category) = match entry_store.get(*entry_id).await {
            Ok(record) => (record.title.clone(), record.category.clone()),
            Err(_) => (String::new(), String::new()),
        };
        fetched.insert(*entry_id, (title, category));
    }

    // Third pass: insert new entries or update entries added between passes (back under lock)
    if !fetched.is_empty() {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if let Some((title, category)) = fetched.get(&entry_id) {
                    let in_bucket = pending_guard
                        .buckets
                        .get_mut(feature_cycle)
                        .and_then(|b| b.entries.get_mut(&entry_id))
                        .is_some();
                    if in_bucket {
                        // Entry exists (added between passes or by earlier signal iteration)
                        if session_counted.insert((signal.session_id.clone(), entry_id)) {
                            if let Some(b) = pending_guard.buckets.get_mut(feature_cycle) {
                                if let Some(existing) = b.entries.get_mut(&entry_id) {
                                    existing.success_session_count += 1;
                                }
                            }
                        }
                    } else {
                        // New entry — insert with session-aware count via upsert
                        let is_new_session =
                            session_counted.insert((signal.session_id.clone(), entry_id));
                        let analysis = unimatrix_observe::EntryAnalysis {
                            entry_id,
                            title: title.clone(),
                            category: category.clone(),
                            rework_flag_count: 0,
                            injection_count: 0,
                            success_session_count: if is_new_session { 1 } else { 0 },
                            rework_session_count: 0,
                        };
                        pending_guard.upsert(feature_cycle, analysis);
                    }
                }
            }
        }
    }
}

/// Drain Flagged signals from SIGNAL_QUEUE and update PendingEntriesAnalysis.
///
/// `feature_cycle`: the feature cycle key for the bucket to write into.
/// Pass an empty string for sessions with no feature cycle attribution.
pub(crate) async fn run_retrospective_consumer(
    store: &Arc<Store>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    feature_cycle: &str,
) {
    // Step 1: Drain all Flagged signals.
    let signals = match store.drain_signals(SignalType::Flagged).await {
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
            .filter(|id| {
                !pending_guard
                    .buckets
                    .get(feature_cycle)
                    .map_or(false, |b| b.entries.contains_key(id))
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    };

    // Step 3: Fetch metadata for new entries (outside lock — async I/O)
    let mut fetched: std::collections::HashMap<u64, (String, String)> =
        std::collections::HashMap::new();
    for entry_id in entries_needing_fetch {
        let (title, category) = match entry_store.get(entry_id).await {
            Ok(record) => (record.title.clone(), record.category.clone()),
            Err(_) => (String::new(), String::new()),
        };
        fetched.insert(entry_id, (title, category));
    }

    // Step 4: Apply updates to PendingEntriesAnalysis (under lock)
    //
    // Dedup: rework_session_count increments at most once per unique
    // (session_id, entry_id) pair per drain cycle (ADR-001, crt-011).
    //
    // rework_flag_count is intentionally NOT deduplicated — it counts
    // individual rework flagging events (not sessions) and serves as a
    // severity/priority signal for PendingEntriesAnalysis cap eviction.
    // Higher values = more problematic = keep for analysis (ADR-002, crt-011).
    let mut session_counted: HashSet<(String, u64)> = HashSet::new();
    {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                let in_bucket = pending_guard
                    .buckets
                    .get(feature_cycle)
                    .map_or(false, |b| b.entries.contains_key(&entry_id));
                if in_bucket {
                    // rework_flag_count: always increment (event counter, no dedup)
                    if let Some(b) = pending_guard.buckets.get_mut(feature_cycle) {
                        if let Some(existing) = b.entries.get_mut(&entry_id) {
                            existing.rework_flag_count += 1;
                            // rework_session_count: dedup per (session_id, entry_id)
                            if session_counted.insert((signal.session_id.clone(), entry_id)) {
                                existing.rework_session_count += 1;
                            }
                        }
                    }
                } else {
                    let (title, category) = fetched.get(&entry_id).cloned().unwrap_or_default();
                    let is_new_session =
                        session_counted.insert((signal.session_id.clone(), entry_id));
                    let analysis = unimatrix_observe::EntryAnalysis {
                        entry_id,
                        title,
                        category,
                        rework_flag_count: 1,
                        injection_count: 0,
                        success_session_count: 0,
                        rework_session_count: if is_new_session { 1 } else { 0 },
                    };
                    pending_guard.upsert(feature_cycle, analysis);
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

// -- col-017: Topic signal majority vote resolution --

use crate::infra::session::TopicTally;

/// Resolve accumulated topic signals to a single feature_cycle via majority vote.
///
/// Resolution rules (FR-06.1, ADR-017-002):
/// 1. If empty → `None`
/// 2. Find max count. Single winner → return it.
/// 3. Tie → highest `last_seen`. Still tied → lexicographic smallest (AR-2).
fn majority_vote(signals: &std::collections::HashMap<String, TopicTally>) -> Option<String> {
    if signals.is_empty() {
        return None;
    }

    let max_count = signals.values().map(|t| t.count).max().unwrap_or(0);
    let candidates: Vec<&String> = signals
        .iter()
        .filter(|(_, t)| t.count == max_count)
        .map(|(k, _)| k)
        .collect();

    if candidates.len() == 1 {
        return Some(candidates[0].clone());
    }

    // Tie: break by most recent last_seen
    let max_last_seen = candidates
        .iter()
        .map(|k| signals[*k].last_seen)
        .max()
        .unwrap_or(0);
    let recency_candidates: Vec<&String> = candidates
        .into_iter()
        .filter(|k| signals[*k].last_seen == max_last_seen)
        .collect();

    if recency_candidates.len() == 1 {
        return Some(recency_candidates[0].clone());
    }

    // Still tied: lexicographic smallest (deterministic fallback, AR-2)
    recency_candidates.into_iter().min().cloned()
}

/// Update the feature_cycle column for a session in the sessions table (col-017).
async fn update_session_feature_cycle(
    store: &Store,
    session_id: &str,
    feature_cycle: &str,
) -> Result<(), unimatrix_store::StoreError> {
    store
        .update_session(session_id, |r| {
            r.feature_cycle = Some(feature_cycle.to_string());
        })
        .await
}

/// Public wrapper for `update_session_feature_cycle` (#198).
///
/// Needed by status.rs to persist feature_cycle for stale sessions resolved during sweep.
pub(crate) async fn update_session_feature_cycle_pub(
    store: &Store,
    session_id: &str,
    feature_cycle: &str,
) -> Result<(), unimatrix_store::StoreError> {
    update_session_feature_cycle(store, session_id, feature_cycle).await
}

// -- col-022: Cycle event helpers --

/// Persist keywords JSON string to the session record (col-022, ADR-003).
///
/// Uses the existing `store.update_session` read-modify-write pattern.
/// Validation happens upstream in `validate_cycle_params`; this function
/// stores the string as-is.
async fn update_session_keywords(
    store: &Store,
    session_id: &str,
    keywords_json: &str,
) -> Result<(), unimatrix_store::StoreError> {
    store
        .update_session(session_id, |record| {
            record.keywords = Some(keywords_json.to_string());
        })
        .await
}

/// Handle a `cycle_start` event: force-set attribution + keywords persistence (col-022, ADR-002).
///
/// Called before the generic #198 feature extraction path. After `set_feature_force`,
/// the subsequent `set_feature_if_absent` in the generic path will be a no-op.
fn handle_cycle_start(
    event: &unimatrix_engine::wire::ImplantEvent,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
) {
    // Step 1: Extract feature_cycle from payload
    let feature_cycle = match event.payload.get("feature_cycle").and_then(|v| v.as_str()) {
        Some(fc) => sanitize_metadata_field(fc),
        None => {
            tracing::warn!(
                session_id = %event.session_id,
                "cycle_start missing feature_cycle in payload"
            );
            return;
        }
    };

    if feature_cycle.is_empty() {
        tracing::warn!(
            session_id = %event.session_id,
            "cycle_start feature_cycle is empty after sanitize"
        );
        return;
    }

    // Step 2: Force-set attribution (ADR-002)
    let result = session_registry.set_feature_force(&event.session_id, &feature_cycle);

    match &result {
        SetFeatureResult::Set => {
            tracing::info!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                "col-022: feature_cycle set via explicit cycle_start"
            );
        }
        SetFeatureResult::AlreadyMatches => {
            tracing::info!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                "col-022: feature_cycle already matches (no-op)"
            );
        }
        SetFeatureResult::Overridden { previous } => {
            tracing::warn!(
                session_id = %event.session_id,
                feature_cycle = %feature_cycle,
                previous = %previous,
                "col-022: feature_cycle overridden by explicit cycle_start"
            );
        }
    }

    // Step 3: Persist feature_cycle to SQLite (fire-and-forget)
    // Only persist if the value changed (Set or Overridden)
    if matches!(
        result,
        SetFeatureResult::Set | SetFeatureResult::Overridden { .. }
    ) {
        let store_fc = Arc::clone(store);
        let sid = event.session_id.clone();
        let fc = feature_cycle.clone();
        let _ = tokio::spawn(async move {
            if let Err(e) = update_session_feature_cycle(&store_fc, &sid, &fc).await {
                tracing::warn!(error = %e, "col-022: feature_cycle persist failed");
            }
        });
    }

    // Step 4: Extract and persist keywords (fire-and-forget, independent of attribution)
    if let Some(keywords_val) = event.payload.get("keywords") {
        let keywords_json = keywords_val.to_string();
        if !keywords_json.is_empty() {
            let store_kw = Arc::clone(store);
            let sid = event.session_id.clone();
            let _ = tokio::spawn(async move {
                if let Err(e) = update_session_keywords(&store_kw, &sid, &keywords_json).await {
                    tracing::warn!(error = %e, "col-022: keywords persist failed");
                }
            });
        }
    }
}

// -- col-012: Observation persistence helpers --

/// Extracted observation row fields ready for SQL insertion.
struct ObservationRow {
    session_id: String,
    ts_millis: i64,
    hook: String,
    tool: Option<String>,
    input: Option<String>,
    response_size: Option<i64>,
    response_snippet: Option<String>,
    /// Hook-side topic signal for feature attribution (col-017).
    topic_signal: Option<String>,
}

/// Extract observation fields from an ImplantEvent for SQL insertion.
fn extract_observation_fields(event: &unimatrix_engine::wire::ImplantEvent) -> ObservationRow {
    let session_id = event.session_id.clone();
    let ts_millis = (event.timestamp as i64).saturating_mul(1000);
    let hook = event.event_type.clone();

    let (tool, input, response_size, response_snippet) = match hook.as_str() {
        "PreToolUse" => {
            let tool = event
                .payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let input = event
                .payload
                .get("tool_input")
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            (tool, input, None, None)
        }
        "PostToolUse" | "post_tool_use_rework_candidate" => {
            let tool = event
                .payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let input = event
                .payload
                .get("tool_input")
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            let (rs, rsnip) = extract_response_fields(&event.payload);
            (tool, input, rs, rsnip)
        }
        "SubagentStart" => {
            let tool = event
                .payload
                .get("agent_type")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let input = event
                .payload
                .get("prompt_snippet")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            (tool, input, None, None)
        }
        "SubagentStop" | _ => (None, None, None, None),
    };

    // col-019: Normalize rework candidate hook type to PostToolUse for observation consistency
    let hook = if hook == "post_tool_use_rework_candidate" {
        "PostToolUse".to_string()
    } else {
        hook
    };

    ObservationRow {
        session_id,
        ts_millis,
        hook,
        tool,
        input,
        response_size,
        response_snippet,
        topic_signal: event.topic_signal.clone(),
    }
}

/// Extract response_size and response_snippet from a PostToolUse event payload.
///
/// Tries `tool_response` first (Claude Code's field name), then falls back to
/// legacy `response_size`/`response_snippet` fields for backward compatibility.
fn extract_response_fields(payload: &serde_json::Value) -> (Option<i64>, Option<String>) {
    // Primary: compute from tool_response (Claude Code's actual field)
    if let Some(response) = payload.get("tool_response") {
        if !response.is_null() {
            let serialized = serde_json::to_string(response).unwrap_or_default();
            let size = serialized.len() as i64;
            let snippet: String = serialized.chars().take(500).collect();
            return (Some(size), Some(snippet));
        }
    }

    // Fallback: legacy field names (test fixtures, future compatibility)
    let rs = payload.get("response_size").and_then(|v| v.as_i64());
    let rsnip = payload
        .get("response_snippet")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (rs, rsnip)
}

/// Insert a single observation row into the observations table.
///
/// Called from within a `spawn_blocking` context; uses `block_on` to bridge
/// the async sqlx pool into this sync environment.
fn insert_observation(
    store: &Store,
    obs: &ObservationRow,
) -> Result<(), unimatrix_store::StoreError> {
    let pool = store.write_pool_server();
    tokio::runtime::Handle::current()
        .block_on(
            sqlx::query(
                "INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&obs.session_id)
            .bind(obs.ts_millis)
            .bind(&obs.hook)
            .bind(&obs.tool)
            .bind(&obs.input)
            .bind(obs.response_size)
            .bind(&obs.response_snippet)
            .bind(&obs.topic_signal)
            .execute(pool),
        )
        .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
    Ok(())
}

/// Insert a batch of observations in a single transaction.
///
/// Called from within a `spawn_blocking` context; uses `block_on` to bridge
/// the async sqlx pool into this sync environment.
fn insert_observations_batch(
    store: &Store,
    batch: &[ObservationRow],
) -> Result<(), unimatrix_store::StoreError> {
    if batch.is_empty() {
        return Ok(());
    }
    let pool = store.write_pool_server();
    let handle = tokio::runtime::Handle::current();
    handle
        .block_on(sqlx::query("BEGIN").execute(pool))
        .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
    let result = (|| -> Result<(), unimatrix_store::StoreError> {
        for obs in batch {
            handle
                .block_on(
                    sqlx::query(
                        "INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .bind(&obs.session_id)
                    .bind(obs.ts_millis)
                    .bind(&obs.hook)
                    .bind(&obs.tool)
                    .bind(&obs.input)
                    .bind(obs.response_size)
                    .bind(&obs.response_snippet)
                    .bind(&obs.topic_signal)
                    .execute(pool),
                )
                .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            handle
                .block_on(sqlx::query("COMMIT").execute(pool))
                .map_err(|e| unimatrix_store::StoreError::Database(e.to_string().into()))?;
            Ok(())
        }
        Err(e) => {
            let _ = handle.block_on(sqlx::query("ROLLBACK").execute(pool));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_engine::wire::ImplantEvent;
    use unimatrix_store::test_helpers::open_test_store;

    // -- Helpers --

    async fn make_store() -> Arc<Store> {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);
        // Leak TempDir so the database file is not deleted during the test.
        // Acceptable for test infrastructure on Linux (file stays accessible via fd).
        std::mem::forget(tmp);
        store
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

    fn make_dispatch_deps(
        store: &Arc<Store>,
    ) -> (
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
        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));
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
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::Ping,
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        match response {
            HookResponse::Pong { server_version } => assert_eq!(server_version, "0.1.0"),
            _ => panic!("expected Pong"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_register_returns_ack() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));

        // col-008: verify session registered
        let state = registry.get_state("s1").unwrap();
        assert_eq!(state.role.as_deref(), Some("dev"));
        assert_eq!(state.feature.as_deref(), Some("col-008"));
    }

    #[tokio::test]
    async fn dispatch_session_close_returns_ack() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));

        // col-008: verify session cleared
        assert!(registry.get_state("s1").is_none());
    }

    #[tokio::test]
    async fn dispatch_record_event_returns_ack() {
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let event = ImplantEvent {
            event_type: "test".to_string(),
            session_id: "s1".to_string(),
            timestamp: 0,
            payload: serde_json::json!({}),
            topic_signal: None,
        };
        let response = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        assert!(matches!(response, HookResponse::Ack));
    }

    // vnc-007: HookRequest::Briefing is now handled (returns BriefingContent).
    // The previous test sent Briefing and expected ERR_UNKNOWN_REQUEST. Now all
    // variants are handled, so this test verifies Briefing returns BriefingContent.
    #[tokio::test]
    async fn dispatch_briefing_returns_content() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        match response {
            HookResponse::BriefingContent { .. } => {}
            other => panic!("expected BriefingContent, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_context_search_embed_not_ready() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        match response {
            HookResponse::Entries {
                items,
                total_tokens,
            } => {
                assert!(items.is_empty());
                assert_eq!(total_tokens, 0);
            }
            _ => panic!("expected Entries, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_close_clears_coaccess_via_registry() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        // After clear + re-register, same set should be considered new
        registry.register_session("s1", None, None);
        assert!(registry.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    // -- CompactPayload dispatch tests (col-008) --

    #[tokio::test]
    async fn dispatch_compact_payload_empty_session_returns_briefing() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;
        match response {
            HookResponse::BriefingContent {
                content,
                token_count,
            } => {
                // No session, no entries in KB -> empty content
                assert!(content.is_empty());
                assert_eq!(token_count, 0);
            }
            _ => panic!("expected BriefingContent, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_compact_payload_increments_compaction_count() {
        let store = make_store().await;
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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        assert_eq!(registry.get_state("s1").unwrap().compaction_count, 1);
    }

    // -- format_compaction_payload unit tests --

    fn make_entry(
        id: u64,
        title: &str,
        content: &str,
        category: &str,
        confidence: f64,
    ) -> unimatrix_store::EntryRecord {
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
            pre_quarantine_status: None,
        }
    }

    #[test]
    fn format_payload_empty_categories_returns_none() {
        let categories = CompactionCategories {
            decisions: vec![],
            injections: vec![],
            conventions: vec![],
        };
        assert!(
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).is_none()
        );
    }

    #[test]
    fn format_payload_header_present() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "ADR", "content", "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.starts_with("--- Unimatrix Compaction Context ---\n"));
    }

    #[test]
    fn format_payload_decisions_before_injections() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(1, "Decision", "dcontent", "decision", 0.9), 0.9)],
            injections: vec![(make_entry(2, "Pattern", "pcontent", "pattern", 0.8), 0.8)],
            conventions: vec![],
        };
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
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
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        let high_pos = result.find("[High]").expect("High entry missing");
        let low_pos = result.find("[Low]").expect("Low entry missing");
        assert!(
            high_pos < low_pos,
            "high-confidence entry must appear before low-confidence entry"
        );
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
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(
            result.len() <= MAX_COMPACTION_BYTES,
            "output {} exceeds budget {}",
            result.len(),
            MAX_COMPACTION_BYTES
        );
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
        )
        .unwrap();
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
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
        assert!(result.contains("[deprecated]"));
    }

    #[test]
    fn format_payload_entry_id_metadata() {
        let categories = CompactionCategories {
            decisions: vec![(make_entry(42, "Test", "c", "decision", 0.9), 0.9)],
            injections: vec![],
            conventions: vec![],
        };
        let result =
            format_compaction_payload(&categories, None, None, 0, MAX_COMPACTION_BYTES).unwrap();
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
        let store = make_store().await;
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
        let id1 = store.insert(entry1).await.unwrap();
        let id2 = store.insert(entry2).await.unwrap();

        // Register session and record injections
        registry.register_session(
            "s1",
            Some("developer".to_string()),
            Some("col-008".to_string()),
        );
        registry.record_injection("s1", &[(id1, 0.92), (id2, 0.75)]);

        let response = dispatch_request(
            HookRequest::CompactPayload {
                session_id: "s1".to_string(),
                injected_entry_ids: vec![],
                role: None,
                feature: None,
                token_limit: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        match response {
            HookResponse::BriefingContent {
                content,
                token_count,
            } => {
                assert!(
                    !content.is_empty(),
                    "primary path should produce non-empty content"
                );
                assert!(token_count > 0);
                // Verify entries from injection history appear in output
                assert!(
                    content.contains("[ADR-Important]"),
                    "decision entry missing"
                );
                assert!(
                    content.contains("[Coding Convention]"),
                    "convention entry missing"
                );
                // Verify decisions appear before conventions (priority ordering)
                let dec_pos = content.find("[ADR-Important]").unwrap();
                let conv_pos = content.find("[Coding Convention]").unwrap();
                assert!(
                    dec_pos < conv_pos,
                    "decisions must appear before conventions"
                );
                // Verify session context
                assert!(content.contains("Role: developer"));
                assert!(content.contains("Feature: col-008"));
            }
            _ => panic!("expected BriefingContent, got {response:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_compact_payload_primary_path_sorts_by_confidence() {
        let store = make_store().await;
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
        let id_low = store.insert(low).await.unwrap();
        let id_high = store.insert(high).await.unwrap();

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
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        match response {
            HookResponse::BriefingContent { content, .. } => {
                let high_pos = content.find("[HighConf]").expect("HighConf missing");
                let low_pos = content.find("[LowConf]").expect("LowConf missing");
                assert!(
                    high_pos < low_pos,
                    "high-confidence entry must appear before low-confidence"
                );
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
        assert!(
            err.contains("invalid character"),
            "expected 'invalid character', got: {err}"
        );
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
        assert!(
            err.contains("must not be empty"),
            "expected 'must not be empty', got: {err}"
        );
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

    // -- crt-011: Consumer dedup tests --

    async fn insert_test_entry_for_signal(store: &Store) -> u64 {
        let entry = unimatrix_store::NewEntry {
            title: "Test entry".to_string(),
            content: "Test content for signal consumer".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
        };
        store.insert(entry).await.expect("insert test entry")
    }

    fn make_signal(session_id: &str, entry_ids: Vec<u64>, signal_type: SignalType) -> SignalRecord {
        SignalRecord {
            signal_id: 0, // assigned by insert_signal
            session_id: session_id.to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            entry_ids,
            signal_type,
            signal_source: SignalSource::ImplicitOutcome,
        }
    }

    /// T-CON-01: Two Helpful signals with same session_id and overlapping entry_ids
    /// should increment success_session_count only once per entry.
    #[tokio::test]
    async fn test_confidence_consumer_dedup_same_session() {
        let store = make_store().await;
        let pending = make_pending();
        let (_, entry_store, _) = make_dispatch_deps(&store);

        let entry_id = insert_test_entry_for_signal(&store).await;

        // Insert two signals with SAME session_id, both referencing entry_id
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Helpful))
            .await
            .unwrap();
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Helpful))
            .await
            .unwrap();

        // vnc-005: pass feature_cycle to consumer; use "" as default
        run_confidence_consumer(&store, &entry_store, &pending, "").await;

        let guard = pending.lock().unwrap();
        let analysis = guard
            .buckets
            .get("")
            .and_then(|b| b.entries.get(&entry_id))
            .expect("entry should exist in pending bucket");
        assert_eq!(
            analysis.success_session_count, 1,
            "same session should count only once"
        );
    }

    /// T-CON-02: Two Helpful signals with different session_ids should increment
    /// success_session_count once per session (total 2).
    #[tokio::test]
    async fn test_confidence_consumer_different_sessions_count_separately() {
        let store = make_store().await;
        let pending = make_pending();
        let (_, entry_store, _) = make_dispatch_deps(&store);

        let entry_id = insert_test_entry_for_signal(&store).await;

        // Insert two signals with DIFFERENT session_ids
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Helpful))
            .await
            .unwrap();
        store
            .insert_signal(&make_signal("sess-B", vec![entry_id], SignalType::Helpful))
            .await
            .unwrap();

        // vnc-005: pass feature_cycle to consumer; use "" as default
        run_confidence_consumer(&store, &entry_store, &pending, "").await;

        let guard = pending.lock().unwrap();
        let analysis = guard
            .buckets
            .get("")
            .and_then(|b| b.entries.get(&entry_id))
            .expect("entry should exist in pending bucket");
        assert_eq!(
            analysis.success_session_count, 2,
            "different sessions should each count"
        );
    }

    /// T-CON-03: Two Flagged signals with same session_id should increment
    /// rework_session_count only once but rework_flag_count twice.
    #[tokio::test]
    async fn test_retrospective_consumer_rework_session_dedup() {
        let store = make_store().await;
        let pending = make_pending();
        let (_, entry_store, _) = make_dispatch_deps(&store);

        let entry_id = insert_test_entry_for_signal(&store).await;

        // Insert two Flagged signals with SAME session_id
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Flagged))
            .await
            .unwrap();
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Flagged))
            .await
            .unwrap();

        // vnc-005: pass feature_cycle to consumer; use "" as default
        run_retrospective_consumer(&store, &pending, &entry_store, "").await;

        let guard = pending.lock().unwrap();
        let analysis = guard
            .buckets
            .get("")
            .and_then(|b| b.entries.get(&entry_id))
            .expect("entry should exist in pending bucket");
        assert_eq!(
            analysis.rework_session_count, 1,
            "same session should count only once"
        );
        assert_eq!(
            analysis.rework_flag_count, 2,
            "flag count should NOT be deduped (ADR-002)"
        );
    }

    /// T-CON-04: Three Flagged signals with same session_id should increment
    /// rework_flag_count 3 times (event counter, no dedup per ADR-002) but
    /// rework_session_count only once.
    #[tokio::test]
    async fn test_retrospective_consumer_flag_count_not_deduped() {
        let store = make_store().await;
        let pending = make_pending();
        let (_, entry_store, _) = make_dispatch_deps(&store);

        let entry_id = insert_test_entry_for_signal(&store).await;

        // Insert three Flagged signals with SAME session_id
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Flagged))
            .await
            .unwrap();
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Flagged))
            .await
            .unwrap();
        store
            .insert_signal(&make_signal("sess-A", vec![entry_id], SignalType::Flagged))
            .await
            .unwrap();

        // vnc-005: pass feature_cycle to consumer; use "" as default
        run_retrospective_consumer(&store, &pending, &entry_store, "").await;

        let guard = pending.lock().unwrap();
        let analysis = guard
            .buckets
            .get("")
            .and_then(|b| b.entries.get(&entry_id))
            .expect("entry should exist in pending bucket");
        assert_eq!(
            analysis.rework_flag_count, 3,
            "every flagging event should count"
        );
        assert_eq!(analysis.rework_session_count, 1, "only one unique session");
    }

    // -- col-017: majority_vote tests (T-07) --

    #[test]
    fn test_majority_vote_clear_winner() {
        // AC-13
        let mut signals = std::collections::HashMap::new();
        signals.insert(
            "col-017".to_string(),
            TopicTally {
                count: 5,
                last_seen: 100,
            },
        );
        signals.insert(
            "col-018".to_string(),
            TopicTally {
                count: 2,
                last_seen: 200,
            },
        );
        assert_eq!(majority_vote(&signals), Some("col-017".to_string()));
    }

    #[test]
    fn test_majority_vote_tie_broken_by_recency() {
        // AC-14
        let mut signals = std::collections::HashMap::new();
        signals.insert(
            "a".to_string(),
            TopicTally {
                count: 3,
                last_seen: 100,
            },
        );
        signals.insert(
            "b".to_string(),
            TopicTally {
                count: 3,
                last_seen: 200,
            },
        );
        assert_eq!(majority_vote(&signals), Some("b".to_string()));
    }

    #[test]
    fn test_majority_vote_deterministic_tie_lexicographic() {
        // AR-2: same count + same timestamp -> lexicographic smallest
        let mut signals = std::collections::HashMap::new();
        signals.insert(
            "b".to_string(),
            TopicTally {
                count: 3,
                last_seen: 100,
            },
        );
        signals.insert(
            "a".to_string(),
            TopicTally {
                count: 3,
                last_seen: 100,
            },
        );
        assert_eq!(majority_vote(&signals), Some("a".to_string()));
    }

    #[test]
    fn test_majority_vote_single_topic() {
        let mut signals = std::collections::HashMap::new();
        signals.insert(
            "col-017".to_string(),
            TopicTally {
                count: 1,
                last_seen: 100,
            },
        );
        assert_eq!(majority_vote(&signals), Some("col-017".to_string()));
    }

    #[test]
    fn test_majority_vote_empty() {
        // AC-15
        let signals = std::collections::HashMap::new();
        assert_eq!(majority_vote(&signals), None);
    }

    #[test]
    fn test_majority_vote_multi_topic() {
        // T-16: 3 topics, highest count wins
        let mut signals = std::collections::HashMap::new();
        signals.insert(
            "a".to_string(),
            TopicTally {
                count: 10,
                last_seen: 100,
            },
        );
        signals.insert(
            "b".to_string(),
            TopicTally {
                count: 8,
                last_seen: 200,
            },
        );
        signals.insert(
            "c".to_string(),
            TopicTally {
                count: 2,
                last_seen: 300,
            },
        );
        assert_eq!(majority_vote(&signals), Some("a".to_string()));
    }

    // -- #169: content_based_attribution_fallback tests --

    async fn insert_observation(store: &Store, session_id: &str, ts: i64, input: &str) {
        sqlx::query(
            "INSERT INTO observations (session_id, ts_millis, hook, tool, input) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(session_id)
        .bind(ts)
        .bind("PreToolUse")
        .bind("Read")
        .bind(input)
        .execute(store.write_pool_server())
        .await
        .expect("insert observation");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_attribution_fallback_empty_session_returns_none() {
        let store = make_store().await;
        // No observations inserted — should return None
        let result = content_based_attribution_fallback(&store, "nonexistent-session");
        assert_eq!(result, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_attribution_fallback_no_topic_signals_returns_none() {
        let store = make_store().await;
        insert_observation(
            &store,
            "sess-169",
            1000,
            "some random text with no feature IDs",
        )
        .await;
        let result = content_based_attribution_fallback(&store, "sess-169");
        assert_eq!(result, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_attribution_fallback_returns_best_topic() {
        let store = make_store().await;
        insert_observation(
            &store,
            "sess-169",
            1000,
            "product/features/col-017/SCOPE.md",
        )
        .await;
        insert_observation(&store, "sess-169", 2000, "product/features/col-017/IMPL.md").await;
        let result = content_based_attribution_fallback(&store, "sess-169");
        assert_eq!(result, Some("col-017".to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_attribution_fallback_multi_observation_returns_best_topic() {
        // Verify attribution picks the most-signalled feature across observations.
        let store = make_store().await;
        for i in 0..10 {
            insert_observation(
                &store,
                "sess-169-lock",
                1000 + i as i64,
                &format!("product/features/col-017/file{i}.md"),
            )
            .await;
        }
        let result = content_based_attribution_fallback(&store, "sess-169-lock");
        assert_eq!(result, Some("col-017".to_string()));
    }

    // -- col-019: extract_response_fields tests --

    #[test]
    fn extract_response_fields_normal_object() {
        // T-04: tool_response is a normal JSON object -> size and snippet populated
        let payload = serde_json::json!({
            "tool_response": {"success": true, "output": "hello world"}
        });
        let (size, snippet) = extract_response_fields(&payload);
        let expected =
            serde_json::to_string(&serde_json::json!({"success": true, "output": "hello world"}))
                .unwrap();
        assert_eq!(size, Some(expected.len() as i64));
        assert_eq!(snippet, Some(expected));
    }

    #[test]
    fn extract_response_fields_absent() {
        // T-05: tool_response is absent -> (None, None)
        let payload = serde_json::json!({"tool_name": "Read"});
        let (size, snippet) = extract_response_fields(&payload);
        assert_eq!(size, None);
        assert_eq!(snippet, None);
    }

    #[test]
    fn extract_response_fields_null() {
        // T-06: tool_response is JSON null -> (None, None)
        let payload = serde_json::json!({"tool_response": null});
        let (size, snippet) = extract_response_fields(&payload);
        assert_eq!(size, None);
        assert_eq!(snippet, None);
    }

    #[test]
    fn extract_response_fields_empty_object() {
        // T-07: tool_response is empty object -> size=2, snippet="{}"
        let payload = serde_json::json!({"tool_response": {}});
        let (size, snippet) = extract_response_fields(&payload);
        assert_eq!(size, Some(2));
        assert_eq!(snippet, Some("{}".to_string()));
    }

    #[test]
    fn extract_response_fields_string_value() {
        // T-08: tool_response is a string value -> correct size and snippet
        let payload = serde_json::json!({"tool_response": "some output text"});
        let (size, snippet) = extract_response_fields(&payload);
        let expected = serde_json::to_string(&serde_json::json!("some output text")).unwrap();
        assert_eq!(size, Some(expected.len() as i64));
        assert_eq!(snippet, Some(expected));
    }

    #[test]
    fn extract_response_fields_large_response_truncated() {
        // T-09: tool_response serialized > 500 chars -> snippet truncated at char boundary
        let long_value = "x".repeat(600);
        let payload = serde_json::json!({"tool_response": long_value});
        let (size, snippet) = extract_response_fields(&payload);
        let serialized = serde_json::to_string(&serde_json::json!(long_value)).unwrap();
        assert_eq!(size, Some(serialized.len() as i64));
        let snippet = snippet.unwrap();
        assert_eq!(snippet.chars().count(), 500);
        assert!(serialized.starts_with(&snippet));
    }

    #[test]
    fn extract_response_fields_legacy_fallback() {
        // T-08b: legacy response_size/response_snippet fields still work
        let payload = serde_json::json!({
            "response_size": 42,
            "response_snippet": "legacy snippet"
        });
        let (size, snippet) = extract_response_fields(&payload);
        assert_eq!(size, Some(42));
        assert_eq!(snippet, Some("legacy snippet".to_string()));
    }

    #[test]
    fn extract_response_fields_multibyte_utf8_truncation() {
        // T-13: Multi-byte UTF-8 characters -> snippet truncated at char boundary, no panic
        // Each emoji is 1 char but multiple bytes
        let emojis: String = std::iter::repeat('\u{1F600}').take(600).collect();
        let payload = serde_json::json!({"tool_response": emojis});
        let (size, snippet) = extract_response_fields(&payload);
        assert!(size.is_some());
        let snippet = snippet.unwrap();
        assert_eq!(snippet.chars().count(), 500);
        // Verify it's valid UTF-8 (would panic on from_utf8 otherwise)
        assert!(snippet.is_char_boundary(snippet.len()));
    }

    // -- col-019: extract_observation_fields with rework candidates --

    #[test]
    fn extract_observation_fields_rework_candidate_normalized() {
        // T-09b: post_tool_use_rework_candidate events -> hook="PostToolUse"
        let event = ImplantEvent {
            event_type: "post_tool_use_rework_candidate".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({
                "tool_name": "Edit",
                "file_path": "src/foo.rs",
                "had_failure": false,
                "tool_input": {"path": "src/foo.rs"},
                "tool_response": {"success": true}
            }),
            topic_signal: None,
        };
        let obs = extract_observation_fields(&event);
        assert_eq!(obs.hook, "PostToolUse");
        assert_eq!(obs.tool, Some("Edit".to_string()));
        assert!(obs.response_size.is_some());
        assert!(obs.response_snippet.is_some());
    }

    #[test]
    fn extract_observation_fields_rework_candidate_with_tool_response() {
        // Verify response fields computed from tool_response in rework candidate
        let event = ImplantEvent {
            event_type: "post_tool_use_rework_candidate".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({
                "tool_name": "Bash",
                "had_failure": true,
                "tool_input": {"command": "ls"},
                "tool_response": {"stdout": "file.txt", "exit_code": 1}
            }),
            topic_signal: None,
        };
        let obs = extract_observation_fields(&event);
        assert_eq!(obs.hook, "PostToolUse");
        let expected =
            serde_json::to_string(&serde_json::json!({"stdout": "file.txt", "exit_code": 1}))
                .unwrap();
        assert_eq!(obs.response_size, Some(expected.len() as i64));
        assert_eq!(obs.response_snippet, Some(expected));
    }

    #[test]
    fn extract_observation_fields_rework_candidate_preserves_topic_signal() {
        // T-10: topic_signal flows through to ObservationRow for rework candidates
        let event = ImplantEvent {
            event_type: "post_tool_use_rework_candidate".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({
                "tool_name": "Edit",
                "file_path": "src/foo.rs",
                "had_failure": false,
                "tool_response": {"success": true}
            }),
            topic_signal: Some("col-019".to_string()),
        };
        let obs = extract_observation_fields(&event);
        assert_eq!(obs.hook, "PostToolUse");
        assert_eq!(obs.topic_signal, Some("col-019".to_string()));
    }

    #[test]
    fn extract_observation_fields_posttooluse_with_tool_response() {
        // T-04b: Non-rework PostToolUse with tool_response -> response fields populated
        let event = ImplantEvent {
            event_type: "PostToolUse".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({
                "tool_name": "Read",
                "tool_input": {"path": "src/main.rs"},
                "tool_response": {"content": "fn main() {}"}
            }),
            topic_signal: None,
        };
        let obs = extract_observation_fields(&event);
        assert_eq!(obs.hook, "PostToolUse");
        assert_eq!(obs.tool, Some("Read".to_string()));
        let expected =
            serde_json::to_string(&serde_json::json!({"content": "fn main() {}"})).unwrap();
        assert_eq!(obs.response_size, Some(expected.len() as i64));
        assert_eq!(obs.response_snippet, Some(expected));
    }

    #[test]
    fn extract_observation_fields_posttooluse_missing_tool_response() {
        // T-05b: PostToolUse without tool_response -> None/None (legacy fallback with no legacy fields)
        let event = ImplantEvent {
            event_type: "PostToolUse".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({
                "tool_name": "Read",
                "tool_input": {"path": "src/main.rs"}
            }),
            topic_signal: None,
        };
        let obs = extract_observation_fields(&event);
        assert_eq!(obs.response_size, None);
        assert_eq!(obs.response_snippet, None);
    }

    // -- col-018: UserPromptSubmit observation tests --

    /// Helper: query the observations table for rows matching session_id.
    async fn query_observations(
        store: &Store,
        session_id: &str,
    ) -> Vec<(
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        use sqlx::Row as _;
        let rows = sqlx::query(
            "SELECT session_id, ts_millis, hook, tool, input, topic_signal \
             FROM observations WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_all(store.read_pool_test())
        .await
        .expect("query observations");
        rows.into_iter()
            .map(|row| {
                (
                    row.get::<String, _>(0),
                    row.get::<i64, _>(1),
                    row.get::<String, _>(2),
                    row.get::<Option<String>, _>(3),
                    row.get::<Option<String>, _>(4),
                    row.get::<Option<String>, _>(5),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn col018_context_search_creates_observation() {
        // T-01, AC-01: ContextSearch with valid session_id produces observation row
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: "implement col-018 feature".to_string(),
                session_id: Some("sess-obs-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        // Allow spawn_blocking to complete
        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-obs-1").await;
        assert_eq!(rows.len(), 1, "expected exactly 1 observation row");
        let (sid, ts, hook, tool, input, topic) = &rows[0];
        assert_eq!(sid, "sess-obs-1");
        assert!(ts > &0, "ts_millis should be positive");
        assert_eq!(hook, "UserPromptSubmit");
        assert!(tool.is_none(), "tool should be None for UserPromptSubmit");
        assert_eq!(input.as_deref(), Some("implement col-018 feature"));
        assert_eq!(topic.as_deref(), Some("col-018"));
    }

    #[tokio::test]
    async fn col018_topic_signal_from_feature_id() {
        // T-03, AC-02: Prompt containing feature ID produces topic_signal
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: "work on col-018 design".to_string(),
                session_id: Some("sess-topic-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-topic-1").await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].5.as_deref(), Some("col-018"));
    }

    #[tokio::test]
    async fn col018_topic_signal_null_for_generic_prompt() {
        // T-04, AC-09: Generic prompt has no topic_signal
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: "help me fix the bug".to_string(),
                session_id: Some("sess-generic-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-generic-1").await;
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].5.is_none(),
            "topic_signal should be NULL for generic prompt"
        );
    }

    #[tokio::test]
    async fn col018_topic_signal_from_file_path() {
        // T-05, AC-02: Prompt with file path containing feature ID
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: "work on product/features/col-018/SCOPE.md".to_string(),
                session_id: Some("sess-path-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-path-1").await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].5.as_deref(), Some("col-018"));
    }

    #[tokio::test]
    async fn col018_long_prompt_truncated() {
        // T-06, AC-08: Prompt > 4096 chars truncated in observation input
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let long_query = "a".repeat(5000);
        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: long_query,
                session_id: Some("sess-trunc-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-trunc-1").await;
        assert_eq!(rows.len(), 1);
        let input = rows[0].4.as_ref().expect("input should be present");
        assert_eq!(input.len(), 4096, "input should be truncated to 4096 chars");
    }

    #[tokio::test]
    async fn col018_prompt_at_limit_not_truncated() {
        // T-07, AC-08: Prompt exactly 4096 chars stored fully
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let exact_query = "b".repeat(4096);
        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: exact_query.clone(),
                session_id: Some("sess-exact-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-exact-1").await;
        assert_eq!(rows.len(), 1);
        let input = rows[0].4.as_ref().expect("input should be present");
        assert_eq!(input.len(), 4096);
        assert_eq!(input, &exact_query);
    }

    #[tokio::test]
    async fn col018_session_id_none_skips_observation() {
        // T-08, AC-06: No observation when session_id is None
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::ContextSearch {
                query: "test query with col-018".to_string(),
                session_id: None,
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        // No observation written
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observations")
            .fetch_one(store.read_pool_test())
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "no observation should be written when session_id is None"
        );

        // Search still returns results
        match response {
            HookResponse::Entries { .. } => {}
            other => panic!("expected Entries, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn col018_empty_query_skips_observation() {
        // T-09, AC-07: No observation when query is empty
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: String::new(),
                session_id: Some("sess-empty-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let rows = query_observations(&store, "sess-empty-1").await;
        assert_eq!(rows.len(), 0, "no observation for empty query");
    }

    #[tokio::test]
    async fn col018_search_results_unchanged_with_observation() {
        // T-10/T-11, AC-04: Search results identical with observation side effect
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = make_registry();
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let response = dispatch_request(
            HookRequest::ContextSearch {
                query: "test col-018".to_string(),
                session_id: Some("sess-search-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        // Embed not started -> empty results (same behavior as pre-col-018)
        match response {
            HookResponse::Entries {
                items,
                total_tokens,
            } => {
                assert!(items.is_empty());
                assert_eq!(total_tokens, 0);
            }
            other => panic!("expected Entries, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn col018_topic_signal_accumulated_in_session_registry() {
        // T-12, AC-03: Topic signal recorded in session registry
        let store = make_store().await;
        let embed = make_embed_service();
        let registry = SessionRegistry::new();
        registry.register_session("sess-reg-1", None, None);
        let (vs, es, adapt) = make_dispatch_deps(&store);

        let _ = dispatch_request(
            HookRequest::ContextSearch {
                query: "implement col-018 now".to_string(),
                session_id: Some("sess-reg-1".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry
            .get_state("sess-reg-1")
            .expect("session should exist");
        assert!(
            state.topic_signals.contains_key("col-018"),
            "topic signal 'col-018' should be accumulated"
        );
        assert_eq!(state.topic_signals["col-018"].count, 1);
    }

    // -- col-022: cycle_start / cycle_stop dispatch tests --

    fn make_cycle_event(
        event_type: &str,
        session_id: &str,
        payload: serde_json::Value,
        topic_signal: Option<String>,
    ) -> ImplantEvent {
        ImplantEvent {
            event_type: event_type.to_string(),
            session_id: session_id.to_string(),
            timestamp: unix_now_secs(),
            payload,
            topic_signal,
        }
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_sets_feature_force() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_overwrites_heuristic_attribution() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, Some("col-017".to_string()));

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_already_matches() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, Some("col-022".to_string()));

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_unknown_session() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "unknown",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        assert!(matches!(resp, HookResponse::Ack));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_persists_keywords() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        store
            .insert_session(&SessionRecord {
                session_id: "s1".to_string(),
                feature_cycle: None,
                agent_role: None,
                started_at: unix_now_secs(),
                ended_at: None,
                status: SessionLifecycleStatus::Active,
                compaction_count: 0,
                outcome: None,
                total_injections: 0,
                keywords: None,
            })
            .await
            .unwrap();

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({
                "feature_cycle": "col-022",
                "keywords": ["attr", "lifecycle"]
            }),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        let session = store
            .get_session("s1")
            .await
            .unwrap()
            .expect("session row should exist");
        assert_eq!(session.keywords.as_deref(), Some(r#"["attr","lifecycle"]"#));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_no_keywords_field() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        store
            .insert_session(&SessionRecord {
                session_id: "s1".to_string(),
                feature_cycle: None,
                agent_role: None,
                started_at: unix_now_secs(),
                ended_at: None,
                status: SessionLifecycleStatus::Active,
                compaction_count: 0,
                outcome: None,
                total_injections: 0,
                keywords: None,
            })
            .await
            .unwrap();

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        let session = store
            .get_session("s1")
            .await
            .unwrap()
            .expect("session row should exist");
        assert_eq!(session.keywords, None);
    }

    #[tokio::test]
    async fn test_dispatch_cycle_stop_does_not_modify_feature() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, Some("col-022".to_string()));

        let event = make_cycle_event(
            CYCLE_STOP_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_stop_without_prior_start() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        let event = make_cycle_event(
            CYCLE_STOP_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            None,
        );

        let resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        assert!(matches!(resp, HookResponse::Ack));
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_missing_feature_cycle() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        let event = make_cycle_event(CYCLE_START_EVENT, "s1", serde_json::json!({}), None);

        let resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        assert!(matches!(resp, HookResponse::Ack));
        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature, None);
    }

    #[tokio::test]
    async fn test_cycle_start_then_heuristic_is_noop() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({"feature_cycle": "col-022"}),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let event2 = make_cycle_event(
            "PreToolUse",
            "s1",
            serde_json::json!({"feature_cycle": "col-099"}),
            Some("col-099".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event: event2 },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        let state = registry.get_state("s1").expect("session should exist");
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    // -- col-022: update_session_keywords unit tests --

    #[tokio::test]
    async fn test_update_session_keywords_valid() {
        let store = make_store().await;
        store
            .insert_session(&SessionRecord {
                session_id: "s1".to_string(),
                feature_cycle: None,
                agent_role: None,
                started_at: 1000,
                ended_at: None,
                status: SessionLifecycleStatus::Active,
                compaction_count: 0,
                outcome: None,
                total_injections: 0,
                keywords: None,
            })
            .await
            .unwrap();

        update_session_keywords(&store, "s1", r#"["a","b"]"#)
            .await
            .unwrap();

        let session = store
            .get_session("s1")
            .await
            .unwrap()
            .expect("session should exist");
        assert_eq!(session.keywords.as_deref(), Some(r#"["a","b"]"#));
    }

    #[tokio::test]
    async fn test_update_session_keywords_unknown_session() {
        let store = make_store().await;
        let result = update_session_keywords(&store, "unknown", "[]").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_session_keywords_malformed_json() {
        let store = make_store().await;
        store
            .insert_session(&SessionRecord {
                session_id: "s1".to_string(),
                feature_cycle: None,
                agent_role: None,
                started_at: 1000,
                ended_at: None,
                status: SessionLifecycleStatus::Active,
                compaction_count: 0,
                outcome: None,
                total_injections: 0,
                keywords: None,
            })
            .await
            .unwrap();

        update_session_keywords(&store, "s1", "not-json")
            .await
            .unwrap();

        let session = store
            .get_session("s1")
            .await
            .unwrap()
            .expect("session should exist");
        assert_eq!(session.keywords.as_deref(), Some("not-json"));
    }

    #[test]
    fn test_dispatch_cycle_start_matches_hook_constant() {
        assert_eq!(CYCLE_START_EVENT, "cycle_start");
        assert_eq!(CYCLE_STOP_EVENT, "cycle_stop");
    }

    #[tokio::test]
    async fn test_dispatch_cycle_start_empty_keywords_stored() {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        let registry = make_registry();
        registry.register_session("s1", None, None);

        store
            .insert_session(&SessionRecord {
                session_id: "s1".to_string(),
                feature_cycle: None,
                agent_role: None,
                started_at: unix_now_secs(),
                ended_at: None,
                status: SessionLifecycleStatus::Active,
                compaction_count: 0,
                outcome: None,
                total_injections: 0,
                keywords: None,
            })
            .await
            .unwrap();

        let event = make_cycle_event(
            CYCLE_START_EVENT,
            "s1",
            serde_json::json!({
                "feature_cycle": "col-022",
                "keywords": []
            }),
            Some("col-022".to_string()),
        );

        let _resp = dispatch_request(
            HookRequest::RecordEvent { event },
            &store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            &registry,
            &make_pending(),
            &make_services(&store, &embed, &vs, &es, &adapt),
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        let session = store
            .get_session("s1")
            .await
            .unwrap()
            .expect("session row should exist");
        assert_eq!(session.keywords.as_deref(), Some("[]"));
    }
}
