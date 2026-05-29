# Test Plan: HTTP Listener (`src/http/listener.rs`)

Covers: C1 — TCP bind, TLS accept loop, connection limiting, per-connection spawning
Risks: R-04 (connection flood), R-09 (semaphore leak), R-18 (connection timeout)

## Integration Tests

These tests require a running TCP listener. Use `start_http_listener` with port 0 (OS-assigned) and TLS disabled for test speed.

### T-HL-01: test_listener_binds_and_accepts_connection
- **Arrange**: Call `start_http_listener` with port 0, TLS disabled, mock service, CancellationToken.
- **Act**: Connect via TCP to the returned SocketAddr.
- **Assert**: Connection accepted. Service receives the request.

### T-HL-02: test_listener_returns_bound_address
- **Arrange**: Call `start_http_listener` with port 0.
- **Act**: Capture the returned `SocketAddr`.
- **Assert**: Address port is non-zero (OS assigned a real port). Address is connectable.

### T-HL-03: test_listener_with_tls_accepts_tls_connection
- **Arrange**: Generate self-signed cert+key. Build TlsAcceptor. Call `start_http_listener` with TLS.
- **Act**: Connect via `tokio-rustls` TLS client to the returned address.
- **Assert**: TLS handshake completes. Service receives the request over encrypted channel.

### T-HL-04: test_listener_without_tls_accepts_plain_http
- **Arrange**: Call `start_http_listener` with `tls = None`.
- **Act**: Connect via plain TCP and send HTTP request.
- **Assert**: HTTP response received. No TLS handshake attempted.

### T-HL-05: test_connection_limit_enforced
- **Risk**: R-04
- **Arrange**: Call `start_http_listener` with `max_concurrent_sessions = 3`.
- **Act**: Open 3 TCP connections (hold them open). Attempt 4th connection.
- **Assert**: 4th connection is rejected (TCP RST or immediate close). First 3 connections remain functional.

### T-HL-06: test_connection_limit_releases_on_close
- **Risk**: R-04
- **Arrange**: `max_concurrent_sessions = 2`. Open 2 connections.
- **Act**: Close 1 connection. Open a new connection.
- **Assert**: New connection is accepted (semaphore permit released by closed connection).

### T-HL-07: test_uds_not_starved_under_http_load
- **Risk**: R-04
- **Arrange**: Start full server with HTTP enabled and `max_concurrent_sessions = 4`. Open 4 HTTP connections sending continuous requests.
- **Act**: Send a UDS tool call via existing MCP path.
- **Assert**: UDS tool call completes within normal latency bounds (< 5 seconds). HTTP load does not starve UDS.

### T-HL-08: test_semaphore_recovery_after_tls_handshake_failure
- **Risk**: R-09
- **Arrange**: `max_concurrent_sessions = 2`. Start listener with TLS enabled.
- **Act**: Connect via plain TCP (no TLS handshake) — causes handshake failure. Then open a valid TLS connection.
- **Assert**: Valid connection succeeds (semaphore permit was released despite handshake failure).

### T-HL-09: test_semaphore_recovery_after_malformed_http
- **Risk**: R-09
- **Arrange**: `max_concurrent_sessions = 2`. Start listener without TLS.
- **Act**: Connect and send garbage bytes (not valid HTTP). Wait for connection to close. Then open a valid connection.
- **Assert**: Valid connection succeeds.

### T-HL-10: test_semaphore_recovery_sequential_connections
- **Risk**: R-09
- **Arrange**: `max_concurrent_sessions = 1`.
- **Act**: Open and close 10 connections sequentially.
- **Assert**: All 10 succeed. No permit leak accumulates across iterations.

### T-HL-11: test_idle_connection_timeout
- **Risk**: R-18
- **Arrange**: Start listener with `connection_timeout_secs = 2` (short for testing).
- **Act**: Open TCP connection, complete TLS handshake (if enabled), send no HTTP data. Wait 3 seconds.
- **Assert**: Connection is dropped by server.

### T-HL-12: test_partial_request_timeout
- **Risk**: R-18
- **Arrange**: Start listener with `connection_timeout_secs = 2`.
- **Act**: Open connection, send partial HTTP request headers (e.g., "POST / HTTP/1.1\r\n" but no body). Stall.
- **Assert**: Connection is dropped after timeout.

### T-HL-13: test_active_connection_not_timed_out
- **Risk**: R-18
- **Arrange**: Start listener with `connection_timeout_secs = 5`.
- **Act**: Open connection, send valid HTTP request, receive response, immediately send another request (within timeout).
- **Assert**: Second request succeeds. Active connections are not prematurely timed out.

## Required Edge-Case Tests

### T-HL-14: test_shutdown_stops_accepting_new_connections
- **Arrange**: Start listener with CancellationToken.
- **Act**: Cancel the token. Attempt a new TCP connection.
- **Assert**: New connection is rejected. Listener is not accepting.

### T-HL-15: test_port_already_in_use_returns_error
- **Arrange**: Bind a TcpListener on port N. Call `start_http_listener` on the same port N.
- **Act**: Observe result.
- **Assert**: Returns `Err` with "address already in use" error.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-01 | T-HL-01, T-HL-02 |
| AC-09 | T-HL-03 |
| AC-10 | T-HL-04 |
| AC-21 | T-HL-11, T-HL-12 (timeout); body size in path-router.md |
| AC-22 | T-HL-05, T-HL-06 |
