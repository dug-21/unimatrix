# Pseudocode: uds-listener

## Purpose

Add a Unix domain socket listener to the MCP server that accepts hook process connections, authenticates them, dispatches requests, and returns responses. Integrates into server startup/shutdown. Lives in `unimatrix-server/src/uds_listener.rs`.

## File: crates/unimatrix-server/src/uds_listener.rs

### SocketGuard (RAII)

```
struct SocketGuard {
    path: PathBuf,
}

impl SocketGuard {
    fn new(path: PathBuf) -> Self:
        Self { path }
}

impl Drop for SocketGuard {
    fn drop(&mut self):
        if let Err(e) = fs::remove_file(&self.path):
            if e.kind() != io::ErrorKind::NotFound:
                tracing::warn!(
                    error = %e,
                    path = %self.path.display(),
                    "failed to remove socket file on drop"
                )
}
```

### Stale Socket Handling

```
fn handle_stale_socket(socket_path: &Path) -> io::Result<()>:
    // Unconditional unlink per ADR-004
    // Called after PidGuard acquisition, so any existing socket is stale
    match fs::remove_file(socket_path):
        Ok(()) =>
            tracing::info!(path = %socket_path.display(), "removed stale socket file")
        Err(e) if e.kind() == io::ErrorKind::NotFound =>
            // No stale socket -- normal case
            ()
        Err(e) =>
            tracing::warn!(
                error = %e,
                path = %socket_path.display(),
                "failed to remove stale socket file"
            )
            return Err(e)

    Ok(())
```

### Bind and Start Listener

```
async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    server_uid: u32,
    server_version: String,
) -> io::Result<(tokio::task::JoinHandle<()>, SocketGuard)>:
    // Bind the listener
    let listener = tokio::net::UnixListener::bind(socket_path)?

    // Set socket file permissions to 0o600 (owner-only)
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?

    tracing::info!(path = %socket_path.display(), "UDS listener bound")

    // Create SocketGuard for RAII cleanup
    let guard = SocketGuard::new(socket_path.to_path_buf())

    // Clone values for the accept loop task
    let socket_path_display = socket_path.display().to_string()

    // Spawn accept loop
    let handle = tokio::spawn(async move {
        accept_loop(listener, store, server_uid, server_version, socket_path_display).await
    })

    Ok((handle, guard))
```

### Accept Loop

```
async fn accept_loop(
    listener: tokio::net::UnixListener,
    store: Arc<Store>,
    server_uid: u32,
    server_version: String,
    socket_path_display: String,
):
    loop:
        match listener.accept().await:
            Ok((stream, _addr)) =>
                let store = Arc::clone(&store)
                let version = server_version.clone()

                // Spawn per-connection handler (panic isolation -- R-19)
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, store, server_uid, version).await:
                        tracing::warn!(error = %e, "UDS connection handler error")
                })

            Err(e) =>
                // Accept error (e.g., too many open files)
                // Log and continue -- do not crash the accept loop (R-19)
                tracing::warn!(
                    error = %e,
                    socket = socket_path_display,
                    "UDS accept error, continuing"
                )
                // Brief pause to avoid tight error loop
                tokio::time::sleep(Duration::from_millis(50)).await
```

### Connection Handler

```
async fn handle_connection(
    stream: tokio::net::UnixStream,
    store: Arc<Store>,
    server_uid: u32,
    server_version: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>:
    // Convert to std UnixStream for auth (peer_cred is on std)
    let std_stream = stream.into_std()?

    // Authenticate (Layer 2 + Layer 3)
    let _creds = match authenticate_connection(&std_stream, server_uid):
        Ok(creds) =>
            tracing::debug!(uid = creds.uid, pid = ?creds.pid, "UDS connection authenticated")
            creds
        Err(e) =>
            // Auth failure: close connection, no response (ADR-003)
            tracing::warn!(error = %e, "UDS authentication failed, closing connection")
            return Ok(())  // Not an error for the accept loop

    // Convert back to tokio stream for async I/O
    let stream = tokio::net::UnixStream::from_std(std_stream)?

    // Read request frame (use spawn_blocking for blocking read on std stream,
    // or use tokio AsyncRead on the tokio stream)
    //
    // Approach: use tokio::io::AsyncReadExt for non-blocking reads
    let (mut reader, mut writer) = stream.into_split()

    // Read 4-byte header
    let mut header = [0u8; 4]
    tokio::io::AsyncReadExt::read_exact(&mut reader, &mut header).await?
    let length = u32::from_be_bytes(header) as usize

    // Validate length
    if length == 0:
        let err_response = HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: "empty payload".into(),
        }
        write_response(&mut writer, &err_response).await?
        return Ok(())

    if length > MAX_PAYLOAD_SIZE:
        let err_response = HookResponse::Error {
            code: ERR_INVALID_PAYLOAD,
            message: format!("payload {} exceeds max {}", length, MAX_PAYLOAD_SIZE),
        }
        write_response(&mut writer, &err_response).await?
        return Ok(())

    // Read payload
    let mut buffer = vec![0u8; length]
    tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buffer).await?

    // Deserialize request
    let request: HookRequest = match serde_json::from_slice(&buffer):
        Ok(req) => req
        Err(e) =>
            let err_response = HookResponse::Error {
                code: ERR_INVALID_PAYLOAD,
                message: format!("invalid request: {e}"),
            }
            write_response(&mut writer, &err_response).await?
            return Ok(())

    // Dispatch request
    let response = dispatch_request(request, &store, &server_version).await

    // Write response frame
    write_response(&mut writer, &response).await?

    Ok(())
```

### Request Dispatch

