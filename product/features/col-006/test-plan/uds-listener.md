# Test Plan: uds-listener

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-03 | High | Socket lifecycle ordering (PidGuard before bind, socket before PidGuard on shutdown) |
| R-04 | High | Stale socket cleanup on startup |
| R-05 | Medium | SocketGuard RAII (drop removes file) |
| R-13 | Medium | Concurrent UDS connections (5+ simultaneous) |
| R-14 | High | Connection failure resource leak (fd, tokio tasks) |
| R-19 | Critical | UDS listener crash isolation (handler panic does not kill server) |
| R-22 | Medium | Fire-and-forget event delivery verification |
| R-23 | Medium | Shutdown drain timeout |

## Unit Tests

Location: `crates/unimatrix-server/src/uds_listener.rs` (within `#[cfg(test)]` module)

### SocketGuard (R-05)

1. **test_socket_guard_drop_removes_file**: Create a socket file in tempdir. Create `SocketGuard`. Drop it. File is removed.

2. **test_socket_guard_drop_already_removed**: Create `SocketGuard` for a path. Delete the file manually. Drop guard. No panic (NotFound handled).

3. **test_socket_guard_drop_bad_path**: Create `SocketGuard` for `/nonexistent/path/sock`. Drop it. No panic, warning logged.

### Stale Socket Handling (R-04)

4. **test_handle_stale_socket_removes_existing**: Create a file at socket_path. Call `handle_stale_socket`. File is removed.

5. **test_handle_stale_socket_no_file**: Call `handle_stale_socket` when no file exists. Returns `Ok(())`.

## Integration Tests

Location: `crates/unimatrix-server/tests/uds_listener_test.rs` (requires tokio runtime)

### Basic UDS Operations

6. **test_uds_listener_binds_socket**: Start UDS listener in tempdir. Verify socket file exists. Verify permissions are `0o600`.

7. **test_uds_ping_pong**: Start UDS listener. Connect via `UnixStream`. Send Ping frame. Read response. Assert `Pong` with correct version.

8. **test_uds_session_register**: Start UDS listener. Send `SessionRegister` request. Receive `Ack` response.

9. **test_uds_session_close**: Send `SessionClose`. Receive `Ack`.

10. **test_uds_record_event**: Send `RecordEvent`. Receive `Ack`.

### Error Handling (R-09, R-19)

11. **test_uds_malformed_json**: Connect to UDS. Send a valid-length frame with invalid JSON. Receive `Error` response with code `-32004`.

12. **test_uds_unknown_request_type**: Send `{"type":"FutureVariant"}`. Receive `Error` response with code `-32003`.

13. **test_uds_empty_payload**: Send frame with 0-length payload. Receive `Error` response with code `-32004`.

14. **test_uds_oversized_payload**: Send frame header claiming > 1 MiB. Receive `Error` response.

### Concurrent Connections (R-13)

15. **test_uds_5_concurrent_pings**: Spawn 5 tasks, each connecting and sending Ping. All 5 receive Pong. No response takes > 50ms.

16. **test_uds_10_concurrent_fire_and_forget**: 10 tasks send SessionRegister concurrently. All receive Ack.

### Resource Management (R-14)

17. **test_uds_client_disconnect_mid_request**: Connect, send partial frame (2 bytes), close connection. Server does not crash. Next connection still works.

18. **test_uds_client_disconnect_before_response**: Connect, send full request, close before reading response. Server handler completes without error.

19. **test_uds_rapid_connect_disconnect**: 100 connect-then-disconnect cycles (no data sent). Server remains operational. No fd leak (optional: check fd count via `/proc/self/fd`).

### Crash Isolation (R-19)

20. **test_uds_handler_error_does_not_crash_accept_loop**: Send a request that triggers a handler error. Verify the accept loop continues. Send another valid Ping. Receive Pong.

### Lifecycle (R-03)

21. **test_startup_socket_after_pidguard**: Verify the startup sequence: PidGuard acquired before socket bind. Simulate by checking that socket file only exists after PidGuard.

22. **test_shutdown_socket_before_compact**: Start server (or mock lifecycle). Initiate shutdown. Verify socket is removed before compaction step.

### Fire-and-Forget (R-22)

23. **test_fire_and_forget_logged**: Send `SessionRegister` via fire-and-forget. Capture tracing output. Verify "session registered" log entry.

## Edge Cases

- Socket path longer than 108 bytes (Unix limit): bind fails, server logs warning and continues stdio-only
- Client sends two requests on same connection: server processes first, ignores second (per single-request model)
- Trailing garbage after JSON payload: ignored (connection closes after one exchange)
- Accept error (EMFILE - too many open files): logged, accept loop continues with 50ms pause

## Assertions

- Socket file permissions: `metadata.permissions().mode() & 0o777 == 0o600` (on Unix)
- Response type matches expected variant (Pong for Ping, Ack for SessionRegister, Error for malformed)
- Error response codes match expected values (-32003, -32004, -32005)
- No panic in accept loop or handler (verified by subsequent successful requests)
- fd count stable after rapid connect-disconnect cycles
