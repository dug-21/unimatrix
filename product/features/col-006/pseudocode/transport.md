# Pseudocode: transport

## Purpose

Define the `Transport` trait and `LocalTransport` implementation over Unix domain sockets. The trait has a synchronous public API per ADR-002 (no tokio in hook process). Lives in `unimatrix-engine/src/transport.rs`.

## File: crates/unimatrix-engine/src/transport.rs

### Transport Trait

```
trait Transport: Send + Sync {
    fn connect(&mut self) -> Result<(), TransportError>;
    fn request(&mut self, req: &HookRequest, timeout: Duration) -> Result<HookResponse, TransportError>;
    fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError>;
    fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
}
```

### LocalTransport

```
struct LocalTransport {
    socket_path: PathBuf,
    timeout: Duration,
    stream: Option<UnixStream>,
}

impl LocalTransport {
    fn new(socket_path: PathBuf, timeout: Duration) -> Self:
        Self {
            socket_path,
            timeout,
            stream: None,
        }
}

impl Transport for LocalTransport {

    fn connect(&mut self) -> Result<(), TransportError>:
        // Check if socket file exists first (fast fail for graceful degradation)
        if !self.socket_path.exists():
            return Err(TransportError::Unavailable(
                format!("socket not found: {}", self.socket_path.display())))

        // Connect with timeout
        let stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| match e.kind():
                ConnectionRefused => TransportError::Unavailable("connection refused"),
                _ => TransportError::Transport(e.to_string()))?

        // Set socket timeouts for read/write operations
        stream.set_read_timeout(Some(self.timeout))
            .map_err(|e| TransportError::Transport(e.to_string()))?
        stream.set_write_timeout(Some(self.timeout))
            .map_err(|e| TransportError::Transport(e.to_string()))?

        self.stream = Some(stream)
        Ok(())

    fn request(&mut self, req: &HookRequest, timeout: Duration) -> Result<HookResponse, TransportError>:
        // Connect if not already connected
        if self.stream.is_none():
            self.connect()?

        let stream = self.stream.as_mut()
            .ok_or(TransportError::Unavailable("not connected"))?

        // Update timeout if different from default
        if timeout != self.timeout:
            stream.set_read_timeout(Some(timeout)).ok()
            stream.set_write_timeout(Some(timeout)).ok()

        // Serialize and write request frame
        let payload = serialize_request(req)?
        write_frame(stream, &payload)
            .map_err(|e| TransportError::Transport(e.to_string()))?

        // Read response frame
        let response_bytes = read_frame(stream, MAX_PAYLOAD_SIZE)?

        // Deserialize response
        let response = deserialize_response(&response_bytes)?

        // Check for server-side error
        if let HookResponse::Error { code, message } = &response:
            return Err(TransportError::Rejected { code: *code, message: message.clone() })

        Ok(response)

    fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError>:
        // Connect if not already connected
        if self.stream.is_none():
            self.connect()?

        let stream = self.stream.as_mut()
            .ok_or(TransportError::Unavailable("not connected"))?

        // Serialize and write request frame
        let payload = serialize_request(req)?
        write_frame(stream, &payload)
            .map_err(|e| TransportError::Transport(e.to_string()))?

        // Do NOT read response -- close immediately
        self.disconnect()
        Ok(())

    fn disconnect(&mut self):
        // Drop the stream (closes the fd)
        self.stream = None

    fn is_connected(&self) -> bool:
        self.stream.is_some()
}
```

### Design Notes

1. **Connection-per-request model**: Each `connect()` opens a new UDS connection. After `request()` or `fire_and_forget()`, the connection is available for reuse within the same `LocalTransport` instance, but the protocol is single-request-per-connection (the server closes after responding). In practice, hook processes create one LocalTransport, make one call, and exit.

2. **Timeout via SO_RCVTIMEO/SO_SNDTIMEO**: Set on the UnixStream after connect. Default 40ms (from ADR-002), leaving 10ms margin in the 50ms budget for process startup and project hash computation.

3. **No tokio dependency**: Uses `std::os::unix::net::UnixStream`, not `tokio::net::UnixStream`. The hook process never initializes a tokio runtime (ADR-002, R-18).

4. **fire_and_forget disconnects after write**: For SessionRegister/SessionClose events, the hook does not wait for a response. It writes the frame and exits. The server still processes the request and closes the connection on its end.

5. **Error mapping**: Connection errors -> Unavailable (triggers graceful degradation in hook). Timeout errors -> Timeout. Broken pipe -> Transport. Serde errors -> Codec.

## Error Handling

- `connect()`: NotFound/ConnectionRefused -> Unavailable (hook degrades gracefully)
- `request()`: Timeout on read -> Timeout; broken pipe -> Transport; malformed response -> Codec
- `fire_and_forget()`: Broken pipe on write -> Transport (hook exits 0 anyway)
- `disconnect()`: Never fails (drop-based cleanup)

## Key Test Scenarios

1. LocalTransport::new creates unconnected instance (is_connected = false)
2. connect() to a real UDS listener succeeds (is_connected = true)
3. connect() when socket file does not exist -> Unavailable
4. connect() when socket file exists but no listener -> ConnectionRefused -> Unavailable
5. request(Ping) returns Pong with server_version
6. fire_and_forget(RecordEvent) returns Ok(())
7. disconnect() sets is_connected to false
8. request() auto-connects if not connected
9. Timeout on slow server -> Timeout error
10. Server closes mid-response -> Transport error
