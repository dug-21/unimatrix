## ADR-004: Pre-TLS Connection Limiting via Semaphore

### Context

The HTTP listener shares the tokio runtime with UDS listeners, background ticks, NLI inference, and the write queue (SR-08). Unbounded HTTP connections could starve background tasks. TLS handshakes are particularly expensive -- a slow-TLS attack (slowloris variant) can exhaust runtime threads by holding connections in the handshake phase indefinitely.

AC-22 requires configurable maximum concurrent HTTP sessions (default 32, matching the UDS MCP listener's `MAX_CONCURRENT_SESSIONS`).

Three placement options for the connection limit:
1. **Pre-TLS (at TCP accept)**: `Arc<Semaphore>` acquired immediately after `TcpListener::accept()`, before TLS handshake. Connections beyond the limit are accepted from the OS queue and immediately dropped.
2. **Post-TLS (in tower middleware)**: A tower `ConcurrencyLimit` layer wrapping the service stack. Connections complete TLS but requests are rejected. Does not prevent TLS-phase resource exhaustion.
3. **Both**: Pre-TLS semaphore + post-TLS tower limit. Defense in depth but redundant for the personal cloud tier.

### Decision

Pre-TLS semaphore (option 1). The listener acquires a permit from `Arc<Semaphore::new(max_concurrent_sessions)>` immediately after `TcpListener::accept()` succeeds. If no permit is available, the accepted TCP stream is dropped immediately (connection reset to the client). The permit is held for the lifetime of the connection task and released on drop.

This placement ensures:
- TLS handshakes count against the limit (SR-08 mitigation)
- Background tasks are never starved by connection floods
- The semaphore is zero-cost for connections within the limit

The connection timeout (`connection_timeout_secs`, default 30s) is enforced per-connection via `tokio::time::timeout` wrapping the entire connection task (TLS + HTTP + MCP session). This prevents slow-read attacks from holding permits indefinitely.

### Consequences

Easier: Connection limiting is enforced at the earliest possible point. No TLS CPU waste on connections that will be rejected. Simple to reason about -- one semaphore, one timeout.

Harder: Clients beyond the connection limit see a TCP RST (connection reset) rather than an HTTP 503 with a helpful error message. This is acceptable for the personal cloud tier (single-user) but may need a post-TLS graceful rejection layer for enterprise (W2-3 scope).
