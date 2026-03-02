## ADR-004: Socket Lifecycle Uses Unconditional Unlink After PidGuard

### Context

The MCP server needs to manage a UDS socket file alongside the existing PID file. Both are filesystem artifacts that must be created on startup and cleaned up on shutdown. The challenge is crash recovery: if the server is killed (SIGKILL), both files persist.

For PID files, vnc-004 solved this with `PidGuard` (RAII flock + cleanup on drop) plus `handle_stale_pid_file()` which checks if the recorded PID is alive and belongs to a unimatrix-server process. Socket files have different detection semantics — there is no "recorded PID" in a socket file.

Two approaches for stale socket detection were considered:
1. **Connect-to-detect:** Try `connect()` to the existing socket. If connection refused, the socket is stale. If connection succeeds, another server is running.
2. **Unconditional unlink:** Always `unlink()` the socket path before `bind()`. Rely on PidGuard for mutual exclusion.

### Decision

Use unconditional unlink after PidGuard establishes mutual exclusion.

**Startup order:**
1. `handle_stale_pid_file()` — detect and terminate any stale server process
2. `open_store_with_retry()` — acquire redb exclusive file lock
3. `PidGuard::acquire()` — acquire advisory flock on PID file
4. `std::fs::remove_file(socket_path)` — unconditional unlink (ignore NotFound)
5. `UnixListener::bind(socket_path)` — bind fresh socket
6. `std::fs::set_permissions(socket_path, 0o600)` — set owner-only permissions

By step 4, we know:
- Any stale server process has been terminated (step 1) or the database lock would have failed (step 2)
- We hold the PID file lock (step 3), so no other server can be starting concurrently
- Therefore, any existing socket file is definitively stale — safe to remove

This is simpler and more robust than connect-to-detect because:
- No race condition between connect() test and unlink() + bind()
- No handling of "connection succeeds but the other server is shutting down"
- No timeout needed for the connect() probe

**Shutdown order:**
1. Stop accepting new UDS connections (drop the `UnixListener`)
2. Wait for active UDS handler tasks to complete (1s timeout, then cancel)
3. `std::fs::remove_file(socket_path)` — remove socket file
4. Dump vector index (existing)
5. Save adaptation state (existing)
6. Drop Arc holders, compact database (existing)
7. PidGuard::drop removes PID file (existing)

Socket removal (step 3) happens before database compaction (step 6) so that hook processes connecting during compaction find no socket and degrade gracefully (queue events) rather than connecting to a server that is shutting down.

**SocketGuard RAII:** Implement a `SocketGuard` that holds the socket path and removes it on Drop, analogous to PidGuard. This handles both graceful shutdown and panics within the server.

```rust
pub struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %self.path.display(),
                    "failed to remove socket file on drop");
            }
        }
    }
}
```

**LifecycleHandles extension:** Add `socket_guard: Option<SocketGuard>` to `LifecycleHandles`. The socket guard is dropped explicitly during shutdown step 3, before compaction.

### Consequences

**Easier:**
- Stale socket detection is trivially correct — unconditional unlink is always safe after PidGuard.
- No need for a connect-probe timeout or race condition handling.
- Follows the existing pattern: PidGuard for mutual exclusion, then cleanup filesystem artifacts.
- SocketGuard provides crash resilience via RAII (handles panics, though not SIGKILL — startup handles that case).

**Harder:**
- If PidGuard fails to acquire (e.g., another server is running), the socket from the other server must not be unlinked. This is handled by the startup order: socket unlink only happens after successful PidGuard acquisition.
- If the startup sequence is reordered (socket unlink before PidGuard), a race condition could delete an active server's socket. The ordering must be documented and enforced.
- SIGKILL still leaves the socket file behind. But the next startup handles it via unconditional unlink after PidGuard.
