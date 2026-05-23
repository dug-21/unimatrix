## ADR-003: Health Check via UDS Socket Connect

### Context

The container needs a `HEALTHCHECK` mechanism to detect daemon liveness. Options considered:

1. **HTTP health endpoint**: Requires W2-2 (HTTPS transport), which is not yet delivered. Adding a health-only HTTP listener is out of scope and creates a security surface (unauthenticated endpoint).

2. **UDS socket connect**: The daemon already binds `unimatrix-mcp.sock` as its MCP UDS socket. A successful Unix socket connection proves the daemon's accept loop is running.

3. **PID file check**: Verifies the process exists but not that it is responsive. The daemon could be deadlocked with a valid PID file.

4. **Signal-based check (kill -0)**: Same limitation as PID file — confirms process exists, not responsiveness.

SR-11 flags socket path consistency between the daemon and health subcommand as a risk.

### Decision

Implement `unimatrix health` as a synchronous CLI subcommand that:

1. Resolves `ProjectPaths` via `ensure_data_directory(cli.project_dir.as_deref(), None)` — the same function used by all other subcommands.
2. Checks if `paths.mcp_socket_path` exists on the filesystem.
3. Attempts `std::os::unix::net::UnixStream::connect(&paths.mcp_socket_path)` with a 3-second connect timeout (via `connect_timeout` or `set_nonblocking` + poll).
4. On successful connect: prints "healthy" to stdout, returns exit code 0.
5. On failure: prints diagnostic to stderr, returns exit code 1.

The Dockerfile `HEALTHCHECK` directive:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["unimatrix", "health", "--project-dir", "/data"]
```

**SR-11 mitigation**: Both `serve --foreground` and `health` pass the same `--project-dir /data` flag, which feeds into the same `ensure_data_directory` function. The project root is `/data`, the hash is `SHA-256("/data")[..16]`, and the MCP socket path is `/data/{hash}/unimatrix-mcp.sock`. No path divergence is possible when both commands receive the same `--project-dir`.

The health subcommand is sync-only (no tokio runtime) following the established pattern for Hook, Export, Import, Version, and Stop subcommands. This keeps startup time minimal (<50ms).

### Consequences

- **Easier**: Works immediately without W2-2. No HTTP listener, no authentication concern, no port exposure.
- **Easier**: UDS connect proves the daemon's accept loop is functional, not just that the process exists.
- **Easier**: Follows the established sync subcommand pattern — no new runtime dependencies.
- **Harder**: When W2-2 delivers HTTPS transport, a proper HTTP health endpoint would be more standard for orchestrators. The UDS health check remains useful as a container-internal liveness check even after W2-2. The Dockerfile `HEALTHCHECK` can be updated to use HTTP when available.
- **Harder**: The health check does not verify semantic readiness (e.g., embedding model loaded, database migrations complete). It verifies socket-level liveness only. Deeper health checks (schema version, model availability) can be added as a follow-up by sending an MCP ping request through the socket.
