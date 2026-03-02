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
use unimatrix_engine::auth;
use unimatrix_engine::wire::{
    HookRequest, HookResponse, ERR_INVALID_PAYLOAD, ERR_UNKNOWN_REQUEST, MAX_PAYLOAD_SIZE,
};
use unimatrix_store::Store;

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
        accept_loop(listener, store, server_uid, server_version, socket_path_display).await;
    });

    Ok((handle, guard))
}

/// Accept loop: waits for connections and spawns per-connection handlers.
///
/// Never panics -- errors in accept are logged and the loop continues (R-19).
async fn accept_loop(
    listener: tokio::net::UnixListener,
    store: Arc<Store>,
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let store = Arc::clone(&store);
                let version = server_version.clone();

                // Per-connection handler in its own task (panic isolation -- R-19)
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, store, server_uid, version).await {
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

    // Dispatch request
    let response = dispatch_request(request, &store, &server_version);

    // Write response frame
    write_response(&mut writer, &response).await?;

    Ok(())
}

/// Dispatch a hook request and return the appropriate response.
///
/// col-006: All handlers log and return Ack. No database writes (ADR-007).
fn dispatch_request(
    request: HookRequest,
    _store: &Arc<Store>,
    server_version: &str,
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

        // Future request types return error (stubs not handled in col-006)
        _ => HookResponse::Error {
            code: ERR_UNKNOWN_REQUEST,
            message: "request type not implemented".into(),
        },
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

    #[test]
    fn dispatch_ping_returns_pong() {
        let store = Arc::new(
            Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap(),
        );
        let response = dispatch_request(HookRequest::Ping, &store, "0.1.0");
        match response {
            HookResponse::Pong { server_version } => {
                assert_eq!(server_version, "0.1.0");
            }
            _ => panic!("expected Pong"),
        }
    }

    #[test]
    fn dispatch_session_register_returns_ack() {
        let store = Arc::new(
            Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap(),
        );
        let response = dispatch_request(
            HookRequest::SessionRegister {
                session_id: "s1".to_string(),
                cwd: "/work".to_string(),
                agent_role: None,
                feature: None,
            },
            &store,
            "0.1.0",
        );
        assert!(matches!(response, HookResponse::Ack));
    }

    #[test]
    fn dispatch_session_close_returns_ack() {
        let store = Arc::new(
            Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap(),
        );
        let response = dispatch_request(
            HookRequest::SessionClose {
                session_id: "s1".to_string(),
                outcome: Some("success".to_string()),
                duration_secs: 60,
            },
            &store,
            "0.1.0",
        );
        assert!(matches!(response, HookResponse::Ack));
    }

    #[test]
    fn dispatch_record_event_returns_ack() {
        let store = Arc::new(
            Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap(),
        );
        let event = unimatrix_engine::wire::ImplantEvent {
            event_type: "test".to_string(),
            session_id: "s1".to_string(),
            timestamp: 0,
            payload: serde_json::json!({}),
        };
        let response =
            dispatch_request(HookRequest::RecordEvent { event }, &store, "0.1.0");
        assert!(matches!(response, HookResponse::Ack));
    }

    #[test]
    fn dispatch_unknown_returns_error() {
        let store = Arc::new(
            Store::open(&tempfile::TempDir::new().unwrap().path().join("test.redb")).unwrap(),
        );
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
            "0.1.0",
        );
        match response {
            HookResponse::Error { code, .. } => {
                assert_eq!(code, ERR_UNKNOWN_REQUEST);
            }
            _ => panic!("expected Error"),
        }
    }
}
