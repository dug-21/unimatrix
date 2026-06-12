use super::*;
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Mock service for testing — returns 200 OK for all requests
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockService;

impl Service<Request<Incoming>> for MockService {
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Incoming>) -> Self::Future {
        Box::pin(async {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(
                    Full::new(Bytes::from("ok"))
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("static response"))
        })
    }
}

// ---------------------------------------------------------------------------
// SlowService: holds connection open until dropped/cancelled
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SlowService {
    /// How long to stall before responding.
    delay: Duration,
}

impl Service<Request<Incoming>> for SlowService {
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Incoming>) -> Self::Future {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(
                    Full::new(Bytes::from("slow"))
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .expect("static response"))
        })
    }
}

// ---------------------------------------------------------------------------
// Helper: make a minimal HTTP config for tests
// ---------------------------------------------------------------------------

fn test_config(max_sessions: usize, timeout_secs: u64) -> HttpConfig {
    HttpConfig {
        enabled: true,
        content_port: 0, // OS-assigned
        bind_address: "127.0.0.1".to_string(),
        max_concurrent_sessions: max_sessions,
        max_request_body_bytes: 1_048_576,
        connection_timeout_secs: timeout_secs,
        allowed_origins: Vec::new(),
    }
}

/// Send a minimal valid HTTP/1.1 GET request and return the response status line.
async fn send_http_get(addr: SocketAddr) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf[..n]).to_string())
}

