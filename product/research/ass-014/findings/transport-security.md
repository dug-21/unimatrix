# D14-4: Transport & Security Design

**Date:** 2026-03-01
**Spike:** ASS-014 (Cortical Implant Architecture)
**Answers:** RQ-4a through RQ-4e
**Status:** Research Complete

---

## Table of Contents

1. [Transport Trait](#1-transport-trait)
2. [Local Transport Design](#2-local-transport-design)
3. [Remote Transport Sketch](#3-remote-transport-sketch)
4. [Authentication Architecture](#4-authentication-architecture)
5. [Instance Discovery](#5-instance-discovery)
6. [Threat Model](#6-threat-model)
7. [Graceful Degradation Matrix](#7-graceful-degradation-matrix)
8. [Local to Centralized Transition](#8-local-to-centralized-transition)
9. [Open Risks](#9-open-risks)

---

## 1. Transport Trait

**Answers RQ-4a: What transport trait/interface abstracts over local and remote?**

### Design Constraints

The transport trait must satisfy conflicting requirements:

1. **Hook processes are ephemeral.** Claude Code spawns a new process per hook event. A full tokio runtime cold start adds ~1-3ms (runtime allocation + thread pool), but the real cost is downstream: opening a Unix domain socket connection (~0.1ms) vs. opening redb + loading HNSW (~50-200ms). The trait must enable connection reuse without requiring a persistent process.

2. **Some operations are synchronous and blocking.** `UserPromptSubmit` and `PreCompact` hooks must return content to stdout before Claude Code proceeds. The transport must support synchronous request/response within the hook's process lifetime.

3. **Some operations are fire-and-forget.** `PostToolUse` event recording does not need a response. The implant should not block on acknowledgment.

4. **Batch operations reduce round-trips.** Session-end hooks may need to flush multiple events. Batch support avoids N round-trips.

5. **The trait must work for both local and remote.** A config change -- not a rewrite -- should switch from Unix domain socket to HTTPS/gRPC.

### Transport Trait Definition (Rust Pseudocode)

```rust
use std::time::Duration;

/// Errors returned by transport operations.
#[derive(Debug)]
pub enum TransportError {
    /// Server is unavailable (not running, network unreachable).
    Unavailable(String),
    /// Operation timed out.
    Timeout(Duration),
    /// Server rejected the request (auth failure, invalid params).
    Rejected { code: i32, message: String },
    /// Serialization/deserialization failure.
    Codec(String),
    /// Transport-specific error (socket broken, TLS handshake failed).
    Transport(String),
}

/// A query sent to the Unimatrix engine.
/// Mirrors the operation types the implant needs.
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    /// Semantic search for context injection (UserPromptSubmit).
    /// Returns matching entries with confidence scores.
    ContextSearch {
        query: String,
        role: Option<String>,
        task: Option<String>,
        feature: Option<String>,
        k: u32,
        max_tokens: u32,
    },
    /// Compiled briefing for compaction defense (PreCompact).
    /// Returns a token-budgeted knowledge payload.
    Briefing {
        role: String,
        task: String,
        feature: Option<String>,
        max_tokens: u32,
    },
    /// Record a structured event (PostToolUse, SessionStart, etc.).
    /// Fire-and-forget -- response is acknowledgment only.
    RecordEvent(ImplantEvent),
    /// Record multiple events in one round-trip (SessionEnd flush).
    RecordEvents(Vec<ImplantEvent>),
    /// Register or update session metadata (SessionStart).
    SessionRegister {
        session_id: String,
        agent_role: Option<String>,
        feature: Option<String>,
    },
    /// Close a session (SessionEnd).
    SessionClose {
        session_id: String,
        outcome: Option<String>,
        duration_secs: u64,
    },
    /// Health check -- is the server alive and ready?
    Ping,
}

/// A response from the Unimatrix engine.
#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    /// Matching entries for context injection.
    Entries {
        items: Vec<EntryPayload>,
        total_tokens: u32,
    },
    /// Compiled briefing text for compaction defense.
    Briefing {
        content: String,
        token_count: u32,
    },
    /// Acknowledgment of recorded event(s).
    Ack,
    /// Pong response with server version.
    Pong { version: String },
}

/// A structured event from hook observation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImplantEvent {
    pub event_type: String,
    pub session_id: String,
    pub timestamp: u64,
    pub payload: serde_json::Value,
}

/// A knowledge entry payload returned to the implant.
#[derive(Debug, Serialize, Deserialize)]
pub struct EntryPayload {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub confidence: f64,
    pub similarity: f64,
    pub category: String,
}

/// The transport abstraction. Implementations handle local (UDS) and
/// remote (HTTPS/gRPC) communication.
///
/// All methods are synchronous -- the implant is an ephemeral process
/// that cannot afford async runtime overhead for simple operations.
/// Implementors may use internal async machinery (e.g., a blocking
/// send on a tokio channel) but the public interface is sync.
pub trait Transport: Send + Sync {
    /// Send a request and wait for a response.
    /// Used for operations where the hook needs the result (context
    /// injection, compaction defense, session registration).
    ///
    /// Timeout is transport-level -- the caller specifies how long
    /// to wait. Recommended: 50ms for injection, 200ms for briefing.
    fn request(
        &self,
        req: Request,
        timeout: Duration,
    ) -> Result<Response, TransportError>;

    /// Send a request without waiting for a response.
    /// Used for fire-and-forget event recording (PostToolUse, etc.).
    ///
    /// The transport should make a best-effort attempt to deliver.
    /// If the server is unavailable, the implant's local queue
    /// handles retry (see Section 7).
    fn fire_and_forget(&self, req: Request) -> Result<(), TransportError>;

    /// Check if the transport connection is healthy.
    /// Used before expensive operations to avoid timeout delays.
    fn is_connected(&self) -> bool;

    /// Establish or re-establish the connection.
    /// Called on implant startup and after connection failures.
    fn connect(&mut self) -> Result<(), TransportError>;

    /// Clean shutdown of the transport connection.
    fn disconnect(&mut self);
}
```

### Sync vs. Async Decision

**Decision: Synchronous public interface, async-capable internals.**

Rationale:

- Hook processes live for <100ms. Tokio runtime initialization is fast (~1-3ms) but unnecessary for a single request/response cycle over a Unix domain socket.
- The `std::os::unix::net::UnixStream` API is synchronous and sufficient for local transport. Setting `SO_RCVTIMEO` and `SO_SNDTIMEO` provides timeout behavior without async.
- For the remote transport (future HTTPS/gRPC), the implementor can internally use a blocking reqwest client or a pre-initialized tokio runtime. The public trait does not force async on callers.
- If the implant evolves into a daemon (long-running process), the daemon's internal event loop is async, but the hook-facing IPC endpoint (which the daemon exposes) still presents a sync interface to the ephemeral hook process.

### Wire Protocol

For the local Unix domain socket transport, the wire protocol is length-prefixed JSON:

```
[4 bytes: payload length as u32 big-endian][JSON payload]
```

JSON is chosen over bincode for the wire protocol because:
- Debuggability: `socat` and `nc` can inspect traffic during development.
- Version tolerance: JSON handles missing/extra fields gracefully (serde `#[serde(default)]`), reducing version coupling between implant and server.
- Performance is adequate: a 2KB JSON payload parses in <0.1ms.

Bincode or MessagePack can be adopted later if profiling shows JSON parsing is a bottleneck. The transport trait abstracts over the wire format -- callers see `Request`/`Response` structs.

---

## 2. Local Transport Design

### Unix Domain Socket Architecture

The Unimatrix MCP server currently uses stdio transport exclusively (rmcp's `transport::io::stdio()`). The cortical implant cannot use this pipe -- it belongs to Claude Code's MCP session. The implant needs its own communication channel.

**Design: The MCP server opens a Unix domain socket listener alongside its stdio transport.**

```
~/.unimatrix/{project_hash}/
  unimatrix.redb          # Database
  unimatrix.pid           # PID file (PidGuard)
  unimatrix.sock          # Unix domain socket (NEW)
  vector/                 # HNSW index
```

### Socket Lifecycle

1. **Server startup** (in `main.rs`, after PidGuard acquisition):
   - Create socket at `~/.unimatrix/{project_hash}/unimatrix.sock`
   - Remove stale socket file if it exists (same pattern as PidGuard stale PID handling)
   - Bind `UnixListener` with permissions `0o600` (owner-only read/write)
   - Spawn a tokio task to accept connections on the socket

2. **Server runtime**:
   - Accept connections in a loop
   - Each connection spawns a handler task
   - Handler reads length-prefixed JSON request, dispatches to the same engine (shared `Arc<Store>`, `Arc<VectorIndex>`, etc.), writes length-prefixed JSON response
   - Connection is short-lived (one request/response per connection for simplicity; persistent connections as optimization later)

3. **Server shutdown**:
   - Close the `UnixListener` (stop accepting)
   - Drain in-flight requests (timeout: 1 second)
   - Remove socket file (in `LifecycleHandles` shutdown sequence, before compaction)

4. **Implant connection**:
   - Open `UnixStream` to `~/.unimatrix/{project_hash}/unimatrix.sock`
   - Set read/write timeouts (`SO_RCVTIMEO`/`SO_SNDTIMEO`)
   - Write request, read response, close connection

### Socket Permissions

The socket file is created with mode `0o600` (owner read/write only). This means:

- Only processes running as the same user can connect.
- On multi-user systems, each user's Unimatrix instance is isolated.
- No additional authentication is needed for same-user connections in the local model.

The socket's parent directory (`~/.unimatrix/{project_hash}/`) is created with `0o700` by `fs::create_dir_all` (default umask applies), providing directory-level isolation.

### Why Unix Domain Socket (Not Named Pipe, Shared Memory, or HTTP)

| Option | Latency | Bidirectional | Auth | Cross-platform | Verdict |
|--------|---------|---------------|------|----------------|---------|
| **Unix domain socket** | ~0.1ms connect | Yes | SO_PEERCRED | Linux + macOS | **Chosen** |
| Named pipe (FIFO) | ~0.1ms | Requires two pipes | No peer cred | Linux + macOS | Unidirectional complexity |
| Shared memory | ~0.01ms | Requires sync primitives | No | Platform-specific | Over-engineering for <50ms target |
| HTTP on localhost | ~1-5ms (TCP handshake) | Yes | Standard HTTP auth | All platforms | Unnecessary overhead for local |
| Named pipe (Windows) | ~0.1ms | Yes | Windows ACLs | Windows only | Windows-only fallback |

Unix domain sockets provide kernel-enforced peer credential verification (`SO_PEERCRED` on Linux, `getpeereid()` on macOS), sub-millisecond connect times, and are the standard IPC mechanism for local daemons. Docker, containerd, systemd, and the MCP ecosystem (Claude Code itself uses UDS for some internal communication) all use this pattern.

### Windows Consideration

Windows lacks Unix domain sockets in the traditional sense. Options for Windows:
1. **Named pipes** (`\\.\pipe\unimatrix-{project_hash}`): Native Windows IPC, similar semantics.
2. **TCP on localhost** with a random port written to a port file: Cross-platform but weaker security (any local process can connect).
3. **Windows Unix domain sockets** (available since Windows 10 1803 via `AF_UNIX`): Limited but functional.

Recommendation: Use `AF_UNIX` on Windows (available in modern Windows), with named pipes as fallback. The transport trait abstracts this -- `LocalTransport` selects the platform-appropriate mechanism.

### Latency Budget

Target: <50ms round-trip for synchronous operations.

| Component | Estimated | Notes |
|-----------|-----------|-------|
| Socket connect | 0.1ms | Local UDS, no TCP handshake |
| Request serialization (JSON) | 0.05ms | Small payloads (<1KB request) |
| Request write | 0.01ms | Kernel copy |
| Server dispatch | 1-5ms | Route to handler, acquire read txn |
| Engine query (search) | 10-30ms | redb read + HNSW search (no embedding needed if pre-computed) |
| Engine query (briefing) | 15-40ms | Multiple lookups + token budgeting |
| Response serialization | 0.1ms | 2-5KB response |
| Response read | 0.01ms | Kernel copy |
| **Total (search)** | **~12-36ms** | Well within 50ms |
| **Total (briefing)** | **~16-46ms** | Tight but achievable |

Note: These estimates assume the embedding for the search query is pre-computed. If the implant needs to embed the prompt at query time, ONNX initialization (~200ms cold) dominates. This forces a design choice: either the MCP server does the embedding (implant sends raw text, server embeds and searches), or a daemon keeps the ONNX model warm. The server-side embedding approach is recommended for the local transport.

---

## 3. Remote Transport Sketch

**For future centralized deployment (dsn-phase, multi-project).**

### Interface

The remote transport implements the same `Transport` trait with HTTPS as the underlying protocol.

```rust
pub struct RemoteTransport {
    /// Base URL of the remote Unimatrix API (e.g., "https://unimatrix.example.com").
    endpoint: String,
    /// Authentication credential (API key, OAuth token, or mTLS cert path).
    auth: AuthCredential,
    /// HTTP client (reqwest blocking or ureq for sync interface).
    client: HttpClient,
    /// Connection timeout.
    connect_timeout: Duration,
    /// Request timeout (per-operation).
    request_timeout: Duration,
}

impl Transport for RemoteTransport {
    fn request(&self, req: Request, timeout: Duration) -> Result<Response, TransportError> {
        // POST https://{endpoint}/api/v1/implant
        // Headers: Authorization: Bearer {token}
        // Body: JSON-serialized Request
        // Response: JSON-serialized Response
        let resp = self.client
            .post(&format!("{}/api/v1/implant", self.endpoint))
            .header("Authorization", self.auth.header_value())
            .json(&req)
            .timeout(timeout)
            .send()
            .map_err(|e| TransportError::Transport(e.to_string()))?;

        if resp.status() == 401 {
            return Err(TransportError::Rejected {
                code: 401,
                message: "Authentication failed".into(),
            });
        }

        resp.json::<Response>()
            .map_err(|e| TransportError::Codec(e.to_string()))
    }

    fn fire_and_forget(&self, req: Request) -> Result<(), TransportError> {
        // Same endpoint, but don't wait for response body.
        // Use a background thread or non-blocking send.
        let _ = self.client
            .post(&format!("{}/api/v1/implant", self.endpoint))
            .header("Authorization", self.auth.header_value())
            .json(&req)
            .timeout(Duration::from_secs(2))
            .send();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // HEAD request to health endpoint.
        self.client
            .head(&format!("{}/api/v1/health", self.endpoint))
            .timeout(Duration::from_secs(1))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn connect(&mut self) -> Result<(), TransportError> {
        // Validate endpoint reachability and auth.
        let resp = self.client
            .get(&format!("{}/api/v1/health", self.endpoint))
            .header("Authorization", self.auth.header_value())
            .timeout(self.connect_timeout)
            .send()
            .map_err(|e| TransportError::Unavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(TransportError::Unavailable(
                format!("Server returned {}", resp.status()),
            ));
        }
        Ok(())
    }

    fn disconnect(&mut self) {
        // HTTP is stateless; nothing to do.
    }
}
```

### Protocol Choice: HTTPS vs. gRPC

| Factor | HTTPS + JSON | gRPC + Protobuf |
|--------|-------------|-----------------|
| Simplicity | Simple, ubiquitous | Requires protobuf toolchain |
| Streaming | SSE or WebSocket | Native bidirectional streaming |
| Performance | Adequate for our payload sizes | Better for high-frequency small messages |
| Debugging | curl, browser, any HTTP tool | grpcurl, limited tooling |
| Firewall traversal | Port 443, universally allowed | Custom port, may be blocked |
| Client libraries | Every language | Requires codegen per language |

**Recommendation: HTTPS + JSON for the initial remote transport.** The payloads are small (<10KB), request frequency is low (1-2 per prompt cycle), and the debugging and tooling advantages are significant. gRPC can be reconsidered if streaming becomes important (e.g., real-time dashboard feed).

---

## 4. Authentication Architecture

**Answers RQ-4b: How does the implant authenticate to Unimatrix?**

### AuthContext Trait

```rust
/// Authentication context resolved from transport-specific credentials.
/// Downstream code (engine queries, event recording, audit) receives this
/// struct -- it never sees the raw credential.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Verified identity of the caller.
    pub identity: CallerIdentity,
    /// Trust level for capability checks.
    pub trust_level: TrustLevel,
    /// Capabilities granted to this caller.
    pub capabilities: Vec<Capability>,
    /// How the identity was established.
    pub auth_method: AuthMethod,
}

/// Who is making the request.
#[derive(Debug, Clone)]
pub enum CallerIdentity {
    /// The cortical implant process for a specific project.
    Implant {
        project_hash: String,
        pid: u32,
        uid: u32,
    },
    /// An MCP agent via stdio (existing path).
    Agent {
        agent_id: String,
    },
    /// A remote client via HTTPS.
    RemoteClient {
        client_id: String,
        token_subject: String,
    },
}

/// How the caller's identity was verified.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Unix domain socket peer credentials (SO_PEERCRED).
    /// Kernel-verified UID/PID -- cannot be spoofed.
    PeerCredentials,
    /// Shared secret file readable only by the project owner.
    SharedSecret,
    /// Self-reported agent_id parameter (existing MCP path).
    SelfReported,
    /// OAuth 2.1 bearer token (future HTTPS).
    OAuthToken,
    /// Mutual TLS certificate (future enterprise).
    MutualTLS,
}

/// Resolve authentication from a Unix domain socket connection.
pub fn authenticate_local(
    stream: &UnixStream,
    expected_uid: u32,
    project_hash: &str,
) -> Result<AuthContext, TransportError> {
    // 1. Extract peer credentials from socket (kernel-verified).
    let cred = get_peer_credentials(stream)?;

    // 2. Verify UID matches the server's UID (same user).
    if cred.uid != expected_uid {
        return Err(TransportError::Rejected {
            code: 403,
            message: format!(
                "UID mismatch: expected {}, got {}",
                expected_uid, cred.uid
            ),
        });
    }

    // 3. Optionally verify the connecting process is a known implant binary.
    //    Extends vnc-004's is_unimatrix_process() pattern.
    let process_verified = verify_implant_process(cred.pid);

    // 4. Build auth context.
    Ok(AuthContext {
        identity: CallerIdentity::Implant {
            project_hash: project_hash.to_string(),
            pid: cred.pid,
            uid: cred.uid,
        },
        // Implant gets Internal trust -- can read and write, but not Admin.
        trust_level: TrustLevel::Internal,
        capabilities: vec![
            Capability::Read,
            Capability::Write,
            Capability::Search,
        ],
        auth_method: if process_verified {
            AuthMethod::PeerCredentials
        } else {
            AuthMethod::SharedSecret
        },
    })
}

/// Platform-specific peer credential extraction.
#[cfg(target_os = "linux")]
fn get_peer_credentials(stream: &UnixStream) -> Result<PeerCred, TransportError> {
    use std::os::unix::io::AsRawFd;
    // Use SO_PEERCRED via libc::getsockopt or the nix crate.
    // Returns ucred { pid, uid, gid }.
    let fd = stream.as_raw_fd();
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return Err(TransportError::Transport(
            "SO_PEERCRED getsockopt failed".into(),
        ));
    }
    Ok(PeerCred {
        pid: cred.pid as u32,
        uid: cred.uid as u32,
        gid: cred.gid as u32,
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn get_peer_credentials(stream: &UnixStream) -> Result<PeerCred, TransportError> {
    use std::os::unix::io::AsRawFd;
    // macOS/BSD: use getpeereid().
    let fd = stream.as_raw_fd();
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let ret = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
    if ret != 0 {
        return Err(TransportError::Transport(
            "getpeereid failed".into(),
        ));
    }
    // macOS getpeereid does not return PID.
    // Use proc_pidinfo or accept PID=0 (unknown).
    Ok(PeerCred {
        pid: 0, // Not available via getpeereid on macOS
        uid: uid as u32,
        gid: gid as u32,
    })
}

/// Verify that a PID belongs to a cortical implant binary.
/// Extends vnc-004's is_unimatrix_process() pattern.
#[cfg(target_os = "linux")]
fn verify_implant_process(pid: u32) -> bool {
    let cmdline_path = format!("/proc/{pid}/cmdline");
    let bytes = match std::fs::read(&cmdline_path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let cmdline = String::from_utf8_lossy(&bytes);
    for arg in cmdline.split('\0') {
        if let Some(name) = std::path::Path::new(arg).file_name() {
            if name == "unimatrix-hook" || name == "unimatrix-implant" {
                return true;
            }
        }
    }
    false
}

struct PeerCred {
    pid: u32,
    uid: u32,
    gid: u32,
}
```

### Local Authentication: Layered Defense

The local authentication model combines three mechanisms:

| Layer | Mechanism | What It Verifies | Spoofable? |
|-------|-----------|-----------------|------------|
| 1. Filesystem permissions | Socket mode `0o600` | Same user (UID) | Requires root |
| 2. SO_PEERCRED / getpeereid | Kernel peer credentials | Same UID, caller PID | Cannot be spoofed (kernel-enforced) |
| 3. Process lineage | `/proc/{pid}/cmdline` check | Caller is known binary | Requires binary name collision |

All three layers are zero-ceremony -- no tokens, no passwords, no configuration. The implant connects to the socket; the server verifies the connection is from the right user running the right binary.

### Shared Secret Fallback

For environments where `SO_PEERCRED` is unavailable (some container runtimes, non-standard kernels), a shared secret file provides backup authentication:

```
~/.unimatrix/{project_hash}/auth.token
```

- Generated on first server startup (32 bytes, random, hex-encoded).
- Permissions: `0o600` (owner-only).
- Implant reads the token and sends it as a header in the request.
- Server validates against the stored value.

This is less secure than peer credentials (any process that can read the file can authenticate) but provides a universal fallback.

### Remote Authentication: Tiered Approach

For the future remote transport, authentication follows a capability tier:

| Tier | Method | Use Case | Complexity |
|------|--------|----------|------------|
| 1 | API key | Single-developer remote access | Low |
| 2 | OAuth 2.1 + PKCE | Team environments, CI/CD | Medium |
| 3 | mTLS | Enterprise, zero-trust networks | High |

```rust
pub enum AuthCredential {
    /// Simple API key (Tier 1).
    /// Stored in ~/.unimatrix/config.toml or env var UNIMATRIX_API_KEY.
    ApiKey(String),

    /// OAuth 2.1 bearer token (Tier 2).
    /// Obtained via Authorization Code flow with PKCE.
    /// Token contains claims: sub, agent_id, trust_level, capabilities.
    OAuthToken {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: u64,
    },

    /// Mutual TLS client certificate (Tier 3).
    /// Client presents cert; server verifies against CA.
    /// Identity extracted from certificate subject/SAN.
    MutualTls {
        cert_path: String,
        key_path: String,
    },
}

impl AuthCredential {
    pub fn header_value(&self) -> String {
        match self {
            AuthCredential::ApiKey(key) => format!("Bearer {key}"),
            AuthCredential::OAuthToken { access_token, .. } => {
                format!("Bearer {access_token}")
            }
            AuthCredential::MutualTls { .. } => {
                // mTLS auth is at the TLS layer, not the HTTP header.
                String::new()
            }
        }
    }
}
```

### Integration with AGENT_REGISTRY

The implant's `AuthContext` maps to the existing trust hierarchy:

| Caller | Resolved Identity | Trust Level | Capabilities |
|--------|------------------|-------------|--------------|
| Local implant (verified PID) | `cortical-implant` | Internal | Read, Write, Search |
| Local implant (shared secret) | `cortical-implant` | Internal | Read, Write, Search |
| MCP agent (self-reported) | `{agent_id}` | Per registry | Per registry |
| Remote client (API key) | `remote-{key_prefix}` | Restricted | Read, Search |
| Remote client (OAuth) | `{token.sub}` | Per token claims | Per token scopes |

The server enrolls `cortical-implant` as a pre-defined Internal agent during `bootstrap_defaults()`, alongside `system` and `human`. This avoids auto-enrollment as Restricted (which would be read-only, preventing event recording).

---

## 5. Instance Discovery

**Answers RQ-4c: How does the implant know WHICH Unimatrix instance to connect to?**

### Local Discovery

The implant uses the same project hash mechanism as the MCP server (`project.rs`):

```rust
/// Discover the Unimatrix instance for the current project.
pub fn discover_instance() -> Result<InstanceConfig, DiscoveryError> {
    // 1. Environment variable override (highest priority).
    if let Ok(socket) = std::env::var("UNIMATRIX_SOCKET") {
        return Ok(InstanceConfig::Local {
            socket_path: PathBuf::from(socket),
        });
    }

    if let Ok(url) = std::env::var("UNIMATRIX_URL") {
        return Ok(InstanceConfig::Remote {
            endpoint: url,
            auth: discover_auth_credential()?,
        });
    }

    // 2. Project-level config (.unimatrix/config.toml in project root).
    let project_root = detect_project_root(None)?;
    let project_config = project_root.join(".unimatrix").join("config.toml");
    if project_config.exists() {
        if let Some(config) = parse_project_config(&project_config)? {
            return Ok(config);
        }
    }

    // 3. Compute project hash and look for local socket.
    let project_hash = compute_project_hash(&project_root);
    let home = dirs::home_dir()
        .ok_or(DiscoveryError::NoHome)?;
    let socket_path = home
        .join(".unimatrix")
        .join(&project_hash)
        .join("unimatrix.sock");

    if socket_path.exists() {
        return Ok(InstanceConfig::Local { socket_path });
    }

    // 4. User-level config (~/.unimatrix/config.toml).
    let user_config = home.join(".unimatrix").join("config.toml");
    if user_config.exists() {
        if let Some(config) = parse_user_config(&user_config)? {
            return Ok(config);
        }
    }

    // 5. No instance found.
    Err(DiscoveryError::NoInstance {
        project_root,
        project_hash,
        socket_path,
    })
}

pub enum InstanceConfig {
    /// Local Unix domain socket.
    Local { socket_path: PathBuf },
    /// Remote HTTPS endpoint.
    Remote { endpoint: String, auth: AuthCredential },
}
```

### Configuration Hierarchy

Priority (highest to lowest):

| Priority | Source | Example |
|----------|--------|---------|
| 1 | Environment variable `UNIMATRIX_SOCKET` | `/tmp/unimatrix-test.sock` |
| 2 | Environment variable `UNIMATRIX_URL` | `https://unimatrix.internal:8443` |
| 3 | Project config `.unimatrix/config.toml` | `[server]\nsocket = "/custom/path.sock"` |
| 4 | Auto-discovered socket `~/.unimatrix/{hash}/unimatrix.sock` | (computed from project root) |
| 5 | User config `~/.unimatrix/config.toml` | `[default]\nurl = "https://..."` |

### Multi-Project Isolation

The project hash provides automatic isolation:

```
~/.unimatrix/
  a1b2c3d4e5f6g7h8/       # Project A (hash of /home/user/projectA)
    unimatrix.redb
    unimatrix.sock
    unimatrix.pid
    vector/
  9876543210abcdef/       # Project B (hash of /home/user/projectB)
    unimatrix.redb
    unimatrix.sock
    unimatrix.pid
    vector/
```

The hook process inherits `cwd` from Claude Code, which is always the project root. The implant computes `SHA-256(canonicalize(cwd))[0..16]` and connects to the matching socket. This is the same algorithm used by `project.rs::compute_project_hash()`.

### Hook Environment Context

Claude Code provides rich context to hook processes via stdin JSON:

```json
{
  "session_id": "4c0ee78c-...",
  "cwd": "/workspaces/unimatrix",
  "transcript_path": "/home/user/.claude/projects/.../session.jsonl",
  "permission_mode": "bypassPermissions",
  "hook_event_name": "UserPromptSubmit"
}
```

The `cwd` field is the authoritative project root. The implant uses it for project hash computation. The `session_id` provides session correlation across hook invocations.

---

## 6. Threat Model

**Answers RQ-4d: What is the threat model for the implant specifically?**

### Attack Surface Overview

The cortical implant introduces a new attack surface distinct from the MCP server:

```
                     Attack Surface Map
                     ==================

  Claude Code Process
       |
       |  spawns hook process (inherits env, cwd, user)
       v
  ┌─────────────────────┐
  │  Cortical Implant    │ <-- Binary on filesystem
  │  (hook process)      │ <-- Reads .claude/settings.json
  │                      │ <-- Inherits environment variables
  └──────────┬──────────┘
             |
             |  Unix domain socket (filesystem permissions)
             v
  ┌─────────────────────┐
  │  Unimatrix Server    │ <-- Listens on unimatrix.sock
  │  (MCP + UDS)         │ <-- Accepts implant connections
  │                      │ <-- Writes to unimatrix.redb
  └─────────────────────┘
```

### Attack Tree

#### Vector 1: Malicious Hook Configuration

**Attack:** An attacker modifies `.claude/settings.json` to point hooks at a rogue binary instead of the legitimate `unimatrix-hook`.

```
settings.json (compromised):
  "command": "/tmp/evil-hook.sh"   # instead of "unimatrix-hook"
```

| Property | Assessment |
|----------|-----------|
| Prerequisite | Write access to `.claude/settings.json` |
| Severity | **HIGH** -- rogue binary sees all hook data (prompts, tool calls, session IDs) |
| Likelihood | LOW -- requires compromising the repo or developer machine |
| Detection | File integrity monitoring, git status |

**Mitigations:**
- M1.1: `.claude/settings.json` should be committed to git. Changes are visible in `git diff`.
- M1.2: The implant binary path should be absolute and match a known installation location. Relative paths or `/tmp` paths are a red flag.
- M1.3: Future: Hook binary signature verification. Claude Code could verify that hook binaries are signed before executing them (this requires Claude Code platform changes).
- M1.4: `.claude/settings.json` file permissions should be restricted (not world-writable).

#### Vector 2: Knowledge Base Poisoning via Rogue Implant

**Attack:** A compromised or rogue implant process connects to the Unimatrix server's Unix domain socket and writes malicious entries (poisoned conventions, misleading patterns) that future agents trust.

```
Rogue implant → unimatrix.sock → context_store("convention", "always rm -rf /")
```

| Property | Assessment |
|----------|-----------|
| Prerequisite | Same user, can reach socket file |
| Severity | **HIGH** -- poisoned entries propagate across feature cycles (amplification effect documented in MCP security analysis) |
| Likelihood | LOW (local), MEDIUM (shared dev environments) |
| Detection | content_hash chain verification, contradiction detection (crt-003), audit log |

**Mitigations:**
- M2.1: **SO_PEERCRED verification** -- server checks connecting process PID and verifies it matches known binary names via `/proc/{pid}/cmdline` (extending vnc-004's `is_unimatrix_process()` pattern). Rejects connections from unknown binaries.
- M2.2: **Trust level enforcement** -- the implant connects as `cortical-implant` (Internal trust). Its writes are tagged with `trust_source: "system"` and `created_by: "cortical-implant"`. Content scanning (vnc-002) applies to implant writes.
- M2.3: **Write rate limiting** -- the server enforces write rate limits per agent (crt-001). An implant writing 100 entries/second triggers anomaly detection.
- M2.4: **Content hash chain** -- every entry has `content_hash` and `previous_hash` fields. Retrospective analysis can detect entries that break the hash chain (indicating external injection).
- M2.5: **Contradiction detection** (crt-003) -- poisoned entries with high embedding similarity to existing entries but conflicting content are flagged.
- M2.6: **Audit trail** -- all implant operations are logged in AUDIT_LOG with `agent_id: "cortical-implant"`, PID, and timestamp. Forensic review can identify poisoning campaigns.

#### Vector 3: Man-in-the-Middle on Local Transport

**Attack:** An attacker creates a rogue socket at the same path, intercepting implant connections.

```
Attacker removes legitimate unimatrix.sock
Attacker creates rogue unimatrix.sock (attacker-controlled process listening)
Implant connects to rogue socket, sends query
Rogue socket returns poisoned results
```

| Property | Assessment |
|----------|-----------|
| Prerequisite | Same user or root, write access to ~/.unimatrix/{hash}/ |
| Severity | **HIGH** -- implant injects rogue results into agent context |
| Likelihood | VERY LOW -- attacker already has shell access as the user |
| Detection | PID file mismatch, process identity check |

**Mitigations:**
- M3.1: **Socket file ownership verification** -- before connecting, the implant verifies the socket file's owner UID matches the current process UID.
- M3.2: **Server PID verification** -- the implant reads `unimatrix.pid`, checks if the PID is alive, and verifies it is a `unimatrix-server` process via `/proc/{pid}/cmdline`. If the PID file and socket disagree (socket exists but PID file shows a different process), refuse to connect.
- M3.3: **Challenge-response on connect** -- the implant sends a nonce with the first request; the server signs it with the shared secret. This proves the socket is controlled by the legitimate server.
- M3.4: **Directory permissions** -- `~/.unimatrix/{hash}/` is `0o700`. No other user can create files in this directory.

#### Vector 4: Environment Variable Injection

**Attack:** A malicious process sets environment variables that redirect the implant to a rogue server.

```
export UNIMATRIX_SOCKET=/tmp/evil-unimatrix.sock
# Hook process inherits this, connects to rogue server
```

| Property | Assessment |
|----------|-----------|
| Prerequisite | Ability to set env vars in Claude Code's environment |
| Severity | **MEDIUM** -- redirects implant to rogue server for data exfiltration or poisoning |
| Likelihood | LOW -- requires modifying the shell profile or CI/CD environment |
| Detection | Log the resolved socket path, compare against expected |

**Mitigations:**
- M4.1: **Validation of override paths** -- if `UNIMATRIX_SOCKET` is set, the implant verifies: (a) the path is within `~/.unimatrix/`, (b) the socket file's owner matches the current UID, (c) the corresponding PID file exists and is valid. Paths outside the expected directory require explicit opt-in via `UNIMATRIX_ALLOW_CUSTOM_SOCKET=1`.
- M4.2: **Log the resolved path** -- every hook invocation logs (to stderr) which socket it connected to. Unexpected paths are visible in Claude Code's log output.
- M4.3: **Prefer auto-discovery over env vars** -- documentation recommends using auto-discovery (project hash) rather than env var overrides for production use.

#### Vector 5: Supply Chain -- Compromised Binary

**Attack:** The distributed `unimatrix-hook` binary is compromised (malicious npm package, compromised build pipeline, tampered GitHub release).

| Property | Assessment |
|----------|-----------|
| Prerequisite | Compromise of distribution channel |
| Severity | **CRITICAL** -- full code execution as user, access to all hook data |
| Likelihood | LOW (for direct attacks), but supply chain attacks are increasingly common |
| Detection | Binary hash verification, reproducible builds |

**Mitigations:**
- M5.1: **Binary hash verification** -- published checksums (SHA-256) for every release. `unimatrix init` verifies the installed binary hash against the published checksum.
- M5.2: **Signed releases** -- GitHub Releases with GPG-signed checksums. npm packages with `npm audit signatures`.
- M5.3: **Reproducible builds** -- CI pipeline produces deterministic binaries. Third parties can verify by rebuilding from source.
- M5.4: **Cargo crate publication** -- `cargo install unimatrix-hook` uses Cargo's built-in package verification (crate checksums on crates.io).
- M5.5: **Lock file pinning** -- teams pin the implant version in their project config. Automatic updates require explicit version bump.

#### Vector 6: Interaction with AGENT_REGISTRY Trust Hierarchy

The implant operates at a distinct trust level from MCP agents:

```
Trust Level     | Who                    | Capabilities
System          | Unimatrix internals    | All operations
Privileged      | Human via MCP          | All tools, all topics
Internal        | Cortical implant       | Read, Write, Search (not Admin)
Internal        | Orchestrator agents    | Read, Write, Search (per enrollment)
Restricted      | Unknown/worker agents  | Read, Search only
```

The implant is at **Internal** trust -- it can read and write knowledge entries but cannot modify the agent registry, change trust levels, or perform administrative operations. This is deliberate:

- The implant records events and sessions (requires Write).
- The implant queries knowledge for injection (requires Read + Search).
- The implant should NOT modify security configuration (no Admin).

**Risk:** If the implant were compromised, it could write poisoned entries but could not escalate its own privileges or modify other agents' trust levels. The damage is bounded by the Write capability's scope, and detectable via content scanning + contradiction detection.

### Summary: Risk Heat Map

| Vector | Severity | Likelihood | Residual Risk (after mitigations) |
|--------|----------|------------|-----------------------------------|
| V1: Malicious hook config | HIGH | LOW | LOW (git tracking, code review) |
| V2: KB poisoning via rogue implant | HIGH | LOW | LOW (SO_PEERCRED, content scanning, contradiction detection) |
| V3: MITM on local transport | HIGH | VERY LOW | NEGLIGIBLE (socket permissions, PID verification) |
| V4: Env var injection | MEDIUM | LOW | LOW (path validation, logging) |
| V5: Supply chain | CRITICAL | LOW | MEDIUM (verification helps but supply chain is inherently hard) |
| V6: Trust hierarchy abuse | MEDIUM | LOW | LOW (bounded capabilities, audit trail) |

### Threat Model Alignment with ASS-011 Findings

The ASS-011 identity threat model (see `product/research/ass-011/findings/identity-threat-model.md`) identified the primary threat as the LLM itself -- an overly helpful agent circumventing boundaries. The cortical implant threat model is different:

- **ASS-011 threat:** LLM circumvents controls (creative persistence, Bash escape hatch).
- **ASS-014 threat:** External compromise of the implant binary or communication channel.

These are complementary, not overlapping. The ASS-011 mitigations (signed capability tokens, tool-level enforcement) protect against LLM behavior. The ASS-014 mitigations (peer credentials, socket permissions, binary verification) protect against infrastructure compromise.

The implant's trust level (Internal) means that even if an LLM directed the implant to write malicious content, the existing content scanning and contradiction detection defenses apply. The implant is not a privilege escalation path.

---

## 7. Graceful Degradation Matrix

**Answers RQ-4e: How does the transport handle the MCP server not being available?**

### Failure Scenarios

| Scenario | Cause | Detection |
|----------|-------|-----------|
| S1: Server not running | Crashed, not started, killed | Socket file missing or connection refused |
| S2: Server overloaded | Too many concurrent requests | Connection timeout or slow response |
| S3: Database locked | Another process holds redb write lock | Server returns error on write operations |
| S4: Network partition (remote) | Internet outage, firewall change | Connection timeout |
| S5: Auth failure | Token expired, credentials rotated | 401/403 response |

### Per-Operation Degradation

| Operation | Hook Event | Mode | On Failure | Impact |
|-----------|-----------|------|------------|--------|
| Context injection | UserPromptSubmit | **Synchronous, results needed** | Skip injection entirely. Print nothing to stdout. Log failure to local event queue. | Agent operates without ambient context -- same as before cortical implant existed. Graceful because agents already work without injection. |
| Compaction defense | PreCompact | **Synchronous, results needed** | Use cached payload if available (see below). If no cache, skip. Log failure. | Risk of context loss on compaction. Cached payload provides partial defense. |
| Event recording | PostToolUse | **Fire-and-forget** | Queue event to local write-ahead log (WAL). Replay on reconnection. | No immediate impact. Events are recovered when server returns. |
| Session start | SessionStart | **Synchronous, advisory** | Queue registration. Proceed without server-side session tracking. | Session metadata may be incomplete in retrospective analysis. |
| Session close | SessionEnd | **Fire-and-forget** | Queue to WAL. Replay on reconnection. | Session boundary may be missed; detectable via gap analysis. |
| Health check | Implant startup | **Synchronous** | Mark transport as unavailable. All subsequent operations use degraded paths. | Implant operates in degraded mode for entire session. |

### Local Event Queue (Write-Ahead Log)

When the transport is unavailable, fire-and-forget events are written to a local file:

```
~/.unimatrix/{project_hash}/event-queue/
  pending-{timestamp}.jsonl     # One file per session
```

Structure per line:
```json
{"request": { ... }, "timestamp": 1709330000, "attempt": 0}
```

**Replay mechanism:**
1. On every successful transport connection, check for pending event files.
2. Read and replay events in timestamp order.
3. On successful replay, delete the pending file.
4. On failed replay, increment `attempt` counter. After 3 attempts, move to `failed/` subdirectory for manual inspection.

**Size limits:**
- Maximum 1000 events per file (rotate to new file).
- Maximum 10 pending files (oldest is dropped if limit exceeded).
- Maximum age: 7 days (older files are pruned on startup).

### Compaction Defense Cache

The compaction defense payload (PreCompact response) is the most latency-critical and loss-sensitive operation. A cached payload provides partial protection:

```
~/.unimatrix/{project_hash}/compaction-cache.json
```

Updated on every successful context injection (UserPromptSubmit):
```json
{
  "session_id": "4c0ee78c",
  "timestamp": 1709330000,
  "entries": [
    {"id": 42, "title": "...", "content": "...", "confidence": 0.87}
  ],
  "role": "developer",
  "feature": "col-006"
}
```

On PreCompact failure:
1. Read compaction cache.
2. If cache is from the current session and <30 minutes old, use it.
3. Format cached entries as briefing text and print to stdout.
4. This is better than nothing -- the agent retains its most recently injected knowledge.

### Recovery Sequence

When the server becomes available after a failure period:

```
1. Transport::connect() succeeds
2. Replay pending event queue (background, non-blocking)
3. Resume normal operation (context injection, event recording)
4. Update compaction cache on next successful injection
```

The implant does not retry connections in a loop. Each hook invocation is independent:
- Hook fires -> attempt connect -> success: normal operation -> exit
- Hook fires -> attempt connect -> failure: degraded operation -> exit

Since hooks are ephemeral (one process per event), there is no persistent retry loop. The "retry" happens naturally on the next hook event (next tool call, next prompt).

### Degradation Transparency

The implant communicates its degradation state to the agent via hook output:

- **Normal mode:** Inject knowledge content to stdout (UserPromptSubmit) or stderr (diagnostics).
- **Degraded mode:** Inject a minimal status line to stderr: `[unimatrix] server unavailable, operating in degraded mode`. This appears in Claude Code's log but not in the agent's context (stderr is not injected into prompts).

The agent never sees degradation. If knowledge cannot be injected, nothing is injected -- the agent operates as if the implant were not installed. This is the zero-regression guarantee: the implant can only add value, never subtract it.

---

## 8. Local to Centralized Transition

**What changes, what stays the same.**

### The Transition as a Config Change

The transport trait is designed so that switching from local to centralized is a configuration change:

```toml
# .unimatrix/config.toml

# Local mode (default):
[server]
transport = "local"
# Socket auto-discovered via project hash

# Remote mode:
[server]
transport = "remote"
url = "https://unimatrix.internal:8443"
auth = "oauth"
client_id = "team-project-alpha"
```

### What Does NOT Change

| Component | Why It Stays |
|-----------|-------------|
| Transport trait interface | Same `Request`/`Response` types regardless of transport |
| Request/Response types | Same operations (search, briefing, record event) |
| Wire protocol (JSON) | HTTPS carries JSON; UDS carries JSON |
| AuthContext struct | Same downstream interface; different `auth_method` |
| Graceful degradation | Same pattern (skip/queue/cache) for remote failures |
| Event queue (WAL) | Same local queue; more important for remote (network is less reliable) |
| Compaction cache | Same mechanism; unchanged by transport |
| Hook input/output format | Claude Code hooks are transport-agnostic |
| Discovery priority chain | `UNIMATRIX_URL` env var already in the chain |

### What Changes

| Component | Local | Remote | Migration Effort |
|-----------|-------|--------|-----------------|
| Transport implementation | `LocalTransport` (UDS) | `RemoteTransport` (HTTPS) | New impl, same trait |
| Authentication | SO_PEERCRED + shared secret | OAuth 2.1 / mTLS | New credential flow |
| Latency budget | <50ms | 100-500ms (network) | Adjust timeouts, consider async |
| Embedding | Server-side | Server-side (same) | None |
| Event queue replay | Local, fast | Remote, may need batching | Batch size tuning |
| Discovery | Project hash -> socket path | Config -> URL | Config file update |
| Error handling | Socket errors | HTTP errors + network errors | Map HTTP status codes to TransportError |

### Hybrid Mode

A future enhancement enables hybrid operation:

```toml
[server]
transport = "hybrid"
local_socket = "auto"  # For reads (low latency)
remote_url = "https://unimatrix.internal:8443"  # For writes (durability)
```

In hybrid mode:
- **Reads** (context injection, briefing) use the local transport for low latency.
- **Writes** (event recording, session lifecycle) use the remote transport for centralized durability.
- **Failover:** If local fails, fall back to remote for reads (higher latency but available). If remote fails, queue writes locally for later replay.

This is a natural extension of the trait-based design -- the `HybridTransport` holds both a `LocalTransport` and a `RemoteTransport` and dispatches based on operation type.

---

## 9. Open Risks

### OR-1: Embedding Latency for Context Injection

Context injection (UserPromptSubmit) requires semantic search, which requires embedding the prompt. ONNX runtime initialization is ~200ms cold start. Options:
- **Server-side embedding** (recommended): The implant sends raw text; the server has a warm ONNX runtime. This works for local transport but adds latency for remote.
- **Daemon mode**: The implant runs as a long-lived process with a warm ONNX runtime. This changes the hook architecture from ephemeral processes to IPC with a daemon.
- **Keyword fallback**: If embedding is unavailable, fall back to keyword search (lower quality but instant).

**Status:** Deferred to RQ-2 (Access Pattern Architecture). The transport trait does not dictate where embedding happens.

### OR-2: macOS Peer Credential Limitations

`getpeereid()` on macOS returns UID and GID but not PID. This means process lineage verification (`verify_implant_process()`) is not available on macOS via peer credentials alone.

**Mitigation:** On macOS, use the shared secret fallback for authentication. The filesystem permissions (socket mode `0o600`, directory mode `0o700`) still provide UID-level isolation.

### OR-3: Container Environment Socket Accessibility

In Docker/Podman containers, the socket path `~/.unimatrix/{hash}/unimatrix.sock` may not be accessible if the home directory is a volume mount. The socket file must be on a filesystem that supports Unix domain sockets.

**Mitigation:** Document the requirement. Provide `UNIMATRIX_SOCKET` env var override for non-standard environments. The dev container setup should map the socket directory as a tmpfs mount.

### OR-4: Concurrent Socket Connections Under Load

During high-frequency hook events (rapid tool calls in a swarm), multiple implant processes may connect to the socket simultaneously. The server must handle concurrent connections without blocking.

**Mitigation:** The tokio-based server naturally handles concurrent connections via task spawning. Each connection is independent (one request/response). Connection pooling is not needed for the ephemeral process model.

### OR-5: Version Mismatch Between Implant and Server

If the implant and server are different versions, the `Request`/`Response` JSON may contain unknown fields.

**Mitigation:** JSON with `#[serde(default)]` and `#[serde(deny_unknown_fields = false)]` provides forward and backward compatibility. Adding new `Request` variants requires the server to handle `Unknown` gracefully (return an error, don't crash). A `protocol_version` field in the `Ping`/`Pong` handshake enables explicit version negotiation.

### OR-6: Windows Platform Support

Unix domain sockets are available on Windows 10 1803+ via `AF_UNIX`, but the ecosystem is less mature. `SO_PEERCRED` has no Windows equivalent. Named pipes provide an alternative with Windows-native ACLs.

**Mitigation:** Abstract the platform-specific socket code behind a `PlatformSocket` trait. Implement `AF_UNIX` for Windows with named pipe fallback. Authentication on Windows uses the shared secret mechanism (no peer credentials). This is acceptable for the initial release; Windows support can be refined based on adoption.

### OR-7: Event Queue Data Loss on Crash

If the implant process crashes after queuing an event but before flushing to disk, the event is lost.

**Mitigation:** The event queue uses append-only JSONL files. Each event is written and `fsync()`-ed immediately. The window for data loss is the time between `write()` and `fsync()` -- typically <1ms. For fire-and-forget events (PostToolUse observation), occasional loss is acceptable. For session lifecycle events, the server can reconstruct boundaries from gap analysis.

---

## References

- [Unix domain sockets (man7.org)](https://man7.org/linux/man-pages/man7/unix.7.html) -- SO_PEERCRED documentation
- [Unix Domain Sockets (Matt Oswalt, 2025)](https://oswalt.dev/2025/08/unix-domain-sockets/) -- Modern UDS security practices
- [Unix Socket Permissions in Linux (linuxvox)](https://linuxvox.com/blog/unix-socket-permissions-linux/) -- Directory-level permission rules
- [Axum Unix Domain Socket Example (tokio-rs)](https://github.com/tokio-rs/axum/blob/main/examples/unix-domain-socket/src/main.rs) -- Rust UDS server with tokio
- [Hyperlocal (Rust crate)](https://lib.rs/crates/hyperlocal) -- Unix socket HTTP client/server for Rust
- [Zero Trust Networking: mTLS (2025)](https://medium.com/beyond-localhost/zero-trust-networking-replacing-api-keys-with-mutual-tls-mtls-b073d79f3b60) -- mTLS for service-to-service auth
- [mTLS Implementation Guide (Smallstep)](https://smallstep.com/docs/mtls/) -- Practical mTLS deployment
- [SO_PEERCRED wrapper (Go example)](https://github.com/joeshaw/peercred) -- Cross-platform peer credential patterns
- Unimatrix ADR-001: Use fs2 crate for advisory file locking (entry #189)
- Unimatrix ADR-007: Enforcement Point Architecture for Security (entry #83)
- Unimatrix ADR-003: Agent Identity via Tool Parameters (entry #31)
- Unimatrix: Security cross-cutting concerns and threat landscape (entry #7)
- ASS-011: Identity Threat Model (`product/research/ass-011/findings/identity-threat-model.md`)
- ASS-011: Hook Capability Validation (`product/research/ass-011/findings/hook-capabilities.md`)
- MCP Security: Roadmap Recommendations (`product/research/mcp-security/ROADMAP-SECURITY-RECOMMENDATIONS.md`)
