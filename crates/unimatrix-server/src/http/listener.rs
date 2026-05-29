//! TCP bind, TLS accept loop, connection limiting, per-connection task spawning.
//!
//! Implements pre-TLS connection limiting via `Arc<Semaphore>` (ADR-004),
//! per-connection timeout, and graceful shutdown via `CancellationToken`.
//! Returns a `JoinHandle` and bound `SocketAddr` for lifecycle integration.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::combinators::BoxBody;
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tower::Service;

use crate::error::ServerError;
use crate::infra::config::HttpConfig;

/// Start the HTTP listener, returning the accept loop handle and bound address.
///
/// Binds a `TcpListener` on `config.bind_address:config.content_port`, creates
/// a pre-TLS connection semaphore (ADR-004), and spawns the accept loop.
///
/// The service `S` must accept `Request<Incoming>` (hyper 1.x body type) and
/// return `Response<BoxBody<Bytes, Infallible>>`. This matches the
/// `StaticTokenAuth<PathRouter<Incoming>>` stack built by lifecycle integration.
pub async fn start_http_listener<S>(
    config: &HttpConfig,
    tls_acceptor: Option<TlsAcceptor>,
    service: S,
    shutdown: CancellationToken,
) -> Result<(JoinHandle<()>, SocketAddr), ServerError>
where
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, Infallible>>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    // 1. Bind TCP listener
    let addr = format!("{}:{}", config.bind_address, config.content_port);
    let tcp_listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| ServerError::Config(format!("HTTP bind on {addr}: {e}")))?;

    let local_addr = tcp_listener
        .local_addr()
        .map_err(|e| ServerError::Config(format!("failed to get local address: {e}")))?;

    tracing::info!("HTTP listener bound on {local_addr}");

    // 2. Create connection semaphore (ADR-004)
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_sessions));
    let timeout_duration = Duration::from_secs(config.connection_timeout_secs);

    // 3. Spawn accept loop
    let handle = tokio::spawn(accept_loop(
        tcp_listener,
        tls_acceptor,
        semaphore,
        service,
        timeout_duration,
        shutdown,
    ));

    Ok((handle, local_addr))
}

/// Accept loop: accepts TCP connections, enforces pre-TLS semaphore, spawns
/// per-connection tasks. Runs until `shutdown` is cancelled.
async fn accept_loop<S>(
    tcp_listener: TcpListener,
    tls_acceptor: Option<TlsAcceptor>,
    semaphore: Arc<Semaphore>,
    service: S,
    timeout: Duration,
    shutdown: CancellationToken,
) where
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, Infallible>>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    loop {
        // Select between accepting a new connection and shutdown signal.
        tokio::select! {
            biased; // check shutdown first

            _ = shutdown.cancelled() => {
                tracing::info!("HTTP accept loop shutting down");
                break;
            }

            accept_result = tcp_listener.accept() => {
                match accept_result {
                    Ok((tcp_stream, peer_addr)) => {
                        // --- PRE-TLS SEMAPHORE (ADR-004) ---
                        // Acquire permit IMMEDIATELY after accept, BEFORE TLS handshake.
                        // This prevents slow-TLS attacks from exhausting resources.
                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                // Connection limit reached. Drop TCP stream -> TCP RST to client.
                                tracing::warn!(
                                    peer = %peer_addr,
                                    "HTTP connection rejected: max_concurrent_sessions reached"
                                );
                                drop(tcp_stream);
                                continue;
                            }
                        };

                        // Spawn per-connection task
                        let svc = service.clone();
                        let tls = tls_acceptor.clone();
                        let shutdown_child = shutdown.child_token();

                        tokio::spawn(async move {
                            // RAII: permit is held for connection lifetime.
                            // Released automatically on drop (R-09 mitigation).
                            let _permit_guard = permit;

                            // Wrap entire connection in timeout (ADR-004, R-18)
                            match tokio::time::timeout(
                                timeout,
                                handle_connection(tcp_stream, tls, svc, shutdown_child),
                            )
                            .await
                            {
                                Ok(Ok(())) => {}
                                Ok(Err(e)) => {
                                    tracing::debug!(
                                        peer = %peer_addr,
                                        error = %e,
                                        "connection error"
                                    );
                                }
                                Err(_) => {
                                    tracing::debug!(
                                        peer = %peer_addr,
                                        "connection timed out"
                                    );
                                }
                            }
                            // _permit_guard dropped here -- permit released
                        });
                    }
                    Err(e) => {
                        // Accept error (fd exhaustion, etc.). Log and continue.
                        tracing::error!(error = %e, "TCP accept error");
                        // Brief sleep to avoid tight loop on persistent errors
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
    }

    // After shutdown: no new connections accepted.
    // In-flight connection tasks continue until their own timeout or shutdown.
    tracing::info!("HTTP accept loop exited");
}

/// Handle a single connection: optional TLS handshake, then HTTP/1.1 serving.
async fn handle_connection<S>(
    tcp_stream: tokio::net::TcpStream,
    tls_acceptor: Option<TlsAcceptor>,
    service: S,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, Infallible>>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    match tls_acceptor {
        Some(acceptor) => {
            let tls_stream = acceptor.accept(tcp_stream).await?;
            serve_http(tls_stream, service, shutdown).await
        }
        None => {
            // Plain HTTP (proxy-terminated deployment)
            serve_http(tcp_stream, service, shutdown).await
        }
    }
}

/// Serve HTTP/1.1 on an IO stream using hyper.
///
/// Uses `hyper::service::service_fn` to bridge the tower service (which uses
/// `&mut self` call convention) to hyper's service trait (which uses `&self`).
/// The tower service is cloned per-request via `service_fn` closure.
async fn serve_http<I, S>(
    io: I,
    service: S,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, Infallible>>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    // Bridge tower::Service to hyper::service::Service via service_fn.
    // hyper::service::Service uses &self (not &mut self), so we wrap in a
    // closure that clones the tower service per request.
    let hyper_service = TowerToHyperService::new(service);

    // Build hyper HTTP/1.1 connection.
    // HTTP/1.1 only -- rmcp's StreamableHttpService uses SSE which works
    // over HTTP/1.1. HTTP/2 support can be added later if needed.
    let conn = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(io), hyper_service)
        .with_upgrades(); // SSE may need upgrades

    // Pin the connection future
    let mut conn = std::pin::pin!(conn);

    // Select between connection completion and shutdown
    tokio::select! {
        result = &mut conn => {
            // Connection completed normally or with error
            result.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        }
        _ = shutdown.cancelled() => {
            // Graceful shutdown: allow in-flight request to complete.
            // hyper's graceful_shutdown stops accepting new requests
            // but finishes the current one.
            conn.as_mut().graceful_shutdown();
            // Wait for the in-flight request to complete
            conn.await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        }
    }
}

// ---------------------------------------------------------------------------
// TowerToHyperService — bridge tower::Service to hyper::service::Service
// ---------------------------------------------------------------------------

/// Adapter that bridges a `tower::Service` (mutable call: `&mut self`) to
/// `hyper::service::Service` (shared call: `&self`).
///
/// On each hyper `call(&self, req)`, clones the inner tower service and calls
/// it. This is the standard pattern for per-connection service usage in
/// hyper 1.x with tower middleware.
struct TowerToHyperService<S> {
    inner: S,
}

impl<S> TowerToHyperService<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> hyper::service::Service<Request<Incoming>> for TowerToHyperService<S>
where
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, Infallible>>>
        + Clone
        + Send
        + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let mut svc = self.inner.clone();
        Box::pin(async move { svc.call(req).await })
    }
}

impl<S: Clone> Clone for TowerToHyperService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
