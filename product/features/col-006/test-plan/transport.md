# Test Plan: transport

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-06 | High | Latency budget (timeout behavior) |
| R-14 | High | Connection failure resource cleanup |
| R-18 | High | No tokio in transport (verified by compilation -- transport is in unimatrix-engine, no tokio dep) |

## Unit Tests

Location: `crates/unimatrix-engine/src/transport.rs` (within `#[cfg(test)]` module)

### LocalTransport State

1. **test_new_is_not_connected**: `LocalTransport::new(path, timeout)` -> `is_connected()` returns false.

2. **test_connect_nonexistent_socket**: `connect()` to a path that does not exist -> `TransportError::Unavailable`.

3. **test_connect_no_listener**: Create a socket file manually (not a real listener), attempt `connect()` -> `TransportError::Unavailable` (connection refused).

4. **test_disconnect_sets_not_connected**: After successful `connect()`, call `disconnect()` -> `is_connected()` returns false.

5. **test_disconnect_when_not_connected**: `disconnect()` on a new transport -> no panic, no error.

## Integration Tests (with real UDS listener)

Location: `crates/unimatrix-engine/tests/transport_test.rs` or inline in `transport.rs` tests

These require a test helper that spawns a real UDS listener in a tempdir.

6. **test_connect_to_real_listener**: Spawn a UDS listener in tempdir. `connect()` succeeds. `is_connected()` returns true.

7. **test_request_ping_pong**: Connect to test listener that responds to Ping with Pong. `request(Ping, 1s)` returns `HookResponse::Pong`.

8. **test_fire_and_forget_succeeds**: Connect to test listener. `fire_and_forget(RecordEvent(...))` returns `Ok(())`. Transport disconnects after send.

9. **test_request_auto_connects**: Without calling `connect()` first, `request(Ping, 1s)` auto-connects and succeeds.

10. **test_request_timeout**: Connect to a test listener that sleeps for 2 seconds before responding. `request(Ping, 100ms)` returns `TransportError::Timeout`.

11. **test_request_server_closes_mid_response**: Connect to a test listener that closes the connection after reading the request (before sending response). `request(Ping, 1s)` returns `TransportError::Transport`.

12. **test_request_server_error_response**: Connect to a test listener that returns `HookResponse::Error`. `request()` returns `TransportError::Rejected` with the error code and message.

## Test Helper: TestUdsServer

A minimal UDS listener for transport tests:

```
struct TestUdsServer {
    socket_path: PathBuf,
    _tempdir: TempDir,
    handle: JoinHandle<()>,
}

impl TestUdsServer {
    fn start(handler: impl Fn(HookRequest) -> HookResponse + Send + 'static) -> Self
    fn socket_path(&self) -> &Path
}
```

The handler receives a deserialized `HookRequest` and returns a `HookResponse`. The test server handles framing (read_frame/write_frame) internally.

## Edge Cases

- `connect()` called twice -> second call is a no-op if already connected (or reconnects)
- `request()` after `disconnect()` -> auto-reconnects
- Socket path as symlink -> `connect()` follows symlink (standard OS behavior)
- Very large response payload -> `read_frame` enforces `MAX_PAYLOAD_SIZE`

## Assertions

- `is_connected()` state transitions: false -> connect -> true -> disconnect -> false
- `request()` return values match expected response variants
- Error types match expected `TransportError` variants
- Timeout durations are enforced (wall-clock measurement)