```
async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    server_version: &str,
) -> HookResponse:
    match request:
        HookRequest::Ping =>
            HookResponse::Pong {
                server_version: server_version.to_string(),
            }

        HookRequest::SessionRegister { session_id, cwd, agent_role, feature } =>
            tracing::info!(
                session_id,
                cwd,
                agent_role = ?agent_role,
                feature = ?feature,
                "UDS: session registered"
            )
            // col-006: log only, no persistence (ADR-007)
            HookResponse::Ack

        HookRequest::SessionClose { session_id, outcome, duration_secs } =>
            tracing::info!(
                session_id,
                outcome = ?outcome,
                duration_secs,
                "UDS: session closed"
            )
            // col-006: log only, no persistence (ADR-007)
            HookResponse::Ack

        HookRequest::RecordEvent(event) =>
            tracing::info!(
                event_type = event.event_type,
                session_id = event.session_id,
                "UDS: event recorded"
            )
            HookResponse::Ack

        HookRequest::RecordEvents(events) =>
            tracing::info!(count = events.len(), "UDS: batch events recorded")
            HookResponse::Ack

        // Future request types return error (stubs not handled)
        _ =>
            HookResponse::Error {
                code: ERR_UNKNOWN_REQUEST,
                message: "request type not implemented".into(),
            }
```

### Response Writer

```
async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &HookResponse,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>:
    let payload = serde_json::to_vec(response)?
    let length = payload.len() as u32
    tokio::io::AsyncWriteExt::write_all(writer, &length.to_be_bytes()).await?
    tokio::io::AsyncWriteExt::write_all(writer, &payload).await?
    tokio::io::AsyncWriteExt::flush(writer).await?
    Ok(())
```

## Integration: main.rs Modifications

The Delivery Leader's hook-subcommand pseudocode covers main.rs changes. For the uds-listener side:

```
// In main.rs server path, after PidGuard acquisition:
handle_stale_socket(&paths.socket_path)?;

// After building shared resources:
let server_uid = get_server_uid();  // obtained once
let (uds_handle, socket_guard) = start_uds_listener(
    &paths.socket_path,
    Arc::clone(&store),
    server_uid,
    env!("CARGO_PKG_VERSION").to_string(),
).await?;

// Add to LifecycleHandles:
let lifecycle_handles = LifecycleHandles {
    store,
    vector_index,
    vector_dir: paths.vector_dir.clone(),
    registry,
    audit,
    adapt_service,
    data_dir: paths.data_dir.clone(),
    socket_guard: Some(socket_guard),  // NEW
    uds_handle: Some(uds_handle),      // NEW for shutdown coordination
};
```

## Integration: shutdown.rs Modifications

Extend LifecycleHandles:

```
struct LifecycleHandles {
    // ... existing fields ...
    pub socket_guard: Option<SocketGuard>,
    pub uds_handle: Option<tokio::task::JoinHandle<()>>,
}
```

In `graceful_shutdown`, before vector dump:

```
// Step 0: Stop UDS listener
if let Some(handle) = handles.uds_handle.take():
    handle.abort()  // Signal the accept loop to stop
    // Wait up to 1s for in-flight handlers to complete
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await

// Step 0b: Remove socket file via SocketGuard drop
drop(handles.socket_guard.take())

// Then continue with existing shutdown steps...
```

## Design Notes

1. **Panic isolation (R-19)**: Each connection handler runs in its own `tokio::spawn` task. A panic in one handler does not affect the accept loop or the stdio MCP transport. The accept loop itself catches errors and continues.

2. **Connection model**: Single-request-per-connection. Handler reads one request, dispatches, writes one response, then the connection closes (handler returns, stream dropped).

3. **Auth before processing**: Authentication happens before any request parsing. If auth fails, the connection is closed with no response. This prevents untrusted input from reaching the dispatcher.

4. **No tokio runtime in auth**: The `authenticate_connection` function uses `std::os::unix::net::UnixStream`. We convert `tokio::net::UnixStream` to std for auth, then back to tokio for async I/O. This reuses the auth module from unimatrix-engine without adding tokio dependencies there.

5. **col-006 dispatch is log-only**: SessionRegister, SessionClose, RecordEvent all log and return Ack. No database writes (ADR-007). col-010 changes the dispatch to persist session data.

6. **Socket permissions**: Set to 0o600 immediately after bind. There is a brief window between bind and set_permissions where the socket has default umask permissions. This is acceptable for local single-user development.

7. **Shutdown drain**: The accept loop is aborted, then in-flight handler tasks have 1 second to complete. After timeout, socket_guard is dropped (removing socket file). This ensures hook processes connecting during shutdown find no socket.

## Error Handling

- `start_uds_listener`: Bind failure -> io::Error propagated (server starts stdio-only)
- `accept_loop`: Accept error -> log warning, sleep 50ms, continue
- `handle_connection`: Auth failure -> close silently. Read/parse error -> send Error response. Dispatch error -> Error response.
- `write_response`: Write failure -> handler returns error (connection was closing anyway)
- `graceful_shutdown`: UDS abort failure -> swallowed. Socket removal failure -> warning logged.

## Key Test Scenarios

1. Start listener on tempdir socket -> socket file exists with mode 0o600
2. Connect and send Ping -> receive Pong with correct version
3. Connect and send SessionRegister -> receive Ack
4. Connect and send malformed JSON -> receive Error response
5. Two concurrent connections both receive responses
6. Auth failure (different UID) -> connection closed, no response
7. Oversized payload -> Error response, no OOM
8. Handler panic does not crash accept loop or stdio
9. handle_stale_socket removes existing socket file
10. handle_stale_socket succeeds when no socket exists (NotFound)
11. Shutdown: socket file removed after graceful_shutdown
12. Rapid connect-disconnect (100x) -> no fd leak, no task leak
