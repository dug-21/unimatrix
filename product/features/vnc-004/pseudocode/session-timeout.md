# Pseudocode: session-timeout

## Purpose

Add a session idle timeout that bounds the maximum time the server can run without activity, preventing zombie servers when stdio connections break.

## File: crates/unimatrix-server/src/main.rs

### New constant

```
/// Maximum idle time before the server shuts down.
/// Prevents zombie servers when stdio connections break silently.
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60); // 30 minutes
```

### Modified section: session waiting + timeout wrapper

Current code:
```rust
let waiting = async { let _ = running.waiting().await; };
shutdown::graceful_shutdown(lifecycle_handles, waiting).await?;
```

New code:
```
let waiting = async {
    match tokio::time::timeout(SESSION_IDLE_TIMEOUT, running.waiting()).await {
        Ok(_) => {
            // Session closed normally (transport closed by client)
            tracing::info!("session closed by client");
        }
        Err(_elapsed) => {
            // Timeout fired — session was idle for SESSION_IDLE_TIMEOUT
            tracing::info!(
                timeout_secs = SESSION_IDLE_TIMEOUT.as_secs(),
                "session idle timeout reached, initiating shutdown"
            );
        }
    }
};
shutdown::graceful_shutdown(lifecycle_handles, waiting).await?;
```

### PidGuard integration in main()

The PidGuard must be acquired AFTER open_store_with_retry and held through the entire main() scope. The binding must be `_pid_guard` (prefixed underscore) to suppress unused warnings while keeping the guard alive until main() returns.

```
// Current:
if let Err(e) = pidfile::write_pid_file(&paths.pid_path) { ... }

// New:
let _pid_guard = match pidfile::PidGuard::acquire(&paths.pid_path) {
    Ok(guard) => {
        tracing::info!("PID guard acquired");
        Some(guard)
    }
    Err(e) => {
        tracing::warn!(error = %e, "failed to acquire PID guard; continuing without it");
        None
    }
};
```

Note: We wrap in Option because PidGuard failure is non-fatal (same as current write_pid_file failure). The guard is held as `Option<PidGuard>` — if Some, drop runs cleanup on exit.

## Error Handling

- Timeout expiry triggers graceful shutdown, not abrupt termination
- PidGuard acquisition failure is a warning, not an error (same behavior as current write_pid_file failure)
- All shutdown paths (normal close, timeout, error return from main) trigger PidGuard::drop

## Key Test Scenarios

1. Timeout fires and triggers graceful shutdown sequence
2. Normal session close completes before timeout
3. PidGuard survives through main() scope and drops on exit
