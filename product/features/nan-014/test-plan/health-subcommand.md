# Test Plan: health-subcommand

## Component

New file: `crates/unimatrix-server/src/health.rs`
Modified: `crates/unimatrix-server/src/main.rs` (add `Health` variant to `Command` enum, add match arm)

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-03 (High) | Health resolves different socket path than serve | `test_health_socket_path_matches_serve` |
| R-03 (High) | Health with --project-dir matches serve with --project-dir | `test_health_project_dir_socket_path_consistency` |
| R-03 (High) | Health in container env (HOME=/data, --project-dir /data) | `test_health_container_env_socket_path` |

## Unit Tests (CLI Parsing)

Located in `crates/unimatrix-server/src/main_tests.rs`.

### test_health_subcommand_parsed

**Arrange**: Parse `["unimatrix", "health"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- `cli.command` matches `Some(Command::Health)`

### test_health_with_project_dir_parsed

**Arrange**: Parse `["unimatrix", "--project-dir", "/data", "health"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- `cli.command` matches `Some(Command::Health)`
- `cli.project_dir` is `Some(PathBuf::from("/data"))`

## Unit Tests (health::run)

Located in `crates/unimatrix-server/src/health.rs` (inline `#[cfg(test)]` module).

### test_health_returns_error_when_no_socket

**Arrange**: Create a temp project directory. No daemon running (no socket file).

**Act**: Call `health::run(Some(temp_dir.path()))`.

**Assert**:
- Returns `Err` (socket file does not exist or connection refused)

### test_health_socket_path_matches_serve

**Arrange**:
- Create a temp directory as `project_dir`
- Call `ensure_data_directory(Some(project_dir), None)` to get `paths_a`
- Call `ensure_data_directory(Some(project_dir), None)` again to get `paths_b`

**Act**: Compare `paths_a.mcp_socket_path` and `paths_b.mcp_socket_path`.

**Assert**:
- `paths_a.mcp_socket_path == paths_b.mcp_socket_path`
- This is the R-03 foundation: both `serve --foreground` and `health` call the same `ensure_data_directory` with the same inputs.

Note: This test already exists in `crates/unimatrix-engine/src/project.rs` as `test_deterministic_data_dir`. Confirm it still passes.

### test_health_project_dir_socket_path_consistency

**Arrange**:
- Create two separate temp directories as `project_dir` and `base_dir`
- Call `ensure_data_directory(Some(project_dir), Some(base_dir))` twice

**Act**: Compare returned `mcp_socket_path` values.

**Assert**:
- Both calls return identical `mcp_socket_path`
- The socket path is under `base_dir`, not under `project_dir` (base_dir contains the `.unimatrix/{hash}/` tree)

### test_health_run_timeout_on_nonresponsive_socket

**Arrange**:
- Create a UDS listener socket at the expected `mcp_socket_path` that accepts but never responds
- Or: create a file at the socket path that is not a socket

**Act**: Call `health::run(Some(project_dir))`.

**Assert**:
- Returns `Err` within the 3-second timeout (does not hang indefinitely)
- Error message written to stderr contains a diagnostic

### test_health_run_success_on_live_socket

**Arrange**:
- Create a UDS listener socket at the expected `mcp_socket_path` that accepts connections
- Spawn a simple listener thread: `let listener = UnixListener::bind(&socket_path); let _conn = listener.accept();`

**Act**: Call `health::run(Some(project_dir))`.

**Assert**:
- Returns `Ok(())` (exit code 0)
- No output to stdout (FR-5.7: silent on success)

## Shell/Container Tests

### AC-07: Health check integration

**Arrange**: Start container with `serve --foreground`.

**Act**:
1. Run `unimatrix health --project-dir /data` inside the container (or via docker exec with debug image)
2. Stop daemon, run `unimatrix health --project-dir /data` again

**Assert**:
1. First invocation exits 0
2. Second invocation exits 1
3. `docker inspect --format='{{.State.Health.Status}}' <container>` shows `healthy` while daemon runs

## Integration Tests

No new infra-001 tests. The `health` subcommand is not an MCP tool. It connects to the UDS socket at the OS level, not via JSON-RPC. Unit tests and container-level tests provide full coverage.

## Edge Cases

- **Socket path length**: `/data/.unimatrix/{16-char-hash}/unimatrix-mcp.sock` is ~60 bytes. Unix socket path limit is 108 bytes on Linux. Assert the path length is under 108.
- **Socket file exists but no listener**: `connect()` returns `ECONNREFUSED`. Health should exit 1 with a clear message.
- **Permissions on socket file**: If the socket file is not readable by UID 65534, the connect fails. Health should report a permissions diagnostic.
