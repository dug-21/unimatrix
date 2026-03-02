# Pseudocode: authentication

## Purpose

Implement peer credential extraction and UID verification for UDS connections. Three authentication layers per ADR-003. Lives in `unimatrix-engine/src/auth.rs`.

## File: crates/unimatrix-engine/src/auth.rs

### Types

```
#[derive(Debug, Clone)]
struct PeerCredentials {
    uid: u32,
    gid: u32,
    pid: Option<u32>,  // Linux only via SO_PEERCRED; None on macOS
}

#[derive(Debug)]
enum AuthError {
    UidMismatch { peer_uid: u32, server_uid: u32 },
    CredentialExtraction(String),
    LineageFailed { pid: u32, reason: String },
}

impl fmt::Display for AuthError { ... }
impl std::error::Error for AuthError {}
```

### Layer 2: Extract Peer Credentials

#### Linux (SO_PEERCRED)

```
#[cfg(target_os = "linux")]
fn extract_peer_credentials(stream: &UnixStream) -> Result<PeerCredentials, AuthError>:
    use std::os::unix::io::AsRawFd;
    use std::mem;

    // ucred struct: pid: i32, uid: u32, gid: u32
    // Use libc types or manual struct definition
    // SO_PEERCRED = 17 on Linux, SOL_SOCKET = 1

    let fd = stream.as_raw_fd()

    // Note: We need to use getsockopt with SO_PEERCRED.
    // Since we forbid unsafe_code, we have two options:
    // Option A: Use the `nix` crate (adds a dependency)
    // Option B: Use std::os::unix::net::UCred (stabilized in Rust 1.75+)
    //
    // Preferred: Use UnixStream::peer_cred() if available (nightly),
    // or implement via Command-based fallback.
    //
    // Practical approach for edition 2024/MSRV 1.89:
    // UnixStream has peer_cred() method returning io::Result<UCred>
    // UCred has uid(), gid(), pid() methods

    let cred = stream.peer_cred()
        .map_err(|e| AuthError::CredentialExtraction(e.to_string()))?

    Ok(PeerCredentials {
        uid: cred.uid(),
        gid: cred.gid(),
        pid: cred.pid().map(|p| p as u32),
    })
```

#### macOS (getpeereid)

```
#[cfg(target_os = "macos")]
fn extract_peer_credentials(stream: &UnixStream) -> Result<PeerCredentials, AuthError>:
    // On macOS, UnixStream::peer_cred() returns UCred with uid/gid but no pid
    let cred = stream.peer_cred()
        .map_err(|e| AuthError::CredentialExtraction(e.to_string()))?

    Ok(PeerCredentials {
        uid: cred.uid(),
        gid: cred.gid(),
        pid: None,  // getpeereid does not provide PID on macOS
    })
```

Note: If `UnixStream::peer_cred()` is not available on the target MSRV, fall back to using `getsockopt` via a safe wrapper or the `nix` crate. Check availability during implementation.

### Layer 2: UID Verification

```
fn verify_uid(peer: &PeerCredentials, server_uid: u32) -> Result<(), AuthError>:
    if peer.uid != server_uid:
        return Err(AuthError::UidMismatch {
            peer_uid: peer.uid,
            server_uid,
        })
    Ok(())
```

### Layer 3: Process Lineage (Linux only)

```
#[cfg(target_os = "linux")]
fn verify_lineage(pid: u32) -> Result<(), AuthError>:
    // Reuse the is_unimatrix_process pattern from pidfile.rs
    let cmdline_path = format!("/proc/{pid}/cmdline")
    let bytes = std::fs::read(&cmdline_path)
        .map_err(|e| AuthError::LineageFailed {
            pid,
            reason: format!("cannot read cmdline: {e}"),
        })?

    if bytes.is_empty():
        return Err(AuthError::LineageFailed {
            pid,
            reason: "empty cmdline (kernel thread or zombie)".into(),
        })

    let cmdline = String::from_utf8_lossy(&bytes)
    for arg in cmdline.split('\0'):
        if arg.is_empty():
            continue
        if let Some(name) = Path::new(arg).file_name():
            if name == "unimatrix-server":
                return Ok(())

    Err(AuthError::LineageFailed {
        pid,
        reason: "no unimatrix-server in cmdline".into(),
    })
```

