# http-listener (C1) -- `src/http/listener.rs`

## Purpose

TCP bind, TLS accept loop, pre-TLS connection limiting via semaphore (ADR-004), per-connection timeout, and spawning per-connection tasks using hyper. Returns a `JoinHandle` and bound address for lifecycle integration.

## Functions

### `start_http_listener`

```
pub(crate) async fn start_http_listener<S>(
    config: &HttpConfig,
    tls_acceptor: Option<TlsAcceptor>,
    service: S,
    shutdown: CancellationToken,
) -> Result<(JoinHandle<()>, SocketAddr), ServerError>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
:
    // 1. Bind TCP listener
    let addr = format!("{}:{}", config.bind_address, config.content_port)
    let tcp_listener = TcpListener::bind(&addr).await
        .map_err(|e| ServerError::Io(format!("HTTP bind on {addr}: {e}")))?

    let local_addr = tcp_listener.local_addr()
        .map_err(|e| ServerError::Io(format!("failed to get local address: {e}")))?

    tracing::info!("HTTP listener bound on {local_addr}")

    // 2. Create connection semaphore (ADR-004)
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_sessions))
    let timeout_duration = Duration::from_secs(config.connection_timeout_secs)

    // 3. Spawn accept loop
    let handle = tokio::spawn(accept_loop(
        tcp_listener,
        tls_acceptor,
        semaphore,
        service,
        timeout_duration,
        shutdown,
    ))

    return Ok((handle, local_addr))
```

### `accept_loop`

```
async fn accept_loop<S>(
    tcp_listener: TcpListener,
    tls_acceptor: Option<TlsAcceptor>,
    semaphore: Arc<Semaphore>,
    service: S,
    timeout: Duration,
    shutdown: CancellationToken,
)
where S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
      S::Error: ..., S::Future: Send,
:
    loop:
        // Select between accepting a new connection and shutdown signal
        tokio::select! {
            biased;  // check shutdown first

            _ = shutdown.cancelled() =>
                tracing::info!("HTTP accept loop shutting down")
                break

            accept_result = tcp_listener.accept() =>
                match accept_result:
                    Ok((tcp_stream, peer_addr)) =>
                        // --- PRE-TLS SEMAPHORE (ADR-004) ---
                        // Acquire permit IMMEDIATELY after accept, BEFORE TLS handshake.
                        // This prevents slow-TLS attacks from exhausting resources.
                        let permit = match semaphore.clone().try_acquire_owned():
                            Ok(permit) => permit,
                            Err(_) =>
                                // Connection limit reached. Drop TCP stream -> TCP RST to client.
                                tracing::warn!(
                                    peer = %peer_addr,
                                    "HTTP connection rejected: max_concurrent_sessions reached"
                                )
                                drop(tcp_stream)
                                continue

                        // Spawn per-connection task
                        let service = service.clone()
                        let tls = tls_acceptor.clone()
                        let shutdown_child = shutdown.child_token()

                        tokio::spawn(async move {
                            // RAII: permit is held for connection lifetime.
                            // Released automatically on drop (R-09 mitigation).
                            let _permit_guard = permit

                            // Wrap entire connection in timeout (ADR-004, R-18)
                            match tokio::time::timeout(timeout,
                                handle_connection(tcp_stream, tls, service, shutdown_child)
                            ).await:
                                Ok(Ok(())) => {}
                                Ok(Err(e)) =>
                                    tracing::debug!(peer = %peer_addr, error = %e, "connection error")
                                Err(_) =>
                                    tracing::debug!(peer = %peer_addr, "connection timed out")
                        })  // _permit_guard dropped here -- permit released

                    Err(e) =>
                        // Accept error (fd exhaustion, etc.). Log and continue.
                        tracing::error!(error = %e, "TCP accept error")
                        // Brief sleep to avoid tight loop on persistent errors
                        tokio::time::sleep(Duration::from_millis(100)).await
        }

    // After shutdown: no new connections accepted.
    // In-flight connection tasks continue until their own timeout or shutdown.
    tracing::info!("HTTP accept loop exited")
```