/// Deadline-poll a GET until it returns a 200, retrying transient failures.
///
/// Per-connection permits are RAII-held by the spawned task and released only
/// AFTER that task ends — asynchronously, with no happens-before edge a caller
/// can observe. So the next acquire can legitimately race the prior release: a
/// not-yet-released permit makes the acceptor drop the stream, surfacing as a
/// connect/read error or a non-200. Polling the OBSERVABLE (a 200) within a
/// bounded deadline asserts EVENTUAL recovery without a fixed-sleep guess. A
/// genuine permit-leak still fails: the deadline expires. Uses
/// `tokio::time::sleep().await` (never `std::thread::sleep`) to keep the
/// current-thread reactor alive.
async fn send_http_get_until_200(addr: SocketAddr) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match send_http_get(addr).await {
            Ok(resp) if resp.contains("200") => return,
            other => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "timed out waiting for a 200 (permit never recovered); last: {other:?}"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

// T-HL-01: test_listener_binds_and_accepts_connection
#[tokio::test(flavor = "multi_thread")]
async fn test_listener_binds_and_accepts_connection() {
    let config = test_config(32, 30);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    let response = send_http_get(addr).await.expect("request must succeed");
    assert!(
        response.contains("200"),
        "expected 200 in response: {response}"
    );

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-02: test_listener_returns_bound_address
#[tokio::test(flavor = "multi_thread")]
async fn test_listener_returns_bound_address() {
    let config = test_config(32, 30);
    let token = CancellationToken::new();

    let (_handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    assert_ne!(addr.port(), 0, "bound port must be non-zero (OS-assigned)");

    // Verify address is connectable
    let stream = tokio::net::TcpStream::connect(addr).await;
    assert!(stream.is_ok(), "address must be connectable");

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), _handle).await;
}

// T-HL-04: test_listener_without_tls_accepts_plain_http
#[tokio::test(flavor = "multi_thread")]
async fn test_listener_without_tls_accepts_plain_http() {
    let config = test_config(32, 30);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    let response = send_http_get(addr).await.expect("HTTP request must work");
    assert!(
        response.contains("HTTP/1.1"),
        "response must be HTTP/1.1: {response}"
    );

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-05: test_connection_limit_enforced
#[tokio::test(flavor = "multi_thread")]
async fn test_connection_limit_enforced() {
    let config = test_config(3, 30);
    let token = CancellationToken::new();

    // Use SlowService to keep connections alive
    let svc = SlowService {
        delay: Duration::from_secs(60),
    };
    let (handle, addr) = start_http_listener(&config, None, svc, token.clone())
        .await
        .expect("listener must bind");

    // Open 3 connections and send requests to keep them alive
    let mut clients = Vec::new();
    for _ in 0..3 {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        clients.push(stream);
    }

    // Give acceptor time to process all connections
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4th connection: should be rejected (TCP RST / immediate close)
    let mut fourth = tokio::net::TcpStream::connect(addr).await.unwrap();
    // Give the acceptor time to accept + drop the 4th stream
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut buf = [0u8; 1];
    let result = tokio::time::timeout(Duration::from_secs(2), fourth.read(&mut buf)).await;

    match result {
        Ok(Ok(0)) => { /* EOF: correct, connection was rejected */ }
        Ok(Err(_)) => { /* Connection error: also acceptable */ }
        Ok(Ok(_n)) => {
            panic!("4th connection should have been rejected but received data");
        }
        Err(_timeout) => {
            panic!("4th connection was not closed by the server within timeout");
        }
    }

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-06: test_connection_limit_releases_on_close
#[tokio::test(flavor = "multi_thread")]
async fn test_connection_limit_releases_on_close() {
    let config = test_config(2, 30);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Fill both slots with HTTP requests that complete
    for _ in 0..2 {
        let resp = send_http_get(addr).await.expect("request must succeed");
        assert!(resp.contains("200"));
    }

    // Give time for permits to be released
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should be able to open a new connection after previous ones closed
    let resp = send_http_get(addr).await.expect("request must succeed");
    assert!(
        resp.contains("200"),
        "new connection should succeed after previous released"
    );

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-10: test_semaphore_recovery_sequential_connections
#[tokio::test(flavor = "multi_thread")]
async fn test_semaphore_recovery_sequential_connections() {
    let config = test_config(1, 30);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Open and close 10 connections sequentially. With a single permit, the next
    // connect can race the prior task's async permit release; poll for EVENTUAL
    // recovery (a 200) per connection rather than asserting immediate success.
    for _ in 0..10 {
        send_http_get_until_200(addr).await;
    }

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-11: test_idle_connection_timeout
#[tokio::test(flavor = "multi_thread")]
async fn test_idle_connection_timeout() {
    let config = test_config(32, 2); // 2-second timeout
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Open connection but send no data — should be timed out
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Wait for longer than the timeout
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Connection should be closed by server
    let mut buf = [0u8; 1];
    let result = stream.read(&mut buf).await;
    match result {
        Ok(0) => { /* EOF: correct, connection timed out */ }
        Err(_) => { /* Connection error: also acceptable */ }
        Ok(n) => {
            panic!("idle connection should have timed out, but received {n} bytes");
        }
    }

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-12: test_partial_request_timeout
#[tokio::test(flavor = "multi_thread")]
async fn test_partial_request_timeout() {
    let config = test_config(32, 2); // 2-second timeout
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Send partial HTTP headers then stall
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .unwrap();
    // Intentionally don't send the final \r\n or body

    // Wait for longer than the timeout
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Connection should be dropped after timeout
    let mut buf = [0u8; 1];
    let result = stream.read(&mut buf).await;
    match result {
        Ok(0) | Err(_) => { /* Expected: connection dropped */ }
        Ok(n) => {
            panic!("partial request should have timed out, but received {n} bytes");
        }
    }

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-14: test_shutdown_stops_accepting_new_connections
#[tokio::test(flavor = "multi_thread")]
async fn test_shutdown_stops_accepting_new_connections() {
    let config = test_config(32, 30);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Cancel the token
    token.cancel();

    // Wait for accept loop to exit
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

    // Attempt a new connection — should fail
    let result =
        tokio::time::timeout(Duration::from_secs(1), tokio::net::TcpStream::connect(addr)).await;

    match result {
        Ok(Ok(mut stream)) => {
            // Connection established to the OS backlog, but server should not respond
            let mut buf = [0u8; 1];
            let read_result =
                tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
            match read_result {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => { /* Expected: no service */ }
                Ok(Ok(_)) => {
                    panic!("server should not serve after shutdown");
                }
            }
        }
        Ok(Err(_)) => { /* Connection refused: expected */ }
        Err(_) => { /* Timeout: also acceptable */ }
    }
}

// T-HL-15: test_port_already_in_use_returns_error
#[tokio::test(flavor = "multi_thread")]
async fn test_port_already_in_use_returns_error() {
    // Bind a listener on an OS-assigned port
    let blocker = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = blocker.local_addr().unwrap().port();

    let config = HttpConfig {
        enabled: true,
        content_port: port,
        bind_address: "127.0.0.1".to_string(),
        max_concurrent_sessions: 32,
        max_request_body_bytes: 1_048_576,
        connection_timeout_secs: 30,
        allowed_origins: Vec::new(),
    };

    let token = CancellationToken::new();
    let result = start_http_listener(&config, None, MockService, token).await;
    assert!(result.is_err(), "must fail when port is already in use");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("HTTP bind on"),
        "error must mention bind failure: {err_msg}"
    );
}

// T-HL-09: test_semaphore_recovery_after_malformed_http
#[tokio::test(flavor = "multi_thread")]
async fn test_semaphore_recovery_after_malformed_http() {
    let config = test_config(2, 5);
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Send garbage bytes (not valid HTTP)
    {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GARBAGE DATA\r\n\r\n").await.unwrap();
        // Wait for server to close connection
        let mut buf = [0u8; 256];
        let _ = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await;
    }

    // The malformed connection's permit is released asynchronously when its task
    // ends — no observable happens-before edge. Poll for EVENTUAL recovery (a
    // 200) within a bounded deadline instead of a fixed 200ms sleep; a genuine
    // permit-leak still fails when the deadline expires.
    send_http_get_until_200(addr).await;

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

// T-HL-13: test_active_connection_not_timed_out (within timeout window)
#[tokio::test(flavor = "multi_thread")]
async fn test_active_connection_not_timed_out() {
    let config = test_config(32, 5); // 5-second timeout
    let token = CancellationToken::new();

    let (handle, addr) = start_http_listener(&config, None, MockService, token.clone())
        .await
        .expect("listener must bind");

    // Send first request, get response
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    let first_resp = String::from_utf8_lossy(&buf[..n]);
    assert!(first_resp.contains("200"), "first request must succeed");

    // Immediately send second request on same connection (within timeout)
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    let mut buf2 = vec![0u8; 4096];
    let result = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf2)).await;
    match result {
        Ok(Ok(n)) if n > 0 => {
            let second_resp = String::from_utf8_lossy(&buf2[..n]);
            assert!(
                second_resp.contains("200"),
                "second request must succeed: {second_resp}"
            );
        }
        other => {
            panic!("second request on keep-alive connection should succeed: {other:?}");
        }
    }

    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}
