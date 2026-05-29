# Agent Report: vnc-021-agent-8-http-listener

## Component
C1 — HTTP Listener (`crates/unimatrix-server/src/http/listener.rs`)

## Files Modified
- `crates/unimatrix-server/src/http/listener.rs` — full implementation (308 lines)
- `crates/unimatrix-server/src/http/listener/tests.rs` — test suite (478 lines)

## Implementation Summary

Replaced the empty placeholder with the full HTTP listener per pseudocode `http-listener.md`.

### Functions Implemented
- `start_http_listener<S>()` — binds TcpListener, creates Arc<Semaphore>, spawns accept loop, returns (JoinHandle, SocketAddr)
- `accept_loop<S>()` — tokio::select! loop: shutdown-biased, pre-TLS semaphore via try_acquire_owned (ADR-004), spawns per-connection tasks
- `handle_connection<S>()` — optional TLS handshake then serve_http
- `serve_http<I, S>()` — hyper http1::Builder serve_connection with graceful_shutdown on CancellationToken
- `TowerToHyperService<S>` — bridges tower::Service (&mut self) to hyper::service::Service (&self) via per-request clone

### Design Decisions
- Used `hyper_util::rt::TokioIo` to bridge tokio AsyncRead/AsyncWrite to hyper's IO traits
- Built a minimal `TowerToHyperService` adapter instead of adding `service` feature to hyper-util (avoids cargo feature creep)
- Error type for bind failure uses `ServerError::Config` (consistent with TLS config errors)
- The `#![allow(dead_code)]` inner attribute mirrors router.rs pattern — removed when C8 wires this into main.rs

## Tests: 12 passed, 0 failed

| Test ID | Test Name | Status |
|---------|-----------|--------|
| T-HL-01 | test_listener_binds_and_accepts_connection | PASS |
| T-HL-02 | test_listener_returns_bound_address | PASS |
| T-HL-04 | test_listener_without_tls_accepts_plain_http | PASS |
| T-HL-05 | test_connection_limit_enforced | PASS |
| T-HL-06 | test_connection_limit_releases_on_close | PASS |
| T-HL-09 | test_semaphore_recovery_after_malformed_http | PASS |
| T-HL-10 | test_semaphore_recovery_sequential_connections | PASS |
| T-HL-11 | test_idle_connection_timeout | PASS |
| T-HL-12 | test_partial_request_timeout | PASS |
| T-HL-13 | test_active_connection_not_timed_out | PASS |
| T-HL-14 | test_shutdown_stops_accepting_new_connections | PASS |
| T-HL-15 | test_port_already_in_use_returns_error | PASS |

### Tests Not Implemented (require TLS infrastructure from other agents)
- T-HL-03: TLS listener test — requires self-signed cert generation in test; deferred to integration test phase
- T-HL-07: UDS not starved under HTTP load — requires full server fixture; integration test
- T-HL-08: Semaphore recovery after TLS handshake failure — requires TLS listener + plain TCP client; integration test

## Issues
None. No blockers.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-004 (#4668) pre-TLS semaphore decision, UDS listener pattern (#316), and vnc-021 ADRs. Applied ADR-004 directly.
- Stored: nothing novel to store -- the TowerToHyperService pattern is standard hyper 1.x + tower bridging, documented in hyper-util upstream. No runtime-invisible gotchas discovered.