### `handle_connection`

```
async fn handle_connection<S>(
    tcp_stream: TcpStream,
    tls_acceptor: Option<TlsAcceptor>,
    service: S,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where S: Service<Request<Body>, Response = Response<Body>> + ...:

    // --- TLS HANDSHAKE (optional) ---
    // If TLS is enabled, wrap the TCP stream in TLS.
    // If TLS handshake fails (bad cert, client disconnect), the error propagates
    // and the permit is released via RAII in the caller.
    match tls_acceptor:
        Some(acceptor) =>
            let tls_stream = acceptor.accept(tcp_stream).await?
            serve_http(tls_stream, service, shutdown).await

        None =>
            // Plain HTTP (proxy-terminated deployment)
            serve_http(tcp_stream, service, shutdown).await
```

### `serve_http`

```
async fn serve_http<I, S>(
    io: I,
    service: S,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Service<Request<Body>, Response = Response<Body>> + ...,
:
    // Build hyper HTTP/1.1 connection
    // NOTE: Start with HTTP/1.1 only. rmcp's StreamableHttpService uses SSE
    // which works over HTTP/1.1. HTTP/2 support can be added later if needed.
    let conn = hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades()  // SSE may need upgrades

    // Pin the connection future
    let mut conn = std::pin::pin!(conn)

    // Select between connection completion and shutdown
    tokio::select! {
        result = &mut conn =>
            // Connection completed normally or with error
            result.map_err(|e| e.into())

        _ = shutdown.cancelled() =>
            // Graceful shutdown: allow in-flight request to complete
            // hyper's graceful_shutdown stops accepting new requests
            // but finishes the current one.
            conn.as_mut().graceful_shutdown()
            // Wait for the in-flight request to complete
            conn.await.map_err(|e| e.into())
    }
```

## Connection Lifecycle State Machine

```
[TCP Accept]
    |
    v
[Semaphore Acquire] -- FAIL --> [TCP RST / Drop]
    |
    | (permit held, RAII guard)
    v
[Timeout Wrapper Starts]
    |
    v
[TLS Handshake] -- if enabled
    | FAIL --> [permit released via RAII]
    v
[HTTP/1.1 Serve]
    |
    +-- Normal completion --> [permit released]
    +-- Timeout fired ------> [permit released]
    +-- Shutdown signal ----> [graceful_shutdown -> permit released]
    +-- Connection error ---> [permit released]
```

## Error Handling

| Error Case | Behavior | Notes |
|-----------|----------|-------|
| Bind failure | `ServerError::Io` | Server refuses to start |
| Connection limit reached | TCP RST (drop stream) | ADR-004: pre-TLS rejection |
| TLS handshake failure | Log debug, drop connection | Permit released via RAII (R-09) |
| Accept loop error | Log error, continue | Brief sleep prevents tight loop |
| Connection timeout | Drop connection | 30s default (R-18) |
| Shutdown signal | Graceful: finish in-flight, reject new | CancellationToken propagation |
| hyper serve error | Log debug, drop connection | Permit released via RAII |

## Key Test Scenarios

1. **Bind and accept**: Start listener on port 0. Connect via TCP. Verify connection accepted.
2. **Connection limit**: Open `max_concurrent_sessions` connections. Verify next connection is refused (R-04).
3. **Semaphore recovery**: Open and close connections. Verify semaphore permits fully recovered (R-09).
4. **TLS handshake**: Connect with TLS client to TLS-enabled listener. Verify handshake completes.
5. **Plain HTTP**: Connect to non-TLS listener. Verify HTTP/1.1 works.
6. **Connection timeout**: Connect but send no data. Verify disconnected after `connection_timeout_secs` (R-18).
7. **Shutdown stops accept**: Trigger shutdown. Verify no new connections accepted.
8. **Graceful drain**: Start request, trigger shutdown mid-request. Verify request completes before connection closes (R-08).
9. **Port 0**: Bind to port 0. Verify actual port returned in `SocketAddr`.
10. **TLS handshake failure**: Connect without TLS to TLS listener. Verify error handled, permit released.
