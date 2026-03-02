## ADR-003: Layered Authentication Without Shared Secret

### Context

The UDS listener accepts connections from hook processes. Since any process on the machine could attempt to connect to the socket, authentication is needed to ensure only authorized processes can send requests. ASS-014 (D6) designed a three-layer model: filesystem permissions, kernel peer credentials, and process lineage verification, with a shared secret file as an optional fourth layer.

The question for col-006 is: which layers to implement, and whether the shared secret is in scope.

The threat model for col-006 is local single-user development. The attacker model is: (a) processes running as a different user on the same machine, and (b) processes running as the same user that are not part of the Unimatrix ecosystem. Threat (a) is blocked by filesystem permissions. Threat (b) is mitigated by UID verification and, on Linux, process lineage checking.

Shared environments (CI runners with shared UIDs, multi-user systems) are not the deployment target for col-006. The shared secret mechanism adds configuration burden (file generation, distribution, rotation) that conflicts with the zero-configuration goal.

### Decision

col-006 implements three authentication layers. No shared secret.

**Layer 1: Filesystem permissions (all platforms)**
- Socket file created with mode `0o600` (owner read/write only)
- Data directory `~/.unimatrix/{hash}/` created with mode `0o700`
- Blocks connections from different users at the OS level
- This is the primary defense and is sufficient for most threat models

**Layer 2: UID verification via kernel peer credentials**
- Linux: `SO_PEERCRED` via `getsockopt()` returns `ucred { pid, uid, gid }`
- macOS: `getpeereid()` returns `(uid, gid)` — no PID
- After accepting a connection, extract the peer UID and compare to the server's UID
- Reject if UIDs do not match (redundant with Layer 1 but defense-in-depth)

**Layer 3: Process lineage (Linux only)**
- When Layer 2 provides a PID (Linux `SO_PEERCRED`), read `/proc/{pid}/cmdline`
- Check if any argument's filename is `unimatrix-server` (reusing the pattern from `pidfile::is_unimatrix_process()`)
- On macOS: Layer 3 is skipped (no PID available from `getpeereid()`)
- Layer 3 failure is a warning, not a rejection — it is advisory defense-in-depth

**Implementation notes:**
- `SO_PEERCRED` requires platform-specific code. Use `#[cfg(target_os = "linux")]` and `#[cfg(target_os = "macos")]` blocks.
- The `PeerCredentials` struct abstracts platform differences: `{ uid: u32, gid: u32, pid: Option<u32> }`. On macOS, `pid` is `None`.
- On auth failure (UID mismatch), close the connection and log to stderr. Do not send an error response — the connection is untrusted.

**Shared secret deferred.** If a future deployment scenario requires stronger authentication (shared-user CI, remote transport), a shared secret or mTLS layer can be added as Layer 4 without changing Layers 1-3.

### Consequences

**Easier:**
- Zero configuration. No token files to generate, distribute, or rotate.
- No secret management complexity in the hook process or server.
- Consistent with the existing PidGuard pattern (filesystem + process checks, no tokens).
- The hook subcommand connects without any authentication credentials — the socket permissions and UID are implicit.

**Harder:**
- On macOS, any process running as the same user can connect and send requests. This is acceptable for single-user development but insufficient for shared environments.
- The `/proc/{pid}/cmdline` check is Linux-specific and fragile (process may have a different binary name if installed via a symlink or wrapper). It is advisory, not blocking.
- Future remote transports (TCP, HTTPS) will need a different authentication mechanism (mTLS, bearer tokens). The current model is UDS-specific.
