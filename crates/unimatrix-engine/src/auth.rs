//! Peer credential extraction and connection authentication.
//!
//! Implements the 3-layer authentication model per ADR-003:
//! 1. Filesystem permissions (socket file mode)
//! 2. UID verification (SO_PEERCRED on Linux, getpeereid on macOS)
//! 3. Process lineage (advisory, Linux only)
//!
//! Uses the `nix` crate for safe Unix API access since `std::os::unix::net::UCred`
//! / `peer_cred()` remain unstable in the Rust standard library.

use std::fmt;

/// Credentials extracted from a connected Unix domain socket peer.
#[derive(Debug, Clone)]
pub struct PeerCredentials {
    /// User ID of the peer process.
    pub uid: u32,
    /// Group ID of the peer process.
    pub gid: u32,
    /// Process ID of the peer (Linux only via SO_PEERCRED; None on macOS).
    pub pid: Option<u32>,
}

/// Errors that can occur during authentication.
#[derive(Debug)]
pub enum AuthError {
    /// Failed to extract peer credentials from the socket.
    CredentialExtraction(String),
    /// UID of the peer does not match the server's UID.
    UidMismatch { expected: u32, actual: u32 },
    /// Process lineage verification failed (advisory).
    LineageFailed(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::CredentialExtraction(msg) => {
                write!(f, "failed to extract peer credentials: {msg}")
            }
            AuthError::UidMismatch { expected, actual } => {
                write!(f, "UID mismatch: expected {expected}, got {actual}")
            }
            AuthError::LineageFailed(msg) => {
                write!(f, "process lineage verification failed: {msg}")
            }
        }
    }
}

impl std::error::Error for AuthError {}

/// Extract peer credentials from a connected `UnixStream`.
///
/// Uses `SO_PEERCRED` on Linux (returns uid, gid, pid) and
/// `LOCAL_PEERCRED` / `getpeereid` on macOS (returns uid, gid; pid is None).
#[cfg(target_os = "linux")]
pub fn extract_peer_credentials(
    stream: &std::os::unix::net::UnixStream,
) -> Result<PeerCredentials, AuthError> {
    use nix::sys::socket::{self, sockopt};

    let cred = socket::getsockopt(stream, sockopt::PeerCredentials)
        .map_err(|e| AuthError::CredentialExtraction(format!("SO_PEERCRED failed: {e}")))?;

    Ok(PeerCredentials {
        uid: cred.uid(),
        gid: cred.gid(),
        pid: Some(cred.pid() as u32),
    })
}

#[cfg(target_os = "macos")]
pub fn extract_peer_credentials(
    stream: &std::os::unix::net::UnixStream,
) -> Result<PeerCredentials, AuthError> {
    use nix::unistd::{Gid, Uid};
    use std::os::fd::AsRawFd;

    // On macOS, use getpeereid via nix
    let fd = stream.as_raw_fd();
    let (uid, gid) = nix::unistd::getpeereid(fd)
        .map_err(|e| AuthError::CredentialExtraction(format!("getpeereid failed: {e}")))?;

    Ok(PeerCredentials {
        uid: uid.as_raw(),
        gid: gid.as_raw(),
        pid: None,
    })
}

/// Authenticate a connection by verifying the peer's UID matches the server's UID.
///
/// Layer 1 (filesystem permissions) is handled by socket file mode (0o700 on parent dir).
/// Layer 2 (UID verification) is performed here.
/// Layer 3 (process lineage) is advisory and logged but does not reject connections.
pub fn authenticate_connection(
    stream: &std::os::unix::net::UnixStream,
    server_uid: u32,
) -> Result<PeerCredentials, AuthError> {
    let creds = extract_peer_credentials(stream)?;

    // Layer 2: UID verification
    if creds.uid != server_uid {
        return Err(AuthError::UidMismatch {
            expected: server_uid,
            actual: creds.uid,
        });
    }

    // Layer 3: Process lineage (advisory, Linux only)
    #[cfg(target_os = "linux")]
    if let Some(pid) = creds.pid {
        if let Err(e) = verify_process_lineage(pid) {
            tracing::warn!(
                "process lineage check failed for pid {pid}: {e} (advisory, allowing connection)"
            );
        }
    }

    Ok(creds)
}

/// Advisory process lineage check (Linux only).
///
/// Reads /proc/{pid}/cmdline to check if the peer process looks like a hook
/// invocation. This is advisory -- failures are logged but do not reject
/// the connection per ADR-003.
#[cfg(target_os = "linux")]
fn verify_process_lineage(pid: u32) -> Result<(), AuthError> {
    use std::fs;

    let cmdline_path = format!("/proc/{pid}/cmdline");
    let cmdline = fs::read_to_string(&cmdline_path)
        .map_err(|e| AuthError::LineageFailed(format!("cannot read {cmdline_path}: {e}")))?;

    // Accept any process with a non-empty cmdline for now.
    // Future versions may verify the process descends from a Claude Code session.
    if cmdline.is_empty() {
        return Err(AuthError::LineageFailed("empty cmdline".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_credentials_debug() {
        let creds = PeerCredentials {
            uid: 1000,
            gid: 1000,
            pid: Some(12345),
        };
        let debug = format!("{creds:?}");
        assert!(debug.contains("1000"));
        assert!(debug.contains("12345"));
    }

    #[test]
    fn peer_credentials_clone() {
        let creds = PeerCredentials {
            uid: 1000,
            gid: 1000,
            pid: None,
        };
        let cloned = creds.clone();
        assert_eq!(cloned.uid, 1000);
        assert!(cloned.pid.is_none());
    }

    #[test]
    fn auth_error_display_credential_extraction() {
        let err = AuthError::CredentialExtraction("test".to_string());
        assert!(format!("{err}").contains("failed to extract"));
    }

    #[test]
    fn auth_error_display_uid_mismatch() {
        let err = AuthError::UidMismatch {
            expected: 1000,
            actual: 1001,
        };
        let msg = format!("{err}");
        assert!(msg.contains("1000"));
        assert!(msg.contains("1001"));
    }

    #[test]
    fn auth_error_display_lineage_failed() {
        let err = AuthError::LineageFailed("no parent".to_string());
        assert!(format!("{err}").contains("lineage"));
    }

    #[test]
    fn auth_error_is_error() {
        let err = AuthError::CredentialExtraction("test".to_string());
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn extract_peer_credentials_from_pair() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let creds = extract_peer_credentials(&a).unwrap();
        // Should match current process UID
        let my_uid = nix::unistd::getuid().as_raw();
        assert_eq!(creds.uid, my_uid);
    }

    #[test]
    fn authenticate_connection_same_uid() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let my_uid = nix::unistd::getuid().as_raw();
        let result = authenticate_connection(&a, my_uid);
        assert!(result.is_ok());
    }

    #[test]
    fn authenticate_connection_different_uid() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        // Use a UID that definitely isn't ours
        let fake_uid = 99999;
        let result = authenticate_connection(&a, fake_uid);
        assert!(result.is_err());
        match result.unwrap_err() {
            AuthError::UidMismatch { expected, actual } => {
                assert_eq!(expected, fake_uid);
                let my_uid = nix::unistd::getuid().as_raw();
                assert_eq!(actual, my_uid);
            }
            _ => panic!("expected UidMismatch"),
        }
    }
}
