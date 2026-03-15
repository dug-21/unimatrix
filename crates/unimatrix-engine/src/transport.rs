//! Transport trait and LocalTransport for UDS IPC.
//!
//! Defines the `Transport` abstraction for hook-to-server communication
//! and the `LocalTransport` implementation using Unix domain sockets.
//! Uses synchronous std I/O per ADR-002 (no tokio in hook process).

use std::path::PathBuf;
use std::time::Duration;

use crate::wire::{
    HookRequest, HookResponse, MAX_PAYLOAD_SIZE, TransportError, deserialize_response, read_frame,
    serialize_request, write_frame,
};

/// Transport abstraction for hook-to-server communication.
///
/// Implementations manage connection lifecycle and request/response framing.
/// The typical lifecycle for a hook process is: create -> request/fire_and_forget -> drop.
pub trait Transport: Send + Sync {
    /// Establish a connection to the server.
    fn connect(&mut self) -> Result<(), TransportError>;

    /// Send a request and wait for a response.
    ///
    /// Auto-connects if not already connected. If the server returns an
    /// `Error` response, it is converted to `TransportError::Rejected`.
    fn request(
        &mut self,
        req: &HookRequest,
        timeout: Duration,
    ) -> Result<HookResponse, TransportError>;

    /// Send a request without waiting for a response.
    ///
    /// Auto-connects if not already connected. Disconnects after write
    /// (connection-per-request model for fire-and-forget).
    fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError>;

    /// Close the connection.
    fn disconnect(&mut self);

    /// Check if the transport is currently connected.
    fn is_connected(&self) -> bool;
}

/// Unix domain socket transport for local IPC.
///
/// Connects to the Unimatrix server's UDS endpoint. Uses synchronous
/// std I/O (no tokio runtime) per ADR-002.
pub struct LocalTransport {
    socket_path: PathBuf,
    timeout: Duration,
    stream: Option<std::os::unix::net::UnixStream>,
}

impl LocalTransport {
    /// Create a new `LocalTransport` targeting the given socket path.
    pub fn new(socket_path: PathBuf, timeout: Duration) -> Self {
        Self {
            socket_path,
            timeout,
            stream: None,
        }
    }
}

impl Transport for LocalTransport {
    fn connect(&mut self) -> Result<(), TransportError> {
        use std::os::unix::net::UnixStream;

        // Fast fail: check if socket file exists before attempting connect
        if !self.socket_path.exists() {
            return Err(TransportError::Unavailable(format!(
                "socket not found: {}",
                self.socket_path.display()
            )));
        }

        let stream = UnixStream::connect(&self.socket_path)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| TransportError::Transport(format!("failed to set read timeout: {e}")))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| TransportError::Transport(format!("failed to set write timeout: {e}")))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn request(
        &mut self,
        req: &HookRequest,
        timeout: Duration,
    ) -> Result<HookResponse, TransportError> {
        // Auto-connect if not connected
        if self.stream.is_none() {
            self.connect()?;
        }

        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| TransportError::Unavailable("not connected".to_string()))?;

        // Update timeout if different from default
        if timeout != self.timeout {
            stream.set_read_timeout(Some(timeout)).ok();
            stream.set_write_timeout(Some(timeout)).ok();
        }

        // Serialize and write request frame
        let payload = serialize_request(req)?;
        write_frame(stream, &payload).map_err(|e| TransportError::Transport(e.to_string()))?;

        // Read response frame
        let response_bytes = read_frame(stream, MAX_PAYLOAD_SIZE)?;
        let response = deserialize_response(&response_bytes)?;

        // Check for server-side error and convert to Rejected
        if let HookResponse::Error { code, message } = &response {
            return Err(TransportError::Rejected {
                code: *code,
                message: message.clone(),
            });
        }

        Ok(response)
    }

    fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError> {
        // Auto-connect if not connected
        if self.stream.is_none() {
            self.connect()?;
        }

        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| TransportError::Unavailable("not connected".to_string()))?;

        // Serialize and write request frame
        let payload = serialize_request(req)?;
        write_frame(stream, &payload).map_err(|e| TransportError::Transport(e.to_string()))?;

        // Do NOT read response -- disconnect immediately
        self.disconnect();
        Ok(())
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn local_transport_new() {
        let t = LocalTransport::new(PathBuf::from("/tmp/test.sock"), Duration::from_secs(5));
        assert!(!t.is_connected());
    }

    #[test]
    fn local_transport_disconnect_when_not_connected() {
        let mut t = LocalTransport::new(PathBuf::from("/tmp/test.sock"), Duration::from_secs(5));
        t.disconnect(); // Should not panic
        assert!(!t.is_connected());
    }

    #[test]
    fn local_transport_request_without_connect_nonexistent_socket() {
        let mut t = LocalTransport::new(
            PathBuf::from("/tmp/nonexistent-unimatrix-test-1234.sock"),
            Duration::from_secs(5),
        );
        // Auto-connect will fail because socket doesn't exist
        let result = t.request(&HookRequest::Ping, Duration::from_secs(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::Unavailable(msg) => {
                assert!(msg.contains("socket not found"));
            }
            other => panic!("expected Unavailable, got: {other}"),
        }
    }

    #[test]
    fn local_transport_fire_and_forget_nonexistent_socket() {
        let mut t = LocalTransport::new(
            PathBuf::from("/tmp/nonexistent-unimatrix-test-5678.sock"),
            Duration::from_secs(5),
        );
        let result = t.fire_and_forget(&HookRequest::Ping);
        assert!(result.is_err());
    }

    #[test]
    fn local_transport_connect_nonexistent_socket() {
        let mut t = LocalTransport::new(
            PathBuf::from("/tmp/nonexistent-unimatrix-test-9012.sock"),
            Duration::from_secs(1),
        );
        let result = t.connect();
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::Unavailable(msg) => {
                assert!(msg.contains("socket not found"));
            }
            other => panic!("expected Unavailable, got: {other}"),
        }
    }

    #[test]
    fn local_transport_connect_existing_file_no_listener() {
        // Create a regular file at the socket path (not a real listener)
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("fake.sock");
        std::fs::write(&sock_path, "not a socket").unwrap();

        let mut t = LocalTransport::new(sock_path, Duration::from_secs(1));
        let result = t.connect();
        // Should fail because it's not a real Unix socket
        assert!(result.is_err());
    }

    #[test]
    fn local_transport_disconnect_after_fire_and_forget() {
        // Verify that fire_and_forget disconnects even if send succeeds.
        // We can't test with a real server here, but we can verify the
        // disconnect path by checking the state after a failed attempt.
        let dir = tempfile::TempDir::new().unwrap();
        let sock_path = dir.path().join("no-such.sock");

        let mut t = LocalTransport::new(sock_path, Duration::from_secs(1));
        let _ = t.fire_and_forget(&HookRequest::Ping);
        assert!(!t.is_connected());
    }
}