### Combined Authentication

```
fn authenticate_connection(
    stream: &UnixStream,
    server_uid: u32,
) -> Result<PeerCredentials, AuthError>:
    // Layer 1: filesystem permissions (0o600 on socket) -- enforced at bind time, not here

    // Layer 2: extract and verify UID
    let creds = extract_peer_credentials(stream)?
    verify_uid(&creds, server_uid)?

    // Layer 3: process lineage (Linux only, advisory)
    #[cfg(target_os = "linux")]
    if let Some(pid) = creds.pid:
        match verify_lineage(pid):
            Ok(()) => tracing::debug!(pid, "process lineage verified"),
            Err(e) => tracing::warn!(
                pid,
                error = %e,
                "process lineage check failed (advisory, connection allowed)"
            ),

    Ok(creds)
```

### Helper: Get Server UID

```
fn get_server_uid() -> u32:
    // Use nix::unistd::getuid() or std equivalent
    // On Unix: unsafe { libc::getuid() }
    // Since we forbid unsafe_code, use Command-based approach or std API
    // Rust std::os::unix::process has no getuid()
    // Practical: use nix crate or std::process::Command("id", "-u")
    // OR: store server UID at startup and pass it through
    //
    // Recommended: The server passes its UID to authenticate_connection.
    // Compute once at startup: unsafe { libc::getuid() } in server code
    // (server crate does not forbid unsafe_code... actually it does).
    //
    // Alternative: std::process::Command::new("id").arg("-u").output()
    // and parse stdout. This is called once at startup, not per-connection.
    //
    // Simplest: The authenticate_connection function receives server_uid
    // as a parameter. The caller (uds_listener) obtains it once at startup.
```

## Design Notes

1. **Layer 3 is advisory**: ADR-003 specifies that lineage failure logs a warning but does NOT reject the connection. Only UID mismatch rejects.

2. **Platform-specific code**: Use `#[cfg(target_os = "linux")]` and `#[cfg(target_os = "macos")]` for credential extraction. The `authenticate_connection` function has a unified signature.

3. **No auth response on failure**: Per ADR-003, when auth fails, close the connection immediately. Do not send an Error response -- the connection is untrusted.

4. **server_uid parameter**: The server UID is obtained once at startup and passed to `authenticate_connection` on each call. This avoids repeated system calls.

5. **Reuse pidfile pattern**: The lineage check in `verify_lineage` follows the same logic as `pidfile::is_unimatrix_process` -- read `/proc/{pid}/cmdline`, check filename components. Consider extracting this to a shared function or calling pidfile's version directly from the server side.

## Error Handling

- Credential extraction failure -> AuthError::CredentialExtraction (connection closed)
- UID mismatch -> AuthError::UidMismatch (connection closed, logged)
- Lineage failure -> Warning logged, connection ALLOWED (advisory only)
- IO errors in lineage check -> Treated as lineage failure (warning, not rejection)

## Key Test Scenarios

1. Same UID passes verify_uid
2. Different UID fails verify_uid with UidMismatch
3. PeerCredentials with pid=Some(valid) on Linux passes lineage if binary is unimatrix-server
4. PeerCredentials with pid=None on macOS skips lineage check
5. Lineage check with non-unimatrix cmdline returns LineageFailed (but authenticate_connection still succeeds)
6. Lineage check with /proc path not found returns LineageFailed gracefully
7. Lineage check with empty cmdline returns LineageFailed
8. extract_peer_credentials returns correct UID for real UDS connection (integration test)
9. authenticate_connection succeeds for same-user UDS connection
10. authenticate_connection rejects different-UID connection (unit test with mock)
